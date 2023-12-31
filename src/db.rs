use std::collections::HashMap;

use anyhow::Result;
use rand::{
    distributions::{Alphanumeric, DistString},
    Rng,
};
use serde::Deserialize;
use sqlx::{sqlite::SqliteConnectOptions, QueryBuilder, Row, SqlitePool};

use crate::sheet::{self, CellValue, SchemaColumnKind, SheetContentColumn};

#[derive(Deserialize)]
#[serde(try_from = "&str")]
pub struct SheetId(String);

impl SheetId {
    // arbitrary - should be long enough to support a very, very large amount of sheets without collisions.
    const LENGTH: usize = 24;

    pub fn generate<R: Rng + ?Sized>(r: &mut R) -> Self {
        let mut inner = String::new();
        Alphanumeric.append_string(r, &mut inner, Self::LENGTH);

        Self(inner)
    }

    pub fn inner(&self) -> &str {
        &self.0
    }
}

impl TryFrom<&str> for SheetId {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self> {
        if value.len() != Self::LENGTH {
            anyhow::bail!("invalid length: expected {}, got {}", Self::LENGTH, value.len());
        } else if value.chars().any(|x| !x.is_ascii_alphanumeric()) {
            anyhow::bail!("invalid content: {value} is not alphanumeric");
        } else {
            Ok(Self(value.into()))
        }
    }
}

pub struct Db {
    pool: SqlitePool,
}

impl Db {
    async fn new_inner(pool: SqlitePool) -> Result<Self> {
        // create the initial "sheets" indexing table that we will use to easily check for column names.
        // `IF NOT EXISTS` enables us to not worry if the database file is new or not.
        sqlx::query(
            "\
                CREATE TABLE IF NOT EXISTS sheets(
                    id      TEXT NOT NULL PRIMARY KEY
                );",
        )
        .execute(&pool)
        .await?;

