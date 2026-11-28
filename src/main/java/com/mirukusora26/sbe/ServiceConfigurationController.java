package com.mirukusora26.sbe;

import io.grpc.StatusRuntimeException;
import org.springframework.http.HttpHeaders;
import org.springframework.http.HttpStatus;
import org.springframework.http.ResponseEntity;
import org.springframework.http.converter.HttpMessageNotReadableException;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.PathVariable;
import org.springframework.web.bind.annotation.RequestHeader;
import org.springframework.web.bind.annotation.RestController;
import org.springframework.web.bind.annotation.ExceptionHandler;
import org.springframework.web.bind.annotation.PutMapping;
import org.springframework.web.bind.annotation.RequestBody;
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
        } catch (JacksonException e) {
            // Rust에서 받은 설정 문자열을 JSON으로 해석할 수 없는 경우
            return ResponseEntity.status(HttpStatus.BAD_GATEWAY).build();
        }
    }

    @ExceptionHandler(StatusRuntimeException.class)
    public ResponseEntity<Void> handleGrpcError(StatusRuntimeException e) {
        var status = switch (e.getStatus().getCode()) {
            case UNAUTHENTICATED -> HttpStatus.UNAUTHORIZED;
            case PERMISSION_DENIED -> HttpStatus.FORBIDDEN;
            case INVALID_ARGUMENT -> HttpStatus.BAD_REQUEST;
            case NOT_FOUND -> HttpStatus.NOT_FOUND;
            case ABORTED -> HttpStatus.CONFLICT;  // 이미 다른 요청이 리비전을 변경했다.
            case UNAVAILABLE -> HttpStatus.SERVICE_UNAVAILABLE;
            case DEADLINE_EXCEEDED -> HttpStatus.GATEWAY_TIMEOUT;
            default -> HttpStatus.BAD_GATEWAY;
        };

        var res = ResponseEntity.status(status);
        if (status == HttpStatus.UNAUTHORIZED) {
            res.header(HttpHeaders.WWW_AUTHENTICATE, "Bearer");
        }

        return res.build();
    }

    @ExceptionHandler(HttpMessageNotReadableException.class)
    public ResponseEntity<Void> handleUnreadableBody() {
        return ResponseEntity.badRequest().build();
    }

    @PutMapping(value = "/services/{id}/configuration", consumes = "application/json")
    public ResponseEntity<ObjectNode> updateConfiguration(
            @PathVariable("id") long serviceId,
            @RequestHeader(value = HttpHeaders.AUTHORIZATION, required = false)
            String authorization,
            @RequestBody ObjectNode body) {

        // 클라이언트가 마지막으로 읽은 리비전으로, 최조 리비전인 0도 유효하다.
        var revision = body.get("expectedRevision");
        if (revision == null || !revision.isIntegralNumber() ||
                !revision.canConvertToLong() || revision.longValue() < 0) {
            return ResponseEntity.badRequest().build();
        }

        // 빌드가 빠지만 Java null이다.
        // 명시적으로 보낸 {"configuration": null}은 JSON 값이므로 저장할 수 있다.
        var configuration = body.get("configuration");
        if (configuration == null) {
            return ResponseEntity.badRequest().build();
        }

        // gRPC는 설정을 JSON 문자열로 받는다.
        // toString()은 객체, 배열, 문자열, null을 각각 올바른 JSON으로 직렬화한다.
        long savedRevision = client.updateConfiguration(
                serviceId,
                revision.longValue(),
                configuration.toString(),
                authorization
        );

        // PostgreSQL에 실제로 저장된 새 리비전을 돌려준다.
        var response = json.createObjectNode();
        response.put("configurationRevision", savedRevision);
        return ResponseEntity.ok(response);
    }
}
