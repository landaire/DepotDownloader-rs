//! TCP transport - the default production transport.

use async_trait::async_trait;
use bytes::Bytes;
use tokio::net::TcpStream;
use tokio::sync::Mutex;

use super::Transport;
use crate::connection::CmServer;
use crate::connection::CmServerAddr;
use crate::connection::framing;
use crate::error::ConnectionError;
use crate::error::Error;

/// A transport backed by a real TCP connection.
pub struct TcpTransport {
    stream: Mutex<TcpStream>,
}

impl TcpTransport {
    pub fn new(stream: TcpStream) -> Self {
        Self {
            stream: Mutex::new(stream),
        }
    }

    /// Connect to a CM server, resolving DNS if needed.
    pub async fn connect(server: &CmServer) -> Result<Self, Error> {
        use tokio::net::lookup_host;

        let stream = match &server.addr {
            CmServerAddr::Resolved(addr) => TcpStream::connect(addr).await?,
            CmServerAddr::Dns { host, port } => {
                let addr = lookup_host(format!("{host}:{port}"))
                    .await?
                    .next()
                    .ok_or_else(|| ConnectionError::DnsResolutionFailed { host: host.clone() })?;
                TcpStream::connect(addr).await?
            }
        };
        stream.set_nodelay(true)?;

        Ok(Self::new(stream))
    }
}

#[async_trait]
impl Transport for TcpTransport {
    async fn send(&self, payload: &[u8]) -> Result<(), Error> {
        use tokio::io::AsyncWriteExt;

        let frame = framing::frame_bytes(payload);
        let mut stream = self.stream.lock().await;
        stream.write_all(&frame).await?;
        Ok(())
    }

    async fn recv(&self) -> Result<Bytes, Error> {
        use tokio::io::AsyncReadExt;

        let mut stream = self.stream.lock().await;

        let mut header = [0u8; 8];
        stream.read_exact(&mut header).await?;

        let len = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
        let magic = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);

        if magic != framing::MAGIC {
            return Err(ConnectionError::BadMagic {
                expected: framing::MAGIC,
                actual: magic,
            }
            .into());
        }

        let mut payload = vec![0u8; len as usize];
        stream.read_exact(&mut payload).await?;

        Ok(Bytes::from(payload))
    }
}
