use std::fmt;

use sha1::Digest;
use sha1::Sha1;

/// A SHA-1 hash (20 bytes).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Sha1Hash(pub [u8; 20]);

impl Sha1Hash {
    pub fn compute(data: &[u8]) -> Self {
        let mut hasher = Sha1::new();
        hasher.update(data);
        Self(hasher.finalize().into())
    }
}

impl fmt::Debug for Sha1Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Sha1Hash(")?;
        for b in &self.0 {
            write!(f, "{b:02x}")?;
        }
        write!(f, ")")
    }
}

impl fmt::Display for Sha1Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for b in &self.0 {
            write!(f, "{b:02x}")?;
        }
        Ok(())
    }
}

/// A standard Adler-32 checksum (RFC 1950, seed = 1).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Adler32(pub u32);

impl Adler32 {
    pub fn compute(data: &[u8]) -> Self {
        Self(adler::adler32_slice(data))
    }
}

impl fmt::Debug for Adler32 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Adler32(0x{:08X})", self.0)
    }
}

/// A Steam/Valve Adler-32 checksum (zero seed, non-standard).
///
/// Steam uses seed 0 instead of the RFC 1950 standard seed of 1
/// for depot chunk verification.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct SteamAdler32(pub u32);

impl SteamAdler32 {
    pub fn compute(data: &[u8]) -> Self {
        const BASE: u32 = 65521;

        let mut s1: u32 = 0;
        let mut s2: u32 = 0;

        for chunk in data.chunks(5552) {
            for &byte in chunk {
                s1 += byte as u32;
                s2 += s1;
            }
            s1 %= BASE;
            s2 %= BASE;
        }

        Self((s2 << 16) | s1)
    }
}

impl fmt::Debug for SteamAdler32 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SteamAdler32(0x{:08X})", self.0)
    }
}

/// A CRC-32 checksum.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Crc32(pub u32);

impl Crc32 {
    pub fn compute(data: &[u8]) -> Self {
        Self(crc32fast::hash(data))
    }
}

impl fmt::Debug for Crc32 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Crc32(0x{:08X})", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha1_known_value() {
        let hash = Sha1Hash::compute(b"");
        assert_eq!(hash.to_string(), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
    }

    #[test]
    fn adler32_known_value() {
        assert_eq!(Adler32::compute(b"Wikipedia"), Adler32(0x11E6_0398));
    }

    #[test]
    fn steam_adler32_zero_seed() {
        // With seed 0, empty data should return 0
        assert_eq!(SteamAdler32::compute(b""), SteamAdler32(0));
        // Standard adler32 with seed 1 returns 1 for empty data
        assert_eq!(Adler32::compute(b""), Adler32(1));
    }
}
