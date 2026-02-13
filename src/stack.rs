// Stack Management Core (Phase 6)
//
// This module implements Docker Compose stack management with:
// - Stack struct for managing compose projects
// - Docker CLI operations via PTY (deploy, stop, restart, etc.)
// - Stack list scanning and status management
// - YAML/ENV file handling with comment preservation
// - Service status parsing from docker compose ps

use crate::server::ServerContext;
use crate::terminal::Terminal;
use crate::utils::constants::{
    ACCEPTED_COMPOSE_FILE_NAMES, COMBINED_TERMINAL_COLS, COMBINED_TERMINAL_ROWS, CREATED_FILE,
    CREATED_STACK, EXITED, PROGRESS_TERMINAL_ROWS, RUNNING, TERMINAL_ROWS, UNKNOWN,
};
use crate::utils::terminal::{get_combined_terminal_name, get_compose_terminal_name};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use socketioxide::extract::SocketRef;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs;
use tokio::process::Command;
use tracing::{debug, error, warn};
use yaml_rust2::YamlLoader;

/// Represents a Docker Compose stack
pub struct Stack {
    /// Stack name (directory name)
    pub name: String,
    /// Stack status
    status: i32,
    /// Endpoint identifier for terminal naming
    pub endpoint: String,
    /// Server context (config, io, db)
    ctx: Arc<ServerContext>,
    /// Lazily loaded compose YAML content
    compose_yaml: Option<String>,
    /// Lazily loaded .env content
    compose_env: Option<String>,
    /// Detected compose file name
    compose_file_name: String,
    /// Config file path from docker (for external stacks)
    config_file_path: Option<String>,
}

/// Simple JSON representation for stack lists
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackSimpleJson {
    pub name: String,
    pub status: i32,
    pub tags: Vec<String>,
    #[serde(rename = "isManagedByDockru")]
    pub is_managed_by_dockru: bool,
    #[serde(rename = "composeFileName")]
    pub compose_file_name: String,
    pub endpoint: String,
}

/// Full JSON representation with compose files
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackJson {
    pub name: String,
    pub status: i32,
    pub tags: Vec<String>,
    #[serde(rename = "isManagedByDockru")]
    pub is_managed_by_dockru: bool,
    #[serde(rename = "composeFileName")]
    pub compose_file_name: String,
    pub endpoint: String,
    #[serde(rename = "composeYAML")]
    pub compose_yaml: String,
    #[serde(rename = "composeENV")]
    pub compose_env: String,
    #[serde(rename = "primaryHostname")]
    pub primary_hostname: String,
}

/// Service status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceStatus {
    pub state: String,
    pub ports: Vec<String>,
}

/// Docker compose ls output format
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ComposeListItem {
    name: String,
    status: String,
    config_files: String,
}

/// Docker compose ps output format
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ComposePsItem {
    service: String,
    state: String,
    health: String,
    ports: String,
}

impl Stack {
    /// Create a new Stack instance
    ///
    /// # Arguments
    /// * `ctx` - Server context
    /// * `name` - Stack name
    /// * `endpoint` - Endpoint identifier
    pub fn new(ctx: Arc<ServerContext>, name: String, endpoint: String) -> Self {
        Self {
            name,
            status: UNKNOWN,
            endpoint,
            ctx,
            compose_yaml: None,
            compose_env: None,
            compose_file_name: "compose.yaml".to_string(),
            config_file_path: None,
        }
    }

    /// Create a new stack with provided YAML and ENV content
    pub fn new_with_content(
        ctx: Arc<ServerContext>,
        name: String,
        endpoint: String,
        compose_yaml: String,
        compose_env: String,
    ) -> Self {
        Self {
            name,
            status: UNKNOWN,
            endpoint,
            ctx,
            compose_yaml: Some(compose_yaml),
            compose_env: Some(compose_env),
            compose_file_name: "compose.yaml".to_string(),
            config_file_path: None,
        }
    }

    /// Get the stack's directory path
    pub fn path(&self) -> PathBuf {
        self.ctx.config.stacks_dir.join(&self.name)
    }

    /// Get the full absolute path
    pub fn full_path(&self) -> PathBuf {
        let path = self.path();
        if path.is_absolute() {
            path
        } else {
            std::env::current_dir().unwrap_or_default().join(path)
        }
    }

