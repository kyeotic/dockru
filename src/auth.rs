// Authentication and security utilities for Phase 4
use anyhow::{Context, Result};
use bcrypt::{hash, verify};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use sha3::Shake256;

/// Number of bcrypt rounds (matches TypeScript bcryptjs saltRounds = 10)
pub const BCRYPT_COST: u32 = 10;

/// Length of shake256 password hash for JWT (16 bytes = 32 hex chars)
pub const SHAKE256_LENGTH: usize = 16;

/// JWT token payload
#[derive(Debug, Serialize, Deserialize)]
pub struct JwtPayload {
    pub username: String,
    pub h: String, // shake256 hash of password
}

/// Generate a bcrypt hash from a password
///
/// # Arguments
/// * `password` - Plain text password to hash
///
/// # Returns
/// Bcrypt hashed password string
pub fn hash_password(password: &str) -> Result<String> {
    hash(password, BCRYPT_COST).context("Failed to hash password with bcrypt")
}

/// Verify a password against a bcrypt hash
///
/// # Arguments
/// * `password` - Plain text password to verify
/// * `hash` - Bcrypt hash to verify against
///
/// # Returns
/// `true` if password matches, `false` otherwise
pub fn verify_password(password: &str, hash: &str) -> Result<bool> {
    verify(password, hash).context("Failed to verify password with bcrypt")
}

/// Check if a hash needs to be rehashed with current cost
///
/// Bcrypt hashes encode the cost factor in the hash string itself.
/// When BCRYPT_COST is increased in the code, this function detects
/// old hashes that need to be upgraded on next login.
///
/// # Arguments
/// * `hash` - Bcrypt hash to check (format: $2b$10$...)
///
/// # Returns
/// `true` if hash cost differs from BCRYPT_COST or hash format is invalid
/// `false` only if hash is valid and cost matches BCRYPT_COST
///
/// # Security Note
/// Returns `true` (needs rehash) for unparseable hashes as a safe default.
/// This ensures malformed or unknown hash formats get replaced with fresh,
/// verifiable hashes using the current cost.
pub fn need_rehash_password(hash: &str) -> bool {
    // Bcrypt hash format: $2a$10$saltsaltsaltsaltsalthashhashhashhashhashhashhash
    // Parts: [$, 2a/2b/2y, cost, salt+hash]
    let parts: Vec<&str> = hash.split('$').collect();

    if parts.len() < 4 {
        // Invalid hash format - needs fresh hash
        return true;
    }

    // Cost is at index 2 (after empty string and version)
    if let Ok(hash_cost) = parts[2].parse::<u32>() {
        hash_cost != BCRYPT_COST
    } else {
        // Failed to parse cost - needs fresh hash
        true
    }
}

/// Generate a shake256 hash of data
///
/// This is used for JWT password fingerprinting to detect password changes
///
/// # Arguments
/// * `data` - Input string to hash
/// * `len` - Output length in bytes
///
/// # Returns
/// Hex-encoded hash string (length = `len * 2`)
pub fn shake256(data: &str, len: usize) -> String {
    if data.is_empty() {
        return String::new();
    }

    use sha3::digest::{ExtendableOutput, Update, XofReader};

    let mut hasher = Shake256::default();
    hasher.update(data.as_bytes());

    let mut reader = hasher.finalize_xof();
    let mut output = vec![0u8; len];
    reader.read(&mut output);

    hex::encode(output)
}

/// Create a JWT token for a user
///
/// Token payload contains username and shake256 hash of password.
/// This allows detecting if the password changed since token was issued.
///
/// # Arguments
/// * `username` - Username to include in token
/// * `password` - Password to fingerprint (not the hash!)
/// * `secret` - JWT signing secret
///
/// # Returns
/// JWT token string
pub fn create_jwt(username: &str, password: &str, secret: &str) -> Result<String> {
    let payload = JwtPayload {
        username: username.to_string(),
        h: shake256(password, SHAKE256_LENGTH),
    };

    encode(
        &Header::default(),
        &payload,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .context("Failed to create JWT token")
}

/// Verify and decode a JWT token
///
/// # Arguments
/// * `token` - JWT token string
/// * `secret` - JWT signing secret
///
/// # Returns
/// Decoded JWT payload
pub fn verify_jwt(token: &str, secret: &str) -> Result<JwtPayload> {
    let mut validation = Validation::default();
    // Don't require exp claim - matches TypeScript implementation
    validation.required_spec_claims.clear();

    let token_data = decode::<JwtPayload>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .context("Failed to verify JWT token")?;

    Ok(token_data.claims)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_and_verify_password() {
        let password = "test_password_123";
        let hash = hash_password(password).unwrap();

        // Should verify correctly
        assert!(verify_password(password, &hash).unwrap());

        // Should fail with wrong password
        assert!(!verify_password("wrong_password", &hash).unwrap());
    }

    #[test]
    fn test_shake256() {
        let data = "test_data";
        let hash = shake256(data, 16);

        // Should be 32 hex chars (16 bytes)
        assert_eq!(hash.len(), 32);

        // Should be consistent
        assert_eq!(shake256(data, 16), hash);

        // Different data should produce different hash
        assert_ne!(shake256("other_data", 16), hash);

        // Empty string should return empty
        assert_eq!(shake256("", 16), "");
    }

    #[test]
    fn test_create_and_verify_jwt() {
        let username = "testuser";
        let password = "password123";
        let secret = "test_secret";

        let token = create_jwt(username, password, secret).unwrap();

        // Should decode successfully
        let payload = verify_jwt(&token, secret).unwrap();
        assert_eq!(payload.username, username);
        assert_eq!(payload.h, shake256(password, SHAKE256_LENGTH));

        // Should fail with wrong secret
        assert!(verify_jwt(&token, "wrong_secret").is_err());

        // Should fail with invalid token
        assert!(verify_jwt("invalid.token.here", secret).is_err());
    }

    #[test]
    fn test_jwt_detects_password_change() {
        let username = "testuser";
        let password1 = "password123";
        let password2 = "different_password";
        let secret = "test_secret";

        let token = create_jwt(username, password1, secret).unwrap();
        let payload = verify_jwt(&token, secret).unwrap();

        // Hash should match original password
        assert_eq!(payload.h, shake256(password1, SHAKE256_LENGTH));

        // Hash should NOT match different password
        assert_ne!(payload.h, shake256(password2, SHAKE256_LENGTH));
    }

    #[test]
    fn test_need_rehash() {
        // Should return false for hash with current cost (10)
        assert!(!need_rehash_password("$2b$10$abcdef..."));

        // Should return true for hash with different cost
        assert!(need_rehash_password("$2b$08$abcdef..."));
        assert!(need_rehash_password("$2b$12$abcdef..."));

        // Should handle different bcrypt versions with same cost
        assert!(!need_rehash_password("$2a$10$abcdef..."));
        assert!(!need_rehash_password("$2y$10$abcdef..."));

        // Should return true for invalid hash formats (safe default)
        assert!(need_rehash_password("invalid"));
        assert!(need_rehash_password("$2b$"));
        assert!(need_rehash_password(""));
        assert!(need_rehash_password("$2b$notanumber$..."));
    }
}
