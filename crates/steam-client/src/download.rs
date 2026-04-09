use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::{Semaphore, mpsc};

use steam::cdn::CdnClient;
use steam::cdn::server::CdnServer;
use steam::depot::chunk::process_chunk;
use steam::depot::manifest::{DepotManifest, ManifestChunk};
use steam::depot::{DepotId, DepotKey};

use crate::event::DownloadEvent;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// Send an event, logging if the receiver has gone away.
macro_rules! send_event {
    ($tx:expr, $event:expr) => {
        if $tx.send($event).is_err() {
            tracing::trace!("Event receiver dropped");
        }
    };
}

/// A depot download job. Carries all shared context needed to download
/// chunks from a single depot.
pub struct DepotJob {
    cdn: CdnClient,
    server: CdnServer,
    depot_id: DepotId,
    depot_key: DepotKey,
    cdn_auth_token: Option<String>,
    install_dir: PathBuf,
    semaphore: Arc<Semaphore>,
    event_tx: mpsc::UnboundedSender<DownloadEvent>,
    /// When true, verify existing files and only re-download corrupted chunks.
    verify: bool,
    /// File filter - only download files matching this filter.
    file_filter: Option<FileFilter>,
    /// Previous manifest for delta downloads.
    previous_manifest: Option<DepotManifest>,
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
            Self::FileList(list) => list
                .iter()
                .any(|f| f.eq_ignore_ascii_case(&normalized)),
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
            if let Some(ref filter) = self.file_filter {
                if !filter.matches(&filename) {
                    send_event!(self.event_tx, DownloadEvent::FileSkipped {
                        depot_id: self.depot_id,
                        filename,
                    });
                    files_completed += 1;
                    continue;
                }
            }

            // Skip directories (flag 0x40 = directory)
            if file.flags.is_some_and(|f| f & 0x40 != 0) {
                send_event!(self.event_tx, DownloadEvent::FileSkipped {
                    depot_id: self.depot_id,
                    filename,
                });
                files_completed += 1;
                send_event!(self.event_tx, DownloadEvent::DepotProgress {
                    depot_id: self.depot_id,
                    files_completed,
                    files_total,
                });
                continue;
            }

            // Find matching file in previous manifest for delta comparison
            let old_chunks: Option<Vec<ManifestChunk>> = self.previous_manifest.as_ref()
                .and_then(|old| {
                    old.files.iter()
                        .find(|f| f.filename.as_deref() == Some(&filename))
                        .map(|f| f.chunks.clone())
                });

            let task = FileTask {
                path: self.install_dir.join(&filename),
                size: file.size,
                flags: file.flags,
                chunks: file.chunks.clone(),
                old_chunks,
                filename: filename.clone(),
            };

            send_event!(self.event_tx, DownloadEvent::FileStarted {
                depot_id: self.depot_id,
                filename,
                total_chunks: task.chunks.len(),
                file_size: file.size,
            });

            let job = self.clone_shared();
            let handle = tokio::spawn(async move {
                let result = job.download_file(&task).await;
                if result.is_ok() {
                    // Set executable permissions on Unix
                    set_executable_if_needed(&task);

                    send_event!(job.event_tx, DownloadEvent::FileCompleted {
                        depot_id: job.depot_id,
                        filename: task.filename.clone(),
                    });
                }
                result
            });

