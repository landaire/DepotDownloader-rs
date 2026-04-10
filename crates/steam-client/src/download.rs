use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use tokio::sync::Semaphore;
use tokio::sync::mpsc;

use steam::cdn::CdnClient;
use steam::cdn::server::CdnServer;
use steam::depot::ChunkId;
use steam::depot::DepotId;
use steam::depot::DepotKey;
use steam::depot::chunk::process_chunk;
use steam::depot::manifest::DepotManifest;
use steam::depot::manifest::ManifestChunk;

use crate::event::DownloadEvent;

/// Trait for fetching raw chunk data. The real implementation goes through
/// the CDN; tests can substitute a mock that serves data from memory.
#[async_trait]
pub trait ChunkFetcher: Send + Sync {
    async fn fetch_chunk(&self, depot_id: DepotId, chunk_id: &ChunkId) -> Result<Bytes, BoxError>;
}

/// Default implementation that fetches via the CDN HTTP client.
pub struct CdnChunkFetcher {
    pub cdn: CdnClient,
    pub server: CdnServer,
    pub cdn_auth_token: Option<String>,
}

#[async_trait]
impl ChunkFetcher for CdnChunkFetcher {
    async fn fetch_chunk(&self, depot_id: DepotId, chunk_id: &ChunkId) -> Result<Bytes, BoxError> {
        Ok(self
            .cdn
            .download_chunk(
                &self.server,
                depot_id,
                chunk_id,
                self.cdn_auth_token.as_deref(),
            )
            .await?)
    }
}

pub type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// Send an event, logging if the receiver has gone away.
macro_rules! send_event {
    ($tx:expr, $event:expr) => {
        if $tx.send($event).is_err() {
            tracing::trace!("Event receiver dropped");
        }
    };
}

#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_attempts: u32,
    pub initial_delay: std::time::Duration,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 5,
            initial_delay: std::time::Duration::from_secs(1),
        }
    }
}

impl RetryConfig {
    pub fn none() -> Self {
        Self {
            max_attempts: 1,
            initial_delay: std::time::Duration::ZERO,
        }
    }
}

/// A depot download job. Carries all shared context needed to download
/// chunks from a single depot.
pub struct DepotJob {
    fetcher: Arc<dyn ChunkFetcher>,
    depot_id: DepotId,
    depot_key: DepotKey,
    install_dir: PathBuf,
    semaphore: Arc<Semaphore>,
    event_tx: mpsc::UnboundedSender<DownloadEvent>,
    verify: bool,
    file_filter: Option<FileFilter>,
    previous_manifest: Option<DepotManifest>,
    retry: RetryConfig,
}

/// Filter to select which files to download.
pub enum FileFilter {
    /// Explicit list of filenames (case-insensitive).
    FileList(Vec<String>),
    /// Regex pattern (case-insensitive).
    Regex(regex::Regex),
}

impl FileFilter {
    /// Load a file list from a text file (one filename per line).
    pub fn from_filelist(path: &std::path::Path) -> Result<Self, std::io::Error> {
        let content = std::fs::read_to_string(path)?;
        let files: Vec<String> = content
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .map(|l| l.replace('\\', "/"))
            .collect();
        Ok(Self::FileList(files))
    }

    /// Create from a regex pattern string.
    pub fn from_regex(pattern: &str) -> Result<Self, regex::Error> {
        let re = regex::RegexBuilder::new(pattern)
            .case_insensitive(true)
            .build()?;
        Ok(Self::Regex(re))
    }

    pub fn matches(&self, filename: &str) -> bool {
        let normalized = filename.replace('\\', "/");
        match self {
            Self::FileList(list) => list.iter().any(|f| f.eq_ignore_ascii_case(&normalized)),
            Self::Regex(re) => re.is_match(&normalized),
        }
    }
}

impl DepotJob {
    pub fn builder() -> DepotJobBuilder {
        DepotJobBuilder::default()
    }

