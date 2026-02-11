// Cryptographic utilities
use rand::Rng;
use std::time::Duration;
use tokio::time::sleep as tokio_sleep;

const ALPHANUMERIC: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

/// Generate a cryptographically secure random alphanumeric string
///
/// # Arguments
/// * `length` - Length of the string to generate (default: 64)
///
/// # Returns
/// A random string of the specified length
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
pub async fn sleep(ms: u64) {
    tokio_sleep(Duration::from_millis(ms)).await;
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
}
