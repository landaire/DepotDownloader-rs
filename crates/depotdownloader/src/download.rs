use indicatif::ProgressBar;
use indicatif::ProgressStyle;
use steam_client::event::DownloadEvent;
use tokio::sync::mpsc;

/// Spawn a task that renders download progress from a stream of events.
///
/// Returns a handle that completes when all events are consumed.
pub fn spawn_progress_renderer(
    mut rx: mpsc::UnboundedReceiver<DownloadEvent>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut bar: Option<ProgressBar> = None;

        while let Some(event) = rx.recv().await {
            match event {
                DownloadEvent::DepotProgress {
                    files_completed,
                    files_total,
                    ..
                } => {
                    let b = bar.get_or_insert_with(|| {
                        let b = ProgressBar::new(files_total as u64);
                        b.set_style(
                            ProgressStyle::default_bar()
                                .template("[{elapsed_precise}] {bar:40} {pos}/{len} files")
                                .expect("valid template"),
                        );
                        b
                    });
                    b.set_position(files_completed as u64);
                }
                DownloadEvent::FileCompleted { filename, .. } => {
                    if let Some(b) = &bar {
                        b.println(format!("  {filename}"));
                    }
                }
                DownloadEvent::ChunkFailed {
                    chunk_id, error, ..
                } => {
                    if let Some(b) = &bar {
                        b.println(format!("  WARN: chunk {chunk_id} failed: {error}"));
                    }
                }
                _ => {}
            }
        }

        if let Some(b) = bar {
            b.finish_with_message("Download complete");
        }
    })
}
