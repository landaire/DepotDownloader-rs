use steam::depot::{ChunkId, DepotId};

/// Status events emitted during a depot download.
///
/// Consumers (CLI, GUI, etc.) subscribe to these and render
/// progress however they like.
#[derive(Debug, Clone)]
pub enum DownloadEvent {
    /// A new file download has started.
    FileStarted {
        depot_id: DepotId,
        filename: String,
        total_chunks: usize,
        file_size: Option<u64>,
    },

    /// A chunk has been successfully downloaded and written.
    ChunkCompleted {
        depot_id: DepotId,
        chunk_id: ChunkId,
        bytes_written: usize,
    },

    /// A file download has completed (all chunks written).
    FileCompleted {
        depot_id: DepotId,
        filename: String,
    },

    /// A file was skipped (e.g., directory entry, filtered out).
    FileSkipped {
        depot_id: DepotId,
        filename: String,
    },

    /// An error occurred downloading a chunk (may be retried).
    ChunkFailed {
        depot_id: DepotId,
        chunk_id: ChunkId,
        error: String,
    },

    /// Overall depot download progress summary.
    DepotProgress {
        depot_id: DepotId,
        files_completed: usize,
        files_total: usize,
    },
}
