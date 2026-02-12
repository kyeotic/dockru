use crate::auth::{create_jwt, verify_jwt, shake256, SHAKE256_LENGTH};
use crate::db::models::{NewUser, Setting, User};
use crate::rate_limiter::{LoginRateLimiter, TwoFaRateLimiter};
use crate::server::ServerContext;
use crate::socket_handlers::{
    broadcast_to_authenticated, callback_error, callback_ok, check_login, error_response,
    error_response_i18n, remove_socket_state, set_endpoint, set_user_id,
};
use anyhow::{anyhow, Result};
use serde::Deserialize;
use serde_json::json;
use socketioxide::extract::{AckSender, Data, SocketRef};
use std::sync::Arc;
use tracing::{debug, info, warn};

#[derive(Debug, Deserialize)]
struct SetupData {
    username: String,
    password: String,
}

#[derive(Debug, Deserialize)]
struct LoginData {
    username: String,
    password: String,
    token: Option<String>, // 2FA token
}

#[derive(Debug, Deserialize)]
struct LoginByTokenData {
    token: String,
}

#[derive(Debug, Deserialize)]
struct ChangePasswordData {
    #[serde(rename = "currentPassword")]
    current_password: String,
    #[serde(rename = "newPassword")]
    new_password: String,
}

/// Setup authentication event handlers
pub fn setup_auth_handlers(socket: SocketRef, ctx: Arc<ServerContext>) {
    let ctx_clone = ctx.clone();
    socket.on("setup", move |socket: SocketRef, Data::<SetupData>(data), ack: AckSender| {
        let ctx = ctx_clone.clone();
        tokio::spawn(async move {
            match handle_setup(&socket, &ctx, data).await {
                Ok(response) => ack.send(&response).ok(),
                Err(e) => ack.send(&error_response(&e.to_string())).ok(),
            };
        });
    });

    let ctx_clone = ctx.clone();
    socket.on("login", move |socket: SocketRef, Data::<LoginData>(data), ack: AckSender| {
        let ctx = ctx_clone.clone();
        tokio::spawn(async move {
            match handle_login(&socket, &ctx, data).await {
                Ok(response) => ack.send(&response).ok(),
                Err(e) => ack.send(&error_response(&e.to_string())).ok(),
            };
        });
    });

    let ctx_clone = ctx.clone();
    socket.on(
        "loginByToken",
        move |socket: SocketRef, Data::<LoginByTokenData>(data), ack: AckSender| {
            let ctx = ctx_clone.clone();
            tokio::spawn(async move {
                match handle_login_by_token(&socket, &ctx, data).await {
                    Ok(response) => ack.send(&response).ok(),
                    Err(e) => ack.send(&error_response_i18n("authInvalidToken")).ok(),
                };
            });
        },
    );

    let ctx_clone = ctx.clone();
    socket.on(
        "changePassword",
        move |socket: SocketRef, Data::<ChangePasswordData>(data), ack: AckSender| {
            let ctx = ctx_clone.clone();
            tokio::spawn(async move {
                if let Err(e) = handle_change_password(&socket, &ctx, data).await {
                    callback_error(Some(ack), e);
                } else {
                    callback_ok(Some(ack), "Password has been updated successfully.", false);
                }
            });
        },
    );

    let ctx_clone = ctx.clone();
    socket.on("disconnectOtherSocketClients", move |socket: SocketRef| {
        let ctx = ctx_clone.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_disconnect_others(&socket, &ctx).await {
                warn!("disconnectOtherSocketClients error: {}", e);
            }
        });
    });

    // Handle disconnect - clean up socket state
    socket.on_disconnect(move |socket: SocketRef| {
        debug!("Socket disconnected: {}", socket.id);
        remove_socket_state(&socket.id.to_string());
    });
}

