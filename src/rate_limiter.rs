// Rate limiting for authentication and API endpoints
use governor::{
    clock::DefaultClock,
    state::{InMemoryState, direct::NotKeyed, keyed::DefaultKeyedStateStore},
    Quota, RateLimiter as GovernorRateLimiter,
};
use std::net::IpAddr;
use std::num::NonZeroU32;
use std::sync::Arc;

/// Rate limiter for login attempts (20 per minute)
pub struct LoginRateLimiter {
    limiter: Arc<GovernorRateLimiter<IpAddr, DefaultKeyedStateStore<IpAddr>, DefaultClock>>,
    error_message: String,
}

impl LoginRateLimiter {
    pub fn new() -> Self {
        let quota = Quota::per_minute(NonZeroU32::new(20).unwrap());
        Self {
            limiter: Arc::new(GovernorRateLimiter::dashmap(quota)),
            error_message: "Too frequently, try again later.".to_string(),
        }
    }

    /// Check if request should be allowed
    ///
    /// # Arguments
    /// * `ip` - Client IP address
    ///
    /// # Returns
    /// `Ok(())` if allowed, `Err(message)` if rate limited
    pub fn check(&self, ip: IpAddr) -> Result<(), String> {
        match self.limiter.check_key(&ip) {
            Ok(_) => Ok(()),
            Err(_) => Err(self.error_message.clone()),
        }
    }
}

/// Rate limiter for 2FA attempts (30 per minute)
pub struct TwoFaRateLimiter {
    limiter: Arc<GovernorRateLimiter<IpAddr, DefaultKeyedStateStore<IpAddr>, DefaultClock>>,
    error_message: String,
}

impl TwoFaRateLimiter {
    pub fn new() -> Self {
        let quota = Quota::per_minute(NonZeroU32::new(30).unwrap());
        Self {
            limiter: Arc::new(GovernorRateLimiter::dashmap(quota)),
            error_message: "Too frequently, try again later.".to_string(),
        }
    }

    /// Check if request should be allowed
    pub fn check(&self, ip: IpAddr) -> Result<(), String> {
        match self.limiter.check_key(&ip) {
            Ok(_) => Ok(()),
            Err(_) => Err(self.error_message.clone()),
        }
    }
}

/// Rate limiter for API requests (60 per minute)
pub struct ApiRateLimiter {
    limiter: Arc<GovernorRateLimiter<IpAddr, DefaultKeyedStateStore<IpAddr>, DefaultClock>>,
    error_message: String,
}

impl ApiRateLimiter {
    pub fn new() -> Self {
        let quota = Quota::per_minute(NonZeroU32::new(60).unwrap());
        Self {
            limiter: Arc::new(GovernorRateLimiter::dashmap(quota)),
            error_message: "Too frequently, try again later.".to_string(),
        }
    }

    /// Check if request should be allowed
    pub fn check(&self, ip: IpAddr) -> Result<(), String> {
        match self.limiter.check_key(&ip) {
            Ok(_) => Ok(()),
            Err(_) => Err(self.error_message.clone()),
        }
    }
}

/// Global rate limiters singleton
pub struct RateLimiters {
    pub login: LoginRateLimiter,
    pub two_fa: TwoFaRateLimiter,
    pub api: ApiRateLimiter,
}

impl RateLimiters {
    pub fn new() -> Self {
        Self {
            login: LoginRateLimiter::new(),
            two_fa: TwoFaRateLimiter::new(),
            api: ApiRateLimiter::new(),
        }
    }
}

impl Default for RateLimiters {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_login_rate_limiter() {
        let limiter = LoginRateLimiter::new();
        let ip = IpAddr::from_str("127.0.0.1").unwrap();

        // First 20 requests should succeed
        for _ in 0..20 {
            assert!(limiter.check(ip).is_ok());
        }

        // 21st request should fail
        assert!(limiter.check(ip).is_err());
    }

    #[test]
    fn test_two_fa_rate_limiter() {
        let limiter = TwoFaRateLimiter::new();
        let ip = IpAddr::from_str("127.0.0.1").unwrap();

        // First 30 requests should succeed
        for _ in 0..30 {
            assert!(limiter.check(ip).is_ok());
        }

        // 31st request should fail
        assert!(limiter.check(ip).is_err());
    }

    #[test]
    fn test_api_rate_limiter() {
        let limiter = ApiRateLimiter::new();
        let ip = IpAddr::from_str("127.0.0.1").unwrap();

        // First 60 requests should succeed
        for _ in 0..60 {
            assert!(limiter.check(ip).is_ok());
        }

        // 61st request should fail
        assert!(limiter.check(ip).is_err());
    }

    #[test]
    fn test_different_ips_independent() {
        let limiter = LoginRateLimiter::new();
        let ip1 = IpAddr::from_str("127.0.0.1").unwrap();
        let ip2 = IpAddr::from_str("192.168.1.1").unwrap();

        // Use up all tokens for ip1
        for _ in 0..20 {
            limiter.check(ip1).unwrap();
        }
        assert!(limiter.check(ip1).is_err());

        // ip2 should still work
        assert!(limiter.check(ip2).is_ok());
    }
}
