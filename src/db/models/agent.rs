use anyhow::{Context, Result};
use redact::Secret;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::collections::HashMap;
use url::Url;

use crate::utils::crypto::{decrypt_password, encrypt_password, is_password_encrypted};

/// Database row for agent (with encrypted password)
#[derive(Debug, Clone, sqlx::FromRow, Deserialize)]
struct AgentRow {
    pub id: i64,
    pub url: String,
    pub username: String,
    pub password: String, // Encrypted in DB
    pub active: bool,
}

/// Agent model representing a remote Dockru instance (application type with decrypted password)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub id: i64,
    pub url: String,
    pub username: String,
    #[serde(skip_serializing)] // Don't expose password in JSON
    pub password: Secret<String>, // Plaintext in memory
    pub active: bool,
    pub endpoint: String,
}

/// Data for creating a new agent
#[derive(Debug, Clone)]
pub struct NewAgent {
    pub url: String,
    pub username: String,
    pub password: Secret<String>,
    pub active: bool,
}

impl AgentRow {
    /// Convert database row to application Agent, decrypting the password
    fn into_agent(self, encryption_secret: &Secret<String>) -> Result<Agent> {
        let password_str = if is_password_encrypted(&self.password) {
            decrypt_password(&self.password, encryption_secret)
                .context("Failed to decrypt agent password")?
        } else {
            // Legacy plaintext password
            Secret::new(self.password)
        };

        let endpoint = parse_endpoint(&self.url)?;

        Ok(Agent {
            id: self.id,
            url: self.url,
            username: self.username,
            password: password_str,
            active: self.active,
            endpoint,
        })
    }
}

fn parse_endpoint(url: &str) -> Result<String> {
    let parsed_url =
        Url::parse(url).with_context(|| format!("Failed to parse agent URL: {}", url))?;

    let host = parsed_url.host_str().context("URL has no host")?;

    let endpoint = if let Some(port) = parsed_url.port() {
        format!("{}:{}", host, port)
    } else {
        host.to_string()
    };

    Ok(endpoint)
}

