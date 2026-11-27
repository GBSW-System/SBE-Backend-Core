package com.mirukusora26.sbe;

import org.springframework.context.annotation.Bean;
import org.springframework.context.annotation.Configuration;
import org.springframework.http.HttpMethod;
import org.springframework.security.config.annotation.web.builders.HttpSecurity;
import org.springframework.security.web.SecurityFilterChain;

import static org.springframework.security.config.Customizer.withDefaults;

@Configuration
public class WebSecurityConfig {

    @Bean
    SecurityFilterChain webSecurity(HttpSecurity http) throws Exception {
        return http
                .authorizeHttpRequests(auth ->  auth
                        // 이 GET 요청은 컨트롤러가 Bearer 세션을 받아 Rust로 전달한다.
                        .requestMatchers(HttpMethod.GET, "/services/{id}/configuration")
                        .permitAll()

                        // 그 밖의 경로는 Spring Security 인증을 요구한다.
                        .anyRequest().authenticated())
                .formLogin(withDefaults())
                .httpBasic(withDefaults())
                .build();
    }
}
