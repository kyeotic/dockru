// Main entry point for Dockru Rust backend
mod agent_manager;
mod auth;
mod config;
mod db;
mod rate_limiter;
mod server;
mod socket_auth;
mod socket_handlers;
mod stack;
mod static_files;
mod terminal;
mod utils;

use anyhow::Result;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_target(true)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .init();

    info!("Welcome to dockru!");

    // Parse configuration
    let config = config::Config::parse()?;

    info!("Starting Dockru server...");
    info!("Port: {}", config.port);
    info!("Stacks directory: {}", config.stacks_dir.display());

    // Start the server
    server::serve(config).await?;

    Ok(())
}
