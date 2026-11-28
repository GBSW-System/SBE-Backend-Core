package com.mirukusora26.sbe;

import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.webmvc.test.autoconfigure.WebMvcTest;
import org.springframework.context.annotation.Import;
import org.springframework.http.HttpHeaders;
import org.springframework.http.MediaType;
import org.springframework.test.context.bean.override.mockito.MockitoBean;
import org.springframework.test.web.servlet.MockMvc;

import static org.mockito.Mockito.verifyNoInteractions;
import static org.mockito.Mockito.when;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.put;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.jsonPath;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.status;

import io.grpc.Status;

@WebMvcTest(ServiceConfigurationController.class)
@Import(WebSecurityConfig.class)
public class ServiceConfigurationControllerTest {

    private static final String URL = "/services/7/configuration";
    private static final String AUTH = "Bearer test-session";

    // MockMvc로 HTTP 요청을 보내면 실제 보안 필터와 컨트롤러를 거친다.
    @Autowired
    private MockMvc mvc;

    // Rust로 나가는 호출은 테스트가 제어한다. 실제 토큰이나 DB는 필요없다.
    @MockitoBean
    private ServiceConfigurationClient client;

    @Test
    void missingExpectedRevisionReturns400() throws Exception {
        // configuration은 유효하고 expectedRevision만 빠진 요청이다.
        mvc.perform(put(URL)
                .header(HttpHeaders.AUTHORIZATION, AUTH)
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
                        {"configuration": {"enabled": true}}
                        """))
                .andExpect(status().isBadRequest());

        // 입력 검증에서 끝나야하므로 저장 요청도 한 번도 발생하지 않아야 한다.
        verifyNoInteractions(client);
    }

    @Test
    void successfulUpdateReturnsConfigurationRevision() throws Exception {
        // 서비스 7의 리비전 0을 수정하면 Rust가 새 리비전 1을 반환한다고 가정한다.
        when(client.updateConfiguration(7L, 0L, "{\"enabled\":true}", AUTH))
                .thenReturn(1L);

        mvc.perform(put(URL)
                        .header(HttpHeaders.AUTHORIZATION, AUTH)
                        .contentType(MediaType.APPLICATION_JSON)
                        .content("""
                                {"expectedRevision":0,"configuration":{"enabled":true}}
                                """))
                .andExpect(status().isOk())
                .andExpect(jsonPath("$.configurationRevision").value(1))
                .andExpect(jsonPath("$.configuration").doesNotExist());
    }

    @Test
    void staleRevisionReturns409() throws Exception {
        // 다른 관리자가 먼저 저장해서 Rust가 리비전 3의 수정을 거절한 상황이다.
        when(client.updateConfiguration(7L, 3L, "{\"enabled\":true}", AUTH))
                .thenThrow(Status.ABORTED.asRuntimeException());

        // 사용자가 마지막으로 읽었던 리비전 3을 그대로 보낸다.
        mvc.perform(put(URL)
                        .header(HttpHeaders.AUTHORIZATION, AUTH)
                        .contentType(MediaType.APPLICATION_JSON)
                        .content("""
                            {"expectedRevision":3,"configuration":{"enabled":true}}
                            """))
                .andExpect(status().isConflict());
    }
}
