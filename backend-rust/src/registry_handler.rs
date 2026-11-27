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

#[derive(Clone)]
pub(crate) struct AuthenticatedUser {
    pub(crate) user_id: i64,
    pub(crate) editable_service_ids: HashSet<i64>,
}

pub struct RegistryHandler {
    db: Database,
}

impl RegistryHandler {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

#[tonic::async_trait]
impl ServiceRegistry for RegistryHandler {
    async fn get_service_configuration(
        &self,
        request: Request<GetServiceConfigurationRequest>,
    ) -> Result<Response<GetServiceConfigurationResponse>, Status> {
        let service_id = request.into_inner().service_id;
        if service_id <= 0 {
            return Err(Status::invalid_argument("service_id는 양수여야 합니다."));
        }

        let result = self.db
            .read(ReadQuery::ServiceConfiguration(service_id))
            .await
            .map_err(|err| {
                eprintln!("서비스 설정 조회 실패 (service_id={service_id}): {err}");
                Status::internal("서비스 설정을 조회할 수 없습니다.")
            })?;

        let row = match result {
            ReadResult::ServiceConfiguration(Some(row)) => row,
            ReadResult::ServiceConfiguration(None) => {
                return Err(Status::not_found("서비스를 찾을 수 없습니다."));
            }
            _ => return Err(Status::internal("조회 결과 타입이 일치하지 않습니다.")),
        };

        Ok(Response::new(GetServiceConfigurationResponse {
            configuration_json: row.configuration.map(|value| value.to_string()),
            configuration_revision: row.configuration_revision,
        }))
    }

    async fn update_service_configuration(
        &self,
        request: Request<UpdateServiceConfigurationRequest>,
    ) -> Result<Response<UpdateServiceConfigurationResponse>, Status> {
        let token = request.metadata()
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
            .filter(|value| !value.is_empty())
            .ok_or_else(|| Status::unauthenticated("세션 토큰이 필요합니다."))?;

        let service_id = request.get_ref().service_id;
        if service_id <= 0 {
            return Err(Status::invalid_argument("service_id는 양수여야 합니다."));
        }

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

        let input = request.into_inner();

        let excepted_revision = input
            .expected_revision
            .ok_or_else(|| Status::invalid_argument("excepted_revision이 필요힙니다."))?;

        if excepted_revision < 0 {
            return Err(Status::invalid_argument("except_revision은 0 이상이어야 합니다."));
        }

        let configuration: Option<serde_json::Value> = match input.change {
            Some(Change::ConfigurationJson(json)) => {
                let value = serde_json::from_str(&json)
                    .map_err(|_| Status::invalid_argument("configuration_json의 JSON 형식이 잘못되었습니다."))?;
                Some(value)
            }

            Some(Change::ClearConfiguration(())) => None,
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
                configuration.as_ref(),
                user_id,
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
