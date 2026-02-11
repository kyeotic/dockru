use crate::config::Config;
use anyhow::{Context, Result};
use axum::{response::Html, routing::get, Router};
use socketioxide::{
    extract::{Data, SocketRef},
    SocketIo,
};
use std::{fs, path::PathBuf, sync::Arc};
use tokio::signal;
use tower::ServiceBuilder;
use tower_http::{compression::CompressionLayer, services::ServeDir, trace::TraceLayer};
use tracing::{error, info, warn};

/// Main server structure
pub struct DockruServer {
    config: Arc<Config>,
    index_html: Option<String>,
}

impl DockruServer {
    pub fn new(config: Config) -> Result<Self> {
        // Try to load index.html
        let index_html = match fs::read_to_string("./frontend-dist/index.html") {
            Ok(content) => Some(content),
            Err(e) => {
                // In development mode, it's okay if frontend-dist doesn't exist
                if cfg!(debug_assertions) {
                    warn!(
                        "frontend-dist/index.html not found (OK in development): {}",
                        e
                    );
                    None
                } else {
                    error!(
                        "Error: Cannot find 'frontend-dist/index.html', did you install correctly?"
                    );
                    return Err(anyhow::anyhow!("frontend-dist/index.html not found"));
                }
            }
        };

        Ok(Self {
            config: Arc::new(config),
            index_html,
        })
    }

    /// Build the router with all routes and middleware
    fn build_router(&self, socket_layer: socketioxide::layer::SocketIoLayer) -> Router {
        let mut router = Router::new();

        // Serve static files from frontend-dist
        if PathBuf::from("./frontend-dist").exists() {
            let serve_dir = ServeDir::new("./frontend-dist").append_index_html_on_directories(true);

            router = router.nest_service("/", serve_dir);
        } else if let Some(ref html) = self.index_html {
            // Fallback: serve index.html only (development mode)
            let html_clone = html.clone();
            router = router.route("/", get(|| async move { Html(html_clone) }));
        }

        // Add middleware
        router = router.layer(
            ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())
                .layer(CompressionLayer::new())
                .layer(socket_layer),
        );

        router
    }

    /// Initialize Socket.io and set up event handlers
    fn setup_socketio(&self) -> (SocketIo, socketioxide::layer::SocketIoLayer) {
        let (socket_layer, io) = SocketIo::new_layer();

        io.ns("/", |socket: SocketRef| {
            info!("Socket connected: {}", socket.id);

            socket.on("info", |socket: SocketRef, Data::<()>(())| {
                info!("Info event received from socket {}", socket.id);
                // TODO: Send server info
                let _ = socket.emit("info", serde_json::json!({
                    "version": env!("CARGO_PKG_VERSION"),
                    "isContainer": std::env::var("DOCKGE_IS_CONTAINER").unwrap_or_default() == "1",
                }));
            });

            socket.on_disconnect(|socket: SocketRef| {
                info!("Socket disconnected: {}", socket.id);
            });
        });

        (io, socket_layer)
    }
}

/// Start the server
pub async fn serve(config: Config) -> Result<()> {
    let server = DockruServer::new(config)?;

    // Create data directory if it doesn't exist
    fs::create_dir_all(&server.config.data_dir).context("Failed to create data directory")?;

    // Create stacks directory if it doesn't exist
    fs::create_dir_all(&server.config.stacks_dir).context("Failed to create stacks directory")?;

    info!("Data directory: {}", server.config.data_dir.display());
    info!("Stacks directory: {}", server.config.stacks_dir.display());

    // Setup Socket.io
    let (_io, socket_layer) = server.setup_socketio();

    // Build router
    let app = server.build_router(socket_layer);

    // Get bind address
    let bind_addr = server.config.bind_address();

    // Create listener
    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .with_context(|| format!("Failed to bind to {}", bind_addr))?;

    if server.config.is_ssl_enabled() {
        info!("Server Type: HTTPS");
        // TODO: Implement HTTPS support with rustls
        warn!("HTTPS support not yet implemented, falling back to HTTP");
    } else {
        info!("Server Type: HTTP");
    }

    info!("Listening on {}", bind_addr);

    // Start server with graceful shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("Server error")?;

    info!("Server shutdown complete");

    Ok(())
}

/// Wait for shutdown signal
async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            info!("Received Ctrl+C signal, shutting down...");
        },
        _ = terminate => {
            info!("Received termination signal, shutting down...");
        },
    }
}
