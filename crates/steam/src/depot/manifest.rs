//! Depot manifest parsing.
//!
//! A manifest is a sequence of magic-delimited protobuf sections:
//! ```text
//! [ManifestMagic::PayloadV5 as u32]  [len: u32] [ContentManifestPayload]
//! [ManifestMagic::Metadata as u32] [len: u32] [ContentManifestMetadata]
//! [ManifestMagic::Signature as u32][len: u32] [ContentManifestSignature]
//! [ManifestMagic::EndOfManifest as u32]
//! ```

use byteorder::LittleEndian;
use byteorder::ReadBytesExt;
use prost::Message;

use crate::depot::ChunkId;
use crate::depot::DepotId;
use crate::depot::DepotKey;
use crate::depot::ManifestId;
use crate::error::ManifestError;
use crate::generated::ContentManifestMetadata;
use crate::generated::ContentManifestPayload;
use crate::generated::ContentManifestSignature;

use crate::enums::ManifestMagic;

/// A parsed depot manifest.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct DepotManifest {
    pub depot_id: Option<DepotId>,
    pub manifest_id: Option<ManifestId>,
    pub creation_time: Option<u32>,
    pub filenames_encrypted: bool,
    pub total_uncompressed_size: Option<u64>,
    pub total_compressed_size: Option<u64>,
    pub files: Vec<ManifestFile>,
}

/// A file entry in a depot manifest.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ManifestFile {
    pub filename: Option<String>,
    pub size: Option<u64>,
    pub flags: Option<u32>,
    pub sha_content: Option<[u8; 20]>,
    pub chunks: Vec<ManifestChunk>,
    pub link_target: Option<String>,
}

/// A chunk reference within a manifest file.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ManifestChunk {
    pub id: Option<ChunkId>,
    pub checksum: Option<u32>,
    pub offset: Option<u64>,
    pub compressed_size: Option<u32>,
    pub uncompressed_size: Option<u32>,
}

impl DepotManifest {
    /// Decrypt encrypted filenames using the depot key.
    ///
    /// Steam encrypts filenames with AES-256: the base64-encoded filename
    /// is decoded, the first 16 bytes are an ECB-encrypted IV, and the
    /// remainder is CBC-encrypted with PKCS7 padding. After decryption,
    /// trailing null bytes are stripped and path separators normalized.
    pub fn decrypt_filenames(&mut self, key: &DepotKey) -> Result<(), ManifestError> {
        if !self.filenames_encrypted {
            return Ok(());
        }

        for file in &mut self.files {
            if let Some(ref encrypted) = file.filename {
                match decrypt_filename(encrypted, key) {
                    Ok(decrypted) => file.filename = Some(decrypted),
                    Err(e) => {
                        tracing::error!("Failed to decrypt filename {encrypted:?}: {e}");
                        return Err(e);
                    }
                }
            }
        }

        // Sort files by name after decryption (matches SteamKit2 behavior)
        self.files.sort_by(|a, b| {
            a.filename
                .as_deref()
                .unwrap_or("")
                .cmp(b.filename.as_deref().unwrap_or(""))
        });

        self.filenames_encrypted = false;
        Ok(())
    }

    /// Parse a manifest from raw bytes (after ZIP decompression).
    ///
    /// Supports both v4 (binary) and v5 (protobuf) manifest formats.
    pub fn parse(data: &[u8]) -> Result<Self, ManifestError> {
        if data.len() < 4 {
            return Err(ManifestError::MissingSection("manifest too short"));
        }

        let magic = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);

