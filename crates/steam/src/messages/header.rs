use std::io::{self, Write};

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};

use super::{EMSG_MASK, PROTO_MASK};
use crate::error::ParseError;
use crate::messages::EMsg;

/// Standard (non-protobuf) message header.
///
/// Wire layout (20 bytes):
/// ```text
/// [4 bytes] EMsg (u32)
/// [8 bytes] target job ID (u64)
/// [8 bytes] source job ID (u64)
/// ```
#[derive(Debug, Clone)]
pub struct MsgHdr {
    pub emsg: EMsg,
    pub target_job_id: u64,
    pub source_job_id: u64,
}

impl MsgHdr {
    pub const SIZE: usize = 20;

    pub fn parse(reader: &mut &[u8]) -> Result<Self, ParseError> {
        let raw_emsg = reader
            .read_u32::<LittleEndian>()
            .map_err(|_| ParseError::UnexpectedEof)?;
        let target_job_id = reader
            .read_u64::<LittleEndian>()
            .map_err(|_| ParseError::UnexpectedEof)?;
        let source_job_id = reader
            .read_u64::<LittleEndian>()
            .map_err(|_| ParseError::UnexpectedEof)?;

        Ok(Self {
            emsg: EMsg::from(raw_emsg & EMSG_MASK),
            target_job_id,
            source_job_id,
        })
    }

    pub fn write_to(&self, w: &mut impl Write) -> io::Result<()> {
        w.write_u32::<LittleEndian>(self.emsg.0)?;
        w.write_u64::<LittleEndian>(self.target_job_id)?;
        w.write_u64::<LittleEndian>(self.source_job_id)?;
        Ok(())
    }
}

/// Extended client message header (non-protobuf).
///
/// Wire layout (36 bytes):
/// ```text
/// [4 bytes]  EMsg (u32)
/// [1 byte]   header size (u8, always 36)
/// [2 bytes]  header version (u16, always 2)
/// [8 bytes]  target job ID (u64)
/// [8 bytes]  source job ID (u64)
/// [1 byte]   header canary (u8, always 239)
/// [8 bytes]  steam ID (u64)
/// [4 bytes]  session ID (i32)
/// ```
#[derive(Debug, Clone)]
pub struct ExtendedClientMsgHdr {
    pub emsg: EMsg,
    pub header_size: u8,
    pub header_version: u16,
    pub target_job_id: u64,
    pub source_job_id: u64,
    pub header_canary: u8,
    pub steam_id: u64,
    pub session_id: i32,
}

impl ExtendedClientMsgHdr {
    pub const SIZE: usize = 36;

    pub fn parse(reader: &mut &[u8]) -> Result<Self, ParseError> {
        let raw_emsg = reader
            .read_u32::<LittleEndian>()
            .map_err(|_| ParseError::UnexpectedEof)?;
        let header_size = reader
            .read_u8()
            .map_err(|_| ParseError::UnexpectedEof)?;
        let header_version = reader
            .read_u16::<LittleEndian>()
            .map_err(|_| ParseError::UnexpectedEof)?;
        let target_job_id = reader
            .read_u64::<LittleEndian>()
            .map_err(|_| ParseError::UnexpectedEof)?;
        let source_job_id = reader
            .read_u64::<LittleEndian>()
            .map_err(|_| ParseError::UnexpectedEof)?;
        let header_canary = reader
            .read_u8()
            .map_err(|_| ParseError::UnexpectedEof)?;
        let steam_id = reader
            .read_u64::<LittleEndian>()
            .map_err(|_| ParseError::UnexpectedEof)?;
        let session_id = reader
            .read_i32::<LittleEndian>()
            .map_err(|_| ParseError::UnexpectedEof)?;

        Ok(Self {
            emsg: EMsg::from(raw_emsg & EMSG_MASK),
            header_size,
            header_version,
            target_job_id,
            source_job_id,
            header_canary,
            steam_id,
            session_id,
        })
    }

    pub fn write_to(&self, w: &mut impl Write) -> io::Result<()> {
        w.write_u32::<LittleEndian>(self.emsg.0)?;
        w.write_u8(self.header_size)?;
        w.write_u16::<LittleEndian>(self.header_version)?;
        w.write_u64::<LittleEndian>(self.target_job_id)?;
        w.write_u64::<LittleEndian>(self.source_job_id)?;
        w.write_u8(self.header_canary)?;
        w.write_u64::<LittleEndian>(self.steam_id)?;
        w.write_i32::<LittleEndian>(self.session_id)?;
        Ok(())
    }
}

/// Protobuf message header.
///
/// Wire layout:
/// ```text
/// [4 bytes]  EMsg (u32, with protobuf flag set in bit 31)
/// [4 bytes]  header length (u32)
/// [N bytes]  CMsgProtoBufHeader (protobuf-encoded)
/// ```
#[derive(Debug, Clone)]
pub struct MsgHdrProtoBuf {
    pub emsg: EMsg,
    pub is_protobuf: bool,
    pub header_data: bytes::Bytes,
}

impl MsgHdrProtoBuf {
    pub fn parse(reader: &mut &[u8]) -> Result<Self, ParseError> {
        let raw_emsg = reader
            .read_u32::<LittleEndian>()
            .map_err(|_| ParseError::UnexpectedEof)?;
        let header_len = reader
            .read_u32::<LittleEndian>()
            .map_err(|_| ParseError::UnexpectedEof)? as usize;

        if reader.len() < header_len {
            return Err(ParseError::UnexpectedEof);
        }
        let header_data = bytes::Bytes::copy_from_slice(&reader[..header_len]);
        *reader = &reader[header_len..];

        Ok(Self {
            emsg: EMsg::from(raw_emsg & EMSG_MASK),
            is_protobuf: (raw_emsg & PROTO_MASK) != 0,
            header_data,
        })
    }

