//! Docker API client integration using Bollard
//!
//! This module provides a thin wrapper around the bollard Docker API client,
//! integrating it with the application's error handling (anyhow) and providing
//! helper functions for common operations.
//!
//! ## Architecture
//! - Docker client initialized in ServerContext (shared via Arc)
//! - Error conversion via BollardResultExt trait
//! - Compose project filtering via labels
//!
//! ## Operations
//! - Network listing and inspection
//! - Container listing by Compose project
//! - Status mapping from API to application types
//!
//! ## Hybrid Approach
//! This module handles information-gathering operations. Docker Compose
//! orchestration (up/down/restart) remains CLI-based via Terminal::exec(),
//! as Compose is not part of the Docker daemon API.

use anyhow::{Context, Result};
use bollard::container::ListContainersOptions;
use bollard::errors::Error as BollardError;
use bollard::models::ContainerSummary;
use bollard::network::ListNetworksOptions;
use bollard::Docker;
use std::collections::HashMap;

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
