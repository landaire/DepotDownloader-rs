//! Depot manifest parsing.
//!
//! A manifest is a sequence of magic-delimited protobuf sections:
//! ```text
//! [PAYLOAD_MAGIC]  [len: u32] [ContentManifestPayload]
//! [METADATA_MAGIC] [len: u32] [ContentManifestMetadata]
//! [SIGNATURE_MAGIC][len: u32] [ContentManifestSignature]
//! [EOF_MAGIC]
//! ```

use byteorder::{LittleEndian, ReadBytesExt};
use prost::Message;

use crate::depot::{ChunkId, DepotId, ManifestId};
use crate::error::ManifestError;
use crate::generated::{ContentManifestMetadata, ContentManifestPayload, ContentManifestSignature};

const PAYLOAD_MAGIC: u32 = 0x71F6_17D0;
const METADATA_MAGIC: u32 = 0x1F48_12BE;
const SIGNATURE_MAGIC: u32 = 0x1B81_B817;
const EOF_MAGIC: u32 = 0x32C4_15AB;

/// A parsed depot manifest.
#[derive(Debug, Clone)]
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
pub struct ManifestChunk {
    pub id: Option<ChunkId>,
    pub checksum: Option<u32>,
    pub offset: Option<u64>,
    pub compressed_size: Option<u32>,
    pub uncompressed_size: Option<u32>,
}

impl DepotManifest {
    /// Parse a manifest from raw bytes (after ZIP decompression).
    pub fn parse(data: &[u8]) -> Result<Self, ManifestError> {
        let mut reader = data;
        let mut payload = None;
        let mut metadata = None;
        let mut _signature = None;

        loop {
            let magic = reader
                .read_u32::<LittleEndian>()
                .map_err(|_| ManifestError::MissingSection("unexpected EOF reading magic"))?;

            if magic == EOF_MAGIC {
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

            match magic {
                PAYLOAD_MAGIC => {
                    payload = Some(ContentManifestPayload::decode(section_data)
                        .map_err(|_| ManifestError::MissingSection("payload decode failed"))?);
                }
                METADATA_MAGIC => {
                    metadata = Some(ContentManifestMetadata::decode(section_data)
                        .map_err(|_| ManifestError::MissingSection("metadata decode failed"))?);
                }
                SIGNATURE_MAGIC => {
                    _signature = Some(ContentManifestSignature::decode(section_data)
                        .map_err(|_| ManifestError::MissingSection("signature decode failed"))?);
                }
                other => return Err(ManifestError::InvalidMagic(other)),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_manifest() -> Vec<u8> {
        let payload = ContentManifestPayload {
            mappings: vec![
                crate::generated::content_manifest_payload::FileMapping {
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
                },
            ],
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
        buf.extend_from_slice(&PAYLOAD_MAGIC.to_le_bytes());
        buf.extend_from_slice(&(payload_bytes.len() as u32).to_le_bytes());
        buf.extend_from_slice(&payload_bytes);

        let metadata_bytes = metadata.encode_to_vec();
        buf.extend_from_slice(&METADATA_MAGIC.to_le_bytes());
        buf.extend_from_slice(&(metadata_bytes.len() as u32).to_le_bytes());
        buf.extend_from_slice(&metadata_bytes);

        let sig = ContentManifestSignature { signature: None };
        let sig_bytes = sig.encode_to_vec();
        buf.extend_from_slice(&SIGNATURE_MAGIC.to_le_bytes());
        buf.extend_from_slice(&(sig_bytes.len() as u32).to_le_bytes());
        buf.extend_from_slice(&sig_bytes);

        buf.extend_from_slice(&EOF_MAGIC.to_le_bytes());
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
