mod initialize_db;
#[cfg(test)]
mod tests;

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
    ServiceConfiguration(i64),
}

#[derive(sqlx::FromRow)]
pub struct ServiceConfigRow {
    pub configuration: Option<serde_json::Value>,
    pub configuration_revision: i64,
}

pub enum ReadResult {
    ServiceName(Option<String>),
    EnrolledCount(i64),
    ServiceConfiguration(Option<ServiceConfigRow>),
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigUpdateError {
    #[error("서비스를 찾을 수 없습니다. (id={0})")]
    NotFound(i64),

    #[error("설정이 이미 변경되었습니다. (요청 리버전 {expected}, 현재 {current})")]
    RevisionConflict { expected: i64, current: i64 },

    #[error(transparent)]
    Database(#[from] sqlx::Error),
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

            ReadQuery::ServiceConfiguration(service_id) => {
                let row = sqlx::query_as::<_, ServiceConfigRow>(
                    "SELECT configuration, configuration_revision
                            FROM services
                            WHERE id = $1",
                )
                .bind(service_id)
                .fetch_optional(&self.pool)
                .await?;

                Ok(ReadResult::ServiceConfiguration(row))
            }
        }
    }

    pub async fn read_many(
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
        let probe = sqlx::query_scalar::<_, i32>("SELECT 1").fetch_one(&self.pool);

        matches!(
            tokio::time::timeout(std::time::Duration::from_secs(2), probe).await,
            Ok(Ok(1))
        )
    }

    pub async fn update_service_configuration(
        &self,
        service_id: i64,
        expected_revision: i64,
        configuration: Option<&serde_json::Value>,
        changed_by: i64,
    ) -> Result<i64, ConfigUpdateError> {
        let new_revision = sqlx::query_scalar::<_, i64>(
            r#"
        WITH updated AS (
            UPDATE services
            SET configuration = $3,
                configuration_revision = configuration_revision + 1
            WHERE id = $1
              AND configuration_revision = $2
            RETURNING id, configuration_revision, configuration
        )
        INSERT INTO service_configuration_versions
            (service_id, revision, configuration, changed_by)
        SELECT id, configuration_revision, configuration, $4
        FROM updated
        RETURNING revision
        "#,
        )
        .bind(service_id)
        .bind(expected_revision)
        .bind(configuration)
        .bind(changed_by)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(revision) = new_revision {
            return Ok(revision);
        }

        let current = sqlx::query_scalar::<_, i64>(
            "SELECT configuration_revision FROM services WHERE id = $1",
        )
        .bind(service_id)
        .fetch_optional(&self.pool)
        .await?;

        match current {
            Some(current) => Err(ConfigUpdateError::RevisionConflict {
                expected: expected_revision,
                current,
            }),
            None => Err(ConfigUpdateError::NotFound(service_id)),
        }
    }

    pub async fn configuration_write_access(
        &self,
        token: &str,
        service_id: i64,
    ) -> Result<Option<(i64, bool)>, sqlx::Error> {
        use sha2::{Digest, Sha256};

        let token_hash = Sha256::digest(token.as_bytes()).to_vec();
        sqlx::query_as::<_, (i64, bool)>(include_str!("configuration_write_access.sql"))
            .bind(token_hash)
            .bind(service_id)
            .fetch_optional(&self.pool)
            .await
    }
}
