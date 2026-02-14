pub mod models;

use anyhow::{Context, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use sqlx::ConnectOptions;
use std::path::Path;
use std::str::FromStr;
use tracing::{debug, info};

/// Database connection pool and management
pub struct Database {
    pool: SqlitePool,
}

impl Database {
    /// Initialize a new database connection
    ///
    /// This sets up the SQLite database with the following configuration:
    /// - WAL journal mode for better concurrency
    /// - 12MB cache size (-12000 pages)
    /// - Incremental auto-vacuum
    /// - Normal synchronous mode (balance safety and performance)
    pub async fn new(data_dir: impl AsRef<Path>) -> Result<Self> {
        let db_path = data_dir.as_ref().join("dockru.db");
        info!("Connecting to database at: {}", db_path.display());

        // Build connection options
        let options = SqliteConnectOptions::from_str(&format!("sqlite:{}", db_path.display()))?
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .synchronous(sqlx::sqlite::SqliteSynchronous::Normal)
            .busy_timeout(std::time::Duration::from_secs(120))
            .disable_statement_logging();

        // Create connection pool
        let pool = SqlitePoolOptions::new()
            .min_connections(1)
            .max_connections(1)
            .acquire_timeout(std::time::Duration::from_secs(120))
            .idle_timeout(std::time::Duration::from_secs(120))
            .connect_with(options)
            .await
            .context("Failed to connect to database")?;

        let db = Database { pool };

        // Initialize SQLite pragmas
        db.init_sqlite().await?;

        info!("Connected to database successfully");

        Ok(db)
    }

    /// Get a reference to the connection pool
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Initialize SQLite-specific settings
    async fn init_sqlite(&self) -> Result<()> {
        // Enable foreign keys
        sqlx::query("PRAGMA foreign_keys = ON")
            .execute(&self.pool)
            .await
            .context("Failed to enable foreign keys")?;

        // Set cache size to 12MB (12000 KB = 12000 pages at default 1KB page size)
        // Negative value means kilobytes
        sqlx::query("PRAGMA cache_size = -12000")
            .execute(&self.pool)
            .await
            .context("Failed to set cache size")?;

        // Set auto vacuum to incremental
        sqlx::query("PRAGMA auto_vacuum = INCREMENTAL")
            .execute(&self.pool)
            .await
            .context("Failed to set auto vacuum")?;

        // Log current settings
        debug!("SQLite configuration:");

        let journal_mode: String = sqlx::query_scalar("PRAGMA journal_mode")
            .fetch_one(&self.pool)
            .await?;
        debug!("  journal_mode: {}", journal_mode);

        let cache_size: i64 = sqlx::query_scalar("PRAGMA cache_size")
            .fetch_one(&self.pool)
            .await?;
        debug!("  cache_size: {}", cache_size);

        let synchronous: i64 = sqlx::query_scalar("PRAGMA synchronous")
            .fetch_one(&self.pool)
            .await?;
        debug!("  synchronous: {}", synchronous);

        let version: String = sqlx::query_scalar("SELECT sqlite_version()")
            .fetch_one(&self.pool)
            .await?;
        info!("SQLite version: {}", version);

        Ok(())
    }

    /// Run database migrations
    pub async fn migrate(&self) -> Result<()> {
        info!("Running database migrations...");

        sqlx::migrate!("./migrations")
            .run(&self.pool)
            .await
            .context("Failed to run migrations")?;

        info!("Database migrations completed successfully");
        Ok(())
    }

    /// Close the database connection gracefully
    #[allow(dead_code)]
    pub async fn close(self) -> Result<()> {
        info!("Closing database connection");

        // Flush WAL to main database
        sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
            .execute(&self.pool)
            .await
            .context("Failed to checkpoint WAL")?;

        self.pool.close().await;
        info!("Database connection closed");

        Ok(())
    }

    /// Get the size of the database file in bytes (SQLite only)
    #[allow(dead_code)]
    pub fn get_size(&self, data_dir: impl AsRef<Path>) -> Result<u64> {
        let db_path = data_dir.as_ref().join("dockru.db");
        let metadata =
            std::fs::metadata(&db_path).context("Failed to read database file metadata")?;
        Ok(metadata.len())
    }

    /// Shrink the database by running VACUUM
    #[allow(dead_code)]
    pub async fn shrink(&self) -> Result<()> {
        info!("Running VACUUM to shrink database");
        sqlx::query("VACUUM")
            .execute(&self.pool)
            .await
            .context("Failed to vacuum database")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_database_creation() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::new(temp_dir.path()).await.unwrap();

        // Verify database was created
        let db_path = temp_dir.path().join("dockru.db");
        assert!(db_path.exists());

        // Verify we can execute a query
        let result: i64 = sqlx::query_scalar("SELECT 1")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(result, 1);

        db.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_pragma_settings() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::new(temp_dir.path()).await.unwrap();

        // Check journal mode is WAL
        let journal_mode: String = sqlx::query_scalar("PRAGMA journal_mode")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(journal_mode.to_lowercase(), "wal");

        // Check foreign keys are enabled
        let foreign_keys: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(foreign_keys, 1);

        db.close().await.unwrap();
    }
}
