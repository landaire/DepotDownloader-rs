use aes::Aes256;
use aes::cipher::BlockDecrypt;
use aes::cipher::BlockEncrypt;
use aes::cipher::KeyInit;
use aes::cipher::block_padding::Pkcs7;
use cbc::cipher::BlockDecryptMut;
use cbc::cipher::BlockEncryptMut;
use cbc::cipher::KeyIvInit;
use hmac::Hmac;
use hmac::Mac;
use sha1::Sha1;

use crate::error::CryptoError;

type HmacSha1 = Hmac<Sha1>;

/// Session encryption/decryption using AES-256-CBC with HMAC-SHA1 IV.
///
/// Used after the channel encryption handshake to wrap all subsequent
/// packets on the Steam CM TCP connection.
pub struct SessionCipher {
    /// 32-byte AES key for CBC encrypt/decrypt.
    aes_key: [u8; 32],
    /// 16-byte HMAC key (first 16 bytes of the session key).
    hmac_key: [u8; 16],
}

impl SessionCipher {
    /// Create a new session cipher from the 32-byte session key.
    pub fn new(session_key: [u8; 32]) -> Self {
        let mut hmac_key = [0u8; 16];
        hmac_key.copy_from_slice(&session_key[..16]);
        Self {
            aes_key: session_key,
            hmac_key,
        }
    }

    /// Encrypt plaintext using AES-256-CBC with HMAC-SHA1 IV.
    ///
    /// Output format: `[16-byte ECB-encrypted IV] [CBC-encrypted plaintext]`
    ///
    /// IV construction:
    /// - Generate 3 random bytes
    /// - Compute HMAC-SHA1(random_3 || plaintext) using the hmac_key
    /// - IV = hmac[0..13] || random_3
    /// - Encrypt IV with AES-256-ECB
    pub fn encrypt(&self, plaintext: &[u8]) -> Vec<u8> {
        let random_3: [u8; 3] = rand_bytes();

        let iv = self.build_iv(&random_3, plaintext);

        // ECB-encrypt the IV
        let ecb_cipher = Aes256::new((&self.aes_key).into());
        let mut iv_block = aes::Block::from(iv);
        ecb_cipher.encrypt_block(&mut iv_block);
        let encrypted_iv: [u8; 16] = iv_block.into();

        // CBC-encrypt the plaintext
        let padded_len = ((plaintext.len() + 16) / 16) * 16; // PKCS7 padding
        let mut ciphertext = vec![0u8; padded_len];
        ciphertext[..plaintext.len()].copy_from_slice(plaintext);

        let encrypted = cbc::Encryptor::<Aes256>::new((&self.aes_key).into(), (&iv).into())
            .encrypt_padded_mut::<Pkcs7>(&mut ciphertext, plaintext.len())
            .expect("buffer is large enough for PKCS7 padding");

        let mut output = Vec::with_capacity(16 + encrypted.len());
        output.extend_from_slice(&encrypted_iv);
        output.extend_from_slice(encrypted);
        output
    }

    /// Decrypt ciphertext produced by [`encrypt`].
    pub fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>, CryptoError> {
        if data.len() < 32 {
            return Err(CryptoError::DecryptionFailed);
        }

        // ECB-decrypt the IV (first 16 bytes)
        let ecb_cipher = Aes256::new((&self.aes_key).into());
        let mut iv_block = aes::Block::default();
        iv_block.copy_from_slice(&data[..16]);
        ecb_cipher.decrypt_block(&mut iv_block);
        let iv: [u8; 16] = iv_block.into();

        // CBC-decrypt the payload
        let mut ciphertext = data[16..].to_vec();
        let plaintext = cbc::Decryptor::<Aes256>::new((&self.aes_key).into(), (&iv).into())
            .decrypt_padded_mut::<Pkcs7>(&mut ciphertext)
            .map_err(|_| CryptoError::InvalidPadding)?;

        // Validate HMAC
        let random_3 = &iv[13..16];
        let expected_iv = self.build_iv(random_3, plaintext);
        if expected_iv[..13] != iv[..13] {
            return Err(CryptoError::DecryptionFailed);
        }

        Ok(plaintext.to_vec())
    }

    /// Build the 16-byte IV: HMAC-SHA1(random_3 || plaintext)[0..13] || random_3
    fn build_iv(&self, random_3: &[u8], plaintext: &[u8]) -> [u8; 16] {
        let mut mac =
            <HmacSha1 as Mac>::new_from_slice(&self.hmac_key).expect("HMAC accepts any key length");
        mac.update(random_3);
        mac.update(plaintext);
        let hmac_result = mac.finalize().into_bytes();

        let mut iv = [0u8; 16];
        iv[..13].copy_from_slice(&hmac_result[..13]);
        iv[13..16].copy_from_slice(random_3);
        iv
    }
}

/// Generate random bytes using getrandom.
fn rand_bytes<const N: usize>() -> [u8; N] {
    let mut buf = [0u8; N];
    getrandom::fill(&mut buf).expect("failed to generate random bytes");
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_decrypt_round_trip() {
        let key = [0x42u8; 32];
        let cipher = SessionCipher::new(key);

        let plaintext = b"Hello, Steam!";
        let encrypted = cipher.encrypt(plaintext);
        let decrypted = cipher.decrypt(&encrypted).unwrap();

        assert_eq!(&decrypted, plaintext);
    }

    #[test]
    fn decrypt_short_data_fails() {
        let cipher = SessionCipher::new([0u8; 32]);
        assert!(cipher.decrypt(&[0u8; 16]).is_err());
    }
}