async fn handle_setup(
    socket: &SocketRef,
    ctx: &ServerContext,
    data: SetupData,
) -> Result<serde_json::Value> {
    // Check if already setup
    let user_count = User::count(&ctx.db).await?;
    if user_count > 0 {
        return Err(anyhow!(
            "Dockge has been initialized. If you want to run setup again, please delete the database."
        ));
    }

    // Validate password strength (basic check)
    if data.password.len() < 6 {
        return Err(anyhow!(
            "Password is too weak. It should contain alphabetic and numeric characters. It must be at least 6 characters in length."
        ));
    }

    // Create user
    let new_user = NewUser {
        username: data.username.clone(),
        password: Some(data.password.clone()),
        active: true,
        timezone: None,
    };
    User::create(&ctx.db, new_user).await?;

    // Broadcast that setup is complete
    broadcast_to_authenticated(&ctx.io, "setup", json!({}));

    Ok(json!({
        "ok": true,
        "msg": "successAdded",
        "msgi18n": true
    }))
}

async fn handle_login(
    socket: &SocketRef,
    ctx: &ServerContext,
    data: LoginData,
) -> Result<serde_json::Value> {
    // Rate limiting
    let ip = get_client_ip(socket);
    let limiter = LoginRateLimiter::new();
    if let Err(_) = limiter.check(ip) {
        info!("Login rate limit exceeded for IP: {:?}", ip);
        return Ok(error_response_i18n("authRateLimitExceeded"));
    }

    // Find and verify user
    let user = User::find_by_username(&ctx.db, &data.username)
        .await?
        .ok_or_else(|| anyhow!("authIncorrectCreds"))?;

    if !user.verify_password(&data.password)? {
        return Ok(error_response_i18n("authIncorrectCreds"));
    }

    // Check 2FA
    if user.twofa_status {
        if let Some(token) = data.token {
            // Verify 2FA token
            let twofa_limiter = TwoFaRateLimiter::new();
            if let Err(_) = twofa_limiter.check(ip) {
                return Ok(error_response_i18n("authRateLimitExceeded"));
            }

            // TODO: Implement 2FA verification in Phase 4 completion
            // For now, always fail if 2FA is enabled
            return Ok(error_response_i18n("authInvalidToken"));
        } else {
            // 2FA token required
            return Ok(json!({
                "tokenRequired": true
            }));
        }
    }

    // Login successful
    after_login(socket, ctx, &user).await?;

    let jwt_secret_value = Setting::get(&ctx.db, &crate::db::models::SettingsCache::default(), "jwtSecret")
        .await?
        .ok_or_else(|| anyhow!("JWT secret not found"))?;
    let jwt_secret = jwt_secret_value.as_str()
        .ok_or_else(|| anyhow!("JWT secret is not a string"))?;
    
    let password_hash = user.password.as_ref()
        .ok_or_else(|| anyhow!("User has no password"))?;
    let token = create_jwt(&user.username, password_hash, jwt_secret)?;

    Ok(json!({
        "ok": true,
        "token": token
    }))
}

async fn handle_login_by_token(
    socket: &SocketRef,
    ctx: &ServerContext,
    data: LoginByTokenData,
) -> Result<serde_json::Value> {
    let ip = get_client_ip(socket);
    info!("Login by token. IP={}", ip);

    let jwt_secret_value = Setting::get(&ctx.db, &crate::db::models::SettingsCache::default(), "jwtSecret")
        .await?
        .ok_or_else(|| anyhow!("JWT secret not found"))?;
    let jwt_secret = jwt_secret_value.as_str()
        .ok_or_else(|| anyhow!("JWT secret is not a string"))?;

    let payload = verify_jwt(&data.token, jwt_secret)?;
    let username = payload.username;
    let password_hash = payload.h;

    let user = User::find_by_username(&ctx.db, &username)
        .await?
        .ok_or_else(|| anyhow!("authUserInactiveOrDeleted"))?;

    if !user.active {
        return Ok(error_response_i18n("authUserInactiveOrDeleted"));
    }

    // Verify password hash matches (detect password change)
    let stored_password = user.password.as_ref()
        .ok_or_else(|| anyhow!("User has no password"))?;
    let stored_hash = shake256(stored_password, SHAKE256_LENGTH);
    if password_hash != stored_hash {
        return Err(anyhow!("The token is invalid due to password change or old token"));
    }

    after_login(socket, ctx, &user).await?;

    info!("Successfully logged in user {}. IP={}", username, ip);

    Ok(json!({
        "ok": true
    }))
}