    /// Check if this stack is managed by Dockru (has a directory in stacks_dir)
    pub async fn is_managed_by_dockru(&self) -> bool {
        let path = self.path();
        match fs::metadata(&path).await {
            Ok(metadata) => metadata.is_dir(),
            Err(_) => false,
        }
    }

    /// Get the compose YAML content (lazy loaded from disk)
    pub async fn compose_yaml(&mut self) -> Result<String> {
        if let Some(ref yaml) = self.compose_yaml {
            return Ok(yaml.clone());
        }

        let path = self.path().join(&self.compose_file_name);
        match fs::read_to_string(&path).await {
            Ok(content) => {
                self.compose_yaml = Some(content.clone());
                Ok(content)
            }
            Err(_) => Ok(String::new()),
        }
    }

    /// Get the .env content (lazy loaded from disk)
    pub async fn compose_env(&mut self) -> Result<String> {
        if let Some(ref env) = self.compose_env {
            return Ok(env.clone());
        }

        let path = self.path().join(".env");
        match fs::read_to_string(&path).await {
            Ok(content) => {
                self.compose_env = Some(content.clone());
                Ok(content)
            }
            Err(_) => Ok(String::new()),
        }
    }

    /// Detect which compose file exists in the stack directory
    pub async fn detect_compose_file(&mut self) -> Result<()> {
        let stack_path = self.path();

        for filename in ACCEPTED_COMPOSE_FILE_NAMES {
            let compose_path = stack_path.join(filename);
            if fs::metadata(&compose_path).await.is_ok() {
                self.compose_file_name = filename.to_string();
                return Ok(());
            }
        }

        // Default to compose.yaml if nothing found
        self.compose_file_name = "compose.yaml".to_string();
        Ok(())
    }

    /// Validate the stack before saving
    pub async fn validate(&mut self) -> Result<()> {
        // Check name, allows [a-z][0-9] _ - only (must be non-empty)
        if self.name.is_empty() {
            anyhow::bail!("Stack name must not be empty");
        }
        if !self
            .name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
        {
            anyhow::bail!("Stack name can only contain [a-z][0-9] _ - only");
        }

        // Check YAML format
        let yaml = self.compose_yaml().await?;
        YamlLoader::load_from_str(&yaml).context("Invalid YAML format")?;

        // Check .env format
        let env = self.compose_env().await?;
        let lines: Vec<&str> = env.lines().collect();

        // Prevent "setenv: The parameter is incorrect"
        // It only happens when there is one line and it doesn't contain "="
        if lines.len() == 1 && !lines[0].contains('=') && !lines[0].is_empty() {
            anyhow::bail!("Invalid .env format");
        }

        Ok(())
    }

    /// Build docker compose command options
    fn get_compose_options(&self, command: &str, extra_options: &[&str]) -> Vec<String> {
        let mut options = vec!["compose".to_string()];

        // Check for global.env
        let global_env_path = self.ctx.config.stacks_dir.join("global.env");
        if global_env_path.exists() {
            options.push("--env-file".to_string());
            options.push("../global.env".to_string());

            // Add per-stack .env if it exists
            let stack_env_path = self.path().join(".env");
            if stack_env_path.exists() {
                options.push("--env-file".to_string());
                options.push("./.env".to_string());
            }
        }

        options.push(command.to_string());
        options.extend(extra_options.iter().map(|s| s.to_string()));

        debug!("Docker compose options: {:?}", options);
        options
    }

    /// Convert status string from docker compose to numeric status
    ///
    /// Examples: "running(2)", "exited(1)", "created(1)"
    pub fn status_convert(status: &str) -> i32 {
        let status_lower = status.to_lowercase();

        if status_lower.starts_with("created") {
            CREATED_STACK
        } else if status_lower.contains("exited") {
            // If one service is exited, consider the stack exited
            EXITED
        } else if status_lower.starts_with("running") {
            RUNNING
        } else {
            UNKNOWN
        }
    }

    /// Convert to simple JSON representation
    pub async fn to_simple_json(&self) -> StackSimpleJson {
        StackSimpleJson {
            name: self.name.clone(),
            status: self.status,
            tags: Vec::new(),
            is_managed_by_dockru: self.is_managed_by_dockru().await,
            compose_file_name: self.compose_file_name.clone(),
            endpoint: self.endpoint.clone(),
        }
    }

