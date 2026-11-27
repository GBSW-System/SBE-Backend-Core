package com.mirukusora26.sbe;

import com.mirukusora26.sbe.grpc.registry.v1.GetServiceConfigurationRequest;
import com.mirukusora26.sbe.grpc.registry.v1.ServiceRegistryGrpc;
import com.mirukusora26.sbe.grpc.registry.v1.UpdateServiceConfigurationRequest;
import io.grpc.ManagedChannelBuilder;
import io.grpc.Metadata;
import io.grpc.StatusRuntimeException;
import io.grpc.stub.MetadataUtils;

import java.util.concurrent.TimeUnit;

public class RustRegistryClient {
    public static void main(String[] args) throws InterruptedException {
        // 인자 없이 실행하면 0을 보내므로, Rust의 ID 검증 경로를 확인할 수 있다.
        long serviceId = args.length == 0 ? 0L : Long.parseLong(args[0]);

        // 이 Java 프로그램이 게이트웨이임을 증명하는 공유 토큰이다.
        String gatewayToken = System.getenv("SBE_GATEWAY_TOKEN");
        if (gatewayToken == null || gatewayToken.isEmpty()
                || !gatewayToken.chars().allMatch(c -> c >= '!' && c <= '~')) {
            throw new IllegalStateException("SBE_GATEWAY_TOKEN에 공백 없는 ASCII 토큰을 설정하세요.");
        }

        // 조회와 수정 모두 같은 채널을 사용한다.
        var channel = ManagedChannelBuilder.forAddress("127.0.0.1", 50051)
                .usePlaintext()
                .build();

        try {
            // Metadata는 요청 본문과 별도로 전달하는 gRPC 헤더이다.
            var gatewayHeaders = new Metadata();
            gatewayHeaders.put(
                    Metadata.Key.of("x-sbe-gateway-token", Metadata.ASCII_STRING_MARSHALLER),
                    gatewayToken
            );

            // 이 스텁으로 보내는 모든 호출에 게이트웨이 토큰을 붙인다.
            var registry = ServiceRegistryGrpc.newBlockingStub(channel)
                    .withInterceptors(MetadataUtils.newAttachHeadersInterceptor(gatewayHeaders));

            // 현재 설정을 조회한다.
            var req = GetServiceConfigurationRequest.newBuilder()
                    .setServiceId(serviceId)
                    .build();

            // BlockingStub이므로, 이 Java 스레드는 응답이나 오류를 기다린다. (제한 시간은 3초이다.)
            var res = registry
                    .withDeadlineAfter(3, TimeUnit.SECONDS)
                    .getServiceConfiguration(req);

            System.out.println("Setting revision = " + res.getConfigurationRevision());
            System.out.println("Setting value exists = " + res.hasConfigurationJson());

            // 사용자가 --update 를 붙였을 때만 변경 요청도 보낸다.
            if (args.length >= 2 && "--update".equals(args[1])) {
                // 이 토큰은 변경 이력에 기록할 최종 사용자를 식별한다.
                String token = System.getenv("SBE_SESSION_TOKEN");
                if (token == null || token.isBlank()) {
                    throw new IllegalStateException("SBE_SESSION_TOKEN을 설정하세요.");
                }

                var headers = new Metadata();
                headers.put(
                        Metadata.Key.of("authorization", Metadata.ASCII_STRING_MARSHALLER),
                        "Bearer " + token
                );

                var update = UpdateServiceConfigurationRequest.newBuilder()
                        .setServiceId(serviceId)
                        // 방금 읽은 값이 4라면 "아직 4일 때만 저장"을 의미한다.
                        .setExpectedRevision(res.getConfigurationRevision())
                        .setConfigurationJson("{\"enabled\":true}")
                        .build();

                // 기존 게이트웨이 인증이 붙은 스텁에 사용자 헤더를 추가한다.
                // 수정 요청에는 조회와 별도로 새 3초 제한을 부여한다.
                var saved = registry
                        .withInterceptors(MetadataUtils.newAttachHeadersInterceptor(headers))
                        .withDeadlineAfter(3, TimeUnit.SECONDS)
                        .updateServiceConfiguration(update);

                System.out.println("저장된 리비전 = " + saved.getConfigurationRevision());
            }
        } catch (StatusRuntimeException e) {
            // 예: UNAUTHENTICATED, PERMISSION_DENIED, ABORTED.
            var status = e.getStatus();
            System.err.println(status.getCode() + " / " + status.getDescription());
        } finally {
            // 성공과 실패 어느 쪽이든 채널을 닫고 종료를 최대 3초 기다린다.
            channel.shutdownNow().awaitTermination(3, TimeUnit.SECONDS);
        }
    }
}
