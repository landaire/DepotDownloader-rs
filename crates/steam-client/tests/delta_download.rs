//! Tests for delta patching behavior using the real DepotJob::download
//! with a mock ChunkFetcher that serves data from memory.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;

use steam::depot::ChunkId;
use steam::depot::DepotId;
use steam::depot::DepotKey;
use steam::depot::ManifestId;
use steam::depot::manifest::DepotManifest;
use steam::depot::manifest::ManifestChunk;
use steam::depot::manifest::ManifestFile;
use steam_client::download::BoxError;
use steam_client::download::ChunkFetcher;
use steam_client::download::DepotJob;
use steam_client::event::DownloadEvent;

/// Serves chunk data from a HashMap. Chunks are stored as plaintext
/// (no encryption/compression) so we use an all-zero depot key.
struct MockFetcher {
    chunks: HashMap<[u8; 20], Vec<u8>>,
}

#[async_trait]
impl ChunkFetcher for MockFetcher {
    async fn fetch_chunk(&self, _depot_id: DepotId, chunk_id: &ChunkId) -> Result<Bytes, BoxError> {
        self.chunks
            .get(&chunk_id.0)
            .map(|data| Bytes::copy_from_slice(data))
            .ok_or_else(|| format!("chunk not found: {chunk_id}").into())
    }
}

fn test_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("depotdownloader_delta_test")
        .join(format!("{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

// The mock fetcher serves raw unencrypted data. We use a zero depot key
// and construct chunk data that's "uncompressed" (no magic prefix).
// process_chunk expects: AES decrypt -> decompress -> verify.
// For testing, we need to produce encrypted+uncompressed chunk data.
// This is complex, so instead let's test the delta logic at the manifest
// level and verify the file system state, using pre-written files.

fn make_chunk(id_byte: u8, size: u32) -> ManifestChunk {
    let mut sha = [0u8; 20];
    sha[0] = id_byte;
    ManifestChunk {
        id: Some(ChunkId(sha)),
        checksum: Some(0),
        offset: Some(0),
        compressed_size: Some(size),
        uncompressed_size: Some(size),
    }
}

fn make_file(name: &str, content_byte: u8, chunks: Vec<ManifestChunk>) -> ManifestFile {
    let mut sha = [0u8; 20];
    sha[0] = content_byte;
    let size: u64 = chunks
        .iter()
        .filter_map(|c| c.uncompressed_size)
        .map(|s| s as u64)
        .sum();
    ManifestFile {
        filename: Some(name.to_string()),
        size: Some(size),
        flags: Some(0),
        sha_content: Some(sha),
        chunks,
        link_target: None,
    }
}

fn make_manifest(id: u64, files: Vec<ManifestFile>) -> DepotManifest {
    DepotManifest {
        depot_id: Some(DepotId(999)),
        manifest_id: Some(ManifestId(id)),
        creation_time: Some(0),
        filenames_encrypted: false,
        total_uncompressed_size: None,
        total_compressed_size: None,
        files,
    }
}

fn build_job(
    install_dir: PathBuf,
    previous: Option<DepotManifest>,
) -> (
    DepotJob,
    tokio::sync::mpsc::UnboundedReceiver<DownloadEvent>,
) {
    let fetcher = Arc::new(MockFetcher {
        chunks: HashMap::new(),
    });
    let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();

    let job = DepotJob::builder()
        .fetcher(fetcher)
        .depot_id(DepotId(999))
        .depot_key(DepotKey([0u8; 32]))
        .install_dir(install_dir)
        .event_sender(event_tx)
        .previous_manifest(previous)
        .build()
        .unwrap();

    (job, event_rx)
}

#[tokio::test]
async fn unchanged_files_are_skipped() {
    let dir = test_dir("unchanged");

    // Simulate an already-downloaded file
    std::fs::write(dir.join("same.txt"), "hello").unwrap();

    let old = make_manifest(
        1,
        vec![make_file("same.txt", 0xAA, vec![make_chunk(0x01, 5)])],
    );
    let new = make_manifest(
        2,
        vec![
            make_file("same.txt", 0xAA, vec![make_chunk(0x01, 5)]), // same content hash
        ],
    );

    let (job, mut rx) = build_job(dir.clone(), Some(old));

    // download will skip because sha_content matches and file exists
    // it won't try to fetch chunks (mock has none, would error if it tried)
    job.download(&new).await.unwrap();

    // Collect events
    drop(job);
    let mut events = Vec::new();
    while let Ok(e) = rx.try_recv() {
        events.push(e);
    }

    // Should have skipped, not started
    let skipped = events
        .iter()
        .any(|e| matches!(e, DownloadEvent::FileSkipped { .. }));
    assert!(skipped, "unchanged file should be skipped");

    // File should still exist unchanged
    assert_eq!(
        std::fs::read_to_string(dir.join("same.txt")).unwrap(),
        "hello"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn removed_files_are_deleted() {
    let dir = test_dir("removed");

    std::fs::write(dir.join("keep.txt"), "keep").unwrap();
    std::fs::write(dir.join("delete_me.txt"), "gone").unwrap();

    let old = make_manifest(
        1,
        vec![
            make_file("keep.txt", 0x01, vec![make_chunk(0x01, 4)]),
            make_file("delete_me.txt", 0x02, vec![make_chunk(0x02, 4)]),
        ],
    );
    let new = make_manifest(
        2,
        vec![
            make_file("keep.txt", 0x01, vec![make_chunk(0x01, 4)]), // unchanged
        ],
    );

    let (job, _rx) = build_job(dir.clone(), Some(old));
    job.download(&new).await.unwrap();

    assert!(
        dir.join("keep.txt").exists(),
        "kept file should still exist"
    );
    assert!(
        !dir.join("delete_me.txt").exists(),
        "removed file should be deleted"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn multiple_removed_files_all_deleted() {
    let dir = test_dir("multi_removed");

    std::fs::write(dir.join("a.txt"), "a").unwrap();
    std::fs::write(dir.join("b.txt"), "b").unwrap();
    std::fs::write(dir.join("c.txt"), "c").unwrap();

    let old = make_manifest(
        1,
        vec![
            make_file("a.txt", 0x01, vec![make_chunk(0x01, 1)]),
            make_file("b.txt", 0x02, vec![make_chunk(0x02, 1)]),
            make_file("c.txt", 0x03, vec![make_chunk(0x03, 1)]),
        ],
    );
    // New manifest has no files at all
    let new = make_manifest(2, vec![]);

    let (job, _rx) = build_job(dir.clone(), Some(old));
    job.download(&new).await.unwrap();

    assert!(!dir.join("a.txt").exists());
    assert!(!dir.join("b.txt").exists());
    assert!(!dir.join("c.txt").exists());

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn no_previous_manifest_does_not_delete() {
    let dir = test_dir("no_prev");

    // Some pre-existing file that's not in the manifest at all
    std::fs::write(dir.join("unrelated.txt"), "keep me").unwrap();

    let new = make_manifest(
        1,
        vec![make_file("same.txt", 0xAA, vec![make_chunk(0x01, 5)])],
    );

    // No previous manifest - fresh install
    let (job, _rx) = build_job(dir.clone(), None);

    // This will fail trying to fetch chunks (mock has none),
    // but the point is: it should NOT delete unrelated.txt
    let _ = job.download(&new).await;

    assert!(
        dir.join("unrelated.txt").exists(),
        "unrelated files should not be deleted on fresh install"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
