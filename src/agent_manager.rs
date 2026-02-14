use crate::db::models::agent::Agent;
use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use futures_util::future::FutureExt;
use rust_socketio::asynchronous::{Client, ClientBuilder};
use rust_socketio::Payload;
use serde_json::{json, Value};
use socketioxide::extract::SocketRef;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Agent connection status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentStatus {
    Connecting,
    Online,
    Offline,
}

impl AgentStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            AgentStatus::Connecting => "connecting",
            AgentStatus::Online => "online",
            AgentStatus::Offline => "offline",
        }
    }
}

/// Agent client wrapper tracking connection state
struct AgentClient {
    client: Client,
    logged_in: bool,
    endpoint: String,
}

/// Dockru Agent Manager
/// One AgentManager per Socket connection
/// Manages Socket.io client connections to remote Dockru instances
pub struct AgentManager {
    socket_id: String,
    socket: SocketRef,
    db: SqlitePool,
    encryption_secret: String,
    agent_clients: Arc<RwLock<HashMap<String, AgentClient>>>,
    first_connect_time: Arc<RwLock<DateTime<Utc>>>,
}

impl AgentManager {
    /// Create a new AgentManager for a socket connection
    pub fn new(socket: SocketRef, db: SqlitePool, encryption_secret: String) -> Self {
        let socket_id = socket.id.to_string();
        info!("Creating AgentManager for socket {}", socket_id);

        Self {
            socket_id,
            socket,
            db,
            encryption_secret,
            agent_clients: Arc::new(RwLock::new(HashMap::new())),
            first_connect_time: Arc::new(RwLock::new(Utc::now())),
        }
    }

    /// Test connection to a remote Dockru instance
    /// Returns Ok(()) if connection and login succeed
    pub async fn test(&self, url: &str, username: &str, password: &str) -> Result<()> {
        let parsed_url = url::Url::parse(url)
            .map_err(|e| anyhow!("Invalid Dockru URL: {}", e))?;

        let endpoint = parsed_url
            .host_str()
            .ok_or_else(|| anyhow!("Invalid Dockru URL: no host"))?;

        let endpoint_with_port = if let Some(port) = parsed_url.port() {
            format!("{}:{}", endpoint, port)
        } else {
            endpoint.to_string()
        };

        // Check if already connected
        {
            let clients = self.agent_clients.read().await;
            if clients.contains_key(&endpoint_with_port) {
                return Err(anyhow!("The Dockru URL already exists"));
            }
        }

        // Try to connect with a timeout
        let test_future = Self::test_connection_internal(url, &endpoint_with_port, username, password);
        
        tokio::time::timeout(Duration::from_secs(30), test_future)
            .await
            .map_err(|_| anyhow!("Connection timeout"))?
    }

    /// Internal test connection helper
    async fn test_connection_internal(
        url: &str,
        endpoint: &str,
        username: &str,
        password: &str,
    ) -> Result<()> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let tx = Arc::new(tokio::sync::Mutex::new(Some(tx)));

        let username = username.to_string();
        let password = password.to_string();
        let endpoint_clone = endpoint.to_string();

        // Clone for the second callback
        let tx_for_error = tx.clone();

