use std::io::Write;
use std::io::{
    self,
};

use byteorder::LittleEndian;
use byteorder::WriteBytesExt;
use prost::Message;

use crate::generated::CMsgProtoBufHeader;
use crate::messages::EMsg;
use crate::messages::PROTO_MASK;

/// A client message ready to be serialized and sent.
///
/// Borrows its body to avoid forcing an allocation for static or
/// already-buffered payloads.
#[derive(Debug, Clone)]
pub struct ClientMsg<'a> {
    pub emsg: EMsg,
    pub header: CMsgProtoBufHeader,
    pub body: &'a [u8],
}

impl<'a> ClientMsg<'a> {
    /// Create a new protobuf message with the given EMsg and empty body.
    pub fn new(emsg: EMsg) -> Self {
        Self {
            emsg,
            header: CMsgProtoBufHeader::default(),
            body: &[],
        }
    }

    /// Create a new protobuf message with a borrowed body.
    pub fn with_body(emsg: EMsg, body: &'a [u8]) -> Self {
        Self {
            emsg,
            header: CMsgProtoBufHeader::default(),
            body,
        }
    }

    /// Exact serialized byte length.
    pub fn serialized_len(&self) -> usize {
        8 + self.header.encoded_len() + self.body.len()
    }

    /// Write the full message to the given writer.
    ///
    /// Format: `[4 bytes EMsg|PROTO_MASK] [4 bytes header_len] [header] [body]`
    pub fn write_to(&self, w: &mut impl Write) -> io::Result<()> {
        let header_bytes = self.header.encode_to_vec();
        w.write_u32::<LittleEndian>(self.emsg.0 | PROTO_MASK)?;
        w.write_u32::<LittleEndian>(header_bytes.len() as u32)?;
        w.write_all(&header_bytes)?;
        w.write_all(self.body)?;
        Ok(())
    }
}
