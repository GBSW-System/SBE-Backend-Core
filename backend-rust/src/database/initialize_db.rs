use super::Database;
use sqlx::postgres::PgPoolOptions;
use std::time::Duration;

impl Database {
    pub async fn connect(url: &str) -> Result<Self, sqlx::Error> {
        let pool = PgPoolOptions::new()
            .max_connections(8) // 이 풀의 최대 연결 수
            .acquire_timeout(Duration::from_secs(5)) // 연결 확보를 기다리는 제한 시간
            .connect(url)
            .await?;

        // 빌드에 포함된 migrations를 읽고, 아직 적용하지 않은 버전을 적용한다.
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Self { pool } )
    }
}