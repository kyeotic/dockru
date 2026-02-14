use crate::check_version::VersionChecker;
use crate::config::Config;
use crate::db::models::setting::SettingsCache;
use crate::db::Database;
use crate::static_files::PreCompressedStaticFiles;
use anyhow::{Context, Result};
use axum::{
    body::Body,
    extract::Request,
    http::{header::CONTENT_TYPE, StatusCode, Uri},
    response::{Html, IntoResponse, Response},
    routing::get,
    Router,
};
use socketioxide::{extract::SocketRef, SocketIo, TransportType};
use sqlx::SqlitePool;
use std::{fs, path::PathBuf, sync::Arc};
use tokio::signal;
use tower::ServiceBuilder;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::{debug, error, info, warn};

/// Shared server context bundling dependencies
#[derive(Clone)]
pub struct ServerContext {
    pub config: Arc<Config>,
    pub io: SocketIo,
    pub db: SqlitePool,
    pub cache: SettingsCache,
    pub version_checker: VersionChecker,
    /// Notifies the broadcast loop to fire immediately (e.g. on first client connect)
    pub broadcast_notify: Arc<tokio::sync::Notify>,
    /// Secret used to encrypt/decrypt agent passwords at rest.
    /// Derived from the jwtSecret setting; empty until setup is complete.
    pub encryption_secret: Arc<std::sync::RwLock<String>>,
}

impl ServerContext {
    pub fn new(
        config: Arc<Config>,
        io: SocketIo,
        db: SqlitePool,
        cache: SettingsCache,
        version_checker: VersionChecker,
    ) -> Self {
        Self {
            config,
            io,
            db,
            cache,
            version_checker,
            broadcast_notify: Arc::new(tokio::sync::Notify::new()),
            encryption_secret: Arc::new(std::sync::RwLock::new(String::new())),
        }
    }

    /// Get the encryption secret. Returns empty string if not yet initialized
    /// (i.e. before first user setup).
    pub fn get_encryption_secret(&self) -> String {
        self.encryption_secret.read().unwrap().clone()
    }

    /// Set the encryption secret (called at startup and after initial setup).
    pub fn set_encryption_secret(&self, secret: String) {
        let mut w = self.encryption_secret.write().unwrap();
        *w = secret;
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

        // Robots.txt route
        router = router.route(
            "/robots.txt",
            get(|| async {
                let txt = "User-agent: *\nDisallow: /";
                Response::builder()
                    .status(StatusCode::OK)
                    .header(CONTENT_TYPE, "text/plain")
                    .body(Body::from(txt))
                    .unwrap()
            }),
        );

        // Serve static files from frontend-dist with pre-compressed support
        // Use fallback_service instead of routes to allow socket.io layer to intercept first
        if PathBuf::from("./frontend-dist").exists() {
            let static_files = Arc::new(PreCompressedStaticFiles::new("./frontend-dist"));
            let index_html = self.index_html.clone();

            // Use fallback for SPA - handler for all unmatched routes
            router = router.fallback(move |uri: Uri, req: Request| {
                let static_files = static_files.clone();
                let index_html = index_html.clone();
                async move {
                    // Try to serve the file first
                    let response = static_files.handle(uri.clone(), req).await;

                    // If 404, serve index.html for SPA routing
                    if response.status() == StatusCode::NOT_FOUND {
                        if let Some(html) = index_html {
                            return Html(html).into_response();
                        }
                    }

                    response
                }
            });
        } else if let Some(ref html) = self.index_html {
            // Fallback: serve index.html only (development mode)
            let html_clone = html.clone();
            router = router.route("/", get(|| async move { Html(html_clone.clone()) }));

            // Fallback for all other routes in dev mode
            let html_clone = html.clone();
            router = router.fallback(move || {
                let html = html_clone.clone();
                async move { Html(html) }
            });
        }

        // Add middleware - layers are applied in reverse order (last = innermost)
        // Socket.io layer must be innermost to handle /socket.io/* paths
        let router = if cfg!(debug_assertions) {
            info!("Development mode: CORS enabled for all origins");
            router.layer(
                ServiceBuilder::new()
                    .layer(TraceLayer::new_for_http())
                    .layer(CorsLayer::permissive())
                    .layer(socket_layer),
            )
        } else {
            router.layer(
                ServiceBuilder::new()
                    .layer(TraceLayer::new_for_http())
                    .layer(socket_layer),
            )
        };

        router
    }