    pub fn write_to(&self, w: &mut impl Write, header_data: &[u8]) -> io::Result<()> {
        let raw_emsg = if self.is_protobuf {
            self.emsg.0 | PROTO_MASK
        } else {
            self.emsg.0
        };
        w.write_u32::<LittleEndian>(raw_emsg)?;
        w.write_u32::<LittleEndian>(header_data.len() as u32)?;
        w.write_all(header_data)?;
        Ok(())
    }
}

/// Dispatch: determine which header type a packet uses.
///
/// Peek at the first 4 bytes to check the protobuf flag, then parse
/// the appropriate header. Returns the header and remaining body bytes.
pub fn parse_packet_header(data: &[u8]) -> Result<PacketHeader, ParseError> {
    if data.len() < 4 {
        return Err(ParseError::UnexpectedEof);
    }

    let raw_emsg = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    let is_proto = (raw_emsg & PROTO_MASK) != 0;

    if is_proto {
        let mut reader = &data[..];
        let header = MsgHdrProtoBuf::parse(&mut reader)?;
        Ok(PacketHeader::Protobuf {
            header,
            body: bytes::Bytes::copy_from_slice(reader),
        })
    } else {
        let emsg = EMsg::from(raw_emsg & EMSG_MASK);
        // Channel encryption messages use the simple MsgHdr
        if matches!(
            emsg,
            EMsg::CHANNEL_ENCRYPT_REQUEST
                | EMsg::CHANNEL_ENCRYPT_RESPONSE
                | EMsg::CHANNEL_ENCRYPT_RESULT
        ) {
            let mut reader = &data[..];
            let header = MsgHdr::parse(&mut reader)?;
            Ok(PacketHeader::Simple {
                header,
                body: bytes::Bytes::copy_from_slice(reader),
            })
        } else {
            let mut reader = &data[..];
            let header = ExtendedClientMsgHdr::parse(&mut reader)?;
            Ok(PacketHeader::Extended {
                header,
                body: bytes::Bytes::copy_from_slice(reader),
            })
        }
    }
}

/// Parsed packet with header variant and remaining body.
#[derive(Debug)]
pub enum PacketHeader {
    Simple {
        header: MsgHdr,
        body: bytes::Bytes,
    },
    Extended {
        header: ExtendedClientMsgHdr,
        body: bytes::Bytes,
    },
    Protobuf {
        header: MsgHdrProtoBuf,
        body: bytes::Bytes,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn msg_hdr_round_trip() {
        let hdr = MsgHdr {
            emsg: EMsg::CHANNEL_ENCRYPT_REQUEST,
            target_job_id: u64::MAX,
            source_job_id: 42,
        };
        let mut buf = Vec::new();
        hdr.write_to(&mut buf).unwrap();
        assert_eq!(buf.len(), MsgHdr::SIZE);

        let mut reader = buf.as_slice();
        let parsed = MsgHdr::parse(&mut reader).unwrap();
        assert_eq!(parsed.emsg, EMsg::CHANNEL_ENCRYPT_REQUEST);
        assert_eq!(parsed.target_job_id, u64::MAX);
        assert_eq!(parsed.source_job_id, 42);
    }

    #[test]
    fn extended_hdr_round_trip() {
        let hdr = ExtendedClientMsgHdr {
            emsg: EMsg::CLIENT_LOG_ON_RESPONSE,
            header_size: 36,
            header_version: 2,
            target_job_id: u64::MAX,
            source_job_id: u64::MAX,
            header_canary: 239,
            steam_id: 76561198000000000,
            session_id: 1,
        };
        let mut buf = Vec::new();
        hdr.write_to(&mut buf).unwrap();
        assert_eq!(buf.len(), ExtendedClientMsgHdr::SIZE);

        let mut reader = buf.as_slice();
        let parsed = ExtendedClientMsgHdr::parse(&mut reader).unwrap();
        assert_eq!(parsed.emsg, EMsg::CLIENT_LOG_ON_RESPONSE);
        assert_eq!(parsed.steam_id, 76561198000000000);
        assert_eq!(parsed.session_id, 1);
    }

    #[test]
    fn protobuf_header_dispatch() {
        let raw_emsg = EMsg::CLIENT_LOG_ON_RESPONSE.0 | PROTO_MASK;
        let fake_proto_header = vec![0x08, 0x01]; // minimal protobuf
        let mut packet = Vec::new();
        packet.extend_from_slice(&raw_emsg.to_le_bytes());
        packet.extend_from_slice(&(fake_proto_header.len() as u32).to_le_bytes());
        packet.extend_from_slice(&fake_proto_header);
        packet.extend_from_slice(b"body_data");

        let parsed = parse_packet_header(&packet).unwrap();
        match parsed {
            PacketHeader::Protobuf { header, body } => {
                assert_eq!(header.emsg, EMsg::CLIENT_LOG_ON_RESPONSE);
                assert!(header.is_protobuf);
                assert_eq!(&body[..], b"body_data");
            }
            _ => panic!("expected Protobuf variant"),
        }
    }
}
