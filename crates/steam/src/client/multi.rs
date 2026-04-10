//! Handles EMsg::Multi - bundled/compressed messages from the CM server.
//!
//! Wire format of Multi body (protobuf: CMsgMulti):
//! - `size_unzipped` (uint32): If > 0, the payload is gzip-compressed
//! - `message_body` (bytes): The payload (possibly compressed)
//!
//! After decompression, the payload contains length-prefixed messages:
//! ```text
//! [4 bytes] message length (u32 LE)
//! [N bytes] message data
//! ...repeat...
//! ```

use crate::error::Error;
use crate::generated::CMsgMulti;
use prost::Message;
use std::io::Read;

/// Unpack a Multi message body into individual message payloads.
pub fn unpack_multi(body: &[u8]) -> Result<Vec<Vec<u8>>, Error> {
    let multi: CMsgMulti = CMsgMulti::decode(body)?;

    let payload = match multi.message_body {
        Some(data) => {
            // 0 means uncompressed per Steam protocol; absent field treated the same
            let size_unzipped = multi.size_unzipped.unwrap_or(0) as usize;
            if size_unzipped > 0 {
                // Gzip-compressed
                let mut decoder = flate2::read::GzDecoder::new(data.as_slice());
                let mut decompressed = Vec::with_capacity(size_unzipped);
                decoder.read_to_end(&mut decompressed)?;
                decompressed
            } else {
                data
            }
        }
        None => return Ok(Vec::new()),
    };

    // Parse length-prefixed messages
    let mut messages = Vec::new();
    let mut cursor = payload.as_slice();

    while cursor.len() >= 4 {
        let len = u32::from_le_bytes([cursor[0], cursor[1], cursor[2], cursor[3]]) as usize;
        cursor = &cursor[4..];

        if cursor.len() < len {
            break;
        }

        messages.push(cursor[..len].to_vec());
        cursor = &cursor[len..];
    }

    Ok(messages)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn unpack_uncompressed() {
        let msg1 = b"hello";
        let msg2 = b"world!";

        let mut payload = Vec::new();
        payload.extend_from_slice(&(msg1.len() as u32).to_le_bytes());
        payload.extend_from_slice(msg1);
        payload.extend_from_slice(&(msg2.len() as u32).to_le_bytes());
        payload.extend_from_slice(msg2);

        let multi = CMsgMulti {
            size_unzipped: Some(0),
            message_body: Some(payload),
        };

        let messages = unpack_multi(&multi.encode_to_vec()).unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0], b"hello");
        assert_eq!(messages[1], b"world!");
    }

    #[test]
    fn unpack_compressed() {
        let msg1 = b"compressed message";

        let mut payload = Vec::new();
        payload.extend_from_slice(&(msg1.len() as u32).to_le_bytes());
        payload.extend_from_slice(msg1);

        // Gzip compress
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&payload).unwrap();
        let compressed = encoder.finish().unwrap();

        let multi = CMsgMulti {
            size_unzipped: Some(payload.len() as u32),
            message_body: Some(compressed),
        };

        let messages = unpack_multi(&multi.encode_to_vec()).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0], b"compressed message");
    }
}
