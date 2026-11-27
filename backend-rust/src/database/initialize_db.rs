use super::Database;
use sqlx::postgres::PgPoolOptions;
use std::time::Duration;

impl Database {
    pub async fn connect(url: &str) -> Result<Self, sqlx::Error> {
        let pool = PgPoolOptions::new()
            .max_connections(8)
            .acquire_timeout(Duration::from_secs(5))
            .connect(url)
            .await?;

        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Self { pool } )
    }
}