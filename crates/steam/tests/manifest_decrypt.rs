//! Test filename decryption using real SteamKit2 test fixtures.
//!
//! Uses the encrypted TF2 depot 440 manifest with the known depot key
//! to verify decryption produces the expected filenames.

use steam::depot::manifest::DepotManifest;
use steam::depot::{DepotId, DepotKey, ManifestId};

const TEST_DATA: &str = "tests/test_data";

/// Depot 440 (TF2) decryption key from SteamKit2 tests.
const DEPOT_440_KEY: DepotKey = DepotKey([
    0x44, 0xCE, 0x5C, 0x52, 0x97, 0xA4, 0x15, 0xA1,
    0xA6, 0xF6, 0x9C, 0x85, 0x60, 0x37, 0xA5, 0xA2,
    0xFD, 0xD8, 0x2C, 0xD4, 0x74, 0xFA, 0x65, 0x9E,
    0xDF, 0xB4, 0xD5, 0x9B, 0x2A, 0xBC, 0x55, 0xFC,
]);

fn load(name: &str) -> Vec<u8> {
    std::fs::read(format!("{TEST_DATA}/{name}")).expect("test data file should exist")
}

#[test]
fn decrypt_v5_manifest_filenames() {
    let data = load("depot_440_1118032470228587934.manifest");
    let mut manifest = DepotManifest::parse(&data).expect("should parse");

    assert!(manifest.filenames_encrypted);
    assert_eq!(manifest.depot_id, Some(DepotId(440)));
    assert_eq!(manifest.manifest_id, Some(ManifestId(1118032470228587934)));
    assert_eq!(manifest.files.len(), 7);

    manifest.decrypt_filenames(&DEPOT_440_KEY).expect("should decrypt");

    assert!(!manifest.filenames_encrypted);

    // Expected filenames from SteamKit2's TestDecryptedManifest
    let expected_filenames = [
        "bin/dxsupport.cfg",
        "bin/dxsupport.csv",
        "bin/dxsupport_episodic.cfg",
        "bin/dxsupport_sp.cfg",
        "bin/vidcfg.bin",
        "hl2/media/startupvids.txt",
        "tf/media/startupvids.txt",
    ];

    let actual_filenames: Vec<&str> = manifest
        .files
        .iter()
        .filter_map(|f| f.filename.as_deref())
        .collect();

    assert_eq!(actual_filenames, expected_filenames);
}

#[test]
fn decrypt_v4_manifest_filenames() {
    let data = load("depot_440_1118032470228587934_v4.manifest");
    let mut manifest = DepotManifest::parse(&data).expect("should parse");

    assert!(manifest.filenames_encrypted);
    assert_eq!(manifest.files.len(), 7);

    manifest.decrypt_filenames(&DEPOT_440_KEY).expect("should decrypt");

    assert!(!manifest.filenames_encrypted);

    let actual_filenames: Vec<&str> = manifest
        .files
        .iter()
        .filter_map(|f| f.filename.as_deref())
        .collect();

    // Same files as v5
    assert_eq!(actual_filenames.len(), 7);
    assert!(actual_filenames.contains(&"bin/dxsupport.cfg"));
    assert!(actual_filenames.contains(&"tf/media/startupvids.txt"));
}

#[test]
fn decrypt_already_decrypted_is_noop() {
    let data = load("depot_440_1118032470228587934_decrypted.manifest");
    let mut manifest = DepotManifest::parse(&data).expect("should parse");

    assert!(!manifest.filenames_encrypted);

    // Should be a no-op
    manifest.decrypt_filenames(&DEPOT_440_KEY).expect("should succeed");
    assert!(!manifest.filenames_encrypted);

    // Filenames should be unchanged (pre-decrypted manifest uses backslashes)
    let name = manifest.files[0].filename.as_deref().unwrap();
    assert!(
        name == "bin/dxsupport.cfg" || name == "bin\\dxsupport.cfg",
        "expected dxsupport.cfg, got: {name}"
    );
}

#[test]
fn decrypted_manifest_matches_expected_metadata() {
    let data = load("depot_440_1118032470228587934.manifest");
    let mut manifest = DepotManifest::parse(&data).expect("should parse");
    manifest.decrypt_filenames(&DEPOT_440_KEY).expect("should decrypt");

    // Values from SteamKit2's TestDecryptedManifest
    assert_eq!(manifest.total_uncompressed_size, Some(825745));
    assert_eq!(manifest.total_compressed_size, Some(43168));

    // First file
    let file0 = &manifest.files[0];
    assert_eq!(file0.filename.as_deref(), Some("bin/dxsupport.cfg"));
    assert_eq!(file0.size, Some(398709));
    assert_eq!(file0.flags, Some(0));
    assert_eq!(file0.chunks.len(), 1);

    // Last file's chunk
    let last_file = &manifest.files[6];
    assert_eq!(last_file.filename.as_deref(), Some("tf/media/startupvids.txt"));
    let chunk = &last_file.chunks[0];
    assert_eq!(chunk.checksum, Some(963249608));
    assert_eq!(chunk.compressed_size, Some(144));
    assert_eq!(chunk.uncompressed_size, Some(17));
}

#[test]
fn decrypt_handles_base64_with_embedded_newlines() {
    // Steam stores encrypted filenames as line-wrapped base64 (newlines at ~64 chars).
    // The TF2 fixture contains filenames with embedded newlines from the proto.
    let data = load("depot_440_1118032470228587934.manifest");
    let manifest = DepotManifest::parse(&data).expect("should parse");
    assert!(manifest.filenames_encrypted);

    // Verify some filenames actually contain newlines (Steam's line wrapping)
    let has_newlines = manifest.files.iter().any(|f| {
        f.filename.as_deref().is_some_and(|n| n.contains('\n'))
    });
    assert!(has_newlines, "TF2 fixture should contain filenames with embedded newlines");

    // Decryption should succeed despite the newlines
    let mut decrypted = manifest.clone();
    decrypted.decrypt_filenames(&DEPOT_440_KEY).expect("should decrypt despite newlines");

    // All filenames should now be clean paths with no newlines
    for file in &decrypted.files {
        let name = file.filename.as_deref().unwrap();
        assert!(
            !name.contains('\n') && !name.contains('\r'),
            "decrypted filename should not contain newlines: {name:?}"
        );
    }

    // Should produce the known filenames
    let names: Vec<&str> = decrypted.files.iter()
        .filter_map(|f| f.filename.as_deref())
        .collect();
    assert!(names.contains(&"bin/dxsupport.cfg"));
    assert!(names.contains(&"tf/media/startupvids.txt"));
}

#[test]
fn decrypt_fails_on_corrupt_base64() {
    let data = load("depot_440_1118032470228587934.manifest");
    let mut manifest = DepotManifest::parse(&data).expect("should parse");

    // Corrupt the first filename
    manifest.files[0].filename = Some("!!!not-valid-base64!!!".to_string());

    let result = manifest.decrypt_filenames(&DEPOT_440_KEY);
    assert!(result.is_err(), "should fail on corrupt base64");
}
