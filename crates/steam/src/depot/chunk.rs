//! Chunk processing: decrypt → decompress → verify.

use crate::depot::DepotKey;
use crate::error::CryptoError;
use crate::util::checksum::SteamAdler32;

/// Compression format of a decrypted chunk, detected from magic bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkCompression {
    /// Valve Zstandard: `VSZa` header + CRC32 + zstd data + 15-byte footer.
    VZstd,
    /// Valve LZMA (VZip): 7-byte header + 5-byte LZMA props + data + 10-byte footer.
    Lzma,
    /// PKzip: standard ZIP archive containing one entry.
    Zip,
    /// No compression — raw data.
    None,
}

impl ChunkCompression {
    /// Detect the compression format from the first few bytes.
    pub fn detect(data: &[u8]) -> Self {
        const VZSTD_MAGIC: &[u8] = b"VSZa";
        const ZIP_MAGIC: &[u8] = b"PK\x03\x04";
        const LZMA_MAGIC: &[u8] = b"VZa";

        if data.len() >= 4 && data[..4] == *VZSTD_MAGIC {
            Self::VZstd
        } else if data.len() >= 4 && data[..4] == *ZIP_MAGIC {
            Self::Zip
        } else if data.len() >= 3 && data[..3] == *LZMA_MAGIC {
            Self::Lzma
        } else {
            Self::None
        }
    }
}

/// Process a raw chunk: decrypt, decompress, and verify.
///
/// The chunk processing order is:
/// 1. **Decrypt** with AES-256 (ECB-encrypted IV + CBC payload)
/// 2. **Decompress** based on magic bytes (VZstd, LZMA, or ZIP)
/// 3. **Verify** Adler32 checksum and expected length
pub fn process_chunk(
    data: &[u8],
    depot_key: &DepotKey,
    expected_size: u32,
    expected_checksum: u32,
) -> Result<Vec<u8>, ChunkError> {
    let decrypted =
        crate::crypto::symmetric_decrypt_ecb(&depot_key.0, data).map_err(ChunkError::Crypto)?;

    let decompressed = decompress(&decrypted)?;

    if decompressed.len() != expected_size as usize {
        return Err(ChunkError::SizeMismatch {
            expected: expected_size as usize,
            actual: decompressed.len(),
        });
    }

    let expected = SteamAdler32(expected_checksum);
    let actual = SteamAdler32::compute(&decompressed);
    if actual != expected {
        return Err(ChunkError::ChecksumMismatch { expected, actual });
    }

    Ok(decompressed)
}

/// Decompress a decrypted chunk based on its detected format.
fn decompress(data: &[u8]) -> Result<Vec<u8>, ChunkError> {
    match ChunkCompression::detect(data) {
        ChunkCompression::VZstd => decompress_vzstd(data),
        ChunkCompression::Lzma => decompress_vzip(data),
        ChunkCompression::Zip => decompress_zip(data),
        ChunkCompression::None => Ok(data.to_vec()),
    }
}

/// Decompress Valve's VZstd format.
///
/// Layout:
/// ```text
/// [4 bytes] magic "VSZa"
/// [4 bytes] CRC32 of decompressed data
/// [N bytes] zstd compressed data
/// [15 bytes] footer (CRC32 + decompressed size + "zsv")
/// ```
fn decompress_vzstd(data: &[u8]) -> Result<Vec<u8>, ChunkError> {
    const HEADER_LEN: usize = 8;
    const FOOTER_LEN: usize = 15;

    if data.len() < HEADER_LEN + FOOTER_LEN {
        return Err(ChunkError::TooShort);
    }

    let zstd_data = &data[HEADER_LEN..data.len() - FOOTER_LEN];
    zstd::stream::decode_all(zstd_data).map_err(ChunkError::Io)
}

/// Decompress Valve's VZip (LZMA) format.
///
/// Layout:
/// ```text
/// [2 bytes] magic "VZ"
/// [1 byte]  version 'a'
/// [4 bytes] timestamp/CRC (ignored)
/// [1 byte]  LZMA property bits
/// [4 bytes] LZMA dictionary size (LE)
/// [N bytes] LZMA compressed data
/// [10 bytes] footer (CRC32 + decompressed size + "vz")
/// ```
///
/// The LZMA decoder expects: `[properties byte] [dict size: 4 bytes LE] [uncompressed size: 8 bytes LE] [data]`
fn decompress_vzip(data: &[u8]) -> Result<Vec<u8>, ChunkError> {
    const HEADER_LEN: usize = 7;
    const LZMA_PROPS_LEN: usize = 5; // 1 byte props + 4 bytes dict
    const FOOTER_LEN: usize = 10;

    if data.len() < HEADER_LEN + LZMA_PROPS_LEN + FOOTER_LEN {
        return Err(ChunkError::TooShort);
    }

    let lzma_props = &data[HEADER_LEN..HEADER_LEN + LZMA_PROPS_LEN]; // props byte + dict size
    let compressed = &data[HEADER_LEN + LZMA_PROPS_LEN..data.len() - FOOTER_LEN];

    // Read decompressed size from footer (4 bytes LE at offset end-6)
    let footer = &data[data.len() - FOOTER_LEN..];
    let decompressed_size = u32::from_le_bytes([footer[4], footer[5], footer[6], footer[7]]) as u64;

    // Build the LZMA stream header that lzma-rs expects:
    // [5 bytes props+dict] [8 bytes uncompressed size LE] [compressed data]
    let mut lzma_stream = Vec::with_capacity(13 + compressed.len());
    lzma_stream.extend_from_slice(lzma_props);
    lzma_stream.extend_from_slice(&decompressed_size.to_le_bytes());
    lzma_stream.extend_from_slice(compressed);

    let mut output = Vec::new();
    lzma_rs::lzma_decompress(&mut std::io::Cursor::new(&lzma_stream), &mut output)
        .map_err(|e| ChunkError::Io(std::io::Error::other(e)))?;
    Ok(output)
}

/// Decompress ZIP/PKzip format.
fn decompress_zip(data: &[u8]) -> Result<Vec<u8>, ChunkError> {
    use std::io::Read;

    let cursor = std::io::Cursor::new(data);
    let mut archive = zip::ZipArchive::new(cursor).map_err(ChunkError::Zip)?;

    if archive.len() == 0 {
        return Err(ChunkError::EmptyArchive);
    }

    let mut file = archive.by_index(0).map_err(ChunkError::Zip)?;
    let mut output = Vec::with_capacity(file.size() as usize);
    file.read_to_end(&mut output).map_err(ChunkError::Io)?;
    Ok(output)
}

/// Errors specific to chunk processing.
#[derive(Debug, thiserror::Error)]
pub enum ChunkError {
    #[error("chunk data too short")]
    TooShort,

    #[error("size mismatch: expected {expected}, got {actual}")]
    SizeMismatch { expected: usize, actual: usize },

    #[error("checksum mismatch: expected {expected:?}, got {actual:?}")]
    ChecksumMismatch {
        expected: SteamAdler32,
        actual: SteamAdler32,
    },

    #[error("empty zip archive")]
    EmptyArchive,

    #[error("crypto error: {0}")]
    Crypto(#[from] CryptoError),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("zip error: {0}")]
    Zip(#[from] zip::result::ZipError),
}
