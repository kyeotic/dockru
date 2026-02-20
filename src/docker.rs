//! Docker Integration Module
//!
//! This module provides a unified interface for all Docker operations in Dockru,
//! including both Docker Engine API (via Bollard) and Docker Compose CLI operations.
//!
//! ## Architecture
//!
//! ### API Operations (Bollard)
//! - Direct Docker Engine API calls for information gathering
//! - Network listing, container status, service status
//! - Fast, structured responses with no text parsing
//!
//! ### CLI Operations (docker compose)
//! - Docker Compose orchestration (up/down/restart)
//! - Interactive terminals (logs, exec)
//! - Compose project discovery
//!
//! ## Hybrid Approach
//!
//! Docker Compose is not part of the Docker daemon API, so we use a hybrid approach:
//! - **Bollard** for container/network information (faster, more reliable)
//! - **CLI** for compose orchestration (only available via CLI)
//!
//! ## Usage
//!
//! Stack operations call functions in this module passing:
//! - `docker: &Docker` - Bollard client from ServerContext
//! - `stack_name: &str` - Compose project name
//! - `stack_path: &Path` - Path to compose directory
//! - `endpoint: &str` - Agent endpoint (or empty for local)
//! - `socket: Option<SocketRef>` - For streaming output to client
//!
//! This module handles all Docker implementation details, allowing Stack to focus
//! on compose file management and high-level orchestration logic.

use anyhow::{Context, Result};
use bollard::container::ListContainersOptions;
use bollard::errors::Error as BollardError;
use bollard::models::ContainerSummary;
use bollard::network::ListNetworksOptions;
use bollard::Docker;
use serde::Deserialize;
use socketioxide::extract::SocketRef;
use std::collections::HashMap;
use std::path::Path;
use tokio::process::Command;

use crate::terminal::Terminal;
use crate::utils::constants::{
    COMBINED_TERMINAL_COLS, COMBINED_TERMINAL_ROWS, CREATED_STACK, EXITED, RUNNING, TERMINAL_ROWS,
    UNKNOWN,
};
use crate::utils::terminal::{
    get_combined_terminal_name, get_compose_terminal_name, get_container_exec_terminal_name,
    get_container_logs_terminal_name,
};

/// Extension trait for converting bollard errors to anyhow::Result
pub trait BollardResultExt<T> {
    fn docker_context(self, context: &str) -> Result<T>;
}

impl<T> BollardResultExt<T> for Result<T, BollardError> {
    fn docker_context(self, context: &str) -> Result<T> {
        self.map_err(|e| match e {
            BollardError::DockerResponseServerError {
                status_code,
                message,
            } => {
                anyhow::anyhow!(
                    "{} - Docker API error ({}): {}",
                    context,
                    status_code,
                    message
                )
            }
            _ => anyhow::anyhow!("{}: {}", context, e),
        })
        .with_context(|| format!("Docker operation failed: {}", context))
    }
}

/// List Docker networks
pub async fn list_networks(docker: &Docker) -> Result<Vec<String>> {
    let networks = docker
        .list_networks(None::<ListNetworksOptions<String>>)
        .await
        .docker_context("Failed to list Docker networks")?;

    let network_names: Vec<String> = networks.into_iter().filter_map(|n| n.name).collect();

    Ok(network_names)
}

/// List containers for a Docker Compose project
pub async fn list_containers_by_project(
    docker: &Docker,
    project_name: &str,
) -> Result<Vec<ContainerSummary>> {
    let mut filters = HashMap::new();
    filters.insert(
        "label".to_string(),
        vec![format!("com.docker.compose.project={}", project_name)],
    );

    let options = ListContainersOptions {
        all: true,
        filters,
        ..Default::default()
    };

    docker
        .list_containers(Some(options))
        .await
        .docker_context(&format!(
            "Failed to list containers for project {}",
            project_name
        ))
}

/// Map container summary to ServiceStatus
pub fn map_to_service_status(
    containers: Vec<ContainerSummary>,
) -> HashMap<String, crate::stack::ServiceStatus> {
    let mut status_map = HashMap::new();

    for container in containers {
        // Extract service name from label
        let service_name = container
            .labels
            .as_ref()
            .and_then(|labels| labels.get("com.docker.compose.service"))
            .map(|s| s.to_string());

        if let Some(service) = service_name {
            // Determine state (prefer Status over State for health info)
            let state = if let Some(status) = container.status.as_ref() {
                status.clone()
            } else if let Some(state) = container.state.as_ref() {
                state.clone()
            } else {
                "unknown".to_string()
            };

            // Extract port mappings
            let ports: Vec<String> = container
                .ports
                .unwrap_or_default()
                .iter()
                .filter_map(|p| {
                    p.public_port.map(|public| format!("{}:{}", public, p.private_port))
                })
                .collect();

            status_map.insert(
                service,
                crate::stack::ServiceStatus { state, ports },
            );
        }
    }

    status_map
}

