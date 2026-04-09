use sha1::{Digest, Sha1};

/// Compute SHA-1 hash of `data`.
pub fn sha1(data: &[u8]) -> [u8; 20] {
    let mut hasher = Sha1::new();
    hasher.update(data);
    hasher.finalize().into()
}

/// Compute Adler-32 checksum of `data`.
pub fn adler32(data: &[u8]) -> u32 {
    adler::adler32_slice(data)
}

/// Compute CRC-32 checksum of `data`.
pub fn crc32(data: &[u8]) -> u32 {
    crc32fast::hash(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha1_known_value() {
        // SHA1("") = da39a3ee5e6b4b0d3255bfef95601890afd80709
        let hash = sha1(b"");
        assert_eq!(
            hex(&hash),
            "da39a3ee5e6b4b0d3255bfef95601890afd80709"
        );
    }

    #[test]
    fn adler32_known_value() {
        // Adler32("Wikipedia") = 0x11E60398
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }
}
