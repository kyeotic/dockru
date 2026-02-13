// Server-to-client broadcast helpers (Phase 10)

use crate::db::models::Setting;
use crate::server::ServerContext;
use anyhow::Result;
use socketioxide::extract::SocketRef;
use tracing::debug;

/// Send server info to a specific socket
///
/// Emits: { version, latestVersion, primaryHostname }
pub async fn send_info(socket: &SocketRef, ctx: &ServerContext, hide_version: bool) -> Result<()> {
    let version = if hide_version {
        None
    } else {
        Some(ctx.version_checker.version().to_string())
    };

    let latest_version = if hide_version {
        None
    } else {
        ctx.version_checker.latest_version().await
    };

    let primary_hostname = Setting::get(&ctx.db, &ctx.cache, "primaryHostname")
        .await?
        .and_then(|v| v.as_str().map(|s| s.to_string()));

    let info = serde_json::json!({
        "version": version,
        "latestVersion": latest_version,
        "primaryHostname": primary_hostname,
    });

    socket.emit("info", info).ok();

    debug!("Sent info to socket {}", socket.id);

    Ok(())
}
