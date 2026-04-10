use std::hint::black_box;
use std::time::Instant;

use steam::depot::DepotKey;
use steam::depot::chunk::process_chunk;
use steam::depot::manifest::DepotManifest;
use steam::types::key_value::parse_binary_kv;
use steam::util::checksum::Sha1Hash;
use steam::util::checksum::SteamAdler32;

const TEST_DATA: &str = "crates/steam/tests/test_data";

fn load(name: &str) -> Vec<u8> {
    std::fs::read(format!("{TEST_DATA}/{name}")).expect("test data file should exist")
}

const DEPOT_440_KEY: [u8; 32] = [
    0x44, 0xCE, 0x5C, 0x52, 0x97, 0xA4, 0x15, 0xA1, 0xA6, 0xF6, 0x9C, 0x85, 0x60, 0x37, 0xA5, 0xA2,
    0xFD, 0xD8, 0x2C, 0xD4, 0x74, 0xFA, 0x65, 0x9E, 0xDF, 0xB4, 0xD5, 0x9B, 0x2A, 0xBC, 0x55, 0xFC,
];

const DEPOT_232250_KEY: [u8; 32] = [
    0xE5, 0xF6, 0xAE, 0xD5, 0x5E, 0x9E, 0xCE, 0x42, 0x9E, 0x56, 0xB8, 0x13, 0xFB, 0xF6, 0xBF, 0xE9,
    0x24, 0xF3, 0xCF, 0x72, 0x97, 0x2F, 0xDB, 0xD0, 0x57, 0x1F, 0xFC, 0xAD, 0x9F, 0x2F, 0x7D, 0xAA,
];

const DEPOT_3441461_KEY: [u8; 32] = [
    0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F, 0x10,
    0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x1B, 0x1C, 0x1D, 0x1E, 0x1F, 0x20,
];

fn build_large_binary_kv() -> Vec<u8> {
    use steam::types::key_value::KvTag;

    let mut buf = Vec::new();
    buf.push(KvTag::None as u8);
    buf.extend_from_slice(b"appinfo\0");

    for i in 0..500 {
        let key = format!("depot_{i}\0");
        buf.push(KvTag::None as u8);
        buf.extend_from_slice(key.as_bytes());

        let name = format!("Depot Content {i}\0");
        buf.push(KvTag::String as u8);
        buf.extend_from_slice(b"name\0");
        buf.extend_from_slice(name.as_bytes());

        buf.push(KvTag::UInt64 as u8);
        buf.extend_from_slice(b"manifest_id\0");
        buf.extend_from_slice(&(1000000u64 + i as u64).to_le_bytes());

        buf.push(KvTag::Int32 as u8);
        buf.extend_from_slice(b"size_mb\0");
        buf.extend_from_slice(&(512i32 * (i + 1)).to_le_bytes());

        for j in 0..10 {
            let branch_key = format!("branch_{j}\0");
            buf.push(KvTag::None as u8);
            buf.extend_from_slice(branch_key.as_bytes());

            buf.push(KvTag::String as u8);
            buf.extend_from_slice(b"build_id\0");
            let build = format!("{}\0", 10000 + j);
            buf.extend_from_slice(build.as_bytes());

            buf.push(KvTag::Int32 as u8);
            buf.extend_from_slice(b"time_updated\0");
            buf.extend_from_slice(&(1700000000i32 + j).to_le_bytes());

            buf.push(KvTag::End as u8);
        }

        buf.push(KvTag::End as u8);
    }

    buf.push(KvTag::End as u8);
    buf
}

fn bench_chunk_processing(iterations: u32) {
    let zip_chunk = load("depot_440_chunk_bac8e2657470b2eb70d6ddcd6c07004be8738697.bin");
    let lzma_chunk = load("depot_232250_chunk_7b8567d9b3c09295cdbf4978c32b348d8e76c750.bin");
    let zstd_chunk = load("depot_3441461_chunk_9e72678e305540630a665b93e1463bc3983eb55a.bin");

    let start = Instant::now();
    for _ in 0..iterations {
        black_box(process_chunk(&zip_chunk, &DepotKey(DEPOT_440_KEY), 544, 2130218374).unwrap());
        black_box(
            process_chunk(&lzma_chunk, &DepotKey(DEPOT_232250_KEY), 798, 2894626744).unwrap(),
        );
        black_box(
            process_chunk(&zstd_chunk, &DepotKey(DEPOT_3441461_KEY), 156, 3753325726).unwrap(),
        );
    }
    let elapsed = start.elapsed();
    eprintln!(
        "chunk processing: {iterations} iterations x 3 codecs = {} total in {elapsed:?} ({:.0} chunks/sec)",
        iterations * 3,
        (iterations * 3) as f64 / elapsed.as_secs_f64()
    );
}

