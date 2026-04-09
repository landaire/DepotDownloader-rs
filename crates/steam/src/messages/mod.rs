pub mod header;

mod emsg;
pub use emsg::EMsg;

/// Bit 31 of a raw EMsg indicates the message uses protobuf serialization.
pub const PROTO_MASK: u32 = 0x8000_0000;
/// Mask to extract the actual EMsg value (bits 0..30).
pub const EMSG_MASK: u32 = !PROTO_MASK;

/// A raw message ID as it appears on the wire (possibly with protobuf flag set).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawEMsg(pub u32);

impl RawEMsg {
    /// Whether this message uses protobuf serialization.
    pub const fn is_protobuf(self) -> bool {
        self.0 & PROTO_MASK != 0
    }

    /// Extract the EMsg value (without protobuf flag).
    pub fn emsg(self) -> EMsg {
        EMsg::from(self.0 & EMSG_MASK)
    }

    /// Create a raw EMsg with the protobuf flag set.
    pub const fn with_proto(emsg: EMsg) -> Self {
        Self(emsg.0 | PROTO_MASK)
    }

    /// Create a raw EMsg without the protobuf flag.
    pub const fn without_proto(emsg: EMsg) -> Self {
        Self(emsg.0)
    }
}