        // Build client with callbacks
        let client = ClientBuilder::new(url)
            .opening_header("endpoint", endpoint_clone.as_str())
            .reconnect(false)
            .on("connect", move |_payload: Payload, socket: Client| {
                let username = username.clone();
                let password = password.clone();
                let endpoint = endpoint_clone.clone();
                let tx = tx.clone();

                async move {
                    debug!("Test connection established to {}", endpoint);
                    
                    // Emit login
                    let login_data = json!({
                        "username": username,
                        "password": password,
                    });

                    let (login_tx, login_rx) = tokio::sync::oneshot::channel();
                    let login_tx = Arc::new(tokio::sync::Mutex::new(Some(login_tx)));

                    if let Err(e) = socket.emit_with_ack(
                        "login",
                        login_data,
                        Duration::from_secs(10),
                        move |payload: Payload, _socket: Client| {
                            let login_tx = login_tx.clone();
                            async move {
                                if let Payload::Text(values) = payload {
                                    if let Some(obj) = values.first() {
                                        if let Some(lock) = login_tx.lock().await.take() {
                                            lock.send(obj.clone()).ok();
                                        }
                                    }
                                }
                            }
                            .boxed()
                        },
                    ).await {
                        error!("Failed to emit login: {}", e);
                        if let Some(lock) = tx.lock().await.take() {
                            lock.send(Err(anyhow!("Failed to emit login"))).ok();
                        }
                        return;
                    }

                    // Wait for login response
                    match tokio::time::timeout(Duration::from_secs(10), login_rx).await {
                        Ok(Ok(response)) => {
                            if let Some(ok) = response.get("ok").and_then(|v| v.as_bool()) {
                                if ok {
                                    if let Some(lock) = tx.lock().await.take() {
                                        lock.send(Ok(())).ok();
                                    }
                                } else {
                                    let msg = response
                                        .get("msg")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("Login failed");
                                    if let Some(lock) = tx.lock().await.take() {
                                        lock.send(Err(anyhow!("{}", msg))).ok();
                                    }
                                }
                            }
                        }
                        Ok(Err(_)) => {
                            if let Some(lock) = tx.lock().await.take() {
                                lock.send(Err(anyhow!("Login response channel closed"))).ok();
                            }
                        }
                        Err(_) => {
                            if let Some(lock) = tx.lock().await.take() {
                                lock.send(Err(anyhow!("Login timeout"))).ok();
                            }
                        }
                    }
                }
                .boxed()
            })
            .on("connect_error", move |_payload: Payload, _socket: Client| {
                let tx = tx_for_error.clone();
                async move {
                    if let Some(lock) = tx.lock().await.take() {
                        lock.send(Err(anyhow!("Unable to connect to the Dockru instance"))).ok();
                    }
                }
                .boxed()
            })
            .connect()
            .await?;

        // Wait for result
        let result = tokio::time::timeout(Duration::from_secs(30), rx)
            .await
            .map_err(|_| anyhow!("Test connection timeout"))??;

        // Disconnect
        client.disconnect().await?;