async fn handle_change_password(
    socket: &SocketRef,
    ctx: &ServerContext,
    data: ChangePasswordData,
) -> Result<()> {
    let user_id = check_login(socket)?;

    // Validate new password
    if data.new_password.len() < 6 {
        return Err(anyhow!(
            "Password is too weak. It should contain alphabetic and numeric characters. It must be at least 6 characters in length."
        ));
    }

    // Verify current password
    let user = User::find_by_id(&ctx.db, user_id)
        .await?
        .ok_or_else(|| anyhow!("User not found"))?;

    if !user.verify_password(&data.current_password)? {
        return Err(anyhow!("Incorrect current password"));
    }

    // Update password
    let mut user = User::find_by_id(&ctx.db, user_id)
        .await?
        .ok_or_else(|| anyhow!("User not found"))?;
    user.update_password(&ctx.db, &data.new_password).await?;

    // Disconnect all other sessions
    disconnect_all_other_sockets(ctx, user_id, &socket.id.to_string()).await;

    Ok(())
}

async fn handle_disconnect_others(socket: &SocketRef, ctx: &ServerContext) -> Result<()> {
    let user_id = check_login(socket)?;
    disconnect_all_other_sockets(ctx, user_id, &socket.id.to_string()).await;
    Ok(())
}

/// After successful login, set up socket state and send initial data
async fn after_login(socket: &SocketRef, ctx: &ServerContext, user: &User) -> Result<()> {
    // Set user ID in socket state
    set_user_id(socket, user.id);

    // Join user room for broadcasting
    socket.join(user.id.to_string()).ok();

    // Set endpoint from request headers or default to empty
    let endpoint = extract_endpoint(socket).unwrap_or_default();
    set_endpoint(socket, endpoint);

    // Send server info
    send_info(socket, ctx).await?;

    // TODO Phase 7: Send stack list
    // TODO Phase 8: Send agent list
    // TODO Phase 8: Connect to agents

    Ok(())
}

/// Send server info to socket
async fn send_info(socket: &SocketRef, ctx: &ServerContext) -> Result<()> {
    let primary_hostname_value = Setting::get(&ctx.db, &crate::db::models::SettingsCache::default(), "primaryHostname").await?;
    let primary_hostname = primary_hostname_value.and_then(|v| v.as_str().map(|s| s.to_string()));

    socket
        .emit(
            "info",
            json!({
                "version": env!("CARGO_PKG_VERSION"),
                "latestVersion": null, // TODO: Implement version checking
                "isContainer": std::env::var("DOCKGE_IS_CONTAINER").unwrap_or_default() == "1",
                "primaryHostname": primary_hostname,
            }),
        )
        .ok();

    Ok(())
}

/// Get client IP from socket
fn get_client_ip(_socket: &SocketRef) -> std::net::IpAddr {
    // TODO: Extract from X-Forwarded-For if trust proxy is enabled
    // For now, return localhost as placeholder
    std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1))
}

/// Extract endpoint from request headers
fn extract_endpoint(_socket: &SocketRef) -> Option<String> {
    // TODO: Extract from request headers
    // For now, default to empty (local endpoint)
    Some("".to_string())
}

/// Disconnect all sockets for a user except the current one
async fn disconnect_all_other_sockets(ctx: &ServerContext, user_id: i64, except_socket_id: &str) {
    // TODO Phase 7: Implement socket iteration and disconnection
    // For now, emit refresh to the user room
    ctx.io
        .to(user_id.to_string())
        .emit("refresh", json!({}))
        .ok();
    debug!(
        "Disconnected other sockets for user {} except {}",
        user_id, except_socket_id
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_setup_data_deserialize() {
        let json = r#"{"username": "admin", "password": "password123"}"#;
        let data: SetupData = serde_json::from_str(json).unwrap();
        assert_eq!(data.username, "admin");
        assert_eq!(data.password, "password123");
    }

    #[test]
    fn test_login_data_deserialize() {
        let json = r#"{"username": "admin", "password": "password123", "token": null}"#;
        let data: LoginData = serde_json::from_str(json).unwrap();
        assert_eq!(data.username, "admin");
        assert!(data.token.is_none());
    }

    #[test]
    fn test_change_password_data_deserialize() {
        let json =
            r#"{"currentPassword": "old123", "newPassword": "new123"}"#;
        let data: ChangePasswordData = serde_json::from_str(json).unwrap();
        assert_eq!(data.current_password, "old123");
        assert_eq!(data.new_password, "new123");
    }
}
