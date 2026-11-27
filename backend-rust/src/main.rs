mod database;
mod gateway_auth;
mod registry_handler;

mod registry {
    // build.rs가 생성한 요청/응답 타입과 ServiceRegistry 트레이트를 가져온다.
    tonic::include_proto!("sbe.registry.v1");
}

use registry::service_registry_server::ServiceRegistryServer;
use registry_handler::RegistryHandler;
use anyhow::{Context, Result};
use database::Database;
use gateway_auth::GatewayAuth;
use std::{env, time::Duration};
use tonic::transport::Server;
use tonic_health::{server::health_reporter, ServingStatus};

// Tokio 런타임에서 실행한다.
#[tokio::main]
async fn main() -> Result<()> {
    // 로컬 .env를 읽는다.
    dotenvy::dotenv().ok();

    let gateway_auth = GatewayAuth::new(
        env::var("SBE_GATEWAY_TOKEN")
            .ok()      // 값을 읽을 수 없으면 None
            .as_deref() // Option<String>을 Option<&str>로 빌려 쓴다.
            .context("SBE_GATEWAY_TOKEN에 ASCII 토큰을 설정하세요.")?,
    )?;

    // DB 풀을 생성하고 적용 대기 중인 마이그레이션을 실행한다.
    let db = Database::connect(&env::var("DATABASE_URL")?).await?;
    let addr = env::var("GPRC_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:50051".into())
        .parse()?;

    // reporter는 상태를 기록하고, health_service는 외부의 상태 조회에 응답한다.
    let (reporter, health_service) = health_reporter();

    reporter.set_service_status("", ServingStatus::NotServing).await;
    
    // 빈 서비스의 이름 ""은 서버 전체 상태를 나타낸다.
    // 첫 DB 점검이 끝나기 전까지는 준비되지 않았다고 보고한다.
    let registry_service =
        ServiceRegistryServer::with_interceptor(RegistryHandler::new(db.clone()), gateway_auth);
    
    // DB 점검을 별도 비동기 테스크로 실행한다.
    // async move는 db와 reporter의 소유권을 이 태스크로 옮긴다.
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
    
    // 같은 포트에 두 서비스를 등록한다.
    // health는 토큰 없이 호출 가능하고 registry에는 위 인터셉터가 적용되어 있다.
    let result = Server::builder()
        .add_service(health_service)
        .add_service(registry_service)
        .serve_with_shutdown(addr, async {
            // Ctrl + C 신호가 들어올 시 서버를 종료한다.
            let _ = tokio::signal::ctrl_c().await;
        })
        .await;

    // 서버 실행이 끝나면 계속 돌던 DB 점검 테스크도 중단한다.
    monitor.abort();
    result?;
    Ok(())

}