impl Agent {
    /// Find an agent by ID
    pub async fn find_by_id(
        pool: &SqlitePool,
        id: i64,
        encryption_secret: &Secret<String>,
    ) -> Result<Option<Self>> {
        let row = sqlx::query_as::<_, AgentRow>("SELECT * FROM agent WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await
            .context("Failed to query agent by id")?;

        row.map(|r| r.into_agent(encryption_secret)).transpose()
    }

    /// Find an agent by URL
    pub async fn find_by_url(
        pool: &SqlitePool,
        url: &str,
        encryption_secret: &Secret<String>,
    ) -> Result<Option<Self>> {
        let row = sqlx::query_as::<_, AgentRow>("SELECT * FROM agent WHERE url = ?")
            .bind(url)
            .fetch_optional(pool)
            .await
            .context("Failed to query agent by url")?;

        row.map(|r| r.into_agent(encryption_secret)).transpose()
    }

    /// Get all agents
    pub async fn find_all(
        pool: &SqlitePool,
        encryption_secret: &Secret<String>,
    ) -> Result<Vec<Self>> {
        let rows = sqlx::query_as::<_, AgentRow>("SELECT * FROM agent")
            .fetch_all(pool)
            .await
            .context("Failed to query all agents")?;

        rows.into_iter()
            .map(|r| r.into_agent(encryption_secret))
            .collect()
    }

    /// Get all agents as a map keyed by endpoint
    #[allow(dead_code)]
    pub async fn get_agent_list(
        pool: &SqlitePool,
        encryption_secret: &Secret<String>,
    ) -> Result<HashMap<String, Agent>> {
        let agents = Self::find_all(pool, encryption_secret).await?;

        let mut result = HashMap::new();
        for agent in agents {
            result.insert(agent.endpoint.clone(), agent);
        }

        Ok(result)
    }

    /// Create a new agent (password is encrypted before storage)
    pub async fn create(
        pool: &SqlitePool,
        new_agent: NewAgent,
        encryption_secret: &Secret<String>,
    ) -> Result<Self> {
        // Validate URL can be parsed
        let _ = Url::parse(&new_agent.url)
            .with_context(|| format!("Invalid agent URL: {}", new_agent.url))?;

        // Encrypt the password before storing
        let encrypted_password = encrypt_password(&new_agent.password, encryption_secret)
            .context("Failed to encrypt agent password")?;

        let result =
            sqlx::query("INSERT INTO agent (url, username, password, active) VALUES (?, ?, ?, ?)")
                .bind(&new_agent.url)
                .bind(&new_agent.username)
                .bind(&encrypted_password)
                .bind(new_agent.active)
                .execute(pool)
                .await
                .context("Failed to insert agent")?;

        let agent_id = result.last_insert_rowid();

        Self::find_by_id(pool, agent_id, encryption_secret)
            .await?
            .context("Failed to find newly created agent")
    }

    /// Update agent's URL
    #[allow(dead_code)]
    pub async fn update_url(&mut self, pool: &SqlitePool, new_url: &str) -> Result<()> {
        // Validate URL can be parsed
        let _ = Url::parse(new_url).with_context(|| format!("Invalid agent URL: {}", new_url))?;

        sqlx::query("UPDATE agent SET url = ? WHERE id = ?")
            .bind(new_url)
            .bind(self.id)
            .execute(pool)
            .await
            .context("Failed to update agent URL")?;

        self.url = new_url.to_string();

        Ok(())
    }

    /// Update agent's credentials (password is encrypted before storage)
    #[allow(dead_code)]
    pub async fn update_credentials(
        &mut self,
        pool: &SqlitePool,
        username: &str,
        password: &str,
        encryption_secret: &Secret<String>,
    ) -> Result<()> {
        let encrypted_password =
            encrypt_password(&Secret::new(password.to_string()), encryption_secret)
                .context("Failed to encrypt agent password")?;

        sqlx::query("UPDATE agent SET username = ?, password = ? WHERE id = ?")
            .bind(username)
            .bind(&encrypted_password)
            .bind(self.id)
            .execute(pool)
            .await
            .context("Failed to update agent credentials")?;

        self.username = username.to_string();
        self.password = Secret::new(password.to_string()); // In-memory stays plaintext

        Ok(())
    }

    /// Update agent's active status
    #[allow(dead_code)]
    pub async fn update_active(&mut self, pool: &SqlitePool, active: bool) -> Result<()> {
        sqlx::query("UPDATE agent SET active = ? WHERE id = ?")
            .bind(active)
            .bind(self.id)
            .execute(pool)
            .await
            .context("Failed to update agent active status")?;

        self.active = active;

        Ok(())
    }

    /// Delete an agent
    pub async fn delete(pool: &SqlitePool, agent_id: i64) -> Result<()> {
        sqlx::query("DELETE FROM agent WHERE id = ?")
            .bind(agent_id)
            .execute(pool)
            .await
            .context("Failed to delete agent")?;

        Ok(())
    }

    /// Migrate any plaintext passwords to encrypted form.
    /// Call this once at startup to handle upgrades from older versions.
    pub async fn migrate_plaintext_passwords(
        pool: &SqlitePool,
        encryption_secret: &Secret<String>,
    ) -> Result<u32> {
        // Load raw agent rows without decrypting
        let rows = sqlx::query_as::<_, AgentRow>("SELECT * FROM agent")
            .fetch_all(pool)
            .await
            .context("Failed to query agents for password migration")?;

        let mut migrated = 0u32;
        for row in &rows {
            if !is_password_encrypted(&row.password) {
                let encrypted =
                    encrypt_password(&Secret::new(row.password.clone()), encryption_secret)
                        .with_context(|| {
                            format!("Failed to encrypt password for agent {}", row.id)
                        })?;

                sqlx::query("UPDATE agent SET password = ? WHERE id = ?")
                    .bind(&encrypted)
                    .bind(row.id)
                    .execute(pool)
                    .await
                    .with_context(|| {
                        format!("Failed to update encrypted password for agent {}", row.id)
                    })?;

                migrated += 1;
            }
        }

        Ok(migrated)
    }

    /// Convert agent to JSON representation for client
    pub fn to_json(&self) -> Result<serde_json::Value> {
        Ok(serde_json::json!({
            "url": self.url,
            "username": self.username,
            "endpoint": self.endpoint,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use tempfile::TempDir;

    fn test_secret() -> Secret<String> {
        Secret::new("test_encryption_secret_for_agents".to_string())
    }

    async fn setup_test_db() -> (Database, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::new(temp_dir.path()).await.unwrap();
        db.migrate().await.unwrap();
        (db, temp_dir)
    }

    #[tokio::test]
    async fn test_create_and_find_agent() {
        let (db, _temp) = setup_test_db().await;
        let pool = db.pool();

        let new_agent = NewAgent {
            url: "https://example.com:5001".to_string(),
            username: "admin".to_string(),
            password: Secret::new("secret".to_string()),
            active: true,
        };

        let agent = Agent::create(pool, new_agent, &test_secret())
            .await
            .unwrap();

        assert_eq!(agent.url, "https://example.com:5001");
        assert_eq!(agent.username, "admin");
        // Password should be decrypted back to plaintext in memory
        assert_eq!(agent.password.expose_secret(), "secret");
        assert!(agent.active);

        // Find by ID
        let found = Agent::find_by_id(pool, agent.id, &test_secret())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(found.url, agent.url);
        assert_eq!(found.password.expose_secret(), "secret");

        // Find by URL
        let found = Agent::find_by_url(pool, "https://example.com:5001", &test_secret())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(found.id, agent.id);
        assert_eq!(found.password.expose_secret(), "secret");
    }

    #[tokio::test]
    async fn test_password_stored_encrypted() {
        let (db, _temp) = setup_test_db().await;
        let pool = db.pool();

        let new_agent = NewAgent {
            url: "https://example.com:5001".to_string(),
            username: "admin".to_string(),
            password: Secret::new("my_secret_pass".to_string()),
            active: true,
        };

        let agent = Agent::create(pool, new_agent, &test_secret())
            .await
            .unwrap();

        // Read the raw value from DB — should be encrypted, not plaintext
        let row: (String,) = sqlx::query_as("SELECT password FROM agent WHERE id = ?")
            .bind(agent.id)
            .fetch_one(pool)
            .await
            .unwrap();

        assert!(
            is_password_encrypted(&row.0),
            "Password in DB should be encrypted"
        );
        assert_ne!(
            row.0, "my_secret_pass",
            "Password in DB should not be plaintext"
        );
    }

    #[tokio::test]
    async fn test_endpoint_parsing() {
        let (db, _temp) = setup_test_db().await;
        let pool = db.pool();

        // With port
        let agent1 = Agent::create(
            pool,
            NewAgent {
                url: "https://example.com:5001".to_string(),
                username: "admin".to_string(),
                password: Secret::new("pass".to_string()),
                active: true,
            },
            &test_secret(),
        )
        .await
        .unwrap();

        assert_eq!(agent1.endpoint, "example.com:5001");

        // Without explicit port (HTTPS default)
        let agent2 = Agent::create(
            pool,
            NewAgent {
                url: "https://example.com".to_string(),
                username: "admin".to_string(),
                password: Secret::new("pass".to_string()),
                active: true,
            },
            &test_secret(),
        )
        .await
        .unwrap();

        assert_eq!(agent2.endpoint, "example.com");

        // HTTP with port
        let agent3 = Agent::create(
            pool,
            NewAgent {
                url: "http://192.168.1.100:8080".to_string(),
                username: "admin".to_string(),
                password: Secret::new("pass".to_string()),
                active: true,
            },
            &test_secret(),
        )
        .await
        .unwrap();

        assert_eq!(agent3.endpoint, "192.168.1.100:8080");
    }

    #[tokio::test]
    async fn test_get_agent_list() {
        let (db, _temp) = setup_test_db().await;
        let pool = db.pool();

        Agent::create(
            pool,
            NewAgent {
                url: "https://agent1.com:5001".to_string(),
                username: "user1".to_string(),
                password: Secret::new("pass1".to_string()),
                active: true,
            },
            &test_secret(),
        )
        .await
        .unwrap();

        Agent::create(
            pool,
            NewAgent {
                url: "https://agent2.com:5002".to_string(),
                username: "user2".to_string(),
                password: Secret::new("pass2".to_string()),
                active: true,
            },
            &test_secret(),
        )
        .await
        .unwrap();

        let agent_list = Agent::get_agent_list(pool, &test_secret()).await.unwrap();

        assert_eq!(agent_list.len(), 2);
        assert!(agent_list.contains_key("agent1.com:5001"));
        assert!(agent_list.contains_key("agent2.com:5002"));
    }

    #[tokio::test]
    async fn test_update_agent() {
        let (db, _temp) = setup_test_db().await;
        let pool = db.pool();

        let mut agent = Agent::create(
            pool,
            NewAgent {
                url: "https://old.com:5001".to_string(),
                username: "olduser".to_string(),
                password: Secret::new("oldpass".to_string()),
                active: true,
            },
            &test_secret(),
        )
        .await
        .unwrap();

        // Update URL
        agent
            .update_url(pool, "https://new.com:5001")
            .await
            .unwrap();
        assert_eq!(agent.url, "https://new.com:5001");

        // Update credentials
        agent
            .update_credentials(pool, "newuser", "newpass", &test_secret())
            .await
            .unwrap();
        assert_eq!(agent.username, "newuser");
        assert_eq!(agent.password.expose_secret(), "newpass");

        // Verify the updated password is stored encrypted in DB
        let row: (String,) = sqlx::query_as("SELECT password FROM agent WHERE id = ?")
            .bind(agent.id)
            .fetch_one(pool)
            .await
            .unwrap();
        assert!(is_password_encrypted(&row.0));

        // Update active status
        agent.update_active(pool, false).await.unwrap();
        assert!(!agent.active);
    }

    #[tokio::test]
    async fn test_to_json() {
        let (db, _temp) = setup_test_db().await;
        let pool = db.pool();

        let agent = Agent::create(
            pool,
            NewAgent {
                url: "https://example.com:5001".to_string(),
                username: "admin".to_string(),
                password: Secret::new("secret".to_string()),
                active: true,
            },
            &test_secret(),
        )
        .await
        .unwrap();

        let json = agent.to_json().unwrap();

        assert_eq!(json["url"], "https://example.com:5001");
        assert_eq!(json["username"], "admin");
        assert_eq!(json["endpoint"], "example.com:5001");
        // Password should not be in JSON
        assert!(json.get("password").is_none());
    }

    #[tokio::test]
    async fn test_invalid_url() {
        let (db, _temp) = setup_test_db().await;
        let pool = db.pool();

        let result = Agent::create(
            pool,
            NewAgent {
                url: "not a valid url".to_string(),
                username: "admin".to_string(),
                password: Secret::new("pass".to_string()),
                active: true,
            },
            &test_secret(),
        )
        .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_delete_agent() {
        let (db, _temp) = setup_test_db().await;
        let pool = db.pool();

        let agent = Agent::create(
            pool,
            NewAgent {
                url: "https://example.com:5001".to_string(),
                username: "admin".to_string(),
                password: Secret::new("pass".to_string()),
                active: true,
            },
            &test_secret(),
        )
        .await
        .unwrap();

        let agent_id = agent.id;

        Agent::delete(pool, agent_id).await.unwrap();

        let found = Agent::find_by_id(pool, agent_id, &test_secret())
            .await
            .unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_migrate_plaintext_passwords() {
        let (db, _temp) = setup_test_db().await;
        let pool = db.pool();

        // Insert agents with plaintext passwords directly (simulating old behavior)
        sqlx::query("INSERT INTO agent (url, username, password, active) VALUES (?, ?, ?, ?)")
            .bind("https://agent1.com:5001")
            .bind("user1")
            .bind("plaintext_pass_1")
            .bind(true)
            .execute(pool)
            .await
            .unwrap();

        sqlx::query("INSERT INTO agent (url, username, password, active) VALUES (?, ?, ?, ?)")
            .bind("https://agent2.com:5002")
            .bind("user2")
            .bind("plaintext_pass_2")
            .bind(true)
            .execute(pool)
            .await
            .unwrap();

        // Run migration
        let migrated = Agent::migrate_plaintext_passwords(pool, &test_secret())
            .await
            .unwrap();
        assert_eq!(migrated, 2);

        // Running again should migrate 0 (already encrypted)
        let migrated = Agent::migrate_plaintext_passwords(pool, &test_secret())
            .await
            .unwrap();
        assert_eq!(migrated, 0);

        // Verify passwords are decrypted correctly
        let agents = Agent::find_all(pool, &test_secret()).await.unwrap();
        assert_eq!(agents.len(), 2);
        assert_eq!(agents[0].password.expose_secret(), "plaintext_pass_1");
        assert_eq!(agents[1].password.expose_secret(), "plaintext_pass_2");
    }
}
