use actix_web::{middleware, App, HttpServer};
use anyhow::Result;

#[actix_web::main]
pub async fn main() -> Result<()> {
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));

    let server = HttpServer::new(move || App::new().wrap(middleware::Logger::default()))
        .shutdown_timeout(10)
        .bind("localhost:8080")?;

    server.run().await?;
    Ok(())
}