        Ok(Self { pool })
    }

    /// Creates a new Db instance using the given filename as the name of the sqlite database.
    pub async fn new(filename: &str) -> Result<Self> {
        let options = SqliteConnectOptions::new()
            .filename(filename)
            // this is necessary so that we don't error when the file doesn't exist. we want to create it anyway
            .create_if_missing(true);

        let pool = SqlitePool::connect_with(options).await?;

        Self::new_inner(pool).await
    }

    /// Creates a new Db instance which uses a database in-memory, to avoid creating files when testing.
    pub async fn new_memory() -> Result<Self> {
        Self::new_inner(SqlitePool::connect(":memory:").await?).await
    }

    async fn register_random_sheetid(
        tr: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    ) -> Result<SheetId> {
        // loop is necessary in case of duplicates. again, astronomically low chance.
        loop {
            let sheetid = SheetId::generate(&mut rand::thread_rng());

            if sqlx::query("INSERT INTO sheets (id) VALUES (?) RETURNING id;")
                .bind(&sheetid.0)
                .fetch_optional(tr.as_mut())
                .await?
                .is_some()
            {
                break Ok(sheetid);
            }
        }
    }

    async fn build_columns_table(
        tr: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        sheetid: &SheetId,
        schema: &sheet::Schema,
    ) -> Result<()> {
        sqlx::query(&format!(
            "CREATE TABLE sheet_{}_columns(
            id      INTEGER NOT NULL PRIMARY KEY,
            name    TEXT    NOT NULL UNIQUE,
            type    TEXT    NOT NULL
        );",
            &sheetid.0
        ))
        .execute(tr.as_mut())
        .await?;

        QueryBuilder::new(format!("INSERT INTO sheet_{}_columns (id, name, type) ", &sheetid.0))
            .push_values(schema.columns.iter().enumerate(), |mut b, (i, col)| {
                b.push_bind(i as i64)
                    .push_bind(&col.name)
                    .push_bind(col.kind.get_sql_text());
            })
            .build()
            .execute(tr.as_mut())
            .await?;

        Ok(())
    }

    async fn build_sheet_table(
        tr: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        sheetid: &SheetId,
        schema: &sheet::Schema,
    ) -> Result<()> {
        let mut builder = QueryBuilder::new(&format!("CREATE TABLE sheet_{} (", &sheetid.0));

        // this essentially generates a bunch of columns like this:
        // row INTEGER NOT NULL PRIMARY KEY,
        // col0 TYPE,
        // col1 TYPE,
        // col2 TYPE,
        // ..etc
        let mut separated = builder.separated(", ");
        separated.push("row INTEGER NOT NULL PRIMARY KEY");
        for (i, col) in schema.columns.iter().enumerate() {
            separated.push(format_args!("col{} {}", i, col.kind.get_sql_text()));
        }
        separated.push_unseparated(");");

        builder.build().execute(tr.as_mut()).await?;

        Ok(())
    }

    async fn build_lookup_table(
        tr: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        sheetid: &SheetId,
    ) -> Result<()> {
        sqlx::query(&format!(
            "CREATE TABLE sheet_{}_lookups(
            col_id          INTEGER NOT NULL,
            row             INTEGER NOT NULL,
            target_col_id   INTEGER NOT NULL,
            target_row      INTEGER NOT NULL
        );",
            &sheetid.0
        ))
        .execute(tr.as_mut())
        .await?;

        sqlx::query(&format!(
            "CREATE UNIQUE INDEX index_lookups ON sheet_{}_lookups (col_id, row);",
            &sheetid.0
        ))
        .execute(tr.as_mut())
        .await?;

        Ok(())
    }

    /// Generates a new sheet with a unique id, according to the given schema.
    ///
    /// # Errors
    /// In case the schema is invalid, or a database failure.
    pub async fn new_sheet(&self, schema: &sheet::Schema) -> Result<SheetId> {
        if !schema.is_valid() {
            anyhow::bail!("Invalid schema");
        }

        // we need a transaction here, to make sure that a generated sheet id isn't accidentally taken by somebody
        // else, causing a race condition. the chance of that happening is astronomically small, but not zero nonetheless.
        let mut tr = self.pool.begin().await?;

        let sheetid = Self::register_random_sheetid(&mut tr).await?;

        // this table is necessary because it's a bad idea to name the database columns using the names that the user gave us.
        // instead we store the names as plain strings, and we'll use the id to derive a column name.
        // the `UNIQUE` modifier implicitly creates an index, so later looking up column ids by name will be efficient.
        Self::build_columns_table(&mut tr, &sheetid, schema).await?;

        // this is where we store the actual cell values, apart from lookup cells
        Self::build_sheet_table(&mut tr, &sheetid, schema).await?;

        // this is where we store only the lookup cells. a cell cannot be in both the above table and this table.
        Self::build_lookup_table(&mut tr, &sheetid).await?;

        tr.commit().await?;
        Ok(sheetid)
    }

    async fn get_column_by_name(
        tr: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        sheetid: &SheetId,
        name: &str,
    ) -> Result<Option<(i64, SchemaColumnKind)>> {
        Ok(sqlx::query_as::<_, (i64, String)>(&format!(
            "SELECT id, type FROM sheet_{}_columns WHERE name = ?;",
            sheetid.inner()
        ))
        .bind(name)
        .fetch_optional(tr.as_mut())
        .await?
        .map(|(id, kind)| (id, SchemaColumnKind::from_sql_text(&kind).unwrap())))
    }

    async fn detect_cycle(
        tr: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        sheetid: &SheetId,
        col_id: i64,
        row: i64,
        mut target_col_id: i64,
        mut target_row: i64,
    ) -> Result<bool> {
        if col_id == target_col_id && row == target_row {
            return Ok(true);
        }

        let query = format!(
            "SELECT target_col_id, target_row FROM sheet_{}_lookups WHERE col_id = ? AND row = ?;",
            &sheetid.0
        );
        loop {
            let Some((new_col_id, new_row)) = sqlx::query_as::<_, (i64, i64)>(&query)
                .bind(target_col_id)
                .bind(target_row)
                .fetch_optional(tr.as_mut())
                .await?
            else {
                return Ok(false);
            };

            if new_col_id == col_id && new_row == row {
                return Ok(true);
            } else {
                target_col_id = new_col_id;
                target_row = new_row;
            }
        }
    }

    async fn sheet_exists(
        tr: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        sheetid: &SheetId,
    ) -> Result<bool> {
        Ok(sqlx::query_scalar::<_, i64>("SELECT EXISTS(SELECT 1 FROM sheets WHERE id = ?);")
            .bind(&sheetid.0)
            .fetch_one(tr.as_mut())
            .await?
            == 1)
    }

    pub async fn insert_cell(&self, sheetid: &SheetId, cell: &sheet::Cell) -> Result<()> {
        let mut tr = self.pool.begin().await?;

        if !Self::sheet_exists(&mut tr, sheetid).await? {
            anyhow::bail!("sheet doesn't exist");
        }

        // this format is ok, since SheetId is sanitized when deserialized
        let Some((col_id, kind)) = Self::get_column_by_name(&mut tr, sheetid, &cell.column).await?
        else {
            anyhow::bail!("invalid column name");
        };

        if let Some(lookup) = cell.value.is_lookup() {
            let Some((target_col_id, target_kind)) =
                Self::get_column_by_name(&mut tr, sheetid, &lookup.target_col).await?
            else {
                anyhow::bail!("invalid target column name");
            };

            if kind != target_kind {
                anyhow::bail!("invalid target column type");
            }

            if Self::detect_cycle(
                &mut tr,
                sheetid,
                col_id,
                cell.row,
                target_col_id,
                lookup.target_row,
            )
            .await?
            {
                anyhow::bail!("detected lookup cycle");
            }

            sqlx::query(&format!(
                "UPDATE sheet_{} SET col{} = NULL WHERE row = ?;",
                &sheetid.0, col_id
            ))
            .bind(cell.row)
            .execute(&mut *tr)
            .await?;

            sqlx::query(&format!(
                "INSERT INTO sheet_{}_lookups
            (col_id, row, target_col_id, target_row)
            VALUES (?, ?, ?, ?)
            ON CONFLICT(col_id, row)
            DO UPDATE SET target_col_id = excluded.target_col_id,
            target_row = excluded.target_row;",
                &sheetid.0
            ))
            .bind(col_id)
            .bind(cell.row)
            .bind(target_col_id)
            .bind(lookup.target_row)
            .execute(&mut *tr)
            .await?;
        } else {
            if kind != SchemaColumnKind::from(&cell.value) {
                anyhow::bail!("invalid column type");
            }

            // we can't have an entry for the same cell in both tables
            sqlx::query(&format!(
                "DELETE FROM sheet_{}_lookups WHERE col_id = ? AND row = ?;",
                &sheetid.0
            ))
            .bind(col_id)
            .bind(cell.row)
            .execute(&mut *tr)
            .await?;

            // again, the format is OK since everything is sanitized
            let query = format!("INSERT INTO sheet_{0} (row, col{1}) VALUES(?, ?) ON CONFLICT(row) DO UPDATE SET col{1} = excluded.col{1};", sheetid.inner(), col_id);
            let query = sqlx::query(&query).bind(cell.row);

            // this is needed because they all have different types
            let query = match &cell.value {
                CellValue::Boolean(x) => query.bind(x),
                CellValue::Double(x) => query.bind(x),
                CellValue::Int(x) => query.bind(x),
                CellValue::String(x) => query.bind(x),
            };

            query.execute(&mut *tr).await?;
        }

        tr.commit().await?;
        Ok(())
    }

    async fn get_column_table(
        tr: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        sheetid: &SheetId,
    ) -> Result<Vec<(String, SchemaColumnKind)>> {
        let res = sqlx::query_as::<_, (String, String)>(&format!(
            "SELECT name, type FROM sheet_{}_columns ORDER BY id ASC;",
            &sheetid.0
        ))
        .fetch_all(tr.as_mut())
        .await?;

        Ok(res
            .into_iter()
            .map(|(name, kind)| (name, SchemaColumnKind::from_sql_text(&kind).unwrap()))
            .collect())
    }

    async fn get_column_content(
        tr: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        sheetid: &SheetId,
        kind: SchemaColumnKind,
        col_id: i64,
    ) -> Result<HashMap<i64, Option<CellValue>>> {
        let rows = sqlx::query(&format!(
            "SELECT row, col{0} FROM sheet_{1} WHERE NOT col{0} IS NULL;",
            col_id, &sheetid.0
        ))
        .fetch_all(tr.as_mut())
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| {
                let id = row.get::<i64, usize>(0);

                let val = match kind {
                    SchemaColumnKind::Boolean => CellValue::Boolean(row.get::<bool, usize>(1)),
                    SchemaColumnKind::Int => CellValue::Int(row.get::<i64, usize>(1)),
                    SchemaColumnKind::Double => CellValue::Double(row.get::<f64, usize>(1)),
                    SchemaColumnKind::String => CellValue::String(row.get::<String, usize>(1)),
                };

                (id, Some(val))
            })
            .collect())
    }

    async fn get_lookups(
        tr: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        sheetid: &SheetId,
    ) -> Result<HashMap<(i64, i64), (i64, i64)>> {
        Ok(sqlx::query_as::<_, (i64, i64, i64, i64)>(&format!(
            "SELECT col_id, row, target_col_id, target_row FROM sheet_{}_lookups;",
            &sheetid.0
        ))
        .fetch_all(tr.as_mut())
        .await?
        .into_iter()
        .map(|(a, b, c, d)| ((a, b), (c, d)))
        .collect())
    }

    pub async fn get_sheet(
        &self,
        sheetid: &SheetId,
        no_lookup_nulls: bool,
    ) -> Result<sheet::SheetContent> {
        let mut tr = self.pool.begin().await?;

        if !Self::sheet_exists(&mut tr, sheetid).await? {
            anyhow::bail!("sheet doesn't exist");
        }

        let column_table = Self::get_column_table(&mut tr, sheetid).await?;
        let mut regular_content = {
            let mut regular_content = vec![];
            for (i, (_, kind)) in column_table.iter().enumerate() {
                let column_content =
                    Self::get_column_content(&mut tr, sheetid, *kind, i as i64).await?;
                regular_content.push(column_content);
            }
            regular_content
        };
        let mut unresolved_lookups = Self::get_lookups(&mut tr, sheetid).await?;
        tr.commit().await?; // we commit here to not hold up the database - we got all the data out at this point

        // this loop efficiently resolves all of the lookup() entries. we find continuous chains of lookup()s, and evaluate them all at once.
        while !unresolved_lookups.is_empty() {
            let mut stack = vec![];

            let mut current_key = *unresolved_lookups.keys().next().unwrap();
            loop {
                if let Some(next_key) = unresolved_lookups.remove(&current_key) {
                    stack.push(current_key);
                    current_key = next_key;
                } else {
                    let val = regular_content[current_key.0 as usize]
                        .get(&current_key.1)
                        .cloned()
                        .flatten();
                    if !no_lookup_nulls {
                        for key in &stack {
                            regular_content[key.0 as usize].insert(key.1, val.clone());
                        }
                    }
                    break;
                }
            }
        }

        let mut output = HashMap::new();
        // using .rev() because we're continously popping from regular_content (so as to not clone anything)
        for (name, _) in column_table.into_iter().rev() {
            let col = regular_content
                .pop()
                .unwrap()
                .into_iter()
                .map(|(row, value)| SheetContentColumn { row, value })
                .collect();

            output.insert(name, col);
        }

        Ok(sheet::SheetContent { columns: output })
    }
}

#[cfg(test)]
mod tests {
    use super::SheetId;

    #[test]
    fn sheet_id_valid_try_from() {
        let str = "abCDefGHijklMnOPqrst1234";
        let sheet_id = SheetId::try_from(str).unwrap();
        assert_eq!(sheet_id.inner(), str)
    }

    #[test]
    #[should_panic]
    fn sheet_id_invalid_try_from_length() {
        let _ = SheetId::try_from("invalidlength").unwrap();
    }

    #[test]
    #[should_panic]
    fn sheet_id_invalid_try_from_content() {
        let _ = SheetId::try_from("invalid characters!zzzzz").unwrap();
    }
}
