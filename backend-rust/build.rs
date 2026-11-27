fn main() -> std::io::Result<()> {
    // 공용 계약을 찾는다.
    let proto_file = "../contract/proto/service_configuration.proto";

    // 계약 파일이 바뀌면 Cargo가 이 빌드 스크립트를 다시 실행하도록 알린다.
    println!("cargo::rerun-if-changed={proto_file}");

    tonic_prost_build::configure()
        // 여기서 Rust는 요청을 받는 서버 역할이다.
        .build_client(false)
        // 첫 번째 인자는 생성할 파일, 두 번째 인자는 proto 검색 경로이다.
        .compile_protos(&[proto_file], &["../contract/proto"])
}