    /// Download all files from a manifest.
    pub async fn download(&self, manifest: &DepotManifest) -> Result<(), BoxError> {
        tracing::info!("Starting download of {} files", manifest.files.len());
        let files_total = manifest.files.len();
        let mut files_completed = 0usize;
        let mut handles = Vec::new();

        for file in &manifest.files {
            let filename = match &file.filename {
                Some(name) => name.clone(),
                None => {
                    tracing::warn!("Manifest file entry has no filename, skipping");
                    continue;
                }
            };

            // Apply file filter
            if let Some(ref filter) = self.file_filter
                && !filter.matches(&filename)
            {
                send_event!(
                    self.event_tx,
                    DownloadEvent::FileSkipped {
                        depot_id: self.depot_id,
                        filename,
                    }
                );
                files_completed += 1;
                continue;
            }

            // Skip directories (flag 0x40 = directory)
            if file
                .flags
                .is_some_and(|f| steam::enums::DepotFileFlags(f).is_directory())
            {
                send_event!(
                    self.event_tx,
                    DownloadEvent::FileSkipped {
                        depot_id: self.depot_id,
                        filename,
                    }
                );
                files_completed += 1;
                send_event!(
                    self.event_tx,
                    DownloadEvent::DepotProgress {
                        depot_id: self.depot_id,
                        files_completed,
                        files_total,
                    }
                );
                continue;
            }

            // Find matching file in previous manifest for delta comparison
            let old_file = self.previous_manifest.as_ref().and_then(|old| {
                old.files
                    .iter()
                    .find(|f| f.filename.as_deref() == Some(&filename))
            });

            // Skip entirely if the file content hash is unchanged
            if let Some(old) = old_file
                && old.sha_content == file.sha_content
                && file.sha_content.is_some()
                && self.install_dir.join(&filename).exists()
            {
                tracing::debug!("{filename}: unchanged, skipping");
                send_event!(
                    self.event_tx,
                    DownloadEvent::FileSkipped {
                        depot_id: self.depot_id,
                        filename,
                    }
                );
                files_completed += 1;
                continue;
            }

            let old_chunks = old_file.map(|f| f.chunks.clone());

            let task = FileTask {
                path: self.install_dir.join(&filename),
                size: file.size,
                flags: file.flags,
                chunks: file.chunks.clone(),
                old_chunks,
                filename: filename.clone(),
            };

            send_event!(
                self.event_tx,
                DownloadEvent::FileStarted {
                    depot_id: self.depot_id,
                    filename,
                    total_chunks: task.chunks.len(),
                    file_size: file.size,
                }
            );

            let job = self.clone_shared();
            let handle = tokio::spawn(async move {
                let result = job.download_file(&task).await;
                if result.is_ok() {
                    // Set executable permissions on Unix
                    set_executable_if_needed(&task);

                    send_event!(
                        job.event_tx,
                        DownloadEvent::FileCompleted {
                            depot_id: job.depot_id,
                            filename: task.filename.clone(),
                        }
                    );
                }
                result
            });

            handles.push((handle, files_completed));
        }

        for (handle, _) in handles {
            handle.await??;
            files_completed += 1;
            send_event!(
                self.event_tx,
                DownloadEvent::DepotProgress {
                    depot_id: self.depot_id,
                    files_completed,
                    files_total,
                }
            );
        }

        // Delete files that existed in the previous manifest but not in the new one
        if let Some(ref old_manifest) = self.previous_manifest {
            let new_filenames: std::collections::HashSet<&str> = manifest
                .files
                .iter()
                .filter_map(|f| f.filename.as_deref())
                .collect();

            for old_file in &old_manifest.files {
                let name = match old_file.filename.as_deref() {
                    Some(n) => n,
                    None => continue,
                };

                if old_file
                    .flags
                    .is_some_and(|f| steam::enums::DepotFileFlags(f).is_directory())
                {
                    continue;
                }

                if !new_filenames.contains(name) {
                    let path = self.install_dir.join(name);
                    if path.exists() {
                        if let Err(e) = std::fs::remove_file(&path) {
                            tracing::warn!("Failed to delete removed file {name}: {e}");
                        } else {
                            tracing::info!("Deleted removed file: {name}");
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Download a single file by fetching all its chunks.
    ///
    /// Downloads to a staging path first, then atomically moves to the
    /// final location on success. This prevents partial files on interruption.
    ///
    /// In verify mode: if the file already exists, validate each chunk's
    /// Adler32 checksum and only re-download corrupted chunks.
    async fn download_file(&self, task: &FileTask) -> Result<(), BoxError> {
        let staging_dir = self.install_dir.join(".staging");
        let staging_path = staging_dir.join(&task.filename);

        if let Some(parent) = staging_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        if let Some(parent) = task.path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // If the final file already exists and is fully valid, skip entirely
        if file_has_valid_chunks(&task.path, task.size, &task.chunks).await? {
            tracing::debug!("{}: final file valid, skipping", task.filename);
            return Ok(());
        }

        // Check staging file from a previous interrupted run
        let mut valid_chunks = std::collections::HashSet::new();
        let resuming = if file_has_correct_size(&staging_path, task.size).await {
            let bad = verify_chunks(&staging_path, &task.chunks).await?;
            for (i, chunk) in task.chunks.iter().enumerate() {
                if !bad.contains(&i)
                    && let Some(ref id) = chunk.id
                {
                    valid_chunks.insert(*id);
                }
            }
            if valid_chunks.len() == task.chunks.len() {
                tracing::debug!("{}: staging file fully valid, finishing", task.filename);
                tokio::fs::rename(&staging_path, &task.path).await?;
                return Ok(());
            }
            if !valid_chunks.is_empty() {
                tracing::info!(
                    "{}: resuming, {}/{} chunks already valid in staging",
                    task.filename,
                    valid_chunks.len(),
                    task.chunks.len()
                );
            }
            true
        } else {
            false
        };

        let file = if resuming {
            tokio::fs::OpenOptions::new()
                .write(true)
                .open(&staging_path)
                .await?
        } else {
            let f = tokio::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&staging_path)
                .await?;
            if let Some(size) = task.size {
                f.set_len(size).await?;
            }
            f
        };

        let file = Arc::new(tokio::sync::Mutex::new(file));

        // If we have a previous manifest, copy unchanged chunks from the old file
        if let Some(ref old_chunks) = task.old_chunks
            && task.path.exists()
        {
            let mut copied = 0usize;
            let old_file = tokio::fs::File::open(&task.path).await?;
            let old_file = Arc::new(tokio::sync::Mutex::new(old_file));

            for chunk in &task.chunks {
                let chunk_id = match &chunk.id {
                    Some(id) => id,
                    None => continue,
                };

                // Find matching chunk in old manifest by ID
                let old_match = old_chunks
                    .iter()
                    .find(|oc| oc.id.as_ref() == Some(chunk_id));

                if let Some(old_chunk) = old_match
                    && let (Some(old_offset), Some(new_offset), Some(size)) =
                        (old_chunk.offset, chunk.offset, chunk.uncompressed_size)
                {
                    use tokio::io::AsyncReadExt;
                    use tokio::io::AsyncSeekExt;
                    use tokio::io::AsyncWriteExt;

                    // Read from old file, write to staging file
                    let mut buf = vec![0u8; size as usize];
                    let mut of = old_file.lock().await;
                    of.seek(std::io::SeekFrom::Start(old_offset)).await?;
                    if of.read_exact(&mut buf).await.is_ok() {
                        let mut nf = file.lock().await;
                        nf.seek(std::io::SeekFrom::Start(new_offset)).await?;
                        nf.write_all(&buf).await?;
                        copied += 1;
                    }
                }
            }

            if copied > 0 {
                tracing::debug!(
                    "{}: reused {copied} of {} chunks from previous version",
                    task.filename,
                    task.chunks.len()
                );
            }
        }

        let mut handles = Vec::new();

        for chunk in &task.chunks {
            let chunk_id = match &chunk.id {
                Some(id) => *id,
                None => {
                    tracing::warn!("Chunk in {} has no ID, skipping", task.filename);
                    continue;
                }
            };

            // Skip chunks already validated (from staging resume or delta reuse)
            if valid_chunks.contains(&chunk_id) {
                continue;
            }

            // Skip chunks that were already copied from the previous version
            if let Some(ref old_chunks) = task.old_chunks
                && task.path.exists()
                && old_chunks.iter().any(|oc| oc.id == Some(chunk_id))
            {
                continue;
            }

            let job = self.clone_shared();
            let offset = chunk.offset;
            let expected_size = chunk.uncompressed_size;
            let expected_checksum = chunk.checksum;
            let file = file.clone();

            let handle = tokio::spawn(async move {
                let _permit = job.semaphore.acquire().await?;

                let max = job.retry.max_attempts;
                let mut raw = None;
                for attempt in 0..max {
                    match job.fetcher.fetch_chunk(job.depot_id, &chunk_id).await {
                        Ok(data) => {
                            tracing::debug!("Downloaded chunk {chunk_id} ({} bytes)", data.len());
                            raw = Some(data);
                            break;
                        }
                        Err(e) => {
                            if attempt + 1 < max {
                                let delay = job.retry.initial_delay * (1 << attempt);
                                tracing::warn!(
                                    "Chunk {chunk_id} attempt {}/{max} failed: {e}, retrying in {}s",
                                    attempt + 1,
                                    delay.as_secs()
                                );
                                tokio::time::sleep(delay).await;
                            } else {
                                tracing::error!(
                                    "Chunk {chunk_id} failed after {max} attempts: {e}"
                                );
                                return Err(e);
                            }
                        }
                    }
                }
                let raw = raw.unwrap();

                let depot_key = job.depot_key.clone();
                let decompressed = tokio::task::spawn_blocking(move || {
                    process_chunk(
                        &raw,
                        &depot_key,
                        expected_size.unwrap_or(0),
                        expected_checksum.unwrap_or(0),
                    )
                })
                .await??;

                let bytes_written = decompressed.len();

                if let Some(off) = offset {
                    use tokio::io::AsyncSeekExt;
                    use tokio::io::AsyncWriteExt;
                    let mut f = file.lock().await;
                    f.seek(std::io::SeekFrom::Start(off)).await?;
                    f.write_all(&decompressed).await?;
                }

                send_event!(
                    job.event_tx,
                    DownloadEvent::ChunkCompleted {
                        depot_id: job.depot_id,
                        chunk_id,
                        bytes_written,
                    }
                );

                Ok::<(), BoxError>(())
            });

            handles.push(handle);
        }

        for handle in handles {
            handle.await??;
        }

        // Atomic move from staging to final location
        // Drop the file handle first so it's not held during rename
        drop(file);
        tokio::fs::rename(&staging_path, &task.path).await?;

        Ok(())
    }

    /// Clone the shared (cheap) state for moving into a spawned task.
    fn clone_shared(&self) -> DepotJob {
        DepotJob {
            fetcher: self.fetcher.clone(),
            depot_id: self.depot_id,
            depot_key: self.depot_key.clone(),
            install_dir: self.install_dir.clone(),
            semaphore: self.semaphore.clone(),
            event_tx: self.event_tx.clone(),
            verify: self.verify,
            file_filter: None,
            previous_manifest: None,
            retry: self.retry.clone(),
        }
    }
}

/// Verify which chunks in an existing file are corrupted.
///
async fn file_has_correct_size(path: &std::path::Path, expected: Option<u64>) -> bool {
    let Some(expected) = expected else {
        return false;
    };
    tokio::fs::metadata(path)
        .await
        .map(|m| m.len() == expected)
        .unwrap_or(false)
}

async fn file_has_valid_chunks(
    path: &std::path::Path,
    expected_size: Option<u64>,
    chunks: &[ManifestChunk],
) -> Result<bool, BoxError> {
    if !file_has_correct_size(path, expected_size).await {
        return Ok(false);
    }
    let bad = verify_chunks(path, chunks).await?;
    Ok(bad.is_empty())
}

/// Returns the list of chunk indices that need to be re-downloaded.
async fn verify_chunks(
    path: &std::path::Path,
    chunks: &[ManifestChunk],
) -> Result<Vec<usize>, BoxError> {
    use steam::util::checksum::SteamAdler32;
    use tokio::io::AsyncReadExt;
    use tokio::io::AsyncSeekExt;

    let mut file = tokio::fs::File::open(path).await?;
    let mut bad_indices = Vec::new();

    for (i, chunk) in chunks.iter().enumerate() {
        let (offset, size, expected_checksum) =
            match (chunk.offset, chunk.uncompressed_size, chunk.checksum) {
                (Some(o), Some(s), Some(c)) => (o, s, c),
                _ => {
                    bad_indices.push(i);
                    continue;
                }
            };

        file.seek(std::io::SeekFrom::Start(offset)).await?;
        let mut buf = vec![0u8; size as usize];
        if file.read_exact(&mut buf).await.is_err() {
            bad_indices.push(i);
            continue;
        }

        let actual = SteamAdler32::compute(&buf);
        if actual != SteamAdler32(expected_checksum) {
            bad_indices.push(i);
        }
    }

    Ok(bad_indices)
}

/// EDepotFileFlag::Executable = 0x04
/// Set Unix executable permissions if the file has the Executable flag.
fn set_executable_if_needed(task: &FileTask) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        if task
            .flags
            .is_some_and(|f| steam::enums::DepotFileFlags(f).is_executable())
        {
            if let Ok(metadata) = std::fs::metadata(&task.path) {
                let mut perms = metadata.permissions();
                let mode = perms.mode();
                // Add execute bits for user/group/other
                perms.set_mode(mode | 0o111);
                if let Err(e) = std::fs::set_permissions(&task.path, perms) {
                    tracing::warn!(
                        "Failed to set executable permissions on {}: {e}",
                        task.filename
                    );
                }
            }
        }
    }

    #[cfg(not(unix))]
    {
        let _ = task; // suppress unused warning
    }
}

/// Per-file download context.
#[allow(dead_code)] // flags is only read on Unix
struct FileTask {
    path: PathBuf,
    size: Option<u64>,
    flags: Option<u32>,
    chunks: Vec<ManifestChunk>,
    /// Chunks from the previously installed manifest (for delta comparison).
    old_chunks: Option<Vec<ManifestChunk>>,
    filename: String,
}

/// Builder for [`DepotJob`].
#[derive(Default)]
pub struct DepotJobBuilder {
    fetcher: Option<Arc<dyn ChunkFetcher>>,
    depot_id: Option<DepotId>,
    depot_key: Option<DepotKey>,
    install_dir: Option<PathBuf>,
    max_downloads: Option<usize>,
    event_tx: Option<mpsc::UnboundedSender<DownloadEvent>>,
    verify: bool,
    file_filter: Option<FileFilter>,
    previous_manifest: Option<DepotManifest>,
    retry: RetryConfig,
}

impl DepotJobBuilder {
    pub fn cdn(self, cdn: CdnClient, server: CdnServer, cdn_auth_token: Option<String>) -> Self {
        self.fetcher(Arc::new(CdnChunkFetcher {
            cdn,
            server,
            cdn_auth_token,
        }))
    }

    pub fn fetcher(mut self, fetcher: Arc<dyn ChunkFetcher>) -> Self {
        self.fetcher = Some(fetcher);
        self
    }

    pub fn depot_id(mut self, id: DepotId) -> Self {
        self.depot_id = Some(id);
        self
    }

    pub fn depot_key(mut self, key: DepotKey) -> Self {
        self.depot_key = Some(key);
        self
    }

    pub fn install_dir(mut self, dir: PathBuf) -> Self {
        self.install_dir = Some(dir);
        self
    }

    pub fn max_downloads(mut self, n: usize) -> Self {
        self.max_downloads = Some(n);
        self
    }

    pub fn event_sender(mut self, tx: mpsc::UnboundedSender<DownloadEvent>) -> Self {
        self.event_tx = Some(tx);
        self
    }

    pub fn verify(mut self, verify: bool) -> Self {
        self.verify = verify;
        self
    }

    pub fn file_filter(mut self, filter: Option<FileFilter>) -> Self {
        self.file_filter = filter;
        self
    }

    pub fn previous_manifest(mut self, manifest: Option<DepotManifest>) -> Self {
        self.previous_manifest = manifest;
        self
    }

    pub fn retry(mut self, config: RetryConfig) -> Self {
        self.retry = config;
        self
    }

    pub fn build(self) -> Result<DepotJob, &'static str> {
        Ok(DepotJob {
            fetcher: self
                .fetcher
                .ok_or("fetcher is required (use .cdn() or .fetcher())")?,
            depot_id: self.depot_id.ok_or("depot_id is required")?,
            depot_key: self.depot_key.ok_or("depot_key is required")?,
            install_dir: self.install_dir.ok_or("install_dir is required")?,
            semaphore: Arc::new(Semaphore::new(self.max_downloads.unwrap_or(8))),
            event_tx: self.event_tx.ok_or("event_sender is required")?,
            verify: self.verify,
            file_filter: self.file_filter,
            previous_manifest: self.previous_manifest,
            retry: self.retry,
        })
    }
}
