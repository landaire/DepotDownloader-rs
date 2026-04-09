//! Network transport abstraction.
//!
//! Allows swapping the underlying I/O for recording and replay:
//!
//! - [`TcpTransport`] - real TCP connection (production)
//! - [`RecordingTransport`] - wraps another transport, records incoming packets to disk
//! - [`ReplayTransport`] - feeds recorded packets back for testing

pub mod capture;
pub mod recording;
pub mod replay;
pub mod tcp;

use async_trait::async_trait;
use bytes::Bytes;

/// A transport that can send and receive raw VT01 frame payloads.
///
/// Implementations handle framing internally - callers deal with
/// payloads only, not length/magic headers.
#[async_trait]
pub trait Transport: Send + Sync + 'static {
    /// Send a raw payload (will be wrapped in a VT01 frame on the wire).
    async fn send(&self, payload: &[u8]) -> Result<(), crate::error::Error>;

    /// Receive a raw payload (VT01 frame already stripped).
    async fn recv(&self) -> Result<Bytes, crate::error::Error>;
}

/// Blanket impl so `Box<dyn Transport>` is itself a Transport.
#[async_trait]
impl Transport for Box<dyn Transport> {
    async fn send(&self, payload: &[u8]) -> Result<(), crate::error::Error> {
        (**self).send(payload).await
    }

    async fn recv(&self) -> Result<Bytes, crate::error::Error> {
        (**self).recv().await
    }
}