        if magic == ManifestMagic::V4 as u32 {
            Self::parse_v4(data)
        } else {
            Self::parse_v5(data)
        }
    }

    /// Parse a v4 (binary) manifest.
    ///
    /// Layout:
    /// ```text
    /// [4] magic (0x16349781)
    /// [4] version (must be 4)
    /// [4] depot_id
    /// [8] manifest_gid
    /// [4] creation_time (unix)
    /// [4] filenames_encrypted (bool as u32)
    /// [8] total_uncompressed_size
    /// [8] total_compressed_size
    /// [4] chunk_count
    /// [4] file_entry_count
    /// [4] file_mapping_size (total bytes of file mapping data)
    /// [4] encrypted_crc
    /// [4] decrypted_crc
    /// [4] flags
    /// [...] file mappings (variable, consumed by file_mapping_size)
    /// [4] end marker (must match magic)
    /// ```
    fn parse_v4(data: &[u8]) -> Result<Self, ManifestError> {
        use std::io::Read;

        let mut r = data;
        let _magic = r
            .read_u32::<LittleEndian>()
            .map_err(|_| ManifestError::MissingSection("v4 header"))?;
        let version = r
            .read_u32::<LittleEndian>()
            .map_err(|_| ManifestError::MissingSection("v4 version"))?;

        if version != 4 {
            return Err(ManifestError::InvalidMagic(version));
        }

        let depot_id = r
            .read_u32::<LittleEndian>()
            .map_err(|_| ManifestError::MissingSection("v4 depot_id"))?;
        let manifest_gid = r
            .read_u64::<LittleEndian>()
            .map_err(|_| ManifestError::MissingSection("v4 manifest_gid"))?;
        let creation_time = r
            .read_u32::<LittleEndian>()
            .map_err(|_| ManifestError::MissingSection("v4 creation_time"))?;
        let filenames_encrypted = r
            .read_u32::<LittleEndian>()
            .map_err(|_| ManifestError::MissingSection("v4 filenames_encrypted"))?
            != 0;
        let total_uncompressed = r
            .read_u64::<LittleEndian>()
            .map_err(|_| ManifestError::MissingSection("v4 total_uncompressed"))?;
        let total_compressed = r
            .read_u64::<LittleEndian>()
            .map_err(|_| ManifestError::MissingSection("v4 total_compressed"))?;
        let _chunk_count = r
            .read_u32::<LittleEndian>()
            .map_err(|_| ManifestError::MissingSection("v4 chunk_count"))?;
        let _file_entry_count = r
            .read_u32::<LittleEndian>()
            .map_err(|_| ManifestError::MissingSection("v4 file_entry_count"))?;
        let file_mapping_size = r
            .read_u32::<LittleEndian>()
            .map_err(|_| ManifestError::MissingSection("v4 file_mapping_size"))?
            as usize;
        let _encrypted_crc = r
            .read_u32::<LittleEndian>()
            .map_err(|_| ManifestError::MissingSection("v4 encrypted_crc"))?;
        let _decrypted_crc = r
            .read_u32::<LittleEndian>()
            .map_err(|_| ManifestError::MissingSection("v4 decrypted_crc"))?;
        let _flags = r
            .read_u32::<LittleEndian>()
            .map_err(|_| ManifestError::MissingSection("v4 flags"))?;

        // Parse file mappings
        let mut files = Vec::new();
        let mut consumed = 0usize;

        while consumed < file_mapping_size {
            let start = r.len();

            // Null-terminated filename
            let nul_pos = r
                .iter()
                .position(|&b| b == 0)
                .ok_or(ManifestError::MissingSection("v4 filename nul terminator"))?;
            let filename = String::from_utf8_lossy(&r[..nul_pos]).into_owned();
            r = &r[nul_pos + 1..];

            let size = r
                .read_u64::<LittleEndian>()
                .map_err(|_| ManifestError::MissingSection("v4 file size"))?;
            let flags = r
                .read_u32::<LittleEndian>()
                .map_err(|_| ManifestError::MissingSection("v4 file flags"))?;
            let mut hash_content = [0u8; 20];
            r.read_exact(&mut hash_content)
                .map_err(|_| ManifestError::MissingSection("v4 hash_content"))?;
            let mut _hash_filename = [0u8; 20];
            r.read_exact(&mut _hash_filename)
                .map_err(|_| ManifestError::MissingSection("v4 hash_filename"))?;
            let num_chunks = r
                .read_u32::<LittleEndian>()
                .map_err(|_| ManifestError::MissingSection("v4 num_chunks"))?;

            let mut chunks = Vec::with_capacity(num_chunks as usize);
            for _ in 0..num_chunks {
                let mut sha = [0u8; 20];
                r.read_exact(&mut sha)
                    .map_err(|_| ManifestError::MissingSection("v4 chunk sha"))?;
                let checksum = r
                    .read_u32::<LittleEndian>()
                    .map_err(|_| ManifestError::MissingSection("v4 chunk checksum"))?;
                let offset = r
                    .read_u64::<LittleEndian>()
                    .map_err(|_| ManifestError::MissingSection("v4 chunk offset"))?;
                let uncompressed_size = r
                    .read_u32::<LittleEndian>()
                    .map_err(|_| ManifestError::MissingSection("v4 chunk uncompressed"))?;
                let compressed_size = r
                    .read_u32::<LittleEndian>()
                    .map_err(|_| ManifestError::MissingSection("v4 chunk compressed"))?;

                chunks.push(ManifestChunk {
                    id: Some(ChunkId(sha)),
                    checksum: Some(checksum),
                    offset: Some(offset),
                    compressed_size: Some(compressed_size),
                    uncompressed_size: Some(uncompressed_size),
                });
            }

            files.push(ManifestFile {
                filename: Some(filename),
                size: Some(size),
                flags: Some(flags),
                sha_content: Some(hash_content),
                chunks,
                link_target: None,
            });

            consumed += start - r.len();
        }

        Ok(Self {
            depot_id: Some(DepotId(depot_id)),
            manifest_id: Some(ManifestId(manifest_gid)),
            creation_time: Some(creation_time),
            filenames_encrypted,
            total_uncompressed_size: Some(total_uncompressed),
            total_compressed_size: Some(total_compressed),
            files,
        })
    }

    /// Parse a v5 (protobuf) manifest.
    fn parse_v5(data: &[u8]) -> Result<Self, ManifestError> {
        let mut reader = data;
        let mut payload = None;
        let mut metadata = None;
        let mut _signature = None;

        loop {
            let magic = reader
                .read_u32::<LittleEndian>()
                .map_err(|_| ManifestError::MissingSection("unexpected EOF reading magic"))?;

            if magic == ManifestMagic::EndOfManifest as u32 {
                break;
            }

            let len = reader
                .read_u32::<LittleEndian>()
                .map_err(|_| ManifestError::MissingSection("unexpected EOF reading length"))?
                as usize;

            if reader.len() < len {
                return Err(ManifestError::MissingSection("section data truncated"));
            }
            let section_data = &reader[..len];
            reader = &reader[len..];

            match ManifestMagic::from_u32(magic) {
                Some(ManifestMagic::PayloadV5) => {
                    payload = Some(
                        ContentManifestPayload::decode(section_data)
                            .map_err(|_| ManifestError::MissingSection("payload decode failed"))?,
                    );
                }
                Some(ManifestMagic::Metadata) => {
                    metadata =
                        Some(ContentManifestMetadata::decode(section_data).map_err(|_| {
                            ManifestError::MissingSection("metadata decode failed")
                        })?);
                }
                Some(ManifestMagic::Signature) => {
                    _signature =
                        Some(ContentManifestSignature::decode(section_data).map_err(|_| {
                            ManifestError::MissingSection("signature decode failed")
                        })?);
                }
                _ => return Err(ManifestError::InvalidMagic(magic)),
            }
        }

        let payload = payload.ok_or(ManifestError::MissingSection("payload"))?;
        let metadata = metadata.ok_or(ManifestError::MissingSection("metadata"))?;

        let files = payload
            .mappings
            .into_iter()
            .map(|m| {
                let sha_content = m.sha_content.as_deref().and_then(try_sha_array);
                let chunks = m
                    .chunks
                    .into_iter()
                    .map(|c| ManifestChunk {
                        id: c.sha.as_deref().and_then(try_sha_array).map(ChunkId),
                        checksum: c.crc,
                        offset: c.offset,
                        compressed_size: c.cb_compressed,
                        uncompressed_size: c.cb_original,
                    })
                    .collect();

                ManifestFile {
                    filename: m.filename,
                    size: m.size,
                    flags: m.flags,
                    sha_content,
                    chunks,
                    link_target: m.linktarget.filter(|s| !s.is_empty()),
                }
            })
            .collect();

        Ok(Self {
            depot_id: metadata.depot_id.map(DepotId),
            manifest_id: metadata.gid_manifest.map(ManifestId),
            creation_time: metadata.creation_time,
            // absent = plaintext filenames (older manifests)
            filenames_encrypted: metadata.filenames_encrypted.unwrap_or(false),
            total_uncompressed_size: metadata.cb_disk_original,
            total_compressed_size: metadata.cb_disk_compressed,
            files,
        })
    }
}

