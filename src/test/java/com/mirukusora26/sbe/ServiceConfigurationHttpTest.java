package com.mirukusora26.sbe;

import org.junit.jupiter.api.Test;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.boot.test.web.server.LocalServerPort;

import java.net.URI;
import java.net.http.HttpClient;
import java.net.http.HttpRequest;
import java.net.http.HttpResponse;
import java.time.Duration;

import static org.junit.jupiter.api.Assertions.assertEquals;

// 실제 서버를 빈 포트에 띄워 서블릿의 오류 처리와 보안 필터까지 거친다.
@SpringBootTest(webEnvironment = SpringBootTest.WebEnvironment.RANDOM_PORT)
class ServiceConfigurationHttpTest {

    @LocalServerPort
    private int port;

    @Test
    void malformedJsonReturns400OverHttp() throws Exception {
        // configuration의 값이 오기 전에 본문이 끊긴 상황을 재현한다.
        String brokenJson = "{\"expectedRevision\":3,\"configuration\":";

        var request = HttpRequest.newBuilder(
                        URI.create("http://127.0.0.1:" + port + "/services/7/configuration"))
                .timeout(Duration.ofSeconds(5))
                .header("Authorization", "Bearer test-session")
                .header("Content-Type", "application/json")
                .header("Accept", "application/json")
                .PUT(HttpRequest.BodyPublishers.ofString(brokenJson))
                .build();

        try (var http = HttpClient.newHttpClient()) {
            var response = http.send(request, HttpResponse.BodyHandlers.discarding());

            // 클라이언트가 고쳐야 할 것은 JSON 본문이다. 이를 400으로 알려야 한다.
            assertEquals(400, response.statusCode());
        }
    }
}