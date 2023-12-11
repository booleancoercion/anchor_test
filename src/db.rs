use anyhow::Result;
use rand::{
    distributions::{Alphanumeric, DistString},
    Rng,
};
use serde::Deserialize;
use sqlx::{sqlite::SqliteConnectOptions, QueryBuilder, SqlitePool};

use crate::sheet::{self, CellValue, SchemaColumnKind};

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
            anyhow::bail!(
                "invalid length: expected {}, got {}",
                Self::LENGTH,
                value.len()
            );
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

    #[cfg(test)]
    /// Creates a new Db instance which uses a database in-memory, to avoid creating files when testing.
    pub async fn new_memory() -> Result<Self> {
        Self::new_inner(SqlitePool::connect(":memory:").await?).await
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

        // loop is necessary in case of duplicates. again, astronomically low chance.
        let sheetid = loop {
            let sheetid = SheetId::generate(&mut rand::thread_rng());

            if sqlx::query("INSERT INTO sheets (id) VALUES (?) RETURNING id;")
                .bind(&sheetid.0)
                .fetch_optional(&mut *tr)
                .await?
                .is_some()
            {
                break sheetid;
            }
        };

        // this table is necessary because it's a bad idea to name the database columns using the names that the user gave us.
        // instead we store the names as plain strings, and we'll use the id to derive a column name.
        // the `UNIQUE` modifier implicitly creates an index, so later looking up column ids by name will be efficient.
        sqlx::query(&format!(
            "CREATE TABLE sheet_{}_columns(
            id      INTEGER NOT NULL PRIMARY KEY,
            name    TEXT    NOT NULL UNIQUE,
            type    TEXT    NOT NULL
        );",
            &sheetid.0
        ))
        .execute(&mut *tr)
        .await?;

        QueryBuilder::new(format!(
            "INSERT INTO sheet_{}_columns (id, name, type) ",
            &sheetid.0
        ))
        .push_values(schema.columns.iter().enumerate(), |mut b, (i, col)| {
            b.push_bind(i as i64)
                .push_bind(&col.name)
                .push_bind(col.kind.get_sql_text());
        })
        .build()
        .execute(&mut *tr)
        .await?;

        // this is where we store the actual cell values
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

        builder.build().execute(&mut *tr).await?;

        tr.commit().await?;
        Ok(sheetid)
    }

    pub async fn insert_cell(&self, sheetid: &SheetId, cell: &sheet::Cell) -> Result<()> {
        let mut tr = self.pool.begin().await?;

        if sqlx::query_scalar::<_, i64>("SELECT EXISTS(SELECT 1 FROM sheets WHERE id = ?);")
            .bind(&sheetid.0)
            .fetch_one(&mut *tr)
            .await?
            == 0
        {
            anyhow::bail!("sheet doesn't exist");
        }

        // this format is ok, since SheetId is sanitized when deserialized
        let Some((colid, kind)): Option<(i64, String)> = sqlx::query_as(&format!(
            "SELECT id, type FROM sheet_{}_columns WHERE name = ?;",
            sheetid.inner()
        ))
        .bind(&cell.column)
        .fetch_optional(&mut *tr)
        .await?
        else {
            anyhow::bail!("invalid column name");
        };

        if kind != SchemaColumnKind::from(&cell.value).get_sql_text() {
            anyhow::bail!("invalid column type");
        }

        // again, the format is OK since everything is sanitized
        let query = format!("INSERT INTO sheet_{0} (row, col{1}) VALUES(?, ?) ON CONFLICT(row) DO UPDATE SET col{1} = excluded.col{1};", sheetid.inner(), colid);
        let query = sqlx::query(&query).bind(cell.row);

        // this is needed because they all have different types
        let query = match &cell.value {
            CellValue::Boolean(x) => query.bind(x),
            CellValue::Double(x) => query.bind(x),
            CellValue::Int(x) => query.bind(x),
            CellValue::String(x) => query.bind(x),
        };

        query.execute(&mut *tr).await?;

        tr.commit().await?;
        Ok(())
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
