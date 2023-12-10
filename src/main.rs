use actix_web::{middleware, App, HttpServer};
use anyhow::Result;

mod db;
mod sheet;

#[actix_web::main]
pub async fn main() -> Result<()> {
    // initializes env_logger with a log level of "info" by default. this can be controlled with the
    // RUST_LOG environment variable.
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));

    let server = HttpServer::new(move || {
        App::new()
            // the logger middleware allows actix_web to tap into our logging library very effortlessly.
            .wrap(middleware::Logger::default())
    })
    // set a shutdown timeout, so that any remaining workers have some leeway
    .shutdown_timeout(10)
    // since this is a test application after all, we use localhost:8080 for now
    .bind("localhost:8080")?;

    server.run().await?;
    Ok(())
}
