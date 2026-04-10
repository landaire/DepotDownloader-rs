//! Recording transport - wraps a real transport and captures incoming packets to disk.

use std::path::PathBuf;
use std::sync::atomic::AtomicU32;
use std::sync::atomic::Ordering;

use async_trait::async_trait;
use bytes::Bytes;
use tokio::sync::Mutex;

use super::Transport;
use super::capture::CaptureFile;
use super::capture::CapturedPacket;

/// A transport that records all incoming packets passing through it.
///
/// Call [`flush`] to write the capture file to disk.
pub struct RecordingTransport<T: Transport> {
    inner: T,
    capture: Mutex<CaptureFile>,
    seq: AtomicU32,
    output_path: PathBuf,
}

impl<T: Transport> RecordingTransport<T> {
    pub fn new(inner: T, output_path: PathBuf, description: impl Into<String>) -> Self {
        Self {
            inner,
            capture: Mutex::new(CaptureFile::new(description)),
            seq: AtomicU32::new(0),
            output_path,
        }
    }

    pub async fn flush(&self) -> Result<(), std::io::Error> {
        let capture = self.capture.lock().await;
        capture.save(&self.output_path)
    }
}

impl<T: Transport> Drop for RecordingTransport<T> {
    fn drop(&mut self) {
        // Best-effort sync flush on drop
        let capture = self.capture.get_mut();
        if capture.packets.is_empty() {
            return;
        }
        if let Err(e) = capture.save(&self.output_path) {
            tracing::warn!("Failed to save capture to {:?}: {e}", self.output_path);
        } else {
            tracing::info!(
                "Saved {} captured packets to {:?}",
                capture.packets.len(),
                self.output_path,
            );
        }
    }
}

#[async_trait]
impl<T: Transport> Transport for RecordingTransport<T> {
    async fn send(&self, payload: &[u8]) -> Result<(), crate::error::Error> {
        self.inner.send(payload).await
    }

    async fn recv(&self) -> Result<Bytes, crate::error::Error> {
        let payload = self.inner.recv().await?;
        let seq = self.seq.fetch_add(1, Ordering::Relaxed);
        {
            let mut capture = self.capture.lock().await;
            capture.packets.push(CapturedPacket::new(seq, &payload));
        }
        Ok(payload)
    }
}
