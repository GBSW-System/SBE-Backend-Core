package com.mirukusora26.sbe;

import io.grpc.ManagedChannel;
import io.grpc.ManagedChannelBuilder;
import org.springframework.beans.factory.annotation.Value;
import org.springframework.context.annotation.Bean;
import org.springframework.context.annotation.Configuration;

// Rust 서버에 연결할 gPRC 채널을 Spring이 관리한다.
@Configuration
public class RegistryGrpcConfig {

    // 기본적으로 Spring 컨텍스트당 하나만 생성한다.
    // Spring이 종료될 때 채넣의 shutdown()을 호출한다.
    @Bean(destroyMethod = "shutdown")
    public ManagedChannel registryChannel(
            @Value("${sbe.grpc.host:127.0.0.1}") String host,
            @Value("${sbe.grpc.port:50051}") int port
    ) {
        return ManagedChannelBuilder.forAddress(host, port)
                .usePlaintext()
                .build();
    }
}