    /// Convert to full JSON representation
    pub async fn to_json(&mut self) -> Result<StackJson> {
        let compose_yaml = self.compose_yaml().await?;
        let compose_env = self.compose_env().await?;

        // Determine primary hostname
        let primary_hostname = if self.endpoint.is_empty() {
            "localhost".to_string()
        } else {
            // Try to parse endpoint as URL
            if let Ok(url) = url::Url::parse(&format!("https://{}", self.endpoint)) {
                url.host_str().unwrap_or("localhost").to_string()
            } else {
                "localhost".to_string()
            }
        };

        Ok(StackJson {
            name: self.name.clone(),
            status: self.status,
            tags: Vec::new(),
            is_managed_by_dockru: self.is_managed_by_dockru().await,
            compose_file_name: self.compose_file_name.clone(),
            endpoint: self.endpoint.clone(),
            compose_yaml,
            compose_env,
            primary_hostname,
        })
    }
}

impl Stack {
    // =============================================================================
    // Docker Operations (via PTY and child processes)
    // =============================================================================

    /// Save stack files to disk
    ///
    /// # Arguments
    /// * `is_add` - If true, create new directory; if false, update existing
    pub async fn save(&mut self, is_add: bool) -> Result<()> {
        self.validate().await?;

        let dir = self.path();
        warn!(
            "Stack save: name={}, is_add={}, dir={}",
            self.name,
            is_add,
            dir.display()
        );

        if is_add {
            if fs::metadata(&dir).await.is_ok() {
                warn!("Stack save: directory already exists at {}", dir.display());
                anyhow::bail!("Stack name already exists");
            }
            fs::create_dir_all(&dir)
                .await
                .context("Failed to create stack directory")?;
        } else {
            if fs::metadata(&dir).await.is_err() {
                anyhow::bail!("Stack not found");
            }
        }

        // Write compose file
        let compose_path = dir.join(&self.compose_file_name);
        let yaml = self.compose_yaml().await?;
        fs::write(&compose_path, yaml)
            .await
            .context("Failed to write compose file")?;

        // Write .env file if it has content or already exists
        let env_path = dir.join(".env");
        let env = self.compose_env().await?;

        if fs::metadata(&env_path).await.is_ok() || !env.trim().is_empty() {
            fs::write(&env_path, env)
                .await
                .context("Failed to write .env file")?;
        }

        Ok(())
    }

    /// Deploy the stack (docker compose up -d --remove-orphans)
    ///
    /// # Arguments
    /// * `socket` - Optional socket for terminal output
    pub async fn deploy(&self, socket: Option<SocketRef>) -> Result<i32> {
        let terminal_name = get_compose_terminal_name(&self.endpoint, &self.name);
        let options = self.get_compose_options("up", &["-d", "--remove-orphans"]);
        let options_refs: Vec<&str> = options.iter().map(|s| s.as_str()).collect();

        let exit_code = Terminal::exec(
            self.ctx.io.clone(),
            socket,
            terminal_name,
            "docker".to_string(),
            options,
            self.path().display().to_string(),
        )
        .await?;

        if exit_code != 0 {
            anyhow::bail!(
                "Failed to deploy, please check the terminal output for more information."
            );
        }

        Ok(exit_code)
    }

    /// Start the stack (same as deploy)
    pub async fn start(&self, socket: Option<SocketRef>) -> Result<i32> {
        self.deploy(socket).await
    }

    /// Stop the stack (docker compose stop)
    pub async fn stop(&self, socket: Option<SocketRef>) -> Result<i32> {
        let terminal_name = get_compose_terminal_name(&self.endpoint, &self.name);
        let options = self.get_compose_options("stop", &[]);

        let exit_code = Terminal::exec(
            self.ctx.io.clone(),
            socket,
            terminal_name,
            "docker".to_string(),
            options,
            self.path().display().to_string(),
        )
        .await?;

        if exit_code != 0 {
            anyhow::bail!("Failed to stop, please check the terminal output for more information.");
        }

        Ok(exit_code)
    }

