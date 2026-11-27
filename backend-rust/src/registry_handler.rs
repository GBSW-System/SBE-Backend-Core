use crate::database::{Database, ReadQuery, ReadResult};
use crate::registry::{
    GetServiceConfigurationRequest, GetServiceConfigurationResponse,
    service_registry_server::ServiceRegistry,
};
use tonic::{Request, Response, Status};
use crate::database::ConfigUpdateError;
use crate::registry::{
    UpdateServiceConfigurationRequest, UpdateServiceConfigurationResponse,
    update_service_configuration_request::Change
};
use std::collections::HashSet;

// 현재 파일에 남아 있는 타입이다.
// 사용자와 권한을 조회하므로, 이 구조체를 참조하지 않는다.
#[derive(Clone)]
pub(crate) struct AuthenticatedUser {
    pub(crate) user_id: i64,
    pub(crate) editable_service_ids: HashSet<i64>,
}

pub struct RegistryHandler {
    // 이 핸들러가 사용할 공용 DB 풀의 핸들이다.
    db: Database,
}

impl RegistryHandler {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

// .proto로부터 생성된 ServiceRegistry 트레이트의 메서드를 구현한다.
#[tonic::async_trait]
impl ServiceRegistry for RegistryHandler {
    async fn get_service_configuration(
        &self,
        request: Request<GetServiceConfigurationRequest>,
    ) -> Result<Response<GetServiceConfigurationResponse>, Status> {
        // inno_inner()는 gRPC 요청에서 실제 protobuf 메시지를 꺼낸다.
        let service_id = request.into_inner().service_id;
        if service_id <= 0 {
            return Err(Status::invalid_argument("service_id는 양수여야 합니다."));
        }

        let result = self.db
            .read(ReadQuery::ServiceConfiguration(service_id))
            .await
            .map_err(|err| {
                // DB 오류는 서버에 기록하고 클라이언트에는 gRPC 오류를 반환한다.
                eprintln!("서비스 설정 조회 실패 (service_id={service_id}): {err}");
                Status::internal("서비스 설정을 조회할 수 없습니다.")
            })?;

        let row = match result {
            // 서비스가 존재하면 설정값이 SQL NULL이어도 row는 존재한다.
            ReadResult::ServiceConfiguration(Some(row)) => row,

            // 여기서 None은 서비스 행 자체를 찾지는 못했다는 뜻이다.
            ReadResult::ServiceConfiguration(None) => {
                return Err(Status::not_found("서비스를 찾을 수 없습니다."));
            }

            // 요청한 쿼리와 다른 종류의 결과가 오면 내부 구현 오류이다.
            _ => return Err(Status::internal("조회 결과 타입이 일치하지 않습니다.")),
        };

        Ok(Response::new(GetServiceConfigurationResponse {
            // Some(JSON)이면 문자열로, SQL NULL이면 필드 생략으로 전달한다.
            configuration_json: row.configuration.map(|value| value.to_string()),
            configuration_revision: row.configuration_revision,
        }))
    }

    async fn update_service_configuration(
        &self,
        request: Request<UpdateServiceConfigurationRequest>,
    ) -> Result<Response<UpdateServiceConfigurationResponse>, Status> {
        // authorization: Bearer <token>에서 실제 찾은 토큰 부분을 빌려 온다.
        let token = request.metadata()
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
            .filter(|value| !value.is_empty())
            .ok_or_else(|| Status::unauthenticated("세션 토큰이 필요합니다."))?;

        // get_ref()는 요청 본문을 빌린다. (빌리기 때문에 request는 계속 쓸 수 있다.)
        let service_id = request.get_ref().service_id;
        if service_id <= 0 {
            return Err(Status::invalid_argument("service_id는 양수여야 합니다."));
        }

        // None이면 유효한 세션 없음
        // Some((사용자 ID, false))이면 로그인은 했지만 이 서비스의 수정 권한이 없다는 뜻이다.
        let (user_id, can_write) = self.db
            .configuration_write_access(token, service_id)
            .await
            .map_err(|error| {
                tracing::error!(%error, "세션 및 권한 조회 실패");
                Status::internal("수정 권한을 확인할 수 없습니다.")
            })?
            .ok_or_else(|| Status::unauthenticated("유효한 세션이 아닙니다."))?;

        if !can_write {
            return Err(Status::permission_denied("이 서비스의 설정을 수정할 권한이 없습니다."));
        }

        // 헤더를 다 사용했으므로 이제 본문을 소유한 값으로 꺼낸다.
        let input = request.into_inner();

        // 실제 원본의 지역 변수 철자는 excepted_revision이다.
        // 실제 proto 필드 excepted_revision은 호출자가 읽은 버전을 뜻한다.
        let excepted_revision = input
            .expected_revision
            .ok_or_else(|| Status::invalid_argument("excepted_revision이 필요힙니다."))?;

        if excepted_revision < 0 {
            return Err(Status::invalid_argument("except_revision은 0 이상이어야 합니다."));
        }

        // proto의 oneof가 Rust에서는 Option<Change>로 생성된다.
        let configuration: Option<serde_json::Value> = match input.change {
            Some(Change::ConfigurationJson(json)) => {
                let value = serde_json::from_str(&json)
                    .map_err(|_| Status::invalid_argument("configuration_json의 JSON 형식이 잘못되었습니다."))?;
                Some(value) // JSON 문자열 "null"도 Some(Value::Null)이 된다.
            }

            Some(Change::ClearConfiguration(())) => None, // SQL NULL로 저장한다.
            None => {
                return Err(Status::invalid_argument(
                    "configuration_json 또는 clear_configuration을 지정하세요."
                ));
            }
        };

        let new_revision = self
            .db
            .update_service_configuration(
                service_id,
                excepted_revision,
                configuration.as_ref(), // Option<Value>를 Option<&Value>로 빌려온다.
                user_id,                // 검증된 세션에서 얻은 변경자 ID
            )
            .await
            .map_err(|e| match e {
                ConfigUpdateError::NotFound(_) => Status::not_found("서비스를 찾을 수 없습니다."),
                ConfigUpdateError::RevisionConflict { expected, current } => {
                    Status::aborted(format!(
                        "설정이 이미 변경되었습니다. 요청 리비전 {expected}, 현재 리비전 {current}"
                    ))
                }
                ConfigUpdateError::Database(e) => {
                    tracing::error!(service_id, error = %e, "서비스 설정 저장 실패");
                    Status::internal("서비스 설정을 저장할 수 없습니다.")
                }
            })?;
        Ok(Response::new(UpdateServiceConfigurationResponse {
            configuration_revision: new_revision,
        }))
    }
}
