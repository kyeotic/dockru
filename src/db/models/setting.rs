use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tokio::time::interval;
use tracing::debug;

/// Setting model representing a key-value setting in the system
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Setting {
    pub id: i64,
    pub key: String,
    pub value: Option<String>,
    #[serde(rename = "type")]
    #[sqlx(rename = "type")]
    pub setting_type: Option<String>,
}

/// Cache entry with timestamp for TTL
#[derive(Debug, Clone)]
struct CacheEntry {
    value: JsonValue,
    timestamp: u64,
}

/// Settings cache with automatic cleanup
#[derive(Clone)]
pub struct SettingsCache {
    cache: Arc<RwLock<HashMap<String, CacheEntry>>>,
    cleanup_started: Arc<tokio::sync::OnceCell<()>>,
}

impl Default for SettingsCache {
    fn default() -> Self {
        Self::new()
    }
}

impl SettingsCache {
    /// Create a new settings cache
    pub fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            cleanup_started: Arc::new(tokio::sync::OnceCell::new()),
        }
    }

    /// Start the cache cleanup task (runs every 60 seconds, removes entries older than 60s)
    fn start_cleanup(&self) {
        let cache = self.cache.clone();
        let cleanup_started = self.cleanup_started.clone();

        tokio::spawn(async move {
            // Only start the cleanup task once
            cleanup_started
                .get_or_init(|| async {
                    let cache = cache.clone();
                    tokio::spawn(async move {
                        let mut interval = interval(Duration::from_secs(60));
                        loop {
                            interval.tick().await;
                            debug!("Settings cache cleanup running");

                            let now = SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap()
                                .as_secs();

                            let mut cache_write = cache.write().await;
                            let mut to_remove = Vec::new();

                            for (key, entry) in cache_write.iter() {
                                if now - entry.timestamp > 60 {
                                    to_remove.push(key.clone());
                                }
                            }

                            for key in to_remove {
                                debug!("Cache cleanup: removing {}", key);
                                cache_write.remove(&key);
                            }
                        }
                    });
                })
                .await;
        });
    }

    /// Get a value from cache, returns None if not found or expired
    async fn get(&self, key: &str) -> Option<JsonValue> {
        let cache = self.cache.read().await;
        let entry = cache.get(key)?;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Check if entry is expired (older than 60 seconds)
        if now - entry.timestamp > 60 {
            return None;
        }

        Some(entry.value.clone())
    }

    /// Set a value in cache with current timestamp
    async fn set(&self, key: String, value: JsonValue) {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let entry = CacheEntry { value, timestamp };

        let mut cache = self.cache.write().await;
        cache.insert(key, entry);
    }

    /// Delete specific keys from cache
    async fn delete(&self, keys: &[String]) {
        let mut cache = self.cache.write().await;
        for key in keys {
            cache.remove(key);
        }
    }

    /// Clear all cached values
    async fn clear(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
    }
}

impl Setting {
    /// Get a single setting value by key
    ///
    /// This method uses an in-memory cache with 60 second TTL
    pub async fn get(pool: &SqlitePool, cache: &SettingsCache, key: &str) -> Result<Option<JsonValue>> {
        // Start cache cleanup task if not started
        cache.start_cleanup();

        // Check cache first
        if let Some(value) = cache.get(key).await {
            debug!("Get setting (cache): {}: {:?}", key, value);
            return Ok(Some(value));
        }

        // Query from database
        let value_str: Option<String> = sqlx::query_scalar("SELECT value FROM setting WHERE key = ?")
            .bind(key)
            .fetch_optional(pool)
            .await
            .context("Failed to query setting")?;

        let value = match value_str {
            Some(v) => {
                // Try to parse as JSON
                let parsed = serde_json::from_str(&v).unwrap_or(JsonValue::String(v));
                cache.set(key.to_string(), parsed.clone()).await;
                debug!("Get setting: {}: {:?}", key, parsed);
                Some(parsed)
            }
            None => None,
        };

        Ok(value)
    }

