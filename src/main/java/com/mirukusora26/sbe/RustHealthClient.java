package com.mirukusora26.sbe;

import io.grpc.ManagedChannelBuilder;
import io.grpc.health.v1.HealthCheckRequest;
import io.grpc.health.v1.HealthGrpc;

import java.util.concurrent.TimeUnit;

public class RustHealthClient {
    static void main() throws InterruptedException {
        checkHealth();
    }
    
    public static void checkHealth() throws InterruptedException {
        var channel = ManagedChannelBuilder.forAddress("127.0.0.1", 50051)
                .usePlaintext()
                .build();

        try {
            var response = HealthGrpc.newBlockingStub(channel)
                    .withDeadlineAfter(3, TimeUnit.SECONDS)
                    .check(HealthCheckRequest.getDefaultInstance());

            System.out.println(response.getStatus());
        } finally {
            channel.shutdownNow().awaitTermination(3, TimeUnit.SECONDS);
        }
    }
}