/// Try to convert a byte slice to a `[u8; 20]` SHA-1 hash.
/// Returns `None` if the slice isn't exactly 20 bytes.
fn try_sha_array(bytes: &[u8]) -> Option<[u8; 20]> {
    bytes.try_into().ok()
}

/// Decrypt a single encrypted filename.
///
/// Process:
/// 1. Base64-decode the encrypted string
/// 2. First 16 bytes → ECB-decrypt to get the IV
/// 3. Remaining bytes → CBC-decrypt with PKCS7 padding
/// 4. Strip trailing null bytes
/// 5. Normalize path separators to OS convention
fn decrypt_filename(encrypted_b64: &str, key: &DepotKey) -> Result<String, ManifestError> {
    use aes::Aes256;
    use aes::cipher::BlockDecrypt;
    use aes::cipher::KeyInit;
    use aes::cipher::block_padding::Pkcs7;
    use cbc::cipher::BlockDecryptMut;
    use cbc::cipher::KeyIvInit;

    // Strip all whitespace - encrypted filenames may contain line breaks
    let cleaned: String = encrypted_b64
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let encrypted = base64::engine::general_purpose::STANDARD
        .decode(&cleaned)
        .map_err(|_| ManifestError::MissingSection("invalid base64 in filename"))?;

    if encrypted.len() < 32 {
        return Err(ManifestError::MissingSection(
            "encrypted filename too short",
        ));
    }

    // ECB-decrypt the first 16 bytes to get the IV
    let ecb_cipher = Aes256::new((&key.0).into());
    let mut iv_block = aes::Block::default();
    iv_block.copy_from_slice(&encrypted[..16]);
    ecb_cipher.decrypt_block(&mut iv_block);
    let iv: [u8; 16] = iv_block.into();

    // CBC-decrypt the rest
    let mut ciphertext = encrypted[16..].to_vec();
    let plaintext = cbc::Decryptor::<Aes256>::new((&key.0).into(), (&iv).into())
        .decrypt_padded_mut::<Pkcs7>(&mut ciphertext)
        .map_err(|_| ManifestError::MissingSection("filename decryption failed"))?;

    // Strip trailing null bytes
    let end = plaintext
        .iter()
        .position(|&b| b == 0)
        .unwrap_or(plaintext.len());
    let name = String::from_utf8_lossy(&plaintext[..end]).into_owned();

    // Normalize path separators
    Ok(name.replace('\\', "/"))
}

