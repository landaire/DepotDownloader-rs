use std::path::PathBuf;

fn main() {
    let proto_dir = PathBuf::from("proto/steam");

    // Only compile if the proto directory exists (after running sync-protos.sh)
    if !proto_dir.exists() {
        println!(
            "cargo::warning=Proto files not found at {:?}. Run scripts/sync-protos.sh first.",
            proto_dir
        );
        return;
    }

    let proto_files: Vec<PathBuf> = [
        "steammessages_base.proto",
        "steammessages_unified_base.steamclient.proto",
        "steammessages_auth.steamclient.proto",
        "steammessages_clientserver_login.proto",
        "steammessages_clientserver.proto",
        "steammessages_clientserver_2.proto",
        "steammessages_clientserver_appinfo.proto",
        "steammessages_contentsystem.steamclient.proto",
        "steammessages_publishedfile.steamclient.proto",
        "content_manifest.proto",
        "enums.proto",
        "enums_clientserver.proto",
        "encrypted_app_ticket.proto",
        "enums_productinfo.proto",
    ]
    .iter()
    .map(|f| proto_dir.join(f))
    .collect();

    // Use protox to parse protos (pure Rust, no system protoc needed)
    let file_descriptors =
        protox::compile(&proto_files, [&proto_dir]).expect("Failed to parse proto files");

    let out_dir = PathBuf::from("src/generated");
    std::fs::create_dir_all(&out_dir).expect("Failed to create generated dir");

    prost_build::Config::new()
        .out_dir(&out_dir)
        .compile_fds(file_descriptors)
        .expect("Failed to generate Rust code from proto files");
}
