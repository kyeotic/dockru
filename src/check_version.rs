// Version checking module
//
// Hybrid version check using:
//   1. GitHub Releases API — detect semver bumps
//   2. GHCR image manifest — detect image updates at the same version
//
// Runs every 48 hours.

use anyhow::{Context, Result};
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

use crate::db::models::setting::SettingsCache;
use crate::db::models::Setting;

/// Version checker that periodically checks for updates via GitHub
#[derive(Clone)]
pub struct VersionChecker {
    /// Current version of Dockru
    version: String,
    /// Git commit SHA embedded at compile time
    current_sha: String,
    /// Latest available version from GitHub Releases (None until first check)
    latest_version: Arc<RwLock<Option<String>>>,
    /// SHA of latest GHCR image (None until first check)
    latest_image_sha: Arc<RwLock<Option<String>>>,
}

impl VersionChecker {
    /// Create a new version checker
    pub fn new(version: String) -> Self {
        Self {
            version,
            current_sha: env!("GIT_COMMIT_SHA").to_string(),
            latest_version: Arc::new(RwLock::new(None)),
            latest_image_sha: Arc::new(RwLock::new(None)),
        }
    }

    /// Get the current version
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Get the git commit SHA embedded at compile time
    pub fn current_sha(&self) -> &str {
        &self.current_sha
    }

    /// Get the latest available version from GitHub Releases
    pub async fn latest_version(&self) -> Option<String> {
        self.latest_version.read().await.clone()
    }

    /// Get the SHA of the latest GHCR image
    pub async fn latest_image_sha(&self) -> Option<String> {
        self.latest_image_sha.read().await.clone()
    }

    /// Check for updates now
    ///
    /// Returns Ok(true) if a check was performed, Ok(false) if disabled
    pub async fn check_now(&self, pool: &SqlitePool, cache: &SettingsCache) -> Result<bool> {
        // Skip version check in development mode
        if cfg!(debug_assertions) {
            debug!("Version check skipped in development mode");
            return Ok(false);
        }

        // Check if update checking is enabled
        let check_update = Setting::get(pool, cache, "checkUpdate")
            .await?
            .and_then(|v| v.as_bool())
            .unwrap_or(true); // Default to true if not set

        if !check_update {
            debug!("Version check disabled in settings");
            return Ok(false);
        }

        info!("Checking for updates");

        if let Err(e) = self.check_github_releases().await {
            info!("GitHub releases check failed: {}", e);
        }

        if let Err(e) = self.check_ghcr_image().await {
            info!("GHCR image check failed: {}", e);
        }

        Ok(true)
    }

    /// Check GitHub Releases API for the latest version
    async fn check_github_releases(&self) -> Result<()> {
        let url = "https://api.github.com/repos/kyeotic/dockru/releases/latest";
        let client = reqwest::Client::new();

        let response = client
            .get(url)
            .header(
                "User-Agent",
                format!("dockru/{}", self.version),
            )
            .header("Accept", "application/vnd.github+json")
            .send()
            .await
            .context("Failed to fetch GitHub releases")?;

        let data: serde_json::Value = response
            .json()
            .await
            .context("Failed to parse GitHub releases response")?;

        let tag = data["tag_name"]
            .as_str()
            .context("Missing tag_name in GitHub releases response")?;

        // Strip leading 'v' prefix if present
        let version = tag.trim_start_matches('v').to_string();

        info!("Latest GitHub release: {}", version);
        let mut latest = self.latest_version.write().await;
        *latest = Some(version);

        Ok(())
    }

    /// Check GHCR image manifest for the latest image SHA
    async fn check_ghcr_image(&self) -> Result<()> {
        let client = reqwest::Client::new();

        // Step 1: Get anonymous token for GHCR
        let token_url =
            "https://ghcr.io/token?service=ghcr.io&scope=repository:kyeotic/dockru:pull";
        let token_resp: serde_json::Value = client
            .get(token_url)
            .header("User-Agent", format!("dockru/{}", self.version))
            .send()
            .await
            .context("Failed to fetch GHCR token")?
            .json()
            .await
            .context("Failed to parse GHCR token response")?;

        let token = token_resp["token"]
            .as_str()
            .context("Missing token in GHCR token response")?
            .to_string();

        // Step 2: Fetch the manifest for the `latest` tag
        let manifest_url =
            "https://ghcr.io/v2/kyeotic/dockru/manifests/latest";
        let manifest_resp: serde_json::Value = client
            .get(manifest_url)
            .header("Authorization", format!("Bearer {}", token))
            .header(
                "Accept",
                "application/vnd.oci.image.manifest.v1+json",
            )
            .header("User-Agent", format!("dockru/{}", self.version))
            .send()
            .await
            .context("Failed to fetch GHCR manifest")?
            .json()
            .await
            .context("Failed to parse GHCR manifest response")?;

        let config_digest = manifest_resp["config"]["digest"]
            .as_str()
            .context("Missing config.digest in GHCR manifest")?
            .to_string();

        // Step 3: Fetch the config blob
        let blob_url = format!(
            "https://ghcr.io/v2/kyeotic/dockru/blobs/{}",
            config_digest
        );
        let blob_resp: serde_json::Value = client
            .get(&blob_url)
            .header("Authorization", format!("Bearer {}", token))
            .header("User-Agent", format!("dockru/{}", self.version))
            .send()
            .await
            .context("Failed to fetch GHCR config blob")?
            .json()
            .await
            .context("Failed to parse GHCR config blob")?;

        // Step 4: Extract the revision label
        let image_sha = blob_resp["config"]["Labels"]
            ["org.opencontainers.image.revision"]
            .as_str()
            .context("Missing org.opencontainers.image.revision label in GHCR config")?
            .to_string();

        info!("Latest GHCR image SHA: {}", &image_sha[..8.min(image_sha.len())]);
        let mut latest = self.latest_image_sha.write().await;
        *latest = Some(image_sha);

        Ok(())
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

    #[test]
    fn test_current_sha_not_empty() {
        let checker = VersionChecker::new("1.5.0".to_string());
        // SHA is embedded at compile time; just verify it's non-empty
        assert!(!checker.current_sha().is_empty());
    }

    #[tokio::test]
    async fn test_latest_version_initially_none() {
        let checker = VersionChecker::new("1.5.0".to_string());
        assert_eq!(checker.latest_version().await, None);
    }

    #[tokio::test]
    async fn test_latest_image_sha_initially_none() {
        let checker = VersionChecker::new("1.5.0".to_string());
        assert_eq!(checker.latest_image_sha().await, None);
    }
}