    /// Restart the stack (docker compose restart)
    pub async fn restart(&self, socket: Option<SocketRef>) -> Result<i32> {
        let terminal_name = get_compose_terminal_name(&self.endpoint, &self.name);
        let options = self.get_compose_options("restart", &[]);

        let exit_code = Terminal::exec(
            self.ctx.io.clone(),
            socket,
            terminal_name,
            "docker".to_string(),
            options,
            self.path().display().to_string(),
        )
        .await?;

        if exit_code != 0 {
            anyhow::bail!(
                "Failed to restart, please check the terminal output for more information."
            );
        }

        Ok(exit_code)
    }

    /// Down the stack (docker compose down)
    pub async fn down(&self, socket: Option<SocketRef>) -> Result<i32> {
        let terminal_name = get_compose_terminal_name(&self.endpoint, &self.name);
        let options = self.get_compose_options("down", &[]);

        let exit_code = Terminal::exec(
            self.ctx.io.clone(),
            socket,
            terminal_name,
            "docker".to_string(),
            options,
            self.path().display().to_string(),
        )
        .await?;

        if exit_code != 0 {
            anyhow::bail!("Failed to down, please check the terminal output for more information.");
        }

        Ok(exit_code)
    }

    /// Update the stack (docker compose pull, then up -d if running)
    pub async fn update(&mut self, socket: Option<SocketRef>) -> Result<i32> {
        let terminal_name = get_compose_terminal_name(&self.endpoint, &self.name);
        let options = self.get_compose_options("pull", &[]);

        let exit_code = Terminal::exec(
            self.ctx.io.clone(),
            socket.as_ref().map(|s| s.clone()),
            terminal_name.clone(),
            "docker".to_string(),
            options,
            self.path().display().to_string(),
        )
        .await?;

        if exit_code != 0 {
            anyhow::bail!("Failed to pull, please check the terminal output for more information.");
        }

        // Update status
        self.update_status().await?;

        debug!("Stack status after pull: {}", self.status);

        // If the stack is not running, don't restart it
        if self.status != RUNNING {
            return Ok(exit_code);
        }

        // Restart the stack
        let options = self.get_compose_options("up", &["-d", "--remove-orphans"]);
        let exit_code = Terminal::exec(
            self.ctx.io.clone(),
            socket,
            terminal_name,
            "docker".to_string(),
            options,
            self.path().display().to_string(),
        )
        .await?;

        if exit_code != 0 {
            anyhow::bail!(
                "Failed to restart, please check the terminal output for more information."
            );
        }

        Ok(exit_code)
    }

    /// Delete the stack (down + remove directory)
    pub async fn delete(&self, socket: Option<SocketRef>) -> Result<i32> {
        let terminal_name = get_compose_terminal_name(&self.endpoint, &self.name);
        let options = self.get_compose_options("down", &["--remove-orphans"]);

        let exit_code = Terminal::exec(
            self.ctx.io.clone(),
            socket,
            terminal_name,
            "docker".to_string(),
            options,
            self.path().display().to_string(),
        )
        .await?;

        if exit_code != 0 {
            anyhow::bail!(
                "Failed to delete, please check the terminal output for more information."
            );
        }

        // Remove the stack directory
        fs::remove_dir_all(&self.path())
            .await
            .context("Failed to remove stack directory")?;

        Ok(exit_code)
    }

    /// Update the status of this stack
    pub async fn update_status(&mut self) -> Result<()> {
        let status_list = Self::get_status_list(self.ctx.clone()).await?;

        if let Some(&status) = status_list.get(&self.name) {
            self.status = status;
        } else {
            self.status = UNKNOWN;
        }

        Ok(())
    }

    /// Get service status list for this stack
    pub async fn get_service_status_list(&self) -> Result<HashMap<String, ServiceStatus>> {
        let mut status_list = HashMap::new();

        let options = self.get_compose_options("ps", &["--format", "json"]);

        let output = Command::new("docker")
            .args(&options)
            .current_dir(&self.path())
            .output()
            .await
            .context("Failed to run docker compose ps")?;

        if !output.status.success() {
            warn!("docker compose ps failed for stack {}", self.name);
            return Ok(status_list);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);

        // Parse each line as JSON
        for line in stdout.lines() {
            if line.trim().is_empty() {
                continue;
            }

            match serde_json::from_str::<ComposePsItem>(line) {
                Ok(item) => {
                    // Filter ports to only those with -> (port mappings)
                    let ports: Vec<String> = item
                        .ports
                        .split(", ")
                        .filter(|s| s.contains("->"))
                        .map(|s| s.to_string())
                        .collect();

                    let state = if !item.health.is_empty() {
                        item.health
                    } else {
                        item.state
                    };

                    status_list.insert(item.service, ServiceStatus { state, ports });
                }
                Err(e) => {
                    warn!("Failed to parse compose ps output line: {} - {}", line, e);
                }
            }
        }

        Ok(status_list)
    }

