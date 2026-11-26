mod initialize_db;

use futures::{StreamExt, TryStreamExt, stream};
use sqlx::PgPool;

#[derive(Clone)]
pub struct Database {
    pool: PgPool,
}

pub enum ReadMode {
    Sequential,
    Parallel,
}

pub enum ReadQuery {
    ServiceName(String),
    EnrolledCount(i32),
}

pub enum ReadResult {
    ServiceName(Option<String>),
    EnrolledCount(i64),
}

impl Database {
    pub async fn read(&self, query: ReadQuery) -> Result<ReadResult, sqlx::Error> {
        match query {
            ReadQuery::ServiceName(code) => {
                let name = sqlx::query_scalar("SELECT name FROM services WHERE code = $1")
                    .bind(code)
                    .fetch_optional(&self.pool)
                    .await?;

                Ok(ReadResult::ServiceName(name))
            }

            ReadQuery::EnrolledCount(year) => {
                let count = sqlx::query_scalar("SELECT COUNT(*) FROM enrollments
                                                                   WHERE academic_year = $1 AND status = 'ENROLLED'")
                    .bind(year)
                    .fetch_one(&self.pool)
                    .await?;
                
                Ok(ReadResult::EnrolledCount(count))
            }
        }
    }
    
    pub async fn read_many (
        &self,
        queries: Vec<ReadQuery>,
        mode: ReadMode,
    ) -> Result<Vec<ReadResult>, sqlx::Error> {
        let concurrency = match mode {
            ReadMode::Sequential => 1,
            ReadMode::Parallel => 4,
        };
        
        stream::iter(queries)
            .map(|q| self.read(q))
            .buffered(concurrency)
            .try_collect()
            .await
    }
    
    pub async fn is_ready(&self) -> bool {
        let probe = sqlx::query_scalar::<_, i32>("SELECT 1")
            .fetch_one(&self.pool);
        
        matches!(
            tokio::time::timeout(std::time::Duration::from_secs(2), probe).await,
            Ok(Ok(1))
        )
    }
}
