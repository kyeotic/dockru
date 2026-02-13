use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::collections::HashMap;
use url::Url;

/// Agent model representing a remote Dockru instance
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Agent {
    pub id: i64,
    pub url: String,
    pub username: String,
    #[serde(skip_serializing)] // Don't expose password in JSON
    pub password: String,
    pub active: bool,
}

/// Data for creating a new agent
#[derive(Debug, Clone)]
pub struct NewAgent {
    pub url: String,
    pub username: String,
    pub password: String,
    pub active: bool,
}

impl Agent {
    /// Extract the endpoint (host:port) from the agent's URL
    ///
    /// Example: "https://example.com:5001" -> "example.com:5001"
    pub fn endpoint(&self) -> Result<String> {
        let parsed_url = Url::parse(&self.url)
            .with_context(|| format!("Failed to parse agent URL: {}", self.url))?;

        let host = parsed_url
            .host_str()
            .context("URL has no host")?;

        let endpoint = if let Some(port) = parsed_url.port() {
            format!("{}:{}", host, port)
        } else {
            host.to_string()
        };

        Ok(endpoint)
    }

    /// Find an agent by ID
    pub async fn find_by_id(pool: &SqlitePool, id: i64) -> Result<Option<Self>> {
        let agent = sqlx::query_as::<_, Agent>("SELECT * FROM agent WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await
            .context("Failed to query agent by id")?;

        Ok(agent)
    }

    /// Find an agent by URL
    pub async fn find_by_url(pool: &SqlitePool, url: &str) -> Result<Option<Self>> {
        let agent = sqlx::query_as::<_, Agent>("SELECT * FROM agent WHERE url = ?")
            .bind(url)
            .fetch_optional(pool)
            .await
            .context("Failed to query agent by url")?;

        Ok(agent)
    }

    /// Get all agents
    pub async fn find_all(pool: &SqlitePool) -> Result<Vec<Self>> {
        let agents = sqlx::query_as::<_, Agent>("SELECT * FROM agent")
            .fetch_all(pool)
            .await
            .context("Failed to query all agents")?;

        Ok(agents)
    }

    /// Get all agents as a map keyed by endpoint
    pub async fn get_agent_list(pool: &SqlitePool) -> Result<HashMap<String, Agent>> {
        let agents = Self::find_all(pool).await?;

        let mut result = HashMap::new();
        for agent in agents {
            if let Ok(endpoint) = agent.endpoint() {
                result.insert(endpoint, agent);
            }
        }

        Ok(result)
    }

    /// Create a new agent
    pub async fn create(pool: &SqlitePool, new_agent: NewAgent) -> Result<Self> {
        // Validate URL can be parsed
        let _ = Url::parse(&new_agent.url)
            .with_context(|| format!("Invalid agent URL: {}", new_agent.url))?;

        let result = sqlx::query(
            "INSERT INTO agent (url, username, password, active) VALUES (?, ?, ?, ?)",
        )
        .bind(&new_agent.url)
        .bind(&new_agent.username)
        .bind(&new_agent.password)
        .bind(new_agent.active)
        .execute(pool)
        .await
        .context("Failed to insert agent")?;

        let agent_id = result.last_insert_rowid();

        Self::find_by_id(pool, agent_id)
            .await?
            .context("Failed to find newly created agent")
    }

    /// Update agent's URL
    pub async fn update_url(&mut self, pool: &SqlitePool, new_url: &str) -> Result<()> {
        // Validate URL can be parsed
        let _ = Url::parse(new_url)
            .with_context(|| format!("Invalid agent URL: {}", new_url))?;

        sqlx::query("UPDATE agent SET url = ? WHERE id = ?")
            .bind(new_url)
            .bind(self.id)
            .execute(pool)
            .await
            .context("Failed to update agent URL")?;

        self.url = new_url.to_string();

        Ok(())
    }

    /// Update agent's credentials
    pub async fn update_credentials(
        &mut self,
        pool: &SqlitePool,
        username: &str,
        password: &str,
    ) -> Result<()> {
        sqlx::query("UPDATE agent SET username = ?, password = ? WHERE id = ?")
            .bind(username)
            .bind(password)
            .bind(self.id)
            .execute(pool)
            .await
            .context("Failed to update agent credentials")?;

        self.username = username.to_string();
        self.password = password.to_string();

        Ok(())
    }

    /// Update agent's active status
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

    /// Convert agent to JSON representation for client
    pub fn to_json(&self) -> Result<serde_json::Value> {
        Ok(serde_json::json!({
            "url": self.url,
            "username": self.username,
            "endpoint": self.endpoint()?,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use tempfile::TempDir;

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
            password: "secret".to_string(),
            active: true,
        };

        let agent = Agent::create(pool, new_agent).await.unwrap();

        assert_eq!(agent.url, "https://example.com:5001");
        assert_eq!(agent.username, "admin");
        assert_eq!(agent.password, "secret");
        assert!(agent.active);

        // Find by ID
        let found = Agent::find_by_id(pool, agent.id).await.unwrap().unwrap();
        assert_eq!(found.url, agent.url);

        // Find by URL
        let found = Agent::find_by_url(pool, "https://example.com:5001")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(found.id, agent.id);
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
                password: "pass".to_string(),
                active: true,
            },
        )
        .await
        .unwrap();

        assert_eq!(agent1.endpoint().unwrap(), "example.com:5001");

        // Without explicit port (HTTPS default)
        let agent2 = Agent::create(
            pool,
            NewAgent {
                url: "https://example.com".to_string(),
                username: "admin".to_string(),
                password: "pass".to_string(),
                active: true,
            },
        )
        .await
        .unwrap();

        assert_eq!(agent2.endpoint().unwrap(), "example.com");

        // HTTP with port
        let agent3 = Agent::create(
            pool,
            NewAgent {
                url: "http://192.168.1.100:8080".to_string(),
                username: "admin".to_string(),
                password: "pass".to_string(),
                active: true,
            },
        )
        .await
        .unwrap();

        assert_eq!(agent3.endpoint().unwrap(), "192.168.1.100:8080");
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
                password: "pass1".to_string(),
                active: true,
            },
        )
        .await
        .unwrap();

        Agent::create(
            pool,
            NewAgent {
                url: "https://agent2.com:5002".to_string(),
                username: "user2".to_string(),
                password: "pass2".to_string(),
                active: true,
            },
        )
        .await
        .unwrap();

        let agent_list = Agent::get_agent_list(pool).await.unwrap();

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
                password: "oldpass".to_string(),
                active: true,
            },
        )
        .await
        .unwrap();

        // Update URL
        agent.update_url(pool, "https://new.com:5001").await.unwrap();
        assert_eq!(agent.url, "https://new.com:5001");

        // Update credentials
        agent
            .update_credentials(pool, "newuser", "newpass")
            .await
            .unwrap();
        assert_eq!(agent.username, "newuser");
        assert_eq!(agent.password, "newpass");

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
                password: "secret".to_string(),
                active: true,
            },
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
                password: "pass".to_string(),
                active: true,
            },
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
                password: "pass".to_string(),
                active: true,
            },
        )
        .await
        .unwrap();

        let agent_id = agent.id;

        Agent::delete(pool, agent_id).await.unwrap();

        let found = Agent::find_by_id(pool, agent_id).await.unwrap();
        assert!(found.is_none());
    }
}
