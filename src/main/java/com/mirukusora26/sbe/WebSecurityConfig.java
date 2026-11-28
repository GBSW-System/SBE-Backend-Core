package com.mirukusora26.sbe;

import org.springframework.context.annotation.Bean;
import org.springframework.context.annotation.Configuration;
import org.springframework.http.HttpMethod;
import org.springframework.security.config.annotation.web.builders.HttpSecurity;
import org.springframework.security.web.SecurityFilterChain;
import org.springframework.security.web.servlet.util.matcher.PathPatternRequestMatcher;

import static org.springframework.security.config.Customizer.withDefaults;

@Configuration
public class WebSecurityConfig {
    @Bean
    SecurityFilterChain webSecurity(HttpSecurity http) throws Exception {
        var putConfiguration = PathPatternRequestMatcher.withDefaults()
                .matcher(HttpMethod.PUT, "/services/{id}/configuration");

        return http
                // 이 PUT은 Authorization 헤더의 Bearer 토큰으로만 사용자를 인증한다.
                // CSRF 예외를 해당 메서드와 경로에만 적용한다.
                .csrf(csrf -> csrf.ignoringRequestMatchers(putConfiguration))
                .authorizeHttpRequests(auth -> auth
                        .requestMatchers(HttpMethod.GET, "/services/{id}/configuration")
                        .permitAll()
                        .requestMatchers(putConfiguration)
                        .permitAll()
                        .anyRequest().authenticated())
                .formLogin(withDefaults())
                .httpBasic(withDefaults())
                .build();
    }
}
