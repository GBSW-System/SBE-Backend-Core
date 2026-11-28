mod database;
mod gateway_auth;
mod registry_handler;

mod registry {
    // build.rs가 생성한 요청/응답 타입과 ServiceRegistry 트레이트를 가져온다.
    tonic::include_proto!("sbe.registry.v1");
}

use anyhow::{bail, Context, Result, anyhow};
use database::Database;
use gateway_auth::GatewayAuth;
use registry::service_registry_server::ServiceRegistryServer;
use registry_handler::RegistryHandler;
use std::{env, time::Duration};
use tonic::transport::Server;
use tonic_health::{ServingStatus, server::health_reporter};
use tokio_util::sync::CancellationToken;

// 종료 신호가 들어온 뒤, 진행 중인 RPC가 끝나기를 기다릴 최대 시간
const SHUTDOWN_GRACE: Duration = Duration::from_secs(5);

// Tokio 런타임에서 실행한다.
#[tokio::main]
async fn main() -> Result<()> {
    // 로컬 .env를 읽는다.
    dotenvy::dotenv().ok();

    let gateway_auth = GatewayAuth::new(
        env::var("SBE_GATEWAY_TOKEN")
            .ok() // 값을 읽을 수 없으면 None
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

    reporter
        .set_service_status("", ServingStatus::NotServing)
        .await;

    // 빈 서비스의 이름 ""은 서버 전체 상태를 나타낸다.
    // 첫 DB 점검이 끝나기 전까지는 준비되지 않았다고 보고한다.
    let registry_service =
        ServiceRegistryServer::with_interceptor(RegistryHandler::new(db.clone()), gateway_auth);

    // 서버가 요청을 받기 전에 운영체제의 종료 신호를 등록한다.
    #[cfg(unix)]
    let (mut sigint, mut sigterm) = {
        use tokio::signal::unix::{SignalKind, signal};

        (
            signal(SignalKind::interrupt()).context("SIGINT 등록 실패")?, // Ctrl + C
            signal(SignalKind::terminate()).context("SIGTERM 등록 실패")?,
        )
    };

    // 이 작업이 끝나면 Tonic이 새 연결을 닫고 진행 중인 RPC를 기다린다.
    let shutdown = async move {
        // macOS와 Linux에서는 두 신호 중 먼저 들어온 것에 반응한다.
        #[cfg(unix)]
        tokio::select! {
            _ = sigint.recv() => {},
            _ = sigterm.recv() => {},
        }

        #[cfg(not(unix))]
        if let Err(error) = tokio::signal::ctrl_c().await {
            eprintln!("Ctrl+C 수신 실패: {error}");
        }
    };

    // 두 핸들은 같은 상태를 공유한다. 한쪽에서 알리면 다른 쪽의 대기가 풀린다.
    let shutdown_started = CancellationToken::new();
    let notify_shutdown = shutdown_started.clone();

    let shutdown_with_health = async move {
        // 시작할 때 바로 DB를 확인하고, 이후 5초마다 상태를 갱신한다.
        let monitor = async {
            loop {
                let status = if db.is_ready().await {
                    ServingStatus::Serving
                } else {
                    ServingStatus::NotServing
                };

                reporter.set_service_status("", status).await;
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        };

        // DB가 점검 중이거나 5초를 기다리는 중에도 종료 신호를 받을 수 있다.
        // 둘 다 준비됐으면 shutdown을 먼저 처리한다.
        tokio::select! {
            biased;
            _ = shutdown => {}
            _ = monitor => {}
        }

        // 종료 신호를 받았으니, 이 시점부터 대기 시간을 샌다.
        notify_shutdown.cancel();

        // 이 시점에는 monitor가 중단돼서 마지막 상태를 다시 덮어쓸 수 없다.
        reporter.set_service_status("", ServingStatus::NotServing).await;
    };

    println!("Rust gPRC: {addr}");

    let server = Server::builder()
        .add_service(health_service)
        .add_service(registry_service)
        .serve_with_shutdown(addr, shutdown_with_health);

    let grace_expired = async {
        shutdown_started.cancelled().await;
        tokio::time::sleep(SHUTDOWN_GRACE).await;
    };

    tokio::select! {
        biased;
        // 서버 종료와 시간 만료가 동시에 준비되면 서버의 실제 결과를 우선한다.
        result = server => result?,
        _ = grace_expired => anyhow::bail!(
            "gRPC 종료 대기 시간 {}초 경과", SHUTDOWN_GRACE.as_secs()
        ),
    }

    Ok(())
}
