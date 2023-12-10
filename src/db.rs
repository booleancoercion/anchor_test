use anyhow::Result;
use sqlx::{sqlite::SqliteConnectOptions, QueryBuilder, SqlitePool};

use crate::sheet;

pub struct SheetId(pub String);

impl SheetId {
    pub fn generate() -> Self {
        todo!()
    }
}

pub struct Db {
    pool: SqlitePool,
}

impl Db {
    /// Creates a new Db instance using the given filename as the name of the sqlite database.
    pub async fn new(filename: &str) -> Result<Self> {
        let options = SqliteConnectOptions::new()
            .filename(filename)
            // this is necessary so that we don't error when the file doesn't exist. we want to create it anyway
            .create_if_missing(true);

        let pool = SqlitePool::connect_with(options).await?;

        // create the initial "sheets" indexing table that we will use to easily check for column names.
        // `IF NOT EXISTS` enables us to not worry if the database file is new or not.
        sqlx::query(
            "\
                CREATE TABLE IF NOT EXISTS sheets(
                    id      TEXT NOT NULL PRIMARY KEY,
                );",
        )
        .execute(&pool)
        .await?;

        Ok(Self { pool })
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
            let sheetid = SheetId::generate();

            if sqlx::query_scalar::<_, i64>("SELECT EXISTS(SELECT 1 FROM sheets WHERE id = ?);")
                .bind(&sheetid.0)
                .fetch_one(&mut *tr)
                .await?
                == 0
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
            type    TEXT    NOT NULL,
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
                .push(col.kind.get_sql_text());
        })
        .build()
        .execute(&mut *tr)
        .await?;

        let mut builder = QueryBuilder::new(&format!(
            "CREATE TABLE sheet_{} (row INTEGER NOT NULL PRIMARY KEY",
            &sheetid.0
        ));

        let mut separated = builder.separated(", ");
        for (i, col) in schema.columns.iter().enumerate() {
            separated.push(format_args!("col{} {}", i, col.kind.get_sql_text()));
        }
        separated.push_unseparated(");");

        builder.build().execute(&mut *tr).await?;

        tr.commit().await?;
        Ok(sheetid)
    }
}
