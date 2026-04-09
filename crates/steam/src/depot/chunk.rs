//! Chunk processing: decrypt → decompress → verify.

use crate::depot::DepotKey;
use crate::error::CryptoError;

/// Compression format of a decrypted chunk, detected from magic bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkCompression {
    /// Valve Zstandard: `VSZa` header + CRC32 + zstd data.
    VZstd,
    /// Valve LZMA: `VZa` header + LZMA data + CRC32 + `zv` trailer.
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

    let checksum = adler::adler32_slice(&decompressed);
    if checksum != expected_checksum {
        return Err(ChunkError::ChecksumMismatch {
            expected: expected_checksum,
            actual: checksum,
        });
    }

    Ok(decompressed)
}

/// Decompress a decrypted chunk based on its detected format.
fn decompress(data: &[u8]) -> Result<Vec<u8>, ChunkError> {
    match ChunkCompression::detect(data) {
        ChunkCompression::VZstd => decompress_vzstd(data),
        ChunkCompression::Lzma => decompress_lzma(data),
        ChunkCompression::Zip => decompress_zip(data),
        ChunkCompression::None => Ok(data.to_vec()),
    }
}

/// Decompress Valve's VZstd format.
///
/// Format: `[4 bytes "VSZa"] [4 bytes CRC32] [zstd data]`
fn decompress_vzstd(data: &[u8]) -> Result<Vec<u8>, ChunkError> {
    if data.len() < 8 {
        return Err(ChunkError::TooShort);
    }
    // Skip magic (4) and CRC32 (4)
    zstd::stream::decode_all(&data[8..]).map_err(ChunkError::Io)
}

/// Decompress Valve's VZip (LZMA) format.
///
/// Format: `[3 bytes "VZa"] [... LZMA data ...] [4 bytes CRC32] [2 bytes "zv"]`
fn decompress_lzma(data: &[u8]) -> Result<Vec<u8>, ChunkError> {
    if data.len() < 9 {
        return Err(ChunkError::TooShort);
    }
    // Strip magic prefix (3 bytes) and CRC+trailer suffix (6 bytes)
    let lzma_data = &data[3..data.len() - 6];
    let mut output = Vec::new();
    lzma_rs::lzma_decompress(&mut std::io::Cursor::new(lzma_data), &mut output)
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

    #[error("checksum mismatch: expected 0x{expected:08X}, got 0x{actual:08X}")]
    ChecksumMismatch { expected: u32, actual: u32 },

    #[error("empty zip archive")]
    EmptyArchive,

    #[error("crypto error: {0}")]
    Crypto(#[from] CryptoError),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("zip error: {0}")]
    Zip(#[from] zip::result::ZipError),
}