        result
    }

    /// Add a remote Dockru agent to the database
    pub async fn add(&self, url: &str, username: &str, password: &str) -> Result<Agent> {
        use crate::db::models::agent::NewAgent;
        let new_agent = NewAgent {
            url: url.to_string(),
            username: username.to_string(),
            password: password.to_string(),
            active: true,
        };
        let agent = Agent::create(&self.db, new_agent, &self.encryption_secret).await?;
        let endpoint = agent.endpoint()?;
        info!("Added agent: {} (endpoint: {})", url, endpoint);
        Ok(agent)
    }

    /// Remove a remote Dockru agent
    pub async fn remove(&self, url: &str) -> Result<()> {
        let agent = Agent::find_by_url(&self.db, url, &self.encryption_secret)
            .await?
            .ok_or_else(|| anyhow!("Agent not found"))?;

        let endpoint = agent.endpoint()?;

        // Disconnect first
        self.disconnect(&endpoint).await;

        // Delete from database
        Agent::delete(&self.db, agent.id).await?;

        info!("Removed agent: {} (endpoint: {})", url, endpoint);

        // Send updated agent list
        self.send_agent_list().await;

        Ok(())
    }

    /// Connect to a remote Dockru instance
    pub async fn connect(&self, url: &str, username: &str, password: &str) {
        let parsed_url = match url::Url::parse(url) {
            Ok(u) => u,
            Err(e) => {
                error!("Invalid endpoint URL {}: {}", url, e);
                return;
            }
        };

        let endpoint_host = match parsed_url.host_str() {
            Some(h) => h,
            None => {
                error!("Invalid endpoint: no host in URL {}", url);
                return;
            }
        };

        let endpoint = if let Some(port) = parsed_url.port() {
            format!("{}:{}", endpoint_host, port)
        } else {
            endpoint_host.to_string()
        };

        // Emit connecting status
        self.emit_agent_status(&endpoint, AgentStatus::Connecting, None)
            .await;

        // Check if already connected
        {
            let clients = self.agent_clients.read().await;
            if clients.contains_key(&endpoint) {
                debug!("Already connected to socket server: {}", endpoint);
                return;
            }
        }

        info!("Connecting to socket server: {}", endpoint);

        let socket_ref = self.socket.clone();
        let agent_clients = self.agent_clients.clone();
        let endpoint_clone = endpoint.clone();
        let username = username.to_string();
        let password = password.to_string();
        let url = url.to_string();

        // Spawn connection task
        tokio::spawn(async move {
            Self::connect_internal(
                socket_ref,
                agent_clients,
                url,
                endpoint_clone,
                username,
                password,
            )
            .await;
        });
    }

    /// Internal connection logic
    async fn connect_internal(
        socket_ref: SocketRef,
        agent_clients: Arc<RwLock<HashMap<String, AgentClient>>>,
        url: String,
        endpoint: String,
        username: String,
        password: String,
    ) {
        // Create clones for each callback (can't move the same value into multiple closures)
        let socket_ref_for_connect = socket_ref.clone();
        let socket_ref_for_error = socket_ref.clone();
        let socket_ref_for_disconnect = socket_ref.clone();
        let socket_ref_for_agent = socket_ref.clone();
        let socket_ref_for_info = socket_ref.clone();
        
        let endpoint_for_connect = endpoint.clone();
        let endpoint_for_error = endpoint.clone();
        let endpoint_for_disconnect = endpoint.clone();
        let endpoint_for_info = endpoint.clone();
        
        let agent_clients_for_connect = agent_clients.clone();
        let username_for_connect = username.clone();
        let password_for_connect = password.clone();

        match ClientBuilder::new(&url)
            .opening_header("endpoint", endpoint.as_str())
            .on("connect", move |_payload: Payload, socket: Client| {
                let socket_ref = socket_ref_for_connect.clone();
                let endpoint = endpoint_for_connect.clone();
                let agent_clients = agent_clients_for_connect.clone();
                let username = username_for_connect.clone();
                let password = password_for_connect.clone();

                async move {
                    info!("Connected to socket server: {}", endpoint);

                    // Clone endpoint for error message (in case emit_with_ack fails)
                    let endpoint_for_error = endpoint.clone();

                    // Emit login
                    let login_data = json!({
                        "username": username,
                        "password": password,
                    });

                    if let Err(e) = socket.emit_with_ack(
                        "login",
                        login_data,
                        Duration::from_secs(10),
                        move |payload: Payload, _socket: Client| {
                            let socket_ref = socket_ref.clone();
                            let endpoint = endpoint.clone();
                            let agent_clients = agent_clients.clone();

                            async move {
                                if let Payload::Text(values) = payload {
                                    if let Some(obj) = values.first() {
                                        if let Some(ok) = obj.get("ok").and_then(|v| v.as_bool()) {
                                            if ok {
                                                    info!("Logged in to socket server: {}", endpoint);
                                                    
                                                    // Update logged_in status
                                                    {
                                                        let mut clients = agent_clients.write().await;
                                                        if let Some(client) = clients.get_mut(&endpoint) {
                                                            client.logged_in = true;
                                                        }
                                                    }

                                                    // Emit online status
                                                    socket_ref.emit("agentStatus", json!({
                                                        "endpoint": endpoint,
                                                        "status": "online",
                                                    })).ok();
                                                } else {
                                                    error!("Failed to login to socket server: {}", endpoint);
                                                    socket_ref.emit("agentStatus", json!({
                                                        "endpoint": endpoint,
                                                        "status": "offline",
                                                    })).ok();
                                                }
                                            }
                                        }
                                    }
                                }
                            .boxed()
                        }
                    ).await {
                        error!("Failed to emit login to {}: {}", endpoint_for_error, e);
                    }
                }
                .boxed()
            })
            .on("connect_error", move |_payload: Payload, _socket: Client| {
                let socket_ref = socket_ref_for_error.clone();
                let endpoint = endpoint_for_error.clone();
                async move {
                    error!("Connection error from socket server: {}", endpoint);
                    socket_ref.emit("agentStatus", json!({
                        "endpoint": endpoint,
                        "status": "offline",
                    })).ok();
                }
                .boxed()
            })
            .on("disconnect", move |_payload: Payload, _socket: Client| {
                let socket_ref = socket_ref_for_disconnect.clone();
                let endpoint = endpoint_for_disconnect.clone();
                async move {
                    info!("Disconnected from socket server: {}", endpoint);
                    socket_ref.emit("agentStatus", json!({
                        "endpoint": endpoint,
                        "status": "offline",
                    })).ok();
                }
                .boxed()
            })
            .on("agent", move |payload: Payload, _socket: Client| {
                let socket_ref = socket_ref_for_agent.clone();
                async move {
                    // Forward agent events to the main socket
                    if let Payload::Text(values) = payload {
                        socket_ref.emit("agent", values).ok();
                    }
                }
                .boxed()
            })
            .on("info", move |payload: Payload, socket: Client| {
                let socket_ref = socket_ref_for_info.clone();
                let endpoint = endpoint_for_info.clone();
                async move {
                    if let Payload::Text(values) = payload {
                        if let Some(info) = values.first() {
                            debug!("Agent info from {}: {:?}", endpoint, info);

                            // Check version compatibility (>= 1.4.0)
                            if let Some(version_str) = info.get("version").and_then(|v| v.as_str()) {
                                match semver::Version::parse(version_str) {
                                    Ok(version) => {
                                        let min_version = semver::Version::new(1, 4, 0);
                                        if version < min_version {
                                            warn!("Agent {} has unsupported version: {}", endpoint, version_str);
                                            socket_ref.emit("agentStatus", json!({
                                                "endpoint": endpoint,
                                                "status": "offline",
                                                "msg": format!("{}: Unsupported version: {}", endpoint, version_str),
                                            })).ok();
                                            socket.disconnect().await.ok();
                                        }
                                    }
                                    Err(e) => {
                                        warn!("Failed to parse version {} from {}: {}", version_str, endpoint, e);
                                    }
                                }
                            }
                        }
                    }
                }
                .boxed()
            })
            .connect()
            .await
        {
            Ok(client) => {
                // Store the client
                let mut clients = agent_clients.write().await;
                clients.insert(
                    endpoint.clone(),
                    AgentClient {
                        client,
                        logged_in: false,
                        endpoint: endpoint.clone(),
                    },
                );
                info!("Agent client stored for endpoint: {}", endpoint);
            }
            Err(e) => {
                error!("Failed to connect to {}: {}", endpoint, e);
                socket_ref.emit("agentStatus", json!({
                    "endpoint": endpoint,
                    "status": "offline",
                })).ok();
            }
        }
    }

    /// Disconnect from a specific endpoint
    pub async fn disconnect(&self, endpoint: &str) {
        let mut clients = self.agent_clients.write().await;
        if let Some(agent_client) = clients.remove(endpoint) {
            if let Err(e) = agent_client.client.disconnect().await {
                warn!("Error disconnecting from {}: {}", endpoint, e);
            }
            info!("Disconnected from agent: {}", endpoint);
        }
    }

    /// Connect to all agents in the database
    pub async fn connect_all(&self, endpoint: &str) {
        // Update first connect time
        {
            let mut first_time = self.first_connect_time.write().await;
            *first_time = Utc::now();
        }

        // If this socket is itself an agent, don't connect to others
        if !endpoint.is_empty() {
            info!("This connection is an agent ({}), skipping connectAll()", endpoint);
            return;
        }

        let agents = match Agent::find_all(&self.db, &self.encryption_secret).await {
            Ok(agents) => agents,
            Err(e) => {
                error!("Failed to list agents: {}", e);
                return;
            }
        };

        if !agents.is_empty() {
            info!("Connecting to {} agent socket server(s)...", agents.len());
        }

        for agent in agents {
            self.connect(&agent.url, &agent.username, &agent.password).await;
        }
    }

    /// Disconnect from all agents
    pub async fn disconnect_all(&self) {
        let mut clients = self.agent_clients.write().await;
        for (endpoint, agent_client) in clients.drain() {
            if let Err(e) = agent_client.client.disconnect().await {
                warn!("Error disconnecting from {}: {}", endpoint, e);
            }
        }
        info!("Disconnected from all agents for socket {}", self.socket_id);
    }

    /// Emit an event to a specific endpoint with retry logic
    pub async fn emit_to_endpoint(
        &self,
        endpoint: &str,
        event_name: &str,
        args: Value,
    ) -> Result<()> {
        debug!("Emitting event {} to endpoint: {}", event_name, endpoint);

        let client = {
            let clients = self.agent_clients.read().await;
            clients.get(endpoint).map(|c| c.client.clone())
        };

            let client = client.ok_or_else(|| {
            error!("Socket client not found for endpoint: {}", endpoint);
            anyhow!("Socket client not found for endpoint: {}", endpoint)
        })?;

        // Check if connected and logged in, with retry logic
        let is_ready = {
            let clients = self.agent_clients.read().await;
            if let Some(agent_client) = clients.get(endpoint) {
                agent_client.logged_in
            } else {
                false
            }
        };

        if !is_ready {
            // Check if within the 10-second window for retries
            let first_connect = *self.first_connect_time.read().await;
            let elapsed = (Utc::now() - first_connect).num_seconds();
            debug!("Endpoint {} not ready, elapsed: {}s", endpoint, elapsed);

            if elapsed < 10 {
                // Retry logic: poll every 1 second
                let mut attempts = 0;
                let max_attempts = (10 - elapsed).max(1) as u32;

                while attempts < max_attempts {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    
                    let clients = self.agent_clients.read().await;
                    if let Some(agent_client) = clients.get(endpoint) {
                        if agent_client.logged_in {
                            debug!("{}: Connected & Logged in after {} attempts", endpoint, attempts + 1);
                            drop(clients);
                            break;
                        }
                    }
                    
                    attempts += 1;
                    debug!("{}: not ready yet, retrying... (attempt {})", endpoint, attempts);
                }

                // Final check
                let clients = self.agent_clients.read().await;
                let is_logged_in = clients
                    .get(endpoint)
                    .map(|c| c.logged_in)
                    .unwrap_or(false);

                if !is_logged_in {
                    return Err(anyhow!(
                        "{}: Socket client not connected after retries",
                        endpoint
                    ));
                }
            } else {
                return Err(anyhow!("{}: Socket client not connected", endpoint));
            }
        }

        // Emit the event via the agent proxy
        let wrapped_args = json!([endpoint, event_name, args]);
        client
            .emit("agent", wrapped_args)
            .await
            .map_err(|e| anyhow!("Failed to emit to {}: {}", endpoint, e))?;

        Ok(())
    }

    /// Emit an event to all endpoints
    pub async fn emit_to_all_endpoints(&self, event_name: &str, args: Value) {
        debug!("Emitting event {} to all endpoints", event_name);
        
        let endpoints: Vec<String> = {
            let clients = self.agent_clients.read().await;
            clients.keys().cloned().collect()
        };

        for endpoint in endpoints {
            if let Err(e) = self.emit_to_endpoint(&endpoint, event_name, args.clone()).await {
                warn!("Failed to emit to {}: {}", endpoint, e);
            }
        }
    }

    /// Send the agent list to the client
    pub async fn send_agent_list(&self) {
        let agents = match Agent::find_all(&self.db, &self.encryption_secret).await {
            Ok(agents) => agents,
            Err(e) => {
                error!("Failed to list agents: {}", e);
                return;
            }
        };

        let mut agent_list = serde_json::Map::new();

        // Add myself (local endpoint)
        agent_list.insert(
            "".to_string(),
            json!({
                "url": "",
                "username": "",
                "endpoint": "",
            }),
        );

        // Add remote agents
        for agent in agents {
            if let Ok(endpoint) = agent.endpoint() {
                if let Ok(agent_json) = agent.to_json() {
                    agent_list.insert(endpoint, agent_json);
                }
            }
        }

        self.socket.emit("agentList", json!({
            "ok": true,
            "agentList": agent_list,
        })).ok();

        debug!("Sent agent list to socket {}", self.socket_id);
    }

    /// Emit agent status to the client
    async fn emit_agent_status(&self, endpoint: &str, status: AgentStatus, msg: Option<String>) {
        let mut data = json!({
            "endpoint": endpoint,
            "status": status.as_str(),
        });

        if let Some(msg) = msg {
            data["msg"] = json!(msg);
        }

        self.socket.emit("agentStatus", data).ok();
    }
}

impl Drop for AgentManager {
    fn drop(&mut self) {
        debug!("AgentManager dropped for socket {}", self.socket_id);
    }
}

/// Type alias for the global agent manager registry
type AgentManagerRegistry = Arc<RwLock<HashMap<String, Arc<AgentManager>>>>;

/// Global registry of AgentManagers by socket ID
static AGENT_MANAGERS: once_cell::sync::Lazy<AgentManagerRegistry> =
    once_cell::sync::Lazy::new(|| Arc::new(RwLock::new(HashMap::new())));

/// Store an AgentManager for a socket
pub async fn set_agent_manager(socket_id: &str, manager: Arc<AgentManager>) {
    let mut managers = AGENT_MANAGERS.write().await;
    managers.insert(socket_id.to_string(), manager);
}

/// Get an AgentManager for a socket
pub async fn get_agent_manager(socket_id: &str) -> Option<Arc<AgentManager>> {
    let managers = AGENT_MANAGERS.read().await;
    managers.get(socket_id).cloned()
}

/// Remove an AgentManager for a socket
pub async fn remove_agent_manager(socket_id: &str) {
    let mut managers = AGENT_MANAGERS.write().await;
    managers.remove(socket_id);
}