    /// Join the combined terminal (docker compose logs -f --tail 100)
    pub async fn join_combined_terminal(&self, socket: SocketRef) -> Result<()> {
        let terminal_name = get_combined_terminal_name(&self.endpoint, &self.name);
        let options = self.get_compose_options("logs", &["-f", "--tail", "100"]);

        let terminal = Terminal::get_or_create_terminal(
            self.ctx.io.clone(),
            terminal_name,
            "docker".to_string(),
            options,
            self.path().display().to_string(),
        )
        .await;

        // Enable keep-alive and set dimensions
        terminal.enable_keep_alive(true).await;
        terminal.set_rows(COMBINED_TERMINAL_ROWS).await?;
        terminal.set_cols(COMBINED_TERMINAL_COLS).await?;
        terminal.join(socket).await?;
        terminal
            .start(
                "docker".to_string(),
                self.get_compose_options("logs", &["-f", "--tail", "100"]),
                self.path().display().to_string(),
            )
            .await?;

        Ok(())
    }

    /// Leave the combined terminal
    pub async fn leave_combined_terminal(&self, socket: SocketRef) -> Result<()> {
        let terminal_name = get_combined_terminal_name(&self.endpoint, &self.name);

        if let Some(terminal) = Terminal::get_terminal(&terminal_name).await {
            terminal.leave(socket).await?;
        }

        Ok(())
    }

    /// Join a container's interactive terminal (docker compose exec <service> <shell>)
    ///
    /// # Arguments
    /// * `socket` - Socket to join for terminal I/O
    /// * `service_name` - Service name from compose file
    /// * `shell` - Shell to execute (e.g., "/bin/bash", "sh", "ash")
    /// * `index` - Terminal instance index (for multiple connections to same service)
    pub async fn join_container_terminal(
        &self,
        socket: SocketRef,
        service_name: &str,
        shell: &str,
        index: usize,
    ) -> Result<()> {
        let terminal_name = crate::utils::terminal::get_container_exec_terminal_name(
            &self.endpoint,
            &self.name,
            service_name,
            index,
        );

        let terminal = if let Some(term) = Terminal::get_terminal(&terminal_name).await {
            debug!("Terminal {} already exists, reusing it", terminal_name);
            term
        } else {
            // Create new interactive terminal
            let options = self.get_compose_options("exec", &[service_name, shell]);
            let term = Terminal::new_interactive(
                self.ctx.io.clone(),
                terminal_name.clone(),
                "docker".to_string(),
                options.clone(),
                self.path().display().to_string(),
            );
            term.set_rows(TERMINAL_ROWS).await?;
            debug!(
                "Terminal {} created for service {} with shell {}",
                terminal_name, service_name, shell
            );
            term
        };

        terminal.join(socket).await?;
        terminal
            .start(
                "docker".to_string(),
                self.get_compose_options("exec", &[service_name, shell]),
                self.path().display().to_string(),
            )
            .await?;

        Ok(())
    }

    // =============================================================================
    // Static Methods
    // =============================================================================

    /// Check if a compose file exists in the specified directory
    pub async fn compose_file_exists(stacks_dir: &Path, name: &str) -> bool {
        let stack_path = stacks_dir.join(name);

        for filename in ACCEPTED_COMPOSE_FILE_NAMES {
            let compose_path = stack_path.join(filename);
            if fs::metadata(&compose_path).await.is_ok() {
                return true;
            }
        }

        false
    }

    /// Get the status list from docker compose ls
    pub async fn get_status_list(ctx: Arc<ServerContext>) -> Result<HashMap<String, i32>> {
        let mut status_list = HashMap::new();

        let output = Command::new("docker")
            .args(&["compose", "ls", "--all", "--format", "json"])
            .output()
            .await
            .context("Failed to run docker compose ls")?;

        if !output.status.success() {
            return Ok(status_list);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);

        match serde_json::from_str::<Vec<ComposeListItem>>(&stdout) {
            Ok(compose_list) => {
                for item in compose_list {
                    status_list.insert(item.name, Self::status_convert(&item.status));
                }
            }
            Err(e) => {
                warn!("Failed to parse docker compose ls output: {}", e);
            }
        }

        Ok(status_list)
    }

