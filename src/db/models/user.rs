use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

/// User model representing a user in the system
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub password: Option<String>,
    pub active: bool,
    pub timezone: Option<String>,
    pub twofa_secret: Option<String>,
    pub twofa_status: bool,
    pub twofa_last_token: Option<String>,
}

/// Data for creating a new user
#[derive(Debug, Clone)]
pub struct NewUser {
    pub username: String,
    pub password: Option<String>,
    pub active: bool,
    pub timezone: Option<String>,
}

impl User {
    /// Find a user by ID
    pub async fn find_by_id(pool: &SqlitePool, id: i64) -> Result<Option<Self>> {
        let user = sqlx::query_as::<_, User>("SELECT * FROM user WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await
            .context("Failed to query user by id")?;

        Ok(user)
    }

    /// Find a user by username
    pub async fn find_by_username(pool: &SqlitePool, username: &str) -> Result<Option<Self>> {
        let user = sqlx::query_as::<_, User>("SELECT * FROM user WHERE username = ?")
            .bind(username)
            .fetch_optional(pool)
            .await
            .context("Failed to query user by username")?;

        Ok(user)
    }

    /// Get all users
    pub async fn find_all(pool: &SqlitePool) -> Result<Vec<Self>> {
        let users = sqlx::query_as::<_, User>("SELECT * FROM user")
            .fetch_all(pool)
            .await
            .context("Failed to query all users")?;

        Ok(users)
    }

    /// Count total number of users
    pub async fn count(pool: &SqlitePool) -> Result<i64> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM user")
            .fetch_one(pool)
            .await
            .context("Failed to count users")?;

        Ok(count)
    }

    /// Create a new user
    pub async fn create(pool: &SqlitePool, new_user: NewUser) -> Result<Self> {
        // Hash password if provided
        let hashed_password = if let Some(ref password) = new_user.password {
            Some(crate::auth::hash_password(password)
                .context("Failed to hash password")?)
        } else {
            None
        };

        let result = sqlx::query(
            "INSERT INTO user (username, password, active, timezone) VALUES (?, ?, ?, ?)",
        )
        .bind(&new_user.username)
        .bind(&hashed_password)
        .bind(new_user.active)
        .bind(&new_user.timezone)
        .execute(pool)
        .await
        .context("Failed to insert user")?;

        let user_id = result.last_insert_rowid();

        // Fetch and return the created user
        Self::find_by_id(pool, user_id)
            .await?
            .context("Failed to find newly created user")
    }

    /// Update user's password
    ///
    /// Hashes password with bcrypt before storing
    pub async fn update_password(&mut self, pool: &SqlitePool, new_password: &str) -> Result<()> {
        let hashed_password = crate::auth::hash_password(new_password)
            .context("Failed to hash new password")?;

        sqlx::query("UPDATE user SET password = ? WHERE id = ?")
            .bind(&hashed_password)
            .bind(self.id)
            .execute(pool)
            .await
            .context("Failed to update user password")?;

        self.password = Some(hashed_password);

        Ok(())
    }

    /// Reset user password by user ID (static version)
    pub async fn reset_password(pool: &SqlitePool, user_id: i64, new_password: &str) -> Result<()> {
        let hashed_password = crate::auth::hash_password(new_password)
            .context("Failed to hash new password")?;

        sqlx::query("UPDATE user SET password = ? WHERE id = ?")
            .bind(&hashed_password)
            .bind(user_id)
            .execute(pool)
            .await
            .context("Failed to reset user password")?;

        Ok(())
    }

    /// Update user's active status
    pub async fn update_active(&mut self, pool: &SqlitePool, active: bool) -> Result<()> {
        sqlx::query("UPDATE user SET active = ? WHERE id = ?")
            .bind(active)
            .bind(self.id)
            .execute(pool)
            .await
            .context("Failed to update user active status")?;

        self.active = active;

        Ok(())
    }

