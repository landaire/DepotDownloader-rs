//! Capture file format for recorded incoming server packets.
//!
//! A capture is a JSON array of packets received from the server,
//! each with a sequence number and base64-encoded payload.

use serde::{Deserialize, Serialize};

/// A single captured incoming packet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapturedPacket {
    /// Sequence number (0-indexed, monotonic).
    pub seq: u32,
    /// Raw EMsg value from the first 4 bytes (for readability in the file).
    /// Protobuf flag stripped.
    pub emsg: Option<u32>,
    /// Base64-encoded raw payload bytes (the VT01 frame payload, before decryption).
    pub payload_b64: String,
}

impl CapturedPacket {
    pub fn new(seq: u32, payload: &[u8]) -> Self {
        use base64::Engine;

        let emsg = if payload.len() >= 4 {
            let raw = u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]);
            Some(raw & 0x7FFF_FFFF)
        } else {
            None
        };

        Self {
            seq,
            emsg,
            payload_b64: base64::engine::general_purpose::STANDARD.encode(payload),
        }
    }

    pub fn decode_payload(&self) -> Result<Vec<u8>, base64::DecodeError> {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.decode(&self.payload_b64)
    }
}

/// A full capture of incoming server packets for a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureFile {
    /// Human-readable description of what this capture contains.
    pub description: String,
    /// Incoming packets in the order they were received.
    pub packets: Vec<CapturedPacket>,
}

impl CaptureFile {
    pub fn new(description: impl Into<String>) -> Self {
        Self {
            description: description.into(),
            packets: Vec::new(),
        }
    }

    pub fn save(&self, path: &std::path::Path) -> Result<(), std::io::Error> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(path, json)
    }

    pub fn load(path: &std::path::Path) -> Result<Self, std::io::Error> {
        let json = std::fs::read_to_string(path)?;
        serde_json::from_str(&json)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }
}