//------------------------------------------------------------------------------
// Compose Command Building
//------------------------------------------------------------------------------

/// Build docker compose command options including env files
///
/// Constructs the complete argument list for docker compose commands:
/// - Starts with ["compose"]
/// - Adds global.env if it exists in stacks_dir parent
/// - Adds .env if it exists in stack directory (only if global.env exists)
/// - Appends the command (up, stop, logs, etc.)
/// - Extends with extra options
///
/// # Arguments
/// * `stacks_dir` - Path to the stacks directory
/// * `stack_name` - Name of the stack (directory name)
/// * `command` - Docker compose subcommand ("up", "stop", "logs", etc.)
/// * `extra_options` - Additional flags/options for the command
pub fn compose_options(
    stacks_dir: &Path,
    stack_name: &str,
    command: &str,
    extra_options: &[&str],
) -> Vec<String> {
    let mut options = vec!["compose".to_string()];

    // Check for global.env in stacks_dir
    let global_env_path = stacks_dir.join("global.env");
    if global_env_path.exists() {
        options.push("--env-file".to_string());
        options.push("../global.env".to_string());

        // Add per-stack .env if it exists (only if global.env exists)
        let stack_env_path = stacks_dir.join(stack_name).join(".env");
        if stack_env_path.exists() {
            options.push("--env-file".to_string());
            options.push("./.env".to_string());
        }
    }

    // Add the command
    options.push(command.to_string());

    // Add extra options
    options.extend(extra_options.iter().map(|s| s.to_string()));

    options
}

//------------------------------------------------------------------------------
// Compose Orchestration
//------------------------------------------------------------------------------

/// Deploy a compose stack (up -d --remove-orphans)
///
/// # Arguments
/// * `io` - SocketIo instance for terminal communication
/// * `stack_name` - Name of the compose project
/// * `stack_path` - Path to the directory containing compose file
/// * `stacks_dir` - Path to the stacks directory (for env file resolution)
/// * `endpoint` - Agent endpoint (empty string for local)
/// * `socket` - Optional socket for streaming output
///
/// # Returns
/// Exit code from docker compose command (0 = success)
pub async fn deploy(
    io: socketioxide::SocketIo,
    stack_name: &str,
    stack_path: &Path,
    stacks_dir: &Path,
    endpoint: &str,
    socket: Option<SocketRef>,
) -> Result<i32> {
    let terminal_name = get_compose_terminal_name(endpoint, stack_name);
    let options = compose_options(stacks_dir, stack_name, "up", &["-d", "--remove-orphans"]);

    let exit_code = Terminal::exec(
        io,
        socket,
        terminal_name,
        "docker".to_string(),
        options,
        stack_path.display().to_string(),
    )
    .await
    .context("Failed to execute docker compose up")?;

    if exit_code != 0 {
        anyhow::bail!("Failed to deploy, please check the terminal output for more information.");
    }

    Ok(exit_code)
}

/// Stop a compose stack
pub async fn stop(
    io: socketioxide::SocketIo,
    stack_name: &str,
    stack_path: &Path,
    stacks_dir: &Path,
    endpoint: &str,
    socket: Option<SocketRef>,
) -> Result<i32> {
    let terminal_name = get_compose_terminal_name(endpoint, stack_name);
    let options = compose_options(stacks_dir, stack_name, "stop", &[]);

    let exit_code = Terminal::exec(
        io,
        socket,
        terminal_name,
        "docker".to_string(),
        options,
        stack_path.display().to_string(),
    )
    .await
    .context("Failed to execute docker compose stop")?;

    if exit_code != 0 {
        anyhow::bail!("Failed to stop, please check the terminal output for more information.");
    }

    Ok(exit_code)
}

/// Restart a compose stack
pub async fn restart(
    io: socketioxide::SocketIo,
    stack_name: &str,
    stack_path: &Path,
    stacks_dir: &Path,
    endpoint: &str,
    socket: Option<SocketRef>,
) -> Result<i32> {
    let terminal_name = get_compose_terminal_name(endpoint, stack_name);
    let options = compose_options(stacks_dir, stack_name, "restart", &[]);

    let exit_code = Terminal::exec(
        io,
        socket,
        terminal_name,
        "docker".to_string(),
        options,
        stack_path.display().to_string(),
    )
    .await
    .context("Failed to execute docker compose restart")?;

    if exit_code != 0 {
        anyhow::bail!("Failed to restart, please check the terminal output for more information.");
    }

    Ok(exit_code)
}

