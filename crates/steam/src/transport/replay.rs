//! Replay transport - feeds recorded packets back for testing.

use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use async_trait::async_trait;
use bytes::Bytes;

use super::Transport;
use super::capture::CaptureFile;
use crate::error::ConnectionError;
use crate::error::Error;

/// A transport that replays packets from a capture file.
///
/// Sends are silently discarded. Receives return packets in order.
pub struct ReplayTransport {
    packets: Vec<Vec<u8>>,
    cursor: AtomicUsize,
}

impl ReplayTransport {
    /// Create from a capture file.
    pub fn from_capture(capture: &CaptureFile) -> Result<Self, base64::DecodeError> {
        let mut packets = Vec::with_capacity(capture.packets.len());
        for pkt in &capture.packets {
            packets.push(pkt.decode_payload()?);
        }
        Ok(Self {
            packets,
            cursor: AtomicUsize::new(0),
        })
    }

    /// Create from a capture file on disk.
    pub fn from_file(path: &std::path::Path) -> Result<Self, Error> {
        let capture = CaptureFile::load(path).map_err(Error::Io)?;
        Self::from_capture(&capture).map_err(|e| Error::Io(std::io::Error::other(e)))
    }
}

#[async_trait]
impl Transport for ReplayTransport {
    async fn send(&self, _payload: &[u8]) -> Result<(), Error> {
        // Sends are discarded during replay
        Ok(())
    }

    async fn recv(&self) -> Result<Bytes, Error> {
        let idx = self.cursor.fetch_add(1, Ordering::Relaxed);
        self.packets
            .get(idx)
            .map(|p| Bytes::copy_from_slice(p))
            .ok_or_else(|| ConnectionError::Disconnected.into())
    }
}
