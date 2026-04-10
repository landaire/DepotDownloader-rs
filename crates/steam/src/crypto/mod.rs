pub mod rsa;

use aes::Aes256;
use aes::cipher::BlockCipherDecrypt;
use aes::cipher::KeyInit;
use aes::cipher::block_padding::Pkcs7;
use cbc::cipher::BlockModeDecrypt;
use cbc::cipher::BlockModeEncrypt;
use cbc::cipher::KeyIvInit;

use crate::error::CryptoError;

/// AES-256-ECB decrypt (used for depot chunk decryption).
///
/// Decrypts `data` in place using ECB mode with PKCS7 padding.
/// The first 16 bytes are an ECB-encrypted IV, followed by CBC-encrypted payload.
pub fn symmetric_decrypt_ecb(key: &[u8; 32], data: &[u8]) -> Result<Vec<u8>, CryptoError> {
    if data.len() < 32 {
        return Err(CryptoError::DecryptionFailed);
    }

    let ecb_cipher = Aes256::new(key.into());

    // Decrypt IV (first block)
    let mut iv_block = aes::Block::default();
    iv_block.copy_from_slice(&data[..16]);
    ecb_cipher.decrypt_block(&mut iv_block);
    let iv: [u8; 16] = iv_block.into();

    // Decrypt payload with CBC
    cbc::Decryptor::<Aes256>::new(key.into(), (&iv).into())
        .decrypt_padded_vec::<Pkcs7>(&data[16..])
        .map_err(|_| CryptoError::InvalidPadding)
}

/// AES-256-CBC encrypt with a given IV.
pub fn symmetric_encrypt_cbc(key: &[u8; 32], iv: &[u8; 16], plaintext: &[u8]) -> Vec<u8> {
    cbc::Encryptor::<Aes256>::new(key.into(), iv.into())
        .encrypt_padded_vec::<Pkcs7>(plaintext)
}

/// AES-256-CBC decrypt with a given IV.
pub fn symmetric_decrypt_cbc(
    key: &[u8; 32],
    iv: &[u8; 16],
    ciphertext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    cbc::Decryptor::<Aes256>::new(key.into(), iv.into())
        .decrypt_padded_vec::<Pkcs7>(ciphertext)
        .map_err(|_| CryptoError::InvalidPadding)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cbc_round_trip() {
        let key = [0x42u8; 32];
        let iv = [0x01u8; 16];
        let plaintext = b"test data for encryption";

        let encrypted = symmetric_encrypt_cbc(&key, &iv, plaintext);
        let decrypted = symmetric_decrypt_cbc(&key, &iv, &encrypted).unwrap();

        assert_eq!(&decrypted, plaintext);
    }
}