/// Shut down a compose stack (down)
pub async fn down(
    io: socketioxide::SocketIo,
    stack_name: &str,
    stack_path: &Path,
    stacks_dir: &Path,
    endpoint: &str,
    socket: Option<SocketRef>,
) -> Result<i32> {
    let terminal_name = get_compose_terminal_name(endpoint, stack_name);
    let options = compose_options(stacks_dir, stack_name, "down", &[]);

    let exit_code = Terminal::exec(
        io,
        socket,
        terminal_name,
        "docker".to_string(),
        options,
        stack_path.display().to_string(),
    )
    .await
    .context("Failed to execute docker compose down")?;

    if exit_code != 0 {
        anyhow::bail!("Failed to shut down, please check the terminal output for more information.");
    }

    Ok(exit_code)
}

/// Update a compose stack (pull + redeploy if running)
///
/// Returns exit code from final operation (pull or deploy)
pub async fn update(
    io: socketioxide::SocketIo,
    docker: &Docker,
    stack_name: &str,
    stack_path: &Path,
    stacks_dir: &Path,
    endpoint: &str,
    socket: Option<SocketRef>,
) -> Result<i32> {
    let terminal_name = get_compose_terminal_name(endpoint, stack_name);
    let options = compose_options(stacks_dir, stack_name, "pull", &[]);

    // Pull latest images
    let exit_code = Terminal::exec(
        io.clone(),
        socket.clone(),
        terminal_name,
        "docker".to_string(),
        options,
        stack_path.display().to_string(),
    )
    .await
    .context("Failed to execute docker compose pull")?;

    if exit_code != 0 {
        anyhow::bail!("Failed to pull, please check the terminal output for more information.");
    }

    // Check if stack is running
    let containers = list_containers_by_project(docker, stack_name)
        .await
        .unwrap_or_default();

    let is_running = containers.iter().any(|c| {
        c.state.as_ref().map(|s| s == "running").unwrap_or(false)
    });

    // Only restart if it was running
    if is_running {
        deploy(io, stack_name, stack_path, stacks_dir, endpoint, socket).await
    } else {
        Ok(exit_code)
    }
}

/// Delete a compose stack (down --remove-orphans + remove directory)
///
/// Two-phase operation:
/// 1. Run docker compose down --remove-orphans
/// 2. Remove stack directory from filesystem
///
/// Returns exit code from docker compose down
pub async fn delete(
    io: socketioxide::SocketIo,
    stack_name: &str,
    stack_path: &Path,
    stacks_dir: &Path,
    endpoint: &str,
    socket: Option<SocketRef>,
) -> Result<i32> {
    let terminal_name = get_compose_terminal_name(endpoint, stack_name);
    let options = compose_options(stacks_dir, stack_name, "down", &["--remove-orphans"]);

    let exit_code = Terminal::exec(
        io,
        socket,
        terminal_name,
        "docker".to_string(),
        options,
        stack_path.display().to_string(),
    )
    .await
    .context("Failed to execute docker compose down")?;

    if exit_code != 0 {
        anyhow::bail!("Failed to delete, please check the terminal output for more information.");
    }

    // Remove the stack directory
    tokio::fs::remove_dir_all(stack_path)
        .await
        .context("Failed to remove stack directory")?;

    Ok(exit_code)
}

//------------------------------------------------------------------------------
// Terminal Operations (Logs & Exec)
//------------------------------------------------------------------------------

/// Join the combined logs terminal for a stack
///
/// Creates or reuses a persistent terminal streaming logs from all services.
/// Uses keep-alive mode - terminal closes after 60s if no clients connected.
///
/// # Arguments
/// * `io` - SocketIo instance for terminal communication
/// * `stack_name` - Name of the compose project
/// * `stack_path` - Path to the directory containing compose file
/// * `stacks_dir` - Path to the stacks directory (for env file resolution)
/// * `endpoint` - Agent endpoint (empty string for local)
/// * `socket` - Socket to join to terminal room
pub async fn join_logs_terminal(
    io: socketioxide::SocketIo,
    stack_name: &str,
    stack_path: &Path,
    stacks_dir: &Path,
    endpoint: &str,
    socket: SocketRef,
) -> Result<()> {
    let terminal_name = get_combined_terminal_name(endpoint, stack_name);
    let options = compose_options(stacks_dir, stack_name, "logs", &["-f", "--tail", "100"]);

    let terminal = Terminal::get_or_create_terminal(
        io,
        terminal_name,
        "docker".to_string(),
        options.clone(),
        stack_path.display().to_string(),
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
            options,
            stack_path.display().to_string(),
        )
        .await?;

    Ok(())
}

