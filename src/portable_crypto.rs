use anyhow::{anyhow, Context, Result};
use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::{
    aead::{Aead, KeyInit, OsRng},
    XChaCha20Poly1305,
};
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 24; // XChaCha20 uses 24-byte nonce
const KEY_LEN: usize = 32;

/// Encrypted credentials stored in config.toml
/// Format: base64(salt || nonce || ciphertext)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedCredentials {
    /// Version for future format migrations
    pub version: u8,
    /// Base64 encoded: salt (16) || nonce (24) || ciphertext (variable)
    pub data: String,
}

#[derive(Serialize, Deserialize)]
struct CredentialsPayload {
    access_key: String,
    secret_key: String,
}

/// Derive 32-byte key from password using Argon2id
/// RFC 9106 "SECOND RECOMMENDED" params: 64 MiB, 3 iterations, 4 parallelism
fn derive_key(password: &str, salt: &[u8]) -> Result<[u8; KEY_LEN]> {
    let params = Params::new(65536, 3, 4, Some(KEY_LEN))
        .map_err(|e| anyhow!("Invalid Argon2 params: {}", e))?;
    
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    
    let mut key = [0u8; KEY_LEN];
    argon2
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .map_err(|e| anyhow!("Key derivation failed: {}", e))?;
    
    Ok(key)
}

/// Encrypt credentials with password
pub fn encrypt_credentials(
    password: &str,
    access_key: &str,
    secret_key: &str,
) -> Result<EncryptedCredentials> {
    use rand::RngCore;
    
    let mut salt = [0u8; SALT_LEN];
    let mut nonce_bytes = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut salt);
    OsRng.fill_bytes(&mut nonce_bytes);
    
    let mut key = derive_key(password, &salt)?;
    
    let cipher = XChaCha20Poly1305::new_from_slice(&key)
        .map_err(|e| anyhow!("Cipher creation failed: {}", e))?;
    
    // Zeroize key immediately
    key.zeroize();
    
    let payload = CredentialsPayload {
        access_key: access_key.to_string(),
        secret_key: secret_key.to_string(),
    };
    let plaintext = serde_json::to_vec(&payload)
        .context("Failed to serialize credentials")?;
    
    let nonce = chacha20poly1305::XNonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_ref())
        .map_err(|e| anyhow!("Encryption failed: {}", e))?;
    
    // Combine: salt || nonce || ciphertext
    let mut combined = Vec::with_capacity(SALT_LEN + NONCE_LEN + ciphertext.len());
    combined.extend_from_slice(&salt);
    combined.extend_from_slice(&nonce_bytes);
    combined.extend_from_slice(&ciphertext);
    
    use base64::Engine;
    let data = base64::engine::general_purpose::STANDARD.encode(&combined);
    
    Ok(EncryptedCredentials { version: 1, data })
}

/// Decrypt credentials with password
pub fn decrypt_credentials(
    password: &str,
    encrypted: &EncryptedCredentials,
) -> Result<(String, String)> {
    if encrypted.version != 1 {
        return Err(anyhow!("Unsupported credentials version: {}", encrypted.version));
    }
    
    use base64::Engine;
    let combined = base64::engine::general_purpose::STANDARD
        .decode(&encrypted.data)
        .context("Invalid base64 in credentials")?;
    
    if combined.len() < SALT_LEN + NONCE_LEN + 16 {
        return Err(anyhow!("Encrypted data too short"));
    }
    
    let salt = &combined[..SALT_LEN];
    let nonce_bytes = &combined[SALT_LEN..SALT_LEN + NONCE_LEN];
    let ciphertext = &combined[SALT_LEN + NONCE_LEN..];
    
    let mut key = derive_key(password, salt)?;
    
    let cipher = XChaCha20Poly1305::new_from_slice(&key)
        .map_err(|e| anyhow!("Cipher creation failed: {}", e))?;
    
    // Zeroize key immediately
    key.zeroize();
    
    let nonce = chacha20poly1305::XNonce::from_slice(nonce_bytes);
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| anyhow!("Decryption failed - wrong password or corrupted data"))?;
    
    let payload: CredentialsPayload = serde_json::from_slice(&plaintext)
        .context("Failed to parse decrypted credentials")?;
    
    Ok((payload.access_key, payload.secret_key))
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let password = "test_password_123!";
        let access_key = "AKIAIOSFODNN7EXAMPLE";
        let secret_key = "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY";
        
        let encrypted = encrypt_credentials(password, access_key, secret_key).unwrap();
        let (dec_access, dec_secret) = decrypt_credentials(password, &encrypted).unwrap();
        
        assert_eq!(access_key, dec_access);
        assert_eq!(secret_key, dec_secret);
    }
    
    #[test]
    fn test_wrong_password_fails() {
        let password = "correct_password";
        let wrong_password = "wrong_password";
        
        let encrypted = encrypt_credentials(password, "key", "secret").unwrap();
        let result = decrypt_credentials(wrong_password, &encrypted);
        
        assert!(result.is_err());
    }
    
    #[test]
    fn test_different_encryptions_produce_different_output() {
        let password = "test_password";
        let access_key = "key";
        let secret_key = "secret";
        
        let encrypted1 = encrypt_credentials(password, access_key, secret_key).unwrap();
        let encrypted2 = encrypt_credentials(password, access_key, secret_key).unwrap();
        
        assert_ne!(encrypted1.data, encrypted2.data);
    }
}
