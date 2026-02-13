// Version checking module (Phase 10)
//
// Fetches the latest version from https://dockru.kuma.pet/version
// and stores it for client broadcast. Runs every 48 hours.

use anyhow::{Context, Result};
use serde::Deserialize;
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

use crate::db::models::setting::SettingsCache;
use crate::db::models::Setting;

/// Version check response from the update server
#[derive(Debug, Deserialize)]
struct VersionResponse {
    /// Stable/slow release version
    slow: Option<String>,
    /// Beta release version (not used yet)
    #[allow(dead_code)]
    beta: Option<String>,
}

/// Version checker that periodically checks for updates
#[derive(Clone)]
pub struct VersionChecker {
    /// Current version of Dockru
    version: String,
    /// Latest available version (None until first check)
    latest_version: Arc<RwLock<Option<String>>>,
}

impl VersionChecker {
    /// Create a new version checker
    pub fn new(version: String) -> Self {
        Self {
            version,
            latest_version: Arc::new(RwLock::new(None)),
        }
    }

    /// Get the current version
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Get the latest available version
    pub async fn latest_version(&self) -> Option<String> {
        self.latest_version.read().await.clone()
    }

    /// Check for updates now
    ///
    /// Returns Ok(true) if a check was performed, Ok(false) if disabled
    pub async fn check_now(&self, pool: &SqlitePool, cache: &SettingsCache) -> Result<bool> {
        // Check if update checking is enabled
        let check_update = Setting::get(pool, cache, "checkUpdate")
            .await?
            .and_then(|v| v.as_bool())
            .unwrap_or(true); // Default to true if not set

        if !check_update {
            debug!("Version check disabled in settings");
            return Ok(false);
        }

        info!("Checking for updates from https://dockru.kuma.pet/version");

        // Fetch version info
        let response = reqwest::get("https://dockru.kuma.pet/version")
            .await
            .context("Failed to fetch version info")?;

        let data: VersionResponse = response
            .json()
            .await
            .context("Failed to parse version response")?;

        // For now, only use stable/slow channel
        if let Some(slow_version) = data.slow {
            let mut latest = self.latest_version.write().await;
            *latest = Some(slow_version.clone());
            info!("Latest stable version: {}", slow_version);
        }

        Ok(true)
    }

    /// Start periodic version checking (every 48 hours)
    ///
    /// Returns a task handle that can be aborted to stop checking
    pub fn start_interval(
        &self,
        pool: SqlitePool,
        cache: SettingsCache,
    ) -> tokio::task::JoinHandle<()> {
        let checker = self.clone();

        tokio::spawn(async move {
            // Check immediately on startup
            if let Err(e) = checker.check_now(&pool, &cache).await {
                info!("Failed to check for updates: {}", e);
            }

            // Then check every 48 hours
            let mut interval =
                tokio::time::interval(tokio::time::Duration::from_secs(48 * 60 * 60));
            interval.tick().await; // First tick completes immediately

            loop {
                interval.tick().await;

                if let Err(e) = checker.check_now(&pool, &cache).await {
                    info!("Failed to check for updates: {}", e);
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_checker_creation() {
        let checker = VersionChecker::new("1.5.0".to_string());
        assert_eq!(checker.version(), "1.5.0");
    }

    #[tokio::test]
    async fn test_latest_version_initially_none() {
        let checker = VersionChecker::new("1.5.0".to_string());
        assert_eq!(checker.latest_version().await, None);
    }
}