/// Leave the combined logs terminal for a stack
///
/// Removes socket from terminal room. May trigger terminal closure if
/// keep-alive is enabled and room becomes empty.
pub async fn leave_logs_terminal(
    stack_name: &str,
    endpoint: &str,
    socket: SocketRef,
) -> Result<()> {
    let terminal_name = get_combined_terminal_name(endpoint, stack_name);

    if let Some(terminal) = Terminal::get_terminal(&terminal_name).await {
        terminal.leave(socket).await?;
    }

    Ok(())
}

/// Join an interactive exec terminal for a service container
///
/// Creates or reuses an interactive terminal for shell access to a service.
/// Multiple terminals can exist for the same service (differentiated by index).
///
/// # Arguments
/// * `io` - SocketIo instance for terminal communication
/// * `stack_name` - Name of the compose project
/// * `stack_path` - Path to the directory containing compose file
/// * `stacks_dir` - Path to the stacks directory (for env file resolution)
/// * `endpoint` - Agent endpoint (empty string for local)
/// * `service_name` - Service name from compose file
/// * `shell` - Shell to execute (e.g., "bash", "sh", "/bin/sh")
/// * `index` - Terminal index (allows multiple terminals per service)
/// * `socket` - Socket to join to terminal room
pub async fn join_exec_terminal(
    io: socketioxide::SocketIo,
    stack_name: &str,
    stack_path: &Path,
    stacks_dir: &Path,
    endpoint: &str,
    service_name: &str,
    shell: &str,
    index: usize,
    socket: SocketRef,
) -> Result<()> {
    let terminal_name = get_container_exec_terminal_name(endpoint, stack_name, service_name, index);
    let options = compose_options(stacks_dir, stack_name, "exec", &[service_name, shell]);

    // Check if terminal already exists
    let terminal = if let Some(term) = Terminal::get_terminal(&terminal_name).await {
        term
    } else {
        // Create new interactive terminal
        let term = Terminal::new_interactive(
            io,
            terminal_name,
            "docker".to_string(),
            options.clone(),
            stack_path.display().to_string(),
        );
        term.set_rows(TERMINAL_ROWS).await?;
        term
    };

    terminal.join(socket).await?;
    terminal
        .start(
            "docker".to_string(),
            options,
            stack_path.display().to_string(),
        )
        .await?;

    Ok(())
}

/// Join or create a container logs terminal (docker compose logs -f --tail 100 <service>)
pub async fn join_container_logs_terminal(
    io: socketioxide::SocketIo,
    stack_name: &str,
    stack_path: &Path,
    stacks_dir: &Path,
    endpoint: &str,
    service_name: &str,
    socket: SocketRef,
) -> Result<()> {
    let terminal_name = get_container_logs_terminal_name(endpoint, stack_name, service_name);
    let options = compose_options(stacks_dir, stack_name, "logs", &["-f", "--tail", "100", service_name]);

    // Get or create terminal
    let terminal = Terminal::get_or_create_terminal(
        io,
        terminal_name,
        "docker".to_string(),
        options.clone(),
        stack_path.display().to_string(),
    )
    .await;
    terminal.set_rows(TERMINAL_ROWS).await?;

    terminal.join(socket).await?;
    terminal
        .start(
            "docker".to_string(),
            options,
            stack_path.display().to_string(),
        )
        .await?;

    Ok(())
}

//------------------------------------------------------------------------------
// Compose Project Discovery
//------------------------------------------------------------------------------

/// Docker compose ls output format
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ComposeListItem {
    name: String,
    status: String,
    config_files: String,
}

/// Convert docker compose status string to app status constant
///
/// Maps Docker status strings like "running(2)", "exited(1)", "created(1)"
/// to application status constants.
pub fn status_convert(status: &str) -> i32 {
    let status_lower = status.to_lowercase();

    if status_lower.starts_with("created") {
        CREATED_STACK
    } else if status_lower.contains("exited") {
        EXITED
    } else if status_lower.starts_with("running") {
        RUNNING
    } else {
        UNKNOWN
    }
}

/// List all compose projects known to Docker
///
/// Runs `docker compose ls --all --format json` to get all compose projects,
/// including stopped and created stacks.
///
/// Returns HashMap of (project_name, (status, config_files))
pub async fn list_compose_projects() -> Result<HashMap<String, (i32, String)>> {
    let mut project_map = HashMap::new();

    let output = Command::new("docker")
        .args(["compose", "ls", "--all", "--format", "json"])
        .output()
        .await
        .context("Failed to run docker compose ls")?;

    if !output.status.success() {
        return Ok(project_map); // Return empty on failure
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    match serde_json::from_str::<Vec<ComposeListItem>>(&stdout) {
        Ok(compose_list) => {
            for item in compose_list {
                let status = status_convert(&item.status);
                project_map.insert(item.name, (status, item.config_files));
            }
        }
        Err(e) => {
            tracing::warn!("Failed to parse docker compose ls output: {}", e);
        }
    }

    Ok(project_map)
}