            handles.push((handle, files_completed));
        }

        for (handle, _) in handles {
            handle.await??;
            files_completed += 1;
            send_event!(self.event_tx, DownloadEvent::DepotProgress {
                depot_id: self.depot_id,
                files_completed,
                files_total,
            });
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
        // If verifying and the file exists with correct size, check chunks
        if self.verify && task.path.exists() {
            if let Some(expected_size) = task.size {
                if let Ok(meta) = tokio::fs::metadata(&task.path).await {
                    if meta.len() == expected_size {
                        let needs_download = verify_chunks(&task.path, &task.chunks).await?;
                        if needs_download.is_empty() {
                            tracing::debug!("{}: all chunks valid, skipping", task.filename);
                            return Ok(());
                        }
                        tracing::info!(
                            "{}: {} of {} chunks need re-download",
                            task.filename,
                            needs_download.len(),
                            task.chunks.len()
                        );
                        // TODO: only download the bad chunks instead of the whole file
                    }
                }
            }
        }

        // Create staging path: .staging/<filename>
        let staging_dir = self.install_dir.join(".staging");
        let staging_path = staging_dir.join(&task.filename);

        if let Some(parent) = staging_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        if let Some(parent) = task.path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let file = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&staging_path)
            .await?;

        if let Some(size) = task.size {
            file.set_len(size).await?;
        }

        let file = Arc::new(tokio::sync::Mutex::new(file));

        // If we have a previous manifest, copy unchanged chunks from the old file
        if let Some(ref old_chunks) = task.old_chunks {
            if task.path.exists() {
                let mut copied = 0usize;
                let old_file = tokio::fs::File::open(&task.path).await?;
                let old_file = Arc::new(tokio::sync::Mutex::new(old_file));

                for chunk in &task.chunks {
                    let chunk_id = match &chunk.id {
                        Some(id) => id,
                        None => continue,
                    };

                    // Find matching chunk in old manifest by ID
                    let old_match = old_chunks.iter().find(|oc| oc.id.as_ref() == Some(chunk_id));

                    if let Some(old_chunk) = old_match {
                        if let (Some(old_offset), Some(new_offset), Some(size)) =
                            (old_chunk.offset, chunk.offset, chunk.uncompressed_size)
                        {
                            use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

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
                }

                if copied > 0 {
                    tracing::debug!("{}: reused {copied} of {} chunks from previous version", task.filename, task.chunks.len());
                }
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

            // Skip chunks that were already copied from the previous version
            if let Some(ref old_chunks) = task.old_chunks {
                if task.path.exists() && old_chunks.iter().any(|oc| oc.id == Some(chunk_id)) {
                    continue;
                }
            }

            let job = self.clone_shared();
            let offset = chunk.offset;
            let expected_size = chunk.uncompressed_size;
            let expected_checksum = chunk.checksum;
            let file = file.clone();

            let handle = tokio::spawn(async move {
                let _permit = job.semaphore.acquire().await?;
                tracing::debug!("Fetching chunk {chunk_id}");

                let raw = match job
                    .cdn
                    .download_chunk(
                        &job.server,
                        job.depot_id,
                        &chunk_id,
                        job.cdn_auth_token.as_deref(),
                    )
                    .await
                {
                    Ok(data) => {
                        tracing::debug!("Downloaded chunk {chunk_id} ({} bytes)", data.len());
                        data
                    }
                    Err(e) => {
                        tracing::error!("Failed to download chunk {chunk_id}: {e}");
                        return Err(e.into());
                    }
                };

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
                    use tokio::io::{AsyncSeekExt, AsyncWriteExt};
                    let mut f = file.lock().await;
                    f.seek(std::io::SeekFrom::Start(off)).await?;
                    f.write_all(&decompressed).await?;
                }

                send_event!(job.event_tx, DownloadEvent::ChunkCompleted {
                    depot_id: job.depot_id,
                    chunk_id,
                    bytes_written,
                });

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
            cdn: self.cdn.clone(),
            server: self.server.clone(),
            depot_id: self.depot_id,
            depot_key: self.depot_key.clone(),
            cdn_auth_token: self.cdn_auth_token.clone(),
            install_dir: self.install_dir.clone(),
            semaphore: self.semaphore.clone(),
            event_tx: self.event_tx.clone(),
            verify: self.verify,
            file_filter: None, // filter only needed at the top-level download loop
            previous_manifest: None,
        }
    }
}

/// Verify which chunks in an existing file are corrupted.
///
/// Returns the list of chunks that need to be re-downloaded.
async fn verify_chunks(
    path: &std::path::Path,
    chunks: &[ManifestChunk],
) -> Result<Vec<usize>, BoxError> {
    use steam::util::checksum::SteamAdler32;
    use tokio::io::{AsyncReadExt, AsyncSeekExt};

    let mut file = tokio::fs::File::open(path).await?;
    let mut bad_indices = Vec::new();

    for (i, chunk) in chunks.iter().enumerate() {
        let (offset, size, expected_checksum) = match (chunk.offset, chunk.uncompressed_size, chunk.checksum) {
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
#[cfg(unix)]
const FLAG_EXECUTABLE: u32 = 0x04;

/// Set Unix executable permissions if the file has the Executable flag.
fn set_executable_if_needed(task: &FileTask) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        if task.flags.is_some_and(|f| f & FLAG_EXECUTABLE != 0) {
            if let Ok(metadata) = std::fs::metadata(&task.path) {
                let mut perms = metadata.permissions();
                let mode = perms.mode();
                // Add execute bits for user/group/other
                perms.set_mode(mode | 0o111);
                if let Err(e) = std::fs::set_permissions(&task.path, perms) {
                    tracing::warn!("Failed to set executable permissions on {}: {e}", task.filename);
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
    cdn: Option<CdnClient>,
    server: Option<CdnServer>,
    depot_id: Option<DepotId>,
    depot_key: Option<DepotKey>,
    cdn_auth_token: Option<String>,
    install_dir: Option<PathBuf>,
    max_downloads: Option<usize>,
    event_tx: Option<mpsc::UnboundedSender<DownloadEvent>>,
    verify: bool,
    file_filter: Option<FileFilter>,
    previous_manifest: Option<DepotManifest>,
}

impl DepotJobBuilder {
    pub fn cdn(mut self, cdn: CdnClient) -> Self {
        self.cdn = Some(cdn);
        self
    }

    pub fn server(mut self, server: CdnServer) -> Self {
        self.server = Some(server);
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

    pub fn cdn_auth_token(mut self, token: Option<String>) -> Self {
        self.cdn_auth_token = token;
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

    pub fn build(self) -> Result<DepotJob, &'static str> {
        Ok(DepotJob {
            cdn: self.cdn.ok_or("cdn is required")?,
            server: self.server.ok_or("server is required")?,
            depot_id: self.depot_id.ok_or("depot_id is required")?,
            depot_key: self.depot_key.ok_or("depot_key is required")?,
            cdn_auth_token: self.cdn_auth_token,
            install_dir: self.install_dir.ok_or("install_dir is required")?,
            semaphore: Arc::new(Semaphore::new(self.max_downloads.unwrap_or(8))),
            event_tx: self.event_tx.ok_or("event_sender is required")?,
            verify: self.verify,
            file_filter: self.file_filter,
            previous_manifest: self.previous_manifest,
        })
    }
}
