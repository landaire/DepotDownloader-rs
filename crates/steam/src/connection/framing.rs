use std::io;
use std::io::Write;

use byteorder::LittleEndian;
use byteorder::WriteBytesExt;
use bytes::Bytes;
use winnow::ModalResult;
use winnow::Parser;
use winnow::binary::le_u32;
use winnow::error::StrContext;
use winnow::error::StrContextValue;
use winnow::stream::Partial;
use winnow::token::take;

/// TCP packet magic: "VT01" as little-endian u32.
pub const MAGIC: u32 = u32::from_le_bytes(*b"VT01");

/// A parsed VT01 TCP frame.
#[derive(Debug, Clone)]
pub struct Frame {
    pub payload: Bytes,
}

/// Parse a single VT01 frame from a streaming byte buffer (sans-IO).
///
/// Returns `Incomplete` if more data is needed.
///
/// Wire format (all little-endian):
/// ```text
/// [4 bytes] payload length
/// [4 bytes] magic (0x31305456 = "VT01")
/// [N bytes] payload
/// ```
pub fn parse_frame(input: &mut Partial<&[u8]>) -> ModalResult<Frame> {
    let len = le_u32
        .context(StrContext::Label("packet length"))
        .parse_next(input)?;

    le_u32
        .verify(|m| *m == MAGIC)
        .context(StrContext::Expected(StrContextValue::Description(
            "VT01 magic (0x31305456)",
        )))
        .context(StrContext::Label("frame magic"))
        .parse_next(input)?;

    let payload = take(len as usize)
        .context(StrContext::Label("payload"))
        .parse_next(input)?;

    Ok(Frame {
        payload: Bytes::copy_from_slice(payload),
    })
}

/// Write a VT01 frame to the given writer.
pub fn write_frame(w: &mut impl Write, payload: &[u8]) -> io::Result<()> {
    w.write_u32::<LittleEndian>(payload.len() as u32)?;
    w.write_u32::<LittleEndian>(MAGIC)?;
    w.write_all(payload)?;
    Ok(())
}

/// Serialize a VT01 frame into a new Vec. Convenience wrapper.
pub fn frame_bytes(payload: &[u8]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(8 + payload.len());
    write_frame(&mut buf, payload).expect("Vec<u8> Write never fails");
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let original = b"hello steam";
        let frame_bytes = frame_bytes(original);

        let mut input = Partial::new(frame_bytes.as_slice());
        let frame = parse_frame(&mut input).unwrap();
        assert_eq!(&frame.payload[..], original);
        assert!(input.is_empty());
    }

    #[test]
    fn incomplete_header() {
        let mut input = Partial::new(&[0x01, 0x02, 0x03][..]);
        let result = parse_frame(&mut input);
        assert!(result.is_err());
    }

    #[test]
    fn bad_magic() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&4u32.to_le_bytes());
        buf.extend_from_slice(&0xDEADBEEFu32.to_le_bytes());
        buf.extend_from_slice(b"test");

        let mut input = Partial::new(buf.as_slice());
        let result = parse_frame(&mut input);
        assert!(result.is_err());
    }

    #[test]
    fn incomplete_payload() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&100u32.to_le_bytes());
        buf.extend_from_slice(&MAGIC.to_le_bytes());
        buf.extend_from_slice(b"short");

        let mut input = Partial::new(buf.as_slice());
        let result = parse_frame(&mut input);
        assert!(result.is_err());
    }
}