    /// Set a single setting value by key
    pub async fn set(
        pool: &SqlitePool,
        cache: &SettingsCache,
        key: &str,
        value: &JsonValue,
        setting_type: Option<&str>,
    ) -> Result<()> {
        // Serialize value to JSON string
        let value_str = serde_json::to_string(value)?;

        // Check if setting exists
        let exists: bool = sqlx::query_scalar("SELECT COUNT(*) > 0 FROM setting WHERE key = ?")
            .bind(key)
            .fetch_one(pool)
            .await
            .context("Failed to check if setting exists")?;

        if exists {
            // Update existing setting
            sqlx::query("UPDATE setting SET value = ?, type = ? WHERE key = ?")
                .bind(&value_str)
                .bind(setting_type)
                .bind(key)
                .execute(pool)
                .await
                .context("Failed to update setting")?;
        } else {
            // Insert new setting
            sqlx::query("INSERT INTO setting (key, value, type) VALUES (?, ?, ?)")
                .bind(key)
                .bind(&value_str)
                .bind(setting_type)
                .execute(pool)
                .await
                .context("Failed to insert setting")?;
        }

        // Clear from cache
        cache.delete(&[key.to_string()]).await;

        Ok(())
    }

    /// Get all settings of a specific type
    pub async fn get_settings(pool: &SqlitePool, setting_type: &str) -> Result<HashMap<String, JsonValue>> {
        let rows: Vec<(String, String)> =
            sqlx::query_as("SELECT key, value FROM setting WHERE type = ?")
                .bind(setting_type)
                .fetch_all(pool)
                .await
                .context("Failed to query settings by type")?;

        let mut result = HashMap::new();

        for (key, value_str) in rows {
            let value = serde_json::from_str(&value_str).unwrap_or(JsonValue::String(value_str));
            result.insert(key, value);
        }

        Ok(result)
    }

    /// Set multiple settings of a specific type
    pub async fn set_settings(
        pool: &SqlitePool,
        cache: &SettingsCache,
        setting_type: &str,
        data: HashMap<String, JsonValue>,
    ) -> Result<()> {
        let keys: Vec<String> = data.keys().cloned().collect();

        // Process each setting
        for (key, value) in data.iter() {
            let value_str = serde_json::to_string(value)?;

            // Check if setting exists
            let existing: Option<String> =
                sqlx::query_scalar("SELECT type FROM setting WHERE key = ?")
                    .bind(key)
                    .fetch_optional(pool)
                    .await
                    .context("Failed to check existing setting")?;

            match existing {
                Some(existing_type) if existing_type == setting_type => {
                    // Update if type matches
                    sqlx::query("UPDATE setting SET value = ? WHERE key = ?")
                        .bind(&value_str)
                        .bind(key)
                        .execute(pool)
                        .await
                        .context("Failed to update setting")?;
                }
                None => {
                    // Insert new setting
                    sqlx::query("INSERT INTO setting (key, value, type) VALUES (?, ?, ?)")
                        .bind(key)
                        .bind(&value_str)
                        .bind(setting_type)
                        .execute(pool)
                        .await
                        .context("Failed to insert setting")?;
                }
                _ => {
                    // Skip if type doesn't match
                    continue;
                }
            }
        }

        // Clear cache for all affected keys
        cache.delete(&keys).await;

        Ok(())
    }