    /// Update user's timezone
    pub async fn update_timezone(&mut self, pool: &SqlitePool, timezone: Option<&str>) -> Result<()> {
        sqlx::query("UPDATE user SET timezone = ? WHERE id = ?")
            .bind(timezone)
            .bind(self.id)
            .execute(pool)
            .await
            .context("Failed to update user timezone")?;

        self.timezone = timezone.map(|s| s.to_string());

        Ok(())
    }

    /// Enable 2FA for user
    pub async fn enable_twofa(&mut self, pool: &SqlitePool, secret: &str) -> Result<()> {
        sqlx::query("UPDATE user SET twofa_secret = ?, twofa_status = ? WHERE id = ?")
            .bind(secret)
            .bind(true)
            .bind(self.id)
            .execute(pool)
            .await
            .context("Failed to enable 2FA")?;

        self.twofa_secret = Some(secret.to_string());
        self.twofa_status = true;

        Ok(())
    }

    /// Disable 2FA for user
    pub async fn disable_twofa(&mut self, pool: &SqlitePool) -> Result<()> {
        sqlx::query("UPDATE user SET twofa_secret = NULL, twofa_status = ?, twofa_last_token = NULL WHERE id = ?")
            .bind(false)
            .bind(self.id)
            .execute(pool)
            .await
            .context("Failed to disable 2FA")?;

        self.twofa_secret = None;
        self.twofa_status = false;
        self.twofa_last_token = None;

        Ok(())
    }

    /// Update the last used 2FA token
    pub async fn update_twofa_last_token(&mut self, pool: &SqlitePool, token: &str) -> Result<()> {
        sqlx::query("UPDATE user SET twofa_last_token = ? WHERE id = ?")
            .bind(token)
            .bind(self.id)
            .execute(pool)
            .await
            .context("Failed to update 2FA last token")?;

        self.twofa_last_token = Some(token.to_string());

        Ok(())
    }

    /// Delete a user
    pub async fn delete(pool: &SqlitePool, user_id: i64) -> Result<()> {
        sqlx::query("DELETE FROM user WHERE id = ?")
            .bind(user_id)
            .execute(pool)
            .await
            .context("Failed to delete user")?;

        Ok(())
    }

    /// Create a JWT token for this user
    ///
    /// Token contains username and shake256 hash of password for detecting password changes
    pub fn create_jwt(&self, password: &str, jwt_secret: &str) -> Result<String> {
        crate::auth::create_jwt(&self.username, password, jwt_secret)
            .context("Failed to create JWT for user")
    }

