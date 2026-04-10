//! Snapshot tests for depot chunk processing (decrypt + decompress + verify).
//!
//! Each test uses a real encrypted chunk from SteamKit2's test suite,
//! with known depot keys and expected checksums.

use steam::depot::DepotKey;
use steam::depot::chunk::process_chunk;
use steam::util::checksum::Sha1Hash;

const TEST_DATA: &str = "tests/test_data";

fn load(name: &str) -> Vec<u8> {
    std::fs::read(format!("{TEST_DATA}/{name}")).expect("test data file should exist")
}

/// Depot 440 (TF2) key, shared with manifest tests.
const DEPOT_440_KEY: [u8; 32] = [
    0x44, 0xCE, 0x5C, 0x52, 0x97, 0xA4, 0x15, 0xA1, 0xA6, 0xF6, 0x9C, 0x85, 0x60, 0x37, 0xA5, 0xA2,
    0xFD, 0xD8, 0x2C, 0xD4, 0x74, 0xFA, 0x65, 0x9E, 0xDF, 0xB4, 0xD5, 0x9B, 0x2A, 0xBC, 0x55, 0xFC,
];

#[test]
fn pkzip_chunk() {
    let data = load("depot_440_chunk_bac8e2657470b2eb70d6ddcd6c07004be8738697.bin");
    assert_eq!(data.len(), 320, "compressed length");

    let result = process_chunk(&data, &DepotKey(DEPOT_440_KEY), 544, 2130218374)
        .expect("should decrypt and decompress");

    assert_eq!(result.len(), 544);
    assert_eq!(
        Sha1Hash::compute(&result).to_string().to_uppercase(),
        "BAC8E2657470B2EB70D6DDCD6C07004BE8738697"
    );
}

#[test]
fn lzma_chunk() {
    let data = load("depot_232250_chunk_7b8567d9b3c09295cdbf4978c32b348d8e76c750.bin");
    assert_eq!(data.len(), 304, "compressed length");

    let key = DepotKey([
        0xE5, 0xF6, 0xAE, 0xD5, 0x5E, 0x9E, 0xCE, 0x42, 0x9E, 0x56, 0xB8, 0x13, 0xFB, 0xF6, 0xBF,
        0xE9, 0x24, 0xF3, 0xCF, 0x72, 0x97, 0x2F, 0xDB, 0xD0, 0x57, 0x1F, 0xFC, 0xAD, 0x9F, 0x2F,
        0x7D, 0xAA,
    ]);

    let result =
        process_chunk(&data, &key, 798, 2894626744).expect("should decrypt and decompress");

    assert_eq!(result.len(), 798);
    assert_eq!(
        Sha1Hash::compute(&result).to_string().to_uppercase(),
        "7B8567D9B3C09295CDBF4978C32B348D8E76C750"
    );
}

#[test]
fn zstd_chunk() {
    let data = load("depot_3441461_chunk_9e72678e305540630a665b93e1463bc3983eb55a.bin");
    assert_eq!(data.len(), 176, "compressed length");

    let key = DepotKey([
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F,
        0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x1B, 0x1C, 0x1D, 0x1E,
        0x1F, 0x20,
    ]);

    let result =
        process_chunk(&data, &key, 156, 3753325726).expect("should decrypt and decompress");

    assert_eq!(result.len(), 156);
    assert_eq!(
        Sha1Hash::compute(&result).to_string().to_uppercase(),
        "9E72678E305540630A665B93E1463BC3983EB55A"
    );
}
