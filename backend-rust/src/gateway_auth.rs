use anyhow::{Result, ensure};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use tonic::{Request, Status, service::Interceptor};

// Java가 넣은 이름과 완전히 (정확히) 같아야한다.
const GATEWAY_HEADER: &str = "x-sbe-gateway-token";

#[derive(Clone)]
pub(crate) struct GatewayAuth {
    // 서버가 기대하는 토큰의 SHA-256 해시, 항상 32 바이트이다.
    token_hash: [u8; 32],
}

impl GatewayAuth {
    pub(crate) fn new(token: &str) -> Result<Self> {
        // 서버 시작 시 한 번 검증한다.
        ensure!(
            !token.is_empty() && token.bytes().all(|byte| byte.is_ascii_graphic()),
            "SBE_GATEWAY_TOKEN에는 공백 없는 ASCII 토큰을 설정하세요."
        );
        Ok(Self {
            token_hash: Sha256::digest(token.as_bytes()).into(),
        })
    }
}

impl Interceptor for GatewayAuth {
    fn call(&mut self, request: Request<()>) -> Result<Request<()>, Status> {
        // 같은 이름의 헤더가 여러 번 올 수도 있으므로 전부 가져온다.
        let values = request.metadata().get_all(GATEWAY_HEADER);
        let mut tokens = values.iter();

        // 첫 번째 값은 있고 두 번째 값은 없어야 한다. 즉 정확히 한 개다.
        if let (Some(token), None) = (tokens.next(), tokens.next()) {
            let received = Sha256::digest(token.as_encoded_bytes());

            // 고정 길이 해시를 상수 시간 비교 함수로 확인한다.
            if bool::from(self.token_hash.as_slice().ct_eq(received.as_slice())) {
                return Ok(request); // 실제 RPC 핸들러로 진행한다.
            }
        }

        // 누락, 중복, 불일치는 모두 같은 인증 오류로 응답한다.
        Err(Status::unauthenticated("게이트웨이 인증이 필요합니다."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tonic::Code;

    #[test]
    fn requires_one_matching_gateway_token() {
        let mut auth = GatewayAuth::new("test-token").unwrap();

        // 각 입력에서 기대하는 오류 코드를 짝지어 같은 검사를 반복한다.
        // 마지막 행의 None은 "오류 없음"이다.
        for (headers, expected) in [
            (vec![], Some(Code::Unauthenticated)),
            (vec!["other"], Some(Code::Unauthenticated)),
            (vec!["test-token", "other"], Some(Code::Unauthenticated)),
            (vec!["test-token"], None),
        ] {
            let mut request = Request::new(());
            for value in headers {
                request
                    .metadata_mut()
                    .append(GATEWAY_HEADER, value.parse().unwrap());
            }
            assert_eq!(auth.call(request).err().map(|e| e.code()), expected);
        }
    }

    #[test]
    fn rejects_unusable_configured_tokens() {
        for token in ["", " ", "test\ntoken", "토큰"] {
            // 서버 설정 자체가 잘못된 경우도 검사한다.
            assert!(GatewayAuth::new(token).is_err());
        }
    }
}
