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
}

impl DepotJob {
    pub fn builder() -> DepotJobBuilder {
        DepotJobBuilder::default()
    }

    /// Download all files from a manifest.
    pub async fn download(&self, manifest: &DepotManifest) -> Result<(), BoxError> {
        let files_total = manifest.files.len();
        let mut files_completed = 0usize;
        let mut handles = Vec::new();

        for file in &manifest.files {
            let filename = match &file.filename {
                Some(name) => name.clone(),
                None => continue,
            };

            // Skip directories (flag 0x40 = directory)
            if file.flags.is_some_and(|f| f & 0x40 != 0) {
                let _ = self.event_tx.send(DownloadEvent::FileSkipped {
                    depot_id: self.depot_id,
                    filename,
                });
                files_completed += 1;
                let _ = self.event_tx.send(DownloadEvent::DepotProgress {
                    depot_id: self.depot_id,
                    files_completed,
                    files_total,
                });
                continue;
            }

            let task = FileTask {
                path: self.install_dir.join(&filename),
                size: file.size,
                chunks: file.chunks.clone(),
                filename: filename.clone(),
            };

            let _ = self.event_tx.send(DownloadEvent::FileStarted {
                depot_id: self.depot_id,
                filename,
                total_chunks: task.chunks.len(),
                file_size: file.size,
            });

            let job = self.clone_shared();
            let handle = tokio::spawn(async move {
                let result = job.download_file(&task).await;
                if result.is_ok() {
                    let _ = job.event_tx.send(DownloadEvent::FileCompleted {
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
            let _ = self.event_tx.send(DownloadEvent::DepotProgress {
                depot_id: self.depot_id,
                files_completed,
                files_total,
            });
        }

        Ok(())
    }

    /// Download a single file by fetching all its chunks.
    async fn download_file(&self, task: &FileTask) -> Result<(), BoxError> {
        if let Some(parent) = task.path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let file = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&task.path)
            .await?;

        if let Some(size) = task.size {
            file.set_len(size).await?;
        }

        let file = Arc::new(tokio::sync::Mutex::new(file));
        let mut handles = Vec::new();

        for chunk in &task.chunks {
            let chunk_id = match &chunk.id {
                Some(id) => *id,
                None => continue,
            };

            let job = self.clone_shared();
            let offset = chunk.offset;
            let expected_size = chunk.uncompressed_size;
            let expected_checksum = chunk.checksum;
            let file = file.clone();

            let handle = tokio::spawn(async move {
                let _permit = job.semaphore.acquire().await?;

                let raw = job
                    .cdn
                    .download_chunk(
                        &job.server,
                        job.depot_id,
                        &chunk_id,
                        job.cdn_auth_token.as_deref(),
                    )
                    .await?;

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

                let _ = job.event_tx.send(DownloadEvent::ChunkCompleted {
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
        }
    }
}

/// Per-file download context.
struct FileTask {
    path: PathBuf,
    size: Option<u64>,
    chunks: Vec<ManifestChunk>,
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
        })
    }
}
