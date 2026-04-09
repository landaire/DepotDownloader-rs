pub mod encryption;
pub mod framing;

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

/// A Steam CM server endpoint.
#[derive(Debug, Clone, Copy)]
pub struct CmServer {
    pub addr: SocketAddr,
    pub protocol: Protocol,
}

/// Connection protocol to a CM server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    Tcp,
    WebSocket,
}

/// Default CM servers (fallback when directory API is unavailable).
pub static DEFAULT_CM_SERVERS: &[CmServer] = &[
    CmServer::tcp(155, 133, 246, 50, 27017),
    CmServer::tcp(155, 133, 246, 51, 27017),
    CmServer::tcp(162, 254, 197, 40, 27017),
    CmServer::tcp(162, 254, 197, 41, 27017),
    CmServer::tcp(162, 254, 197, 42, 27017),
    CmServer::tcp(185, 25, 180, 14, 27017),
    CmServer::tcp(185, 25, 180, 15, 27017),
];

impl CmServer {
    const fn tcp(a: u8, b: u8, c: u8, d: u8, port: u16) -> Self {
        Self {
            addr: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(a, b, c, d), port)),
            protocol: Protocol::Tcp,
        }
    }
}