    /// Delete a setting by key
    pub async fn delete(pool: &SqlitePool, cache: &SettingsCache, key: &str) -> Result<()> {
        sqlx::query("DELETE FROM setting WHERE key = ?")
            .bind(key)
            .execute(pool)
            .await
            .context("Failed to delete setting")?;

        cache.delete(&[key.to_string()]).await;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use tempfile::TempDir;

    async fn setup_test_db() -> (Database, TempDir, SettingsCache) {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::new(temp_dir.path()).await.unwrap();
        db.migrate().await.unwrap();
        let cache = SettingsCache::new();
        (db, temp_dir, cache)
    }

    #[tokio::test]
    async fn test_set_and_get_setting() {
        let (db, _temp, cache) = setup_test_db().await;
        let pool = db.pool();

        // Set a string value
        Setting::set(
            pool,
            &cache,
            "test_key",
            &JsonValue::String("test_value".to_string()),
            Some("general"),
        )
        .await
        .unwrap();

        // Get it back
        let value = Setting::get(pool, &cache, "test_key").await.unwrap().unwrap();
        assert_eq!(value, JsonValue::String("test_value".to_string()));

        // Update it
        Setting::set(
            pool,
            &cache,
            "test_key",
            &JsonValue::String("updated_value".to_string()),
            Some("general"),
        )
        .await
        .unwrap();

        // Verify update
        let value = Setting::get(pool, &cache, "test_key").await.unwrap().unwrap();
        assert_eq!(value, JsonValue::String("updated_value".to_string()));
    }

    #[tokio::test]
    async fn test_get_settings_by_type() {
        let (db, _temp, cache) = setup_test_db().await;
        let pool = db.pool();

        // Set multiple settings of "general" type
        Setting::set(pool, &cache, "key1", &JsonValue::String("value1".to_string()), Some("general"))
            .await
            .unwrap();
        Setting::set(pool, &cache, "key2", &JsonValue::Number(42.into()), Some("general"))
            .await
            .unwrap();
        Setting::set(pool, &cache, "key3", &JsonValue::Bool(true), Some("other"))
            .await
            .unwrap();

        // Get settings by type
        let settings = Setting::get_settings(pool, "general").await.unwrap();

        assert_eq!(settings.len(), 2);
        assert_eq!(settings.get("key1").unwrap(), &JsonValue::String("value1".to_string()));
        assert_eq!(settings.get("key2").unwrap(), &JsonValue::Number(42.into()));
    }

    #[tokio::test]
    async fn test_set_settings_bulk() {
        let (db, _temp, cache) = setup_test_db().await;
        let pool = db.pool();

        let mut data = HashMap::new();
        data.insert("bulk1".to_string(), JsonValue::String("value1".to_string()));
        data.insert("bulk2".to_string(), JsonValue::Number(100.into()));

        Setting::set_settings(pool, &cache, "general", data)
            .await
            .unwrap();

        let settings = Setting::get_settings(pool, "general").await.unwrap();
        assert_eq!(settings.len(), 2);
        assert!(settings.contains_key("bulk1"));
        assert!(settings.contains_key("bulk2"));
    }

    #[tokio::test]
    async fn test_cache() {
        let (db, _temp, cache) = setup_test_db().await;
        let pool = db.pool();

        // Set a value
        Setting::set(pool, &cache, "cached_key", &JsonValue::Number(123.into()), Some("general"))
            .await
            .unwrap();

        // First get - should query DB and cache
        let value1 = Setting::get(pool, &cache, "cached_key").await.unwrap().unwrap();
        assert_eq!(value1, JsonValue::Number(123.into()));

        // Second get - should use cache
        let value2 = Setting::get(pool, &cache, "cached_key").await.unwrap().unwrap();
        assert_eq!(value2, JsonValue::Number(123.into()));

        // Update value - should clear cache
        Setting::set(pool, &cache, "cached_key", &JsonValue::Number(456.into()), Some("general"))
            .await
            .unwrap();

        // Get again - should get new value
        let value3 = Setting::get(pool, &cache, "cached_key").await.unwrap().unwrap();
        assert_eq!(value3, JsonValue::Number(456.into()));
    }

    #[tokio::test]
    async fn test_delete_setting() {
        let (db, _temp, cache) = setup_test_db().await;
        let pool = db.pool();

        Setting::set(pool, &cache, "to_delete", &JsonValue::String("delete_me".to_string()), Some("general"))
            .await
            .unwrap();

        let value = Setting::get(pool, &cache, "to_delete").await.unwrap();
        assert!(value.is_some());

        Setting::delete(pool, &cache, "to_delete").await.unwrap();

        let value = Setting::get(pool, &cache, "to_delete").await.unwrap();
        assert!(value.is_none());
    }
}
