package com.mirukusora26.sbe;

import com.mirukusora26.sbe.grpc.registry.v1.GetServiceConfigurationRequest;
import com.mirukusora26.sbe.grpc.registry.v1.ServiceRegistryGrpc;
import io.grpc.ManagedChannelBuilder;
import io.grpc.StatusRuntimeException;
import com.mirukusora26.sbe.grpc.registry.v1.UpdateServiceConfigurationRequest;
import io.grpc.Metadata;
import io.grpc.stub.MetadataUtils;

import java.util.concurrent.TimeUnit;
public class RustRegistryClient {
    public static void main(String[] args) throws InterruptedException {
        long serviceId = args.length == 0 ? 0L : Long.parseLong(args[0]);

        var channel = ManagedChannelBuilder.forAddress("127.0.0.1", 50051)
                .usePlaintext()
                .build();

        try {
            var req = GetServiceConfigurationRequest.newBuilder()
                    .setServiceId(serviceId)
                    .build();

            var res = ServiceRegistryGrpc.newBlockingStub(channel)
                    .withDeadlineAfter(3, TimeUnit.SECONDS)
                    .getServiceConfiguration(req);

            System.out.println("Setting revision = " + res.getConfigurationRevision());
            System.out.println("Setting value exists = " + res.hasConfigurationJson());

            if (args.length >= 2 && "--update".equals(args[1])) {
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
                        .setExpectedRevision(res.getConfigurationRevision())
                        .setConfigurationJson("{\"enabled\":true}")
                        .build();

                var saved = ServiceRegistryGrpc.newBlockingStub(channel)
                        .withInterceptors(MetadataUtils.newAttachHeadersInterceptor(headers))
                        .withDeadlineAfter(3, TimeUnit.SECONDS)
                        .updateServiceConfiguration(update);

                System.out.println("저장된 리비전 = " + saved.getConfigurationRevision());
            }
        } catch (StatusRuntimeException e) {
            var status = e.getStatus();
            System.err.println(status.getCode() + " / " + status.getDescription());
        } finally {
            channel.shutdownNow().awaitTermination(3, TimeUnit.SECONDS);
        }
    }
}
