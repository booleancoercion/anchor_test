use std::env;

use actix_web::{middleware, web, App, HttpServer};
use anyhow::Result;
use db::Db;

mod db;
mod sheet;

struct AppData {
    db: Db,
    no_lookup_nulls: bool,
}

const DB_FILE: &str = "data.sqlite";

#[actix_web::main]
pub async fn main() -> Result<()> {
    // initializes env_logger with a log level of "info" by default. this can be controlled with the
    // RUST_LOG environment variable.
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));

    // this is here for integration testing since we don't want to create files
    let db = if env::var("MEMORY_DB").is_ok() {
        Db::new_memory().await?
    } else {
        Db::new(DB_FILE).await?
    };
    let data = web::Data::new(AppData {
        db,
        no_lookup_nulls: env::var("NO_LOOKUP_NULLS").is_ok(),
    });

    let server = HttpServer::new(move || {
        App::new()
            .app_data(data.clone())
            // the logger middleware allows actix_web to tap into our logging library very effortlessly.
            .wrap(middleware::Logger::default())
            // this will ensure that URIs always trim the trailing slash at the end, for consistency purposes
            .wrap(middleware::NormalizePath::trim())
            .service(web::scope("/sheet").configure(sheet::web::config))
    })
    // set a shutdown timeout, so that any remaining workers have some leeway
    .shutdown_timeout(10)
    // since this is a test application after all, we use localhost:8080 for now
    .bind("localhost:8080")?;

    server.run().await?;
    Ok(())
}