    /// Verify a password against this user's stored password hash
    ///
    /// Uses bcrypt verification
    pub fn verify_password(&self, password: &str) -> Result<bool> {
        let hash = self.password.as_ref()
            .ok_or_else(|| anyhow::anyhow!("User has no password"))?;
        
        crate::auth::verify_password(password, hash)
            .context("Failed to verify password")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use crate::db::Database;

    async fn setup_test_db() -> (Database, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::new(temp_dir.path()).await.unwrap();
        db.migrate().await.unwrap();
        (db, temp_dir)
    }

    #[tokio::test]
    async fn test_create_and_find_user() {
        let (db, _temp) = setup_test_db().await;
        let pool = db.pool();

        let new_user = NewUser {
            username: "testuser".to_string(),
            password: Some("password123".to_string()),
            active: true,
            timezone: Some("UTC".to_string()),
        };

        let user = User::create(pool, new_user).await.unwrap();

        assert_eq!(user.username, "testuser");
        assert_eq!(user.active, true);
        assert_eq!(user.timezone, Some("UTC".to_string()));

        // Find by ID
        let found_user = User::find_by_id(pool, user.id).await.unwrap().unwrap();
        assert_eq!(found_user.username, "testuser");

        // Find by username
        let found_user = User::find_by_username(pool, "testuser").await.unwrap().unwrap();
        assert_eq!(found_user.id, user.id);
    }

    #[tokio::test]
    async fn test_user_count() {
        let (db, _temp) = setup_test_db().await;
        let pool = db.pool();

        let count = User::count(pool).await.unwrap();
        assert_eq!(count, 0);

        let new_user = NewUser {
            username: "user1".to_string(),
            password: Some("pass".to_string()),
            active: true,
            timezone: None,
        };

        User::create(pool, new_user).await.unwrap();

        let count = User::count(pool).await.unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn test_update_password() {
        let (db, _temp) = setup_test_db().await;
        let pool = db.pool();

        let new_user = NewUser {
            username: "testuser".to_string(),
            password: Some("oldpass".to_string()),
            active: true,
            timezone: None,
        };

        let mut user = User::create(pool, new_user).await.unwrap();
        
        // Password should be hashed, not plaintext
        assert_ne!(user.password.as_ref().unwrap(), "oldpass");
        assert!(user.password.as_ref().unwrap().starts_with("$2"));
        
        user.update_password(pool, "newpass").await.unwrap();

        let found_user = User::find_by_id(pool, user.id).await.unwrap().unwrap();
        
        // New password should also be hashed
        assert_ne!(found_user.password.as_ref().unwrap(), "newpass");
        assert!(found_user.password.as_ref().unwrap().starts_with("$2"));
    }

    #[tokio::test]
    async fn test_twofa() {
        let (db, _temp) = setup_test_db().await;
        let pool = db.pool();

        let new_user = NewUser {
            username: "testuser".to_string(),
            password: Some("pass".to_string()),
            active: true,
            timezone: None,
        };

        let mut user = User::create(pool, new_user).await.unwrap();
        assert!(!user.twofa_status);

        user.enable_twofa(pool, "SECRET123").await.unwrap();
        assert!(user.twofa_status);
        assert_eq!(user.twofa_secret, Some("SECRET123".to_string()));

        user.disable_twofa(pool).await.unwrap();
        assert!(!user.twofa_status);
        assert!(user.twofa_secret.is_none());
    }

    #[tokio::test]
    async fn test_verify_password() {
        let (db, _temp) = setup_test_db().await;
        let pool = db.pool();

        let new_user = NewUser {
            username: "testuser".to_string(),
            password: Some("correct_password".to_string()),
            active: true,
            timezone: None,
        };

        let user = User::create(pool, new_user).await.unwrap();

        // Correct password should verify
        assert!(user.verify_password("correct_password").unwrap());

        // Wrong password should not verify
        assert!(!user.verify_password("wrong_password").unwrap());
    }

    #[tokio::test]
    async fn test_create_jwt() {
        let (db, _temp) = setup_test_db().await;
        let pool = db.pool();

        let password = "test_password";
        let new_user = NewUser {
            username: "testuser".to_string(),
            password: Some(password.to_string()),
            active: true,
            timezone: None,
        };

        let user = User::create(pool, new_user).await.unwrap();
        let jwt_secret = "test_jwt_secret";

        // Create JWT - pass the original password, not the hash!
        let token = user.create_jwt(password, jwt_secret).unwrap();

        // Token should decode successfully
        let payload = crate::auth::verify_jwt(&token, jwt_secret).unwrap();
        assert_eq!(payload.username, "testuser");

        // Password hash should match
        assert_eq!(
            payload.h,
            crate::auth::shake256(password, crate::auth::SHAKE256_LENGTH)
        );
    }

    #[tokio::test]
    async fn test_jwt_detects_password_change() {
        let (db, _temp) = setup_test_db().await;
        let pool = db.pool();

        let old_password = "old_password";
        let new_password = "new_password";
        let jwt_secret = "test_jwt_secret";

        let new_user = NewUser {
            username: "testuser".to_string(),
            password: Some(old_password.to_string()),
            active: true,
            timezone: None,
        };

        let mut user = User::create(pool, new_user).await.unwrap();

        // Create JWT with old password
        let token = user.create_jwt(old_password, jwt_secret).unwrap();
        let payload = crate::auth::verify_jwt(&token, jwt_secret).unwrap();

        // Update password
        user.update_password(pool, new_password).await.unwrap();

        // Old token's hash should not match new password
        let new_hash = crate::auth::shake256(new_password, crate::auth::SHAKE256_LENGTH);
        assert_ne!(payload.h, new_hash);
    }
}
