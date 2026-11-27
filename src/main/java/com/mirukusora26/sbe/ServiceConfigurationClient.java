package com.mirukusora26.sbe;


import com.mirukusora26.sbe.grpc.registry.v1.GetServiceConfigurationRequest;
import com.mirukusora26.sbe.grpc.registry.v1.GetServiceConfigurationResponse;
import com.mirukusora26.sbe.grpc.registry.v1.ServiceRegistryGrpc;
import io.grpc.ManagedChannel;
import io.grpc.Metadata;
import io.grpc.Status;
import io.grpc.stub.MetadataUtils;
import org.springframework.beans.factory.annotation.Value;
import org.springframework.stereotype.Service;

import java.util.concurrent.TimeUnit;

// Spring이 이 클래스의 인스턴스를 만들고, 이후 컨트롤러를 주입한다.
@Service
public class ServiceConfigurationClient {

    // Java가 Rust로 보낼 두 gPRC 헤더의 이름
    private static final Metadata.Key<String> GATEWAY_TOKEN =
            Metadata.Key.of("x-sbe-gateway-token", Metadata.ASCII_STRING_MARSHALLER);
    private static final Metadata.Key<String> AUTHORIZATION =
            Metadata.Key.of("authorization", Metadata.ASCII_STRING_MARSHALLER);

    // 모든 호출이 공유하는 스텁, 여기에는 게이트웨이 토큰만 붙인다.
    private final ServiceRegistryGrpc.ServiceRegistryBlockingStub registry;

    public ServiceConfigurationClient(
            ManagedChannel registryChannel,
            @Value("${SBE_GATEWAY_TOKEN}") String gatewayToken
    ) {
        // registryChannel은 앞에서 만든 RegistryGrpcConfig의 @Bean이다.
        // 게이트웨이 토큰은 시작 시 한 번 검증한다.
        if (gatewayToken.isEmpty() ||
                !gatewayToken.chars().allMatch(c -> c >= '!' && c <= '~')) {
            throw new IllegalStateException(
                    "SBE_GATEWAY_TOKEN에 공백 없는 ASCII 토큰을 설정하세요."
            );
        }

        var headers = new Metadata();
        headers.put(GATEWAY_TOKEN, gatewayToken);

        this.registry = ServiceRegistryGrpc.newBlockingStub(registryChannel)
                .withInterceptors(MetadataUtils.newAttachHeadersInterceptor(headers));
    }

    public GetServiceConfigurationResponse getConfiguration(
            long serviceId, String authorization
    ) {
        // 컴트롤러에서 HTTP Authorization 헤더 전체를 전달 받는다.
        // ex. "Bearer abc123". "Bearer " 다음에는 실제 토큰이 있어야한다.
        if (authorization == null || !authorization.startsWith("Bearer ") ||
                authorization.length() == 7 ||
                !authorization.substring(7).chars().allMatch(c -> c >= '!' && c <= '~')) {
            throw Status.UNAUTHENTICATED
                    .withDescription("세션 토큰이 필요합니다.")
                    .asRuntimeException();
        }

        // 사용자 세션은 호출마다 다르므로 메서드 안에서 새로 만든다.
        // authorization에 이미 "Bearer "가 있으므로 그대로 넣는다.
        var userHeaders = new Metadata();
        userHeaders.put(AUTHORIZATION, authorization);

        var req = GetServiceConfigurationRequest.newBuilder()
                .setServiceId(serviceId)
                .build();

        // withInterceptors()는 이 호출에 쓸 새 스텁을 반환한다.
        // 사용자 헤더와 3초 제한은 이번 조회에만 적용된다.
        return registry
                .withInterceptors(MetadataUtils.newAttachHeadersInterceptor(userHeaders))
                .withDeadlineAfter(3, TimeUnit.SECONDS)
                .getServiceConfiguration(req);
    }
}
