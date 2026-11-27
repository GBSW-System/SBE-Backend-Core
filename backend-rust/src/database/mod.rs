mod initialize_db;
#[cfg(test)]
mod tests;

use futures::{StreamExt, TryStreamExt, stream};
use sqlx::PgPool;

#[derive(Clone)]
pub struct Database {
    // PgPool의 clone은 같은 풀을 공유한다.
    pool: PgPool,
}

pub enum ReadMode {
    Sequential,
    Parallel,
}

// 어떤 조회를 할 지와 필요한 인자를 하나의 값으로 묶는다.
pub enum ReadQuery {
    ServiceName(String),       // 서비스 code로 이름 조회
    EnrolledCount(i32),        // 학년도로 재학 중인 학생 수 조회
    ServiceConfiguration(i64), // 서비스 ID로 설정 조회
}

// SQL 결과 행을 컬럼 이름에 맞춰 이 구조체로 변환한다.
#[derive(sqlx::FromRow)]
pub struct ServiceConfigRow {
    pub configuration: Option<serde_json::Value>,
    pub configuration_revision: i64,
}

// 서로 다른 조회 결과를 하나의 read() 함수에서 반환하기 위한 enum.
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

    // #[from] 덕분에 SQLx 오류에 ?를 사용하면 이 변형으로 변환된다.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl Database {
    pub async fn read(&self, query: ReadQuery) -> Result<ReadResult, sqlx::Error> {
        match query {
            ReadQuery::ServiceName(code) => {
                // query_scalar는 결과에서 값 한 개를 읽는다.
                // bind(code)는 SQL의 $1 자리에 값을 별도로 전달했다.
                let name = sqlx::query_scalar("SELECT name FROM services WHERE code = $1")
                    .bind(code)
                    .fetch_optional(&self.pool) // 행이 없을 수 있다.
                    .await?;

                Ok(ReadResult::ServiceName(name))
            }

            ReadQuery::EnrolledCount(year) => {
                // COUNT(*)는 조건을 만족하는 행이 없어도 값 0인 한 행을 반환한다.
                let count = sqlx::query_scalar("SELECT COUNT(*) FROM enrollments
                                                                   WHERE academic_year = $1 AND status = 'ENROLLED'")
                    .bind(year)
                    .fetch_one(&self.pool)
                    .await?;

                Ok(ReadResult::EnrolledCount(count))
            }

            ReadQuery::ServiceConfiguration(service_id) => {
                // query_as는 여러 컬럼을 ServiceConfigRow 한 개로 매핑한다.
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
            ReadMode::Sequential => 1, // 하나가 끝나면 다음 조회 진행
            ReadMode::Parallel => 4,   // 최대 4개의 조회를 동시에 진행
        };

        stream::iter(queries)
            .map(|q| self.read(q)) // 각 조회를 비동기 작업으로 바꾼다.
            .buffered(concurrency) // 동시 실행해도 반환 순서는 입력 순서이다.
            .try_collect() // 성공 결과를 Vec으로 모으고 오류는 전달한다.
            .await
    }

    pub async fn is_ready(&self) -> bool {
        // 실제로 풀에서 연결을 얻어 PostgresSQL이 응답하는지 확인한다.
        let probe = sqlx::query_scalar::<_, i32>("SELECT 1").fetch_one(&self.pool);

        // 2초 안에 쿼리까지 성공해서 실제 값 1을 받은 경우만 true다.
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
            return Ok(revision); // 변경과 이력 저장이 모두 성공했다.
        }

        // 0행 변경에는 두 가지 이유가 있다.
        // 서비스가 사라졌거나, 서비스는 있지만 리비전이 이미 달라졌다.
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

        // user_session.token_hash에는 원본 토큰의 SHA-256 결과 32바이트가 들어 있다.
        let token_hash = Sha256::digest(token.as_bytes()).to_vec();

        // include_str()는 빌드할 때 SQL 파일의 내용을 문자열로 포함한다.
        // 결과 튜플의 첫 값은 사용자 ID, 두 번째 값은 쓰기 허용 여부다.
        sqlx::query_as::<_, (i64, bool)>(include_str!("configuration_write_access.sql"))
            .bind(token_hash)
            .bind(service_id)
            .fetch_optional(&self.pool)
            .await
    }
}