    /// Get a single stack by name
    pub async fn get_stack(ctx: Arc<ServerContext>, name: &str, endpoint: String) -> Result<Stack> {
        let stack_path = ctx.config.stacks_dir.join(name);

        // Check if directory exists in stacks_dir (managed stack)
        if let Ok(metadata) = fs::metadata(&stack_path).await {
            if metadata.is_dir() {
                let mut stack = Stack::new(ctx, name.to_string(), endpoint);
                stack.detect_compose_file().await?;
                stack.status = UNKNOWN;
                stack.config_file_path = Some(stack_path.display().to_string());
                return Ok(stack);
            }
        }

        // Directory doesn't exist — check if it's an unmanaged stack known to docker compose
        let stack_list = Self::get_stack_list(ctx.clone(), endpoint.clone(), true).await?;
        if let Some(stack) = stack_list
            .into_iter()
            .find(|(n, _)| n == name)
            .map(|(_, s)| s)
        {
            return Ok(stack);
        }

        anyhow::bail!("Stack not found");
    }

    /// Get the complete stack list (managed + unmanaged stacks)
    ///
    /// # Arguments
    /// * `ctx` - Server context
    /// * `endpoint` - Endpoint identifier
    /// * `use_cache_for_managed` - If true, use cached managed stack list
    pub async fn get_stack_list(
        ctx: Arc<ServerContext>,
        endpoint: String,
        use_cache_for_managed: bool,
    ) -> Result<HashMap<String, Stack>> {
        let mut stack_list = HashMap::new();

        // TODO: Implement caching mechanism for managed stacks
        // For now, always scan the directory

        // Scan the stacks directory
        let stacks_dir = &ctx.config.stacks_dir;

        let mut entries = match fs::read_dir(stacks_dir).await {
            Ok(entries) => entries,
            Err(e) => {
                warn!("Failed to read stacks directory: {}", e);
                return Ok(stack_list);
            }
        };

        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            let filename = match entry.file_name().into_string() {
                Ok(name) => name,
                Err(_) => continue,
            };

            // Check if it's a directory
            let metadata = match fs::metadata(&path).await {
                Ok(m) => m,
                Err(_) => continue,
            };

            if !metadata.is_dir() {
                continue;
            }

            // Check if compose file exists
            if !Self::compose_file_exists(stacks_dir, &filename).await {
                continue;
            }

            let mut stack = Stack::new(ctx.clone(), filename.clone(), endpoint.clone());
            stack.detect_compose_file().await?;
            stack.status = CREATED_FILE;
            stack_list.insert(filename, stack);
        }

        // Get status from docker compose ls
        let output = Command::new("docker")
            .args(&["compose", "ls", "--all", "--format", "json"])
            .output()
            .await
            .context("Failed to run docker compose ls")?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);

            match serde_json::from_str::<Vec<ComposeListItem>>(&stdout) {
                Ok(compose_list) => {
                    for item in compose_list {
                        // Skip the dockru stack if not managed
                        if item.name == "dockru" && !stack_list.contains_key(&item.name) {
                            continue;
                        }

                        let status = Self::status_convert(&item.status);

                        if let Some(stack) = stack_list.get_mut(&item.name) {
                            // Update existing stack
                            stack.status = status;
                            stack.config_file_path = Some(item.config_files.clone());
                        } else {
                            // Add unmanaged stack
                            let mut stack =
                                Stack::new(ctx.clone(), item.name.clone(), endpoint.clone());
                            stack.status = status;
                            stack.config_file_path = Some(item.config_files.clone());
                            stack_list.insert(item.name, stack);
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to parse docker compose ls output: {}", e);
                }
            }
        }

        Ok(stack_list)
    }
}

// TODO: Implement Docker operations (deploy, stop, restart, etc.)
// TODO: Implement static methods (get_stack_list, get_status_list, etc.)
// TODO: Implement service status parsing
// TODO: Implement terminal operations (join_combined_terminal, etc.)