    /// Create the Socket.IO layer with transport configuration
    fn create_socketio_layer(&self) -> (SocketIo, socketioxide::layer::SocketIoLayer) {
        let (socket_layer, io) = SocketIo::builder()
            .transports([TransportType::Websocket])
            .build_layer();

        info!("Socket.IO configured with WebSocket-only transport");

        (io, socket_layer)
    }

    /// Set up Socket.IO namespace handlers (must be called after ServerContext is created)
    fn setup_socketio_handlers(io: &SocketIo, ctx: Arc<ServerContext>) {
        io.ns("/", move |socket: SocketRef| {
            info!("Socket connected: {} (transport: websocket)", socket.id);

            // Initialize socket state
            use crate::socket_handlers::{set_socket_state, SocketState};
            set_socket_state(&socket.id.to_string(), SocketState::default());

            // Create AgentManager for this socket
            let agent_manager = std::sync::Arc::new(crate::agent_manager::AgentManager::new(
                socket.clone(),
                ctx.db.clone(),
                ctx.get_encryption_secret(),
            ));
            let socket_id = socket.id.to_string();
            let agent_manager_clone = agent_manager.clone();
            tokio::spawn(async move {
                crate::agent_manager::set_agent_manager(&socket_id, agent_manager_clone).await;
            });

            // Notify the broadcast loop so the new client gets a stack list immediately
            ctx.broadcast_notify.notify_one();

            // Send server info
            let ctx_for_info = ctx.clone();
            let socket_for_info = socket.clone();
            tokio::spawn(async move {
                if let Err(e) =
                    crate::broadcasts::send_info(&socket_for_info, &ctx_for_info, true).await
                {
                    warn!("Failed to send info: {}", e);
                }

                // Check if setup is needed and emit "setup" event
                let user_count = crate::db::models::User::count(&ctx_for_info.db)
                    .await
                    .unwrap_or(1);
                if user_count == 0 {
                    info!("No users found, emitting 'setup' to redirect client");
                    match socket_for_info.emit("setup", ()) {
                        Ok(_) => info!(
                            "'setup' event emitted successfully to {}",
                            socket_for_info.id
                        ),
                        Err(e) => warn!("Failed to emit 'setup' event: {:?}", e),
                    }
                }
            });

            // Setup disconnect handler - single handler for all cleanup
            let socket_id_for_disconnect = socket.id.to_string();
            socket.on_disconnect(move || {
                let socket_id = socket_id_for_disconnect.clone();
                async move {
                    info!("Socket disconnected: {}", socket_id);

                    // Clean up agent manager
                    if let Some(manager) = crate::agent_manager::get_agent_manager(&socket_id).await
                    {
                        manager.disconnect_all().await;
                    }
                    crate::agent_manager::remove_agent_manager(&socket_id).await;

                    // Clean up socket state
                    use crate::socket_handlers::remove_socket_state;
                    remove_socket_state(&socket_id);
                }
            });

            // Setup all event handlers
            crate::socket_handlers::setup_all_handlers(socket.clone(), ctx.clone());
        });
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

    // Create settings cache
    let cache = SettingsCache::new();

    // Create version checker
    let version_checker = VersionChecker::new(env!("CARGO_PKG_VERSION").to_string());

    // Create Socket.IO layer first (with transport config)
    let (io, socket_layer) = server.create_socketio_layer();

    // Create server context with the real SocketIo instance
    let ctx = Arc::new(ServerContext::new(
        server.config.clone(),
        io.clone(),
        db.pool().clone(),
        cache,
        version_checker,
    ));

    // Initialize encryption secret from jwtSecret setting (if app has been set up)
    {
        let jwt_secret: Option<(String,)> =
            sqlx::query_as("SELECT value FROM setting WHERE key = 'jwtSecret'")
                .fetch_optional(db.pool())
                .await?;

        if let Some((secret,)) = jwt_secret {
            ctx.set_encryption_secret(secret.clone());

            // Migrate any existing plaintext agent passwords to encrypted form
            use crate::db::models::agent::Agent;
            match Agent::migrate_plaintext_passwords(db.pool(), &secret).await {
                Ok(0) => {}
                Ok(n) => info!("Migrated {} agent password(s) to encrypted storage", n),
                Err(e) => error!("Failed to migrate agent passwords: {}", e),
            }
        }
    }

    // Now set up namespace handlers with the real context
    DockruServer::setup_socketio_handlers(&io, ctx.clone());

    // Build router
    let app = server.build_router(socket_layer);

    // Get bind address
    let bind_addr = server.config.bind_address();

    info!("Server Type: HTTP");
    info!("Listening on {}", bind_addr);

    // Create listener
    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .with_context(|| format!("Failed to bind to {}", bind_addr))?;

    // Phase 10: Start scheduled tasks
    start_scheduled_tasks(ctx.clone());

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

/// Start all scheduled tasks (Phase 10)
fn start_scheduled_tasks(ctx: Arc<ServerContext>) {
    info!("Starting scheduled tasks");

    // Start version checking (every 48 hours)
    let ctx_clone = ctx.clone();
    tokio::spawn(async move {
        ctx_clone
            .version_checker
            .start_interval(ctx_clone.db.clone(), ctx_clone.cache.clone());
    });

    // Start stack list broadcast (every 10 seconds, only when clients are connected)
    // Also fires immediately when a client connects via broadcast_notify.
    let ctx_clone = ctx.clone();
    tokio::spawn(async move {
        use tokio::time::{interval, Duration};
        let mut interval = interval(Duration::from_secs(10));

        loop {
            // Wait for either the 10s tick or a client-connect notification
            tokio::select! {
                _ = interval.tick() => {},
                _ = ctx_clone.broadcast_notify.notified() => {
                    // Reset the interval so we don't double-fire shortly after
                    interval.reset();
                },
            }

            // Skip expensive Docker polling when no clients are connected
            let has_clients = ctx_clone
                .io
                .sockets()
                .map(|s| !s.is_empty())
                .unwrap_or(false);
            if !has_clients {
                debug!("No connected clients, skipping stack list broadcast");
                continue;
            }

            if let Err(e) = broadcast_stack_list_to_authenticated(&ctx_clone).await {
                error!("Failed to broadcast stack list: {}", e);
            }
        }
    });

    info!("All scheduled tasks started");
}

/// Broadcast stack list to all authenticated sockets
async fn broadcast_stack_list_to_authenticated(ctx: &ServerContext) -> Result<()> {
    use crate::stack::Stack;
    use std::collections::HashMap;

    // Get the stack list (empty endpoint for local)
    let ctx_arc = Arc::new(ctx.clone());
    let stack_list = Stack::get_stack_list(ctx_arc, String::new(), false).await?;

    // Convert stack_list to JSON format
    let mut map: HashMap<String, serde_json::Value> = HashMap::new();
    for (name, stack) in stack_list {
        // to_simple_json returns StackSimpleJson directly
        let simple_json = stack.to_simple_json().await;
        // Convert to serde_json::Value
        let json = serde_json::to_value(simple_json)?;
        map.insert(name, json);
    }

    let response = serde_json::json!({
        "ok": true,
        "stackList": map,
    });

    // Broadcast to all connected sockets wrapped in "agent" protocol
    // The frontend listens for socket.on("agent", (eventName, ...args) => ...)
    ctx.io.emit("agent", ("stackList", &response)).ok();

    Ok(())
}
