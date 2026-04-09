pub mod chunk;
pub mod manifest;

use std::fmt;

macro_rules! newtype_id {
    ($(#[$meta:meta])* $name:ident, $inner:ty) => {
        $(#[$meta])*
        #[derive(Clone, Copy, PartialEq, Eq, Hash)]
        #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
        #[repr(transparent)]
        pub struct $name(pub $inner);

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({})", stringify!($name), self.0)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }

        impl From<$inner> for $name {
            fn from(v: $inner) -> Self { Self(v) }
        }

        impl From<$name> for $inner {
            fn from(v: $name) -> Self { v.0 }
        }
    };
}

newtype_id!(
    /// A Steam application ID.
    AppId, u32
);

newtype_id!(
    /// A Steam depot ID.
    DepotId, u32
);

newtype_id!(
    /// A depot manifest ID.
    ManifestId, u64
);

newtype_id!(
    /// A Steam package/sub ID.
    PackageId, u32
);

newtype_id!(
    /// A Steam cell ID (geographic region).
    CellId, u32
);

/// A depot content chunk identifier (SHA-1 hash).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ChunkId(pub [u8; 20]);

impl fmt::Debug for ChunkId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ChunkId(")?;
        for byte in &self.0 {
            write!(f, "{byte:02x}")?;
        }
        write!(f, ")")
    }
}

impl fmt::Display for ChunkId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in &self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// A 256-bit AES depot key.
#[derive(Clone)]
pub struct DepotKey(pub [u8; 32]);

impl fmt::Debug for DepotKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DepotKey([redacted])")
    }
}
