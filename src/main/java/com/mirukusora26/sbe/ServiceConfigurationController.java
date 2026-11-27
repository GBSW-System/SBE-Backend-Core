package com.mirukusora26.sbe;

import io.grpc.StatusRuntimeException;
import org.springframework.http.HttpHeaders;
import org.springframework.http.HttpStatus;
import org.springframework.http.ResponseEntity;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.PathVariable;
import org.springframework.web.bind.annotation.RequestHeader;
import org.springframework.web.bind.annotation.RestController;
import tools.jackson.core.JacksonException;
import tools.jackson.databind.json.JsonMapper;
import tools.jackson.databind.node.ObjectNode;

@RestController
public class ServiceConfigurationController {

    private final ServiceConfigurationClient client;
    private final JsonMapper json;

    // Spring이 기존 gRPC 클라이언트와 JSON 변환기를 주입한다.
    public ServiceConfigurationController(ServiceConfigurationClient client, JsonMapper json) {
        this.client = client;
        this.json = json;
    }

    @GetMapping("/services/{id}/configuration")
    public ResponseEntity<ObjectNode> getConfiguration(
            @PathVariable("id") long serviceId,
            @RequestHeader(value = HttpHeaders.AUTHORIZATION, required = false)
            String authorization) {

        try {
            // ex. GET /services/7/configuration -> serviceId는 7
            // Authorization 헤더 전체를 전달한다. "Bearer "를 다시 붙이지 않는다.
            // required = false 이므로 헤더가 없어도 아래에서 일관되게 401로 처리할 수 있다.
            var result = client.getConfiguration(serviceId, authorization);

            var body = json.createObjectNode();
            body.put("configurationRevision", result.getConfigurationRevision());

            if (result.hasConfigurationJson()) {
                // Rust는 JSON을 문자열로 보낸다. -> readTree로 파싱해야
                // HTTP 응답에서 {"enabled": true}가 되어 실제 JSON 객체로 전달된다.
                body.set("configuration", json.readTree(result.getConfigurationJson()));
            }
            
            return ResponseEntity.ok(body);
        } catch (StatusRuntimeException e) {
            // Rust 내부의 gRPC 상태를 외부 HTTP 상태로 변환한다.
            var status = switch (e.getStatus().getCode()) {
                case UNAUTHENTICATED -> HttpStatus.UNAUTHORIZED; // 401
                case PERMISSION_DENIED -> HttpStatus.FORBIDDEN; // 403
                case INVALID_ARGUMENT -> HttpStatus.BAD_REQUEST; // 400
                case NOT_FOUND -> HttpStatus.NOT_FOUND; // 404
                case UNAVAILABLE -> HttpStatus.SERVICE_UNAVAILABLE; // 503
                case DEADLINE_EXCEEDED -> HttpStatus.GATEWAY_TIMEOUT; // 504
                default -> HttpStatus.BAD_GATEWAY; // 502
            };

            var response = ResponseEntity.status(status);
            if (status == HttpStatus.UNAUTHORIZED) {
                response.header(HttpHeaders.WWW_AUTHENTICATE, "Bearer");
            }
            return response.build();
        } catch (JacksonException e) {
            // Rust에서 받은 설정 문자열을 JSON으로 해석할 수 없는 경우
            return ResponseEntity.status(HttpStatus.BAD_GATEWAY).build();
        }
    }
}
