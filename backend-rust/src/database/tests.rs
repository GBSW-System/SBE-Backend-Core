use super::{ConfigUpdateError, Database, ReadQuery, ReadResult};
use serde_json::json;
use sqlx::PgPool;

// SQLx가 테스트용 DB를 준비하고 마이그레이션을 적용한 뒤 풀을 넘겨준다.
#[sqlx::test(migrations = "./migrations")]
async fn stale_revision_cannot_overwrite_configuration(pool: PgPool) -> anyhow::Result<()> {
    // 새 서비스의 configuration_revision 기본값은 0이다.
    let service_id: i64 = sqlx::query_scalar(
        "INSERT INTO services (code, name) VALUES ('oj','테스트 OJ') RETURNING id",
    )
    .fetch_one(&pool)
    .await?;

    // changed_by가 참조할 실제 사용자 행을 생성한다.
    let user_id: i64 = sqlx::query_scalar(
        "INSERT INTO users (name, account_type) VALUES
            ('테스트 교사','TEACHER') RETURNING id",
    )
    .fetch_one(&pool)
    .await?;

    let db = Database { pool: pool.clone() };
    let settings = json!({"max_submissions_per_minute":5});

    // 첫 번째 요청은 현재 리비전 0을 정확히 알고 있다.
    let revision = db
        .update_service_configuration(service_id, 0, Some(&settings), user_id)
        .await?;
    assert_eq!(revision, 1);

    // 이전 화면에서 읽었던 리비전 0으로 다른 값을 저장하려는 상황
    let other_settings = json!({"max_submissions_per_minute": 99});
    let retry = db
        .update_service_configuration(service_id, 0, Some(&other_settings), user_id)
        .await;

    // 여기서 오류 자체가 기대 결과이므로, 위 await 뒤에 ?를 붙이지 않았다.
    assert!(matches!(
        retry,
        Err(ConfigUpdateError::RevisionConflict {
            expected: 0,
            current: 1
        })
    ));

    // 실제값이 99로 덮어써지지 않았는지 다시 읽는다.
    let result = db.read(ReadQuery::ServiceConfiguration(service_id)).await?;
    let ReadResult::ServiceConfiguration(Some(row)) = result else {
        anyhow::bail!("등록한 서비스의 설정을 조회할 수 없습니다.");
    };
    assert_eq!(row.configuration, Some(settings));
    assert_eq!(row.configuration_revision, 1);

    // 성공한 요청 한 건만 남겨야 한다.
    let history_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM service_configuration_versions WHERE service_id = $1",
    )
    .bind(service_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(history_count, 1);

    Ok(())
}
