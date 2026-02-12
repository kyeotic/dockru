use crate::config::Config;
use crate::db::Database;
use anyhow::{Context, Result};
use axum::{response::Html, routing::get, Router};
use socketioxide::{
    extract::{Data, SocketRef},
    SocketIo,
};
use sqlx::SqlitePool;
use std::{fs, path::PathBuf, sync::Arc};
use tokio::signal;
use tower::ServiceBuilder;
use tower_http::{compression::CompressionLayer, services::ServeDir, trace::TraceLayer};
use tracing::{error, info, warn};

/// Shared server context bundling dependencies
#[derive(Clone)]
pub struct ServerContext {
    pub config: Arc<Config>,
    pub io: SocketIo,
    pub db: SqlitePool,
}

impl ServerContext {
    pub fn new(config: Arc<Config>, io: SocketIo, db: SqlitePool) -> Self {
        Self { config, io, db }
    }
}

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
    fn setup_socketio(&self, ctx: Arc<ServerContext>) -> (SocketIo, socketioxide::layer::SocketIoLayer) {
        let (socket_layer, io) = SocketIo::new_layer();

        io.ns("/", move |socket: SocketRef| {
            info!("Socket connected: {}", socket.id);

            // Initialize socket state
            use crate::socket_handlers::{set_socket_state, SocketState};
            set_socket_state(&socket.id.to_string(), SocketState::default());

            // Setup all event handlers
            crate::socket_handlers::setup_all_handlers(socket.clone(), ctx.clone());
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

    // Initialize database
    let db = Database::new(&server.config.data_dir).await?;

    // Run migrations
    db.migrate().await?;

    // Create server context (before socket setup so we can pass it in)
    let ctx = Arc::new(ServerContext::new(
        server.config.clone(),
        SocketIo::new_layer().1, // Temporary io, will be replaced
        db.pool().clone(),
    ));

    // Setup Socket.io with context
    let (io, socket_layer) = server.setup_socketio(ctx.clone());

    // Update context with the correct io instance
    let ctx = Arc::new(ServerContext::new(
        server.config.clone(),
        io,
        db.pool().clone(),
    ));

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
