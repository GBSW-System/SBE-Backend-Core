package com.mirukusora26.sbe;


import com.mirukusora26.sbe.grpc.registry.v1.GetServiceConfigurationRequest;
import com.mirukusora26.sbe.grpc.registry.v1.GetServiceConfigurationResponse;
import com.mirukusora26.sbe.grpc.registry.v1.UpdateServiceConfigurationRequest;
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

    // 공용 스텁에는 게이트웨이 토큰이 이미 있다.
    private ServiceRegistryGrpc.ServiceRegistryBlockingStub userStub(String authorization) {
        if (authorization == null || !authorization.startsWith("Bearer ")
                || authorization.length() == 7
                || !authorization.substring(7).chars().allMatch(c -> c >= '!' && c <= '~')) {
            throw Status.UNAUTHENTICATED
                    .withDescription("세션 토큰이 필요합니다.")
                    .asRuntimeException();
        }

        var headers = new Metadata();
        headers.put(AUTHORIZATION, authorization);

        return registry
                .withInterceptors(MetadataUtils.newAttachHeadersInterceptor(headers))
                .withDeadlineAfter(3, TimeUnit.SECONDS);
    }

    // 기존 GET 컨트롤러가 호출하는 메서드. 호출 방식은 그대로다.
    public GetServiceConfigurationResponse getConfiguration(long serviceId, String authorization) {
        var request = GetServiceConfigurationRequest.newBuilder()
                .setServiceId(serviceId)
                .build();

        return userStub(authorization).getServiceConfiguration(request);
    }

    // PUT 컨트롤러가 호출할 메서드. 저장에 성공하면 새 리비전을 반환한다.
    public long updateConfiguration(long serviceId, long expectedRevision,
                                    String configurationJson, String authorization) {
        var request = UpdateServiceConfigurationRequest.newBuilder()
                .setServiceId(serviceId)
                .setExpectedRevision(expectedRevision)
                .setConfigurationJson(configurationJson)
                .build();

        return userStub(authorization)
                .updateServiceConfiguration(request)
                .getConfigurationRevision();
    }
}
