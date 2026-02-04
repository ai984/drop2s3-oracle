use anyhow::{anyhow, Context, Result};
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    XChaCha20Poly1305,
};
use serde::{Deserialize, Serialize};

const NONCE_LEN: usize = 24;
const KEY_LEN: usize = 32;

const EMBEDDED_KEY_XOR: [u8; 32] = [
    0xd7, 0x2a, 0x8f, 0x3e, 0x5b, 0xc1, 0x94, 0x6d,
    0xe8, 0x17, 0xa3, 0x4c, 0x9f, 0x62, 0xd5, 0x28,
    0x7b, 0xce, 0x41, 0x96, 0x0d, 0xfa, 0x53, 0xb8,
    0x2f, 0x84, 0xe9, 0x16, 0x6b, 0xc0, 0x35, 0x8a,
];

const EMBEDDED_KEY_MASK: [u8; 32] = [
    0xa4, 0x59, 0xfc, 0x4d, 0x28, 0xb2, 0xe7, 0x1e,
    0x9b, 0x64, 0xd0, 0x3f, 0xec, 0x11, 0xa6, 0x5b,
    0x08, 0xbd, 0x32, 0xe5, 0x7e, 0x89, 0x20, 0xcb,
    0x5c, 0xf7, 0x9a, 0x65, 0x18, 0xb3, 0x46, 0xf9,
];

fn get_embedded_key() -> [u8; KEY_LEN] {
    let mut key = [0u8; KEY_LEN];
    for i in 0..KEY_LEN {
        key[i] = EMBEDDED_KEY_XOR[i] ^ EMBEDDED_KEY_MASK[i];
    }
    key
}

/// Encrypted credentials stored in config.toml
/// Format: base64(nonce || ciphertext)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedCredentials {
    pub version: u8,
    pub data: String,
}

#[derive(Serialize, Deserialize)]
struct CredentialsPayload {
    access_key: String,
    secret_key: String,
}

/// Encrypt credentials with embedded key (for admin CLI tool)
pub fn encrypt_credentials(access_key: &str, secret_key: &str) -> Result<EncryptedCredentials> {
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::fill(&mut nonce_bytes);
    
    let key = get_embedded_key();
    let cipher = XChaCha20Poly1305::new_from_slice(&key)
        .map_err(|e| anyhow!("Cipher creation failed: {e}"))?;
    
    let payload = CredentialsPayload {
        access_key: access_key.to_string(),
        secret_key: secret_key.to_string(),
    };
    let plaintext = serde_json::to_vec(&payload)
        .context("Failed to serialize credentials")?;
    
    let nonce = chacha20poly1305::XNonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_ref())
        .map_err(|e| anyhow!("Encryption failed: {e}"))?;
    
    let mut combined = Vec::with_capacity(NONCE_LEN + ciphertext.len());
    combined.extend_from_slice(&nonce_bytes);
    combined.extend_from_slice(&ciphertext);
    
    use base64::Engine;
    let data = base64::engine::general_purpose::STANDARD.encode(&combined);
    
    Ok(EncryptedCredentials { version: 2, data })
}

/// Decrypt credentials with embedded key
pub fn decrypt_credentials(encrypted: &EncryptedCredentials) -> Result<(String, String)> {
    if encrypted.version != 2 {
        return Err(anyhow!("Unsupported credentials version: {} (expected 2)", encrypted.version));
    }
    
    use base64::Engine;
    let combined = base64::engine::general_purpose::STANDARD
        .decode(&encrypted.data)
        .context("Invalid base64 in credentials")?;
    
    if combined.len() < NONCE_LEN + 16 {
        return Err(anyhow!("Encrypted data too short"));
    }
    
    let nonce_bytes = &combined[..NONCE_LEN];
    let ciphertext = &combined[NONCE_LEN..];
    
    let key = get_embedded_key();
    let cipher = XChaCha20Poly1305::new_from_slice(&key)
        .map_err(|e| anyhow!("Cipher creation failed: {e}"))?;
    
    let nonce = chacha20poly1305::XNonce::from_slice(nonce_bytes);
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| anyhow!("Decryption failed - corrupted data"))?;
    
    let payload: CredentialsPayload = serde_json::from_slice(&plaintext)
        .context("Failed to parse decrypted credentials")?;
    
    Ok((payload.access_key, payload.secret_key))
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let access_key = "AKIAIOSFODNN7EXAMPLE";
        let secret_key = "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY";
        
        let encrypted = encrypt_credentials(access_key, secret_key).unwrap();
        let (dec_access, dec_secret) = decrypt_credentials(&encrypted).unwrap();
        
        assert_eq!(access_key, dec_access);
        assert_eq!(secret_key, dec_secret);
    }
    
    #[test]
    fn test_different_encryptions_produce_different_output() {
        let access_key = "key";
        let secret_key = "secret";
        
        let encrypted1 = encrypt_credentials(access_key, secret_key).unwrap();
        let encrypted2 = encrypt_credentials(access_key, secret_key).unwrap();
        
        assert_ne!(encrypted1.data, encrypted2.data);
    }
    
    #[test]
    fn test_embedded_key_consistency() {
        let key1 = get_embedded_key();
        let key2 = get_embedded_key();
        assert_eq!(key1, key2);
        assert_eq!(key1.len(), KEY_LEN);
    }
}