fn bench_manifest_parsing(iterations: u32) {
    let decrypted = load("depot_440_1118032470228587934_decrypted.manifest");
    let encrypted = load("depot_440_1118032470228587934.manifest");
    let v4 = load("depot_440_1118032470228587934_v4.manifest");

    let start = Instant::now();
    for _ in 0..iterations {
        black_box(DepotManifest::parse(&decrypted).unwrap());
        black_box(DepotManifest::parse(&encrypted).unwrap());
        black_box(DepotManifest::parse(&v4).unwrap());
    }
    let elapsed = start.elapsed();
    eprintln!(
        "manifest parsing: {iterations} iterations x 3 formats in {elapsed:?} ({:.0} parses/sec)",
        (iterations * 3) as f64 / elapsed.as_secs_f64()
    );
}

fn bench_binary_kv_parsing(iterations: u32) {
    let large_kv = build_large_binary_kv();
    eprintln!(
        "binary KV size: {} bytes, 500 depots x 10 branches",
        large_kv.len()
    );

    let start = Instant::now();
    for _ in 0..iterations {
        let mut input = large_kv.as_slice();
        black_box(parse_binary_kv(&mut input).unwrap());
    }
    let elapsed = start.elapsed();
    eprintln!(
        "binary KV parsing: {iterations} iterations in {elapsed:?} ({:.0} parses/sec, {:.0} MB/s)",
        iterations as f64 / elapsed.as_secs_f64(),
        (large_kv.len() as f64 * iterations as f64) / elapsed.as_secs_f64() / 1_000_000.0,
    );
}

fn bench_checksums(iterations: u32) {
    let data: Vec<u8> = (0..1_000_000).map(|i| (i % 256) as u8).collect();

    let start = Instant::now();
    for _ in 0..iterations {
        black_box(SteamAdler32::compute(&data));
    }
    let elapsed = start.elapsed();
    let throughput = (data.len() as f64 * iterations as f64) / elapsed.as_secs_f64() / 1e9;
    eprintln!("SteamAdler32 (1MB): {iterations} iterations in {elapsed:?} ({throughput:.2} GB/s)");

    let start = Instant::now();
    for _ in 0..iterations {
        black_box(Sha1Hash::compute(&data));
    }
    let elapsed = start.elapsed();
    let throughput = (data.len() as f64 * iterations as f64) / elapsed.as_secs_f64() / 1e9;
    eprintln!("SHA-1 (1MB): {iterations} iterations in {elapsed:?} ({throughput:.2} GB/s)");
}

fn bench_aes_decrypt(iterations: u32) {
    let zip_chunk = load("depot_440_chunk_bac8e2657470b2eb70d6ddcd6c07004be8738697.bin");

    let start = Instant::now();
    for _ in 0..iterations {
        black_box(steam::crypto::symmetric_decrypt_ecb(&DEPOT_440_KEY, &zip_chunk).unwrap());
    }
    let elapsed = start.elapsed();
    let throughput = (zip_chunk.len() as f64 * iterations as f64) / elapsed.as_secs_f64() / 1e6;
    eprintln!(
        "AES-256 decrypt ({} bytes): {iterations} iterations in {elapsed:?} ({throughput:.1} MB/s)",
        zip_chunk.len()
    );
}

fn main() {
    let iterations: u32 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000);

    eprintln!("running {iterations} iterations per benchmark\n");

    bench_chunk_processing(iterations);
    bench_manifest_parsing(iterations);
    bench_binary_kv_parsing(iterations);
    bench_checksums(iterations);
    bench_aes_decrypt(iterations);

    eprintln!("\ndone");
}
