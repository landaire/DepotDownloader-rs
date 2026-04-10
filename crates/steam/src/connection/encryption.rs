use aes::Aes256;
use aes::cipher::BlockCipherDecrypt;
use aes::cipher::BlockCipherEncrypt;
use aes::cipher::KeyInit;
use aes::cipher::block_padding::Pkcs7;
use cbc::cipher::BlockModeDecrypt;
use cbc::cipher::BlockModeEncrypt;
use cbc::cipher::KeyIvInit;
use hmac::Hmac;
use hmac::Mac;
use sha1::Sha1;

use crate::error::CryptoError;

type HmacSha1 = Hmac<Sha1>;

const AES_BLOCK_SIZE: usize = 16;
const HMAC_IV_BYTES: usize = 13;
const RANDOM_IV_BYTES: usize = AES_BLOCK_SIZE - HMAC_IV_BYTES;

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
        let mut hmac_key = [0u8; AES_BLOCK_SIZE];
        hmac_key.copy_from_slice(&session_key[..AES_BLOCK_SIZE]);
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
        let random: [u8; RANDOM_IV_BYTES] = rand_bytes();

        let iv = self.build_iv(&random, plaintext);

        // ECB-encrypt the IV
        let ecb_cipher = Aes256::new((&self.aes_key).into());
        let mut iv_block = aes::Block::from(iv);
        ecb_cipher.encrypt_block(&mut iv_block);
        let encrypted_iv: [u8; AES_BLOCK_SIZE] = iv_block.into();

        // CBC-encrypt the plaintext
        let encrypted = cbc::Encryptor::<Aes256>::new((&self.aes_key).into(), (&iv).into())
            .encrypt_padded_vec::<Pkcs7>(plaintext);

        let mut output = Vec::with_capacity(AES_BLOCK_SIZE + encrypted.len());
        output.extend_from_slice(&encrypted_iv);
        output.extend_from_slice(&encrypted);
        output
    }

    /// Decrypt ciphertext produced by [`encrypt`].
    pub fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>, CryptoError> {
        if data.len() < AES_BLOCK_SIZE * 2 {
            return Err(CryptoError::DecryptionFailed);
        }

        // ECB-decrypt the IV
        let ecb_cipher = Aes256::new((&self.aes_key).into());
        let mut iv_block = aes::Block::default();
        iv_block.copy_from_slice(&data[..AES_BLOCK_SIZE]);
        ecb_cipher.decrypt_block(&mut iv_block);
        let iv: [u8; AES_BLOCK_SIZE] = iv_block.into();

        // CBC-decrypt the payload
        let plaintext = cbc::Decryptor::<Aes256>::new((&self.aes_key).into(), (&iv).into())
            .decrypt_padded_vec::<Pkcs7>(&data[AES_BLOCK_SIZE..])
            .map_err(|_| CryptoError::InvalidPadding)?;

        // Validate HMAC
        let random = &iv[HMAC_IV_BYTES..AES_BLOCK_SIZE];
        let expected_iv = self.build_iv(random, &plaintext);
        if expected_iv[..HMAC_IV_BYTES] != iv[..HMAC_IV_BYTES] {
            return Err(CryptoError::DecryptionFailed);
        }

        Ok(plaintext)
    }

    /// Build the IV: HMAC-SHA1(random || plaintext)[0..13] || random
    fn build_iv(&self, random: &[u8], plaintext: &[u8]) -> [u8; AES_BLOCK_SIZE] {
        let mut mac =
            HmacSha1::new_from_slice(&self.hmac_key).expect("HMAC accepts any key length");
        mac.update(random);
        mac.update(plaintext);
        let hmac_result = mac.finalize().into_bytes();

        let mut iv = [0u8; AES_BLOCK_SIZE];
        iv[..HMAC_IV_BYTES].copy_from_slice(&hmac_result[..HMAC_IV_BYTES]);
        iv[HMAC_IV_BYTES..].copy_from_slice(random);
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
