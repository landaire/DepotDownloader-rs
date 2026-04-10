use crate::messages::EMsg;
use thiserror::Error;

/// Top-level error type for the steam crate.
#[derive(Debug, Error)]
pub enum Error {
    #[error("connection error: {0}")]
    Connection(#[from] ConnectionError),

    #[error("crypto error: {0}")]
    Crypto(#[from] CryptoError),

    #[error("protobuf decode error: {0}")]
    ProtobufDecode(#[from] prost::DecodeError),

    #[error("manifest error: {0}")]
    Manifest(#[from] ManifestError),

    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Error)]
pub enum ConnectionError {
    #[error("unexpected EMsg: expected {expected}, got {actual}")]
    UnexpectedEMsg { expected: EMsg, actual: EMsg },

    #[error("unexpected magic: expected 0x{expected:08X}, got 0x{actual:08X}")]
    BadMagic { expected: u32, actual: u32 },

    #[error("packet too short: need {need} bytes, got {got}")]
    PacketTooShort { need: usize, got: usize },

    #[error("DNS resolution failed for {host}")]
    DnsResolutionFailed { host: String },

    #[error("encryption handshake failed: {0}")]
    EncryptionFailed(#[from] crate::enums::EResultError),

    #[error("logon failed: {0}")]
    LogonFailed(crate::enums::EResultError),

    #[error("service method failed: {0}")]
    ServiceMethodFailed(crate::enums::EResultError),

    #[error("access denied for depot {depot_id}: {error}")]
    DepotAccessDenied {
        depot_id: u32,
        error: crate::enums::EResultError,
    },

    #[error("parse error: {0}")]
    Parse(#[from] ParseError),

    #[error("disconnected")]
    Disconnected,

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Errors from binary parsing (winnow / byteorder).
#[derive(Debug, Error)]
pub enum ParseError {
    #[error("unexpected EOF")]
    UnexpectedEof,

    #[error("invalid protobuf header")]
    InvalidProtobufHeader,

    #[error("winnow parse error")]
    Winnow,
}

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("invalid key length: expected {expected}, got {actual}")]
    InvalidKeyLength { expected: usize, actual: usize },

    #[error("decryption failed")]
    DecryptionFailed,

    #[error("invalid padding")]
    InvalidPadding,

    #[error("rsa error: {0}")]
    Rsa(#[from] rsa::Error),
}

#[derive(Debug, Error)]
pub enum ManifestError {
    #[error("invalid manifest magic: 0x{0:08X}")]
    InvalidMagic(u32),

    #[error("manifest section missing: {0}")]
    MissingSection(&'static str),

    #[error("checksum mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },
}

pub type Result<T> = std::result::Result<T, Error>;
