//! Snapshot tests for DepotManifest parsing.
//!
//! Test data is real manifests from SteamKit2's test suite (depot 440 / TF2).
//! Run `cargo insta review` after adding or updating tests.

use steam::depot::manifest::DepotManifest;

const TEST_DATA: &str = "tests/test_data";

fn load(name: &str) -> Vec<u8> {
    std::fs::read(format!("{TEST_DATA}/{name}")).expect("test data file should exist")
}

#[test]
fn decrypted_manifest() {
    let data = load("depot_440_1118032470228587934_decrypted.manifest");
    let manifest = DepotManifest::parse(&data).expect("should parse");
    insta::assert_toml_snapshot!(manifest);
}

#[test]
fn encrypted_manifest() {
    let data = load("depot_440_1118032470228587934.manifest");
    let manifest = DepotManifest::parse(&data).expect("should parse");
    insta::assert_toml_snapshot!(manifest);
}

#[test]
fn v4_manifest() {
    let data = load("depot_440_1118032470228587934_v4.manifest");
    let manifest = DepotManifest::parse(&data).expect("should parse");
    insta::assert_toml_snapshot!(manifest);
}
