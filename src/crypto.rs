use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use windows::Win32::Security::Cryptography::{
    CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
};
use windows::Win32::System::Com::CoTaskMemFree;
use zeroize::Zeroize;

/// Encrypts plaintext using Windows DPAPI (user-scope).
/// Returns Base64-encoded ciphertext suitable for TOML storage.
///
/// # Security
/// - Uses CRYPTPROTECT_UI_FORBIDDEN to prevent UI prompts
/// - User-scope encryption (tied to current Windows user account)
/// - Plaintext is zeroized after encryption
///
/// # Errors
/// Returns error if DPAPI encryption fails or Base64 encoding fails.
pub fn encrypt(plaintext: &str) -> Result<String> {
    if plaintext.is_empty() {
        return Err(anyhow::anyhow!("Cannot encrypt empty string"));
    }

    // Convert plaintext to bytes (will be zeroized later)
    let mut plaintext_bytes = plaintext.as_bytes().to_vec();

    // Prepare input blob for DPAPI
    let input_blob = CRYPT_INTEGER_BLOB {
        cbData: plaintext_bytes.len() as u32,
        pbData: plaintext_bytes.as_mut_ptr(),
    };

    // Output blob (will be allocated by DPAPI)
    let mut output_blob = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };

    // Call DPAPI CryptProtectData
    let result = unsafe {
        CryptProtectData(
            &input_blob,
            None,                          // No description
            None,                          // No optional entropy
            None,                          // Reserved
            None,                          // No prompt struct
            CRYPTPROTECT_UI_FORBIDDEN,     // No UI prompts
            &mut output_blob,
        )
    };

    // Zeroize plaintext immediately after encryption attempt
    plaintext_bytes.zeroize();

    // Check if encryption succeeded
    if result.is_err() {
        return Err(anyhow::anyhow!("DPAPI CryptProtectData failed: {:?}", result.err()));
    }

    // Extract encrypted data from output blob
    let encrypted_data = unsafe {
        std::slice::from_raw_parts(output_blob.pbData, output_blob.cbData as usize).to_vec()
    };

    // Free memory allocated by DPAPI
    unsafe {
        CoTaskMemFree(Some(output_blob.pbData as *const _));
    }

    // Base64 encode for TOML storage
    let encoded = BASE64.encode(&encrypted_data);

    Ok(encoded)
}

/// Decrypts Base64-encoded ciphertext using Windows DPAPI.
/// Returns original plaintext string.
///
/// # Security
/// - Uses CRYPTPROTECT_UI_FORBIDDEN to prevent UI prompts
/// - Only works if encrypted by same Windows user account
/// - Decrypted data is zeroized after conversion to String
///
/// # Errors
/// Returns error if Base64 decoding fails, DPAPI decryption fails, or UTF-8 conversion fails.
pub fn decrypt(ciphertext: &str) -> Result<String> {
    if ciphertext.is_empty() {
        return Err(anyhow::anyhow!("Cannot decrypt empty string"));
    }

    // Base64 decode
    let mut encrypted_data = BASE64
        .decode(ciphertext)
        .context("Invalid Base64 encoding")?;

    // Prepare input blob for DPAPI
    let input_blob = CRYPT_INTEGER_BLOB {
        cbData: encrypted_data.len() as u32,
        pbData: encrypted_data.as_mut_ptr(),
    };

    // Output blob (will be allocated by DPAPI)
    let mut output_blob = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };

    // Call DPAPI CryptUnprotectData
    let result = unsafe {
        CryptUnprotectData(
            &input_blob,
            None,                          // No description output
            None,                          // No optional entropy
            None,                          // Reserved
            None,                          // No prompt struct
            CRYPTPROTECT_UI_FORBIDDEN,     // No UI prompts
            &mut output_blob,
        )
    };

    // Zeroize encrypted data
    encrypted_data.zeroize();

    // Check if decryption succeeded
    if result.is_err() {
        return Err(anyhow::anyhow!(
            "DPAPI CryptUnprotectData failed (wrong user or corrupted data): {:?}",
            result.err()
        ));
    }

    // Extract decrypted data from output blob
    let mut decrypted_data = unsafe {
        std::slice::from_raw_parts(output_blob.pbData, output_blob.cbData as usize).to_vec()
    };

    // Free memory allocated by DPAPI
    unsafe {
        CoTaskMemFree(Some(output_blob.pbData as *const _));
    }

    // Convert to String (UTF-8)
    let plaintext = String::from_utf8(decrypted_data.clone())
        .context("Decrypted data is not valid UTF-8")?;

    // Zeroize decrypted data
    decrypted_data.zeroize();

    Ok(plaintext)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dpapi_roundtrip() {
        let original = "my_secret_access_key_12345";
        let encrypted = encrypt(original).expect("Encryption failed");
        let decrypted = decrypt(&encrypted).expect("Decryption failed");
        assert_eq!(original, decrypted, "Roundtrip failed: plaintext mismatch");
    }

    #[test]
    fn test_encrypted_is_base64() {
        let plaintext = "test_secret";
        let encrypted = encrypt(plaintext).expect("Encryption failed");

        // Should be valid Base64
        let decoded = BASE64.decode(&encrypted);
        assert!(decoded.is_ok(), "Encrypted output is not valid Base64");

        // Should not be empty
        assert!(!encrypted.is_empty(), "Encrypted output is empty");
    }

    #[test]
    fn test_encrypted_not_plaintext() {
        let plaintext = "my_secret_password";
        let encrypted = encrypt(plaintext).expect("Encryption failed");

        // Encrypted output should NOT contain plaintext
        assert!(
            !encrypted.contains(plaintext),
            "Encrypted output contains plaintext (not encrypted!)"
        );

        // Encrypted output should be different from plaintext
        assert_ne!(
            encrypted, plaintext,
            "Encrypted output is identical to plaintext"
        );
    }

    #[test]
    fn test_invalid_base64_errors() {
        let invalid_base64 = "not_valid_base64!!!";
        let result = decrypt(invalid_base64);
        assert!(result.is_err(), "Should fail on invalid Base64");
    }

    #[test]
    fn test_empty_input_errors() {
        let result_encrypt = encrypt("");
        assert!(result_encrypt.is_err(), "Should fail on empty plaintext");

        let result_decrypt = decrypt("");
        assert!(result_decrypt.is_err(), "Should fail on empty ciphertext");
    }

    #[test]
    fn test_corrupted_ciphertext_errors() {
        // Valid Base64 but not DPAPI-encrypted data
        let fake_encrypted = BASE64.encode(b"random_garbage_data");
        let result = decrypt(&fake_encrypted);
        assert!(
            result.is_err(),
            "Should fail on corrupted/non-DPAPI ciphertext"
        );
    }

    #[test]
    fn test_multiple_encryptions_different() {
        let plaintext = "same_secret";
        let encrypted1 = encrypt(plaintext).expect("Encryption 1 failed");
        let encrypted2 = encrypt(plaintext).expect("Encryption 2 failed");

        // DPAPI may produce different ciphertext for same plaintext (due to random salt)
        // But both should decrypt to same plaintext
        let decrypted1 = decrypt(&encrypted1).expect("Decryption 1 failed");
        let decrypted2 = decrypt(&encrypted2).expect("Decryption 2 failed");

        assert_eq!(decrypted1, plaintext);
        assert_eq!(decrypted2, plaintext);
    }

    #[test]
    fn test_unicode_plaintext() {
        let plaintext = "Zażółć gęślą jaźń 🔒🔑";
        let encrypted = encrypt(plaintext).expect("Encryption failed");
        let decrypted = decrypt(&encrypted).expect("Decryption failed");
        assert_eq!(plaintext, decrypted, "Unicode roundtrip failed");
    }
}
