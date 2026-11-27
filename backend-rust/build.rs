fn main() -> std::io::Result<()> {
    let proto_file = "../contract/proto/service_configuration.proto";
    println!("cargo::rerun-if-changed={proto_file}");

    tonic_prost_build::configure()
        .build_client(false)
        .compile_protos(&[proto_file], &["../contract/proto"])
}