use base64::Engine;

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_manifest() -> Vec<u8> {
        let payload = ContentManifestPayload {
            mappings: vec![crate::generated::content_manifest_payload::FileMapping {
                filename: Some("test.txt".to_string()),
                size: Some(1024),
                flags: Some(0),
                sha_filename: None,
                sha_content: Some(vec![0xAA; 20]),
                chunks: vec![
                    crate::generated::content_manifest_payload::file_mapping::ChunkData {
                        sha: Some(vec![0xBB; 20]),
                        crc: Some(0x12345678),
                        offset: Some(0),
                        cb_original: Some(1024),
                        cb_compressed: Some(512),
                    },
                ],
                linktarget: None,
            }],
        };

        let metadata = ContentManifestMetadata {
            depot_id: Some(480),
            gid_manifest: Some(99999),
            creation_time: Some(1700000000),
            filenames_encrypted: Some(false),
            cb_disk_original: Some(1024),
            cb_disk_compressed: Some(512),
            unique_chunks: Some(1),
            crc_encrypted: None,
            crc_clear: None,
        };

        let mut buf = Vec::new();

        let payload_bytes = payload.encode_to_vec();
        buf.extend_from_slice(&(ManifestMagic::PayloadV5 as u32).to_le_bytes());
        buf.extend_from_slice(&(payload_bytes.len() as u32).to_le_bytes());
        buf.extend_from_slice(&payload_bytes);

        let metadata_bytes = metadata.encode_to_vec();
        buf.extend_from_slice(&(ManifestMagic::Metadata as u32).to_le_bytes());
        buf.extend_from_slice(&(metadata_bytes.len() as u32).to_le_bytes());
        buf.extend_from_slice(&metadata_bytes);

        let sig = ContentManifestSignature { signature: None };
        let sig_bytes = sig.encode_to_vec();
        buf.extend_from_slice(&(ManifestMagic::Signature as u32).to_le_bytes());
        buf.extend_from_slice(&(sig_bytes.len() as u32).to_le_bytes());
        buf.extend_from_slice(&sig_bytes);

        buf.extend_from_slice(&(ManifestMagic::EndOfManifest as u32).to_le_bytes());
        buf
    }

    #[test]
    fn parse_manifest() {
        let data = make_test_manifest();
        let manifest = DepotManifest::parse(&data).unwrap();

        assert_eq!(manifest.depot_id, Some(DepotId(480)));
        assert_eq!(manifest.manifest_id, Some(ManifestId(99999)));
        assert!(!manifest.filenames_encrypted);
        assert_eq!(manifest.files.len(), 1);

        let file = &manifest.files[0];
        assert_eq!(file.filename.as_deref(), Some("test.txt"));
        assert_eq!(file.size, Some(1024));
        assert_eq!(file.chunks.len(), 1);

        let chunk = &file.chunks[0];
        assert_eq!(chunk.checksum, Some(0x12345678));
        assert_eq!(chunk.uncompressed_size, Some(1024));
        assert_eq!(chunk.compressed_size, Some(512));
    }
}
