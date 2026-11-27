mod database;

use anyhow::Result;
use database::Database;
use std::{env, time::Duration};
use tonic::transport::Server;
use tonic_health::{server::health_reporter, ServingStatus};

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let db = Database::connect(&env::var("DATABASE_URL")?).await?;
    let addr = env::var("GPRC_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:50051".into())
        .parse()?;

    let (reporter, health_service) = health_reporter();

    reporter.set_service_status("", ServingStatus::NotServing).await;

    let monitor = tokio::spawn(async move {
        loop {
            let status = if db.is_ready().await {
                ServingStatus::Serving
            } else {
                ServingStatus::NotServing
            };

            reporter.set_service_status("", status).await;
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    });

    println!("Rust gPRC: {addr}");
    let result = Server::builder()
        .add_service(health_service)
        .serve_with_shutdown(addr, async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await;

    monitor.abort();
    result?;
    Ok(())

}