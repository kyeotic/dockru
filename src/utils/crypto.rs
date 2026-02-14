// Cryptographic utilities
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rand::Rng;
use redact::Secret;
use sha3::{Digest, Sha3_256};
use std::time::Duration;
use tokio::time::sleep as tokio_sleep;

#[allow(dead_code)]
const ALPHANUMERIC: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

/// Generate a cryptographically secure random alphanumeric string
///
/// # Arguments
/// * `length` - Length of the string to generate (default: 64)
///
/// # Returns
/// A random string of the specified length
#[allow(dead_code)]
pub fn gen_secret(length: usize) -> String {
    let mut rng = rand::thread_rng();
    (0..length)
        .map(|_| {
            let idx = get_crypto_random_int(&mut rng, 0, ALPHANUMERIC.len() - 1);
            ALPHANUMERIC[idx] as char
        })
        .collect()
}

/// Get a cryptographically secure random integer in the range [min, max]
///
/// # Arguments
/// * `rng` - Random number generator
/// * `min` - Minimum value (inclusive)
/// * `max` - Maximum value (inclusive)
///
/// # Returns
/// A random number in the range [min, max]
#[allow(dead_code)]
fn get_crypto_random_int<R: Rng>(rng: &mut R, min: usize, max: usize) -> usize {
    rng.gen_range(min..=max)
}

/// Generate a simple hash from a string
///
/// # Arguments
/// * `s` - Input string
/// * `length` - Hash range (default 10, meaning 0-9)
///
/// # Returns
/// An integer in the range [0, length)
#[allow(dead_code)]
pub fn int_hash(s: &str, length: usize) -> usize {
    let mut hash: usize = 0;
    for ch in s.chars() {
        hash = hash.wrapping_add(ch as usize);
    }
    (hash % length + length) % length
}

/// Async sleep for specified duration
///
/// # Arguments
/// * `ms` - Number of milliseconds to sleep
#[allow(dead_code)]
pub async fn sleep(ms: u64) {
    tokio_sleep(Duration::from_millis(ms)).await;
}

/// Prefix for encrypted values to distinguish them from plaintext
const ENCRYPTED_PREFIX: &str = "enc:";

/// Derive a 256-bit AES key from a secret string using SHA3-256.
///
/// # Arguments
/// * `secret` - The secret string (e.g. jwtSecret from DB)
///
/// # Returns
/// A 32-byte key suitable for AES-256-GCM
fn derive_encryption_key(secret: &Secret<String>) -> [u8; 32] {
    let mut hasher = Sha3_256::new();
    hasher.update(secret.expose_secret().as_bytes());
    let result = hasher.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(&result);
    key
}

/// Encrypt a plaintext string using AES-256-GCM.
///
/// The output is base64-encoded and prefixed with "enc:" to identify encrypted values.
/// Format: `enc:<base64(nonce + ciphertext)>`
///
/// # Arguments
/// * `plaintext` - The string to encrypt
/// * `secret` - Secret used to derive the encryption key
///
/// # Returns
/// An encrypted, base64-encoded string with "enc:" prefix
pub fn encrypt_password(plaintext: &Secret<String>, secret: &Secret<String>) -> Result<String> {
    let key = derive_encryption_key(secret);
    let cipher = Aes256Gcm::new_from_slice(&key).context("Failed to create AES-GCM cipher")?;

    // Generate a random 96-bit nonce
    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext.expose_secret().as_bytes())
        .map_err(|e| anyhow::anyhow!("Encryption failed: {}", e))?;

    // Concatenate nonce + ciphertext and base64-encode
    let mut combined = Vec::with_capacity(12 + ciphertext.len());
    combined.extend_from_slice(&nonce_bytes);
    combined.extend_from_slice(&ciphertext);

    Ok(format!("{}{}", ENCRYPTED_PREFIX, BASE64.encode(&combined)))
}

/// Decrypt an AES-256-GCM encrypted password string.
///
/// Expects input in the format: `enc:<base64(nonce + ciphertext)>`
///
/// # Arguments
/// * `encrypted` - The encrypted string (with "enc:" prefix)
/// * `secret` - Secret used to derive the encryption key
///
/// # Returns
/// The decrypted plaintext string
pub fn decrypt_password(encrypted: &str, secret: &Secret<String>) -> Result<Secret<String>> {
    let encoded = encrypted
        .strip_prefix(ENCRYPTED_PREFIX)
        .context("Encrypted value missing 'enc:' prefix")?;

    let combined = BASE64
        .decode(encoded)
        .context("Failed to base64-decode encrypted value")?;

    if combined.len() < 12 {
        return Err(anyhow::anyhow!("Encrypted data too short (missing nonce)"));
    }

    let (nonce_bytes, ciphertext) = combined.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);

    let key = derive_encryption_key(secret);
    let cipher = Aes256Gcm::new_from_slice(&key).context("Failed to create AES-GCM cipher")?;

    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| anyhow::anyhow!("Decryption failed: {}", e))?;

    Ok(Secret::new(
        String::from_utf8(plaintext).context("Decrypted value is not valid UTF-8")?,
    ))
}

/// Check if a stored password value is already encrypted.
pub fn is_password_encrypted(value: &str) -> bool {
    value.starts_with(ENCRYPTED_PREFIX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gen_secret_length() {
        let secret = gen_secret(32);
        assert_eq!(secret.len(), 32);

        let secret = gen_secret(64);
        assert_eq!(secret.len(), 64);
    }

    #[test]
    fn test_gen_secret_alphanumeric() {
        let secret = gen_secret(100);
        assert!(secret.chars().all(|c| c.is_alphanumeric()));
    }

    #[test]
    fn test_gen_secret_randomness() {
        let secret1 = gen_secret(64);
        let secret2 = gen_secret(64);
        // Should be extremely unlikely to generate the same secret
        assert_ne!(secret1, secret2);
    }

    #[test]
    fn test_int_hash() {
        // Same input should give same output
        assert_eq!(int_hash("hello", 10), int_hash("hello", 10));

        // Different inputs should (usually) give different outputs
        let h1 = int_hash("hello", 10);
        let h2 = int_hash("world", 10);
        // Results should be in range [0, 10)
        assert!(h1 < 10);
        assert!(h2 < 10);
    }

    #[test]
    fn test_int_hash_range() {
        let hash = int_hash("test", 5);
        assert!(hash < 5);

        let hash = int_hash("test", 100);
        assert!(hash < 100);
    }

    #[tokio::test]
    async fn test_sleep() {
        let start = std::time::Instant::now();
        sleep(100).await;
        let elapsed = start.elapsed();
        // Should sleep for at least 100ms
        assert!(elapsed.as_millis() >= 100);
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let secret = Secret::new("my_jwt_secret_value_12345".to_string());
        let plaintext = Secret::new("agent_password_123!@#".to_string());

        let encrypted = encrypt_password(&plaintext, &secret).unwrap();

        // Should have enc: prefix
        assert!(encrypted.starts_with("enc:"));
        assert!(is_password_encrypted(&encrypted));

        // Should decrypt back to original
        let decrypted = decrypt_password(&encrypted, &secret).unwrap();
        assert_eq!(decrypted.expose_secret(), "agent_password_123!@#");
    }

    #[test]
    fn test_encrypt_produces_different_outputs() {
        let secret = Secret::new("my_jwt_secret".to_string());
        let plaintext = Secret::new("same_password".to_string());

        let encrypted1 = encrypt_password(&plaintext, &secret).unwrap();
        let encrypted2 = encrypt_password(&plaintext, &secret).unwrap();

        // Different nonces should produce different ciphertexts
        assert_ne!(encrypted1, encrypted2);

        // Both should decrypt to the same value
        assert_eq!(
            decrypt_password(&encrypted1, &secret).unwrap().expose_secret(),
            decrypt_password(&encrypted2, &secret).unwrap().expose_secret()
        );
    }

    #[test]
    fn test_decrypt_wrong_secret_fails() {
        let correct_secret = Secret::new("correct_secret".to_string());
        let wrong_secret = Secret::new("wrong_secret".to_string());
        let plaintext = Secret::new("password".to_string());

        let encrypted = encrypt_password(&plaintext, &correct_secret).unwrap();
        let result = decrypt_password(&encrypted, &wrong_secret);
        assert!(result.is_err());
    }

    #[test]
    fn test_is_password_encrypted() {
        assert!(is_password_encrypted("enc:AAAA"));
        assert!(!is_password_encrypted("plaintext_password"));
        assert!(!is_password_encrypted(""));
    }

    #[test]
    fn test_decrypt_invalid_format() {
        let secret = Secret::new("test".to_string());
        // Missing prefix
        assert!(decrypt_password("not_encrypted", &secret).is_err());
        // Invalid base64
        assert!(decrypt_password("enc:!!!invalid!!!", &secret).is_err());
        // Too short (no nonce)
        assert!(decrypt_password("enc:AAAA", &secret).is_err());
    }

    #[test]
    fn test_encrypt_empty_password() {
        let secret = Secret::new("secret".to_string());
        let plaintext = Secret::new("".to_string());
        let encrypted = encrypt_password(&plaintext, &secret).unwrap();
        let decrypted = decrypt_password(&encrypted, &secret).unwrap();
        assert_eq!(decrypted.expose_secret(), "");
    }

    #[test]
    fn test_encrypt_unicode_password() {
        let secret = Secret::new("secret".to_string());
        let plaintext = Secret::new("pässwörd_日本語_🔒".to_string());
        let encrypted = encrypt_password(&plaintext, &secret).unwrap();
        let decrypted = decrypt_password(&encrypted, &secret).unwrap();
        assert_eq!(decrypted.expose_secret(), "pässwörd_日本語_🔒");
    }
}
