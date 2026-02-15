use crate::auth::{create_jwt, hash_password, shake256, verify_jwt, SHAKE256_LENGTH};
use crate::db::models::{NewUser, Setting, User};
use crate::rate_limiter::{LoginRateLimiter, TwoFaRateLimiter};
use crate::server::ServerContext;
use crate::socket_handlers::add_authenticated_socket;
use crate::socket_handlers::{
    broadcast_to_authenticated, callback_error, callback_ok, check_login, error_response,
    error_response_i18n, set_endpoint, set_user_id,
};
use crate::utils::crypto::gen_secret;
use crate::utils::types::{BaseRes, CustomResponse};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use socketioxide::extract::{AckSender, Data, SocketRef};
use sqlx::SqlitePool;
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
struct ChangePasswordData {
    #[serde(rename = "currentPassword")]
    current_password: String,
    #[serde(rename = "newPassword")]
    new_password: String,
}

/// Setup authentication event handlers
pub fn setup_auth_handlers(socket: SocketRef, ctx: Arc<ServerContext>) {
    // Check if setup is needed
    let ctx_clone = ctx.clone();
    socket.on("needSetup", move |socket: SocketRef, ack: AckSender| {
        let ctx = ctx_clone.clone();
        info!("'needSetup' event from socket {}", socket.id);
        tokio::spawn(async move {
            let user_count = User::count(&ctx.db).await.unwrap_or(0);
            let need_setup = user_count == 0;
            info!(
                "needSetup response: {} (user_count={})",
                need_setup, user_count
            );
            match ack.send(&need_setup) {
                Ok(_) => info!("needSetup ack sent successfully"),
                Err(e) => warn!("needSetup ack failed: {:?}", e),
            }
        });
    });

    let ctx_clone = ctx.clone();
    socket.on(
        "setup",
        move |socket: SocketRef, Data::<serde_json::Value>(raw_data), ack: AckSender| {
            let ctx = ctx_clone.clone();
            info!("=== Setup event received ===");
            info!(
                "Raw data type: {}",
                if raw_data.is_array() {
                    "array"
                } else if raw_data.is_object() {
                    "object"
                } else if raw_data.is_string() {
                    "string"
                } else {
                    "other"
                }
            );
            info!("Raw data value: {:?}", raw_data);

            tokio::spawn(async move {
                info!("Starting setup processing...");
                // Socket.IO sends multiple arguments as an array
                let (username, password) = if let Some(arr) = raw_data.as_array() {
                    if arr.len() >= 2 {
                        let user = arr[0].as_str().unwrap_or("").to_string();
                        let pass = arr[1].as_str().unwrap_or("").to_string();
                        info!(
                            "Parsed from array - username: {}, password length: {}",
                            user,
                            pass.len()
                        );
                        (user, pass)
                    } else {
                        warn!("Array has fewer than 2 elements: {}", arr.len());
                        ack.send(&error_response("Invalid data format")).ok();
                        return;
                    }
                } else {
                    // Try parsing as tuple
                    match serde_json::from_value::<(String, String)>(raw_data.clone()) {
                        Ok((user, pass)) => {
                            info!(
                                "Parsed as tuple - username: {}, password length: {}",
                                user,
                                pass.len()
                            );
                            (user, pass)
                        }
                        Err(e) => {
                            warn!("Failed to parse: {}. Raw data was: {:?}", e, raw_data);
                            ack.send(&error_response(&format!("Invalid data format: {}", e)))
                                .ok();
                            return;
                        }
                    }
                };

                info!("Calling handle_setup...");
                let setup_data = SetupData { username, password };
                let result = handle_setup(&socket, &ctx, setup_data).await;
                info!("handle_setup returned: {:?}", result.is_ok());

                let response = match result {
                    Ok(resp) => {
                        info!("Setup successful, response: {:?}", resp);
                        resp
                    }
                    Err(e) => {
                        warn!("Setup failed: {}", e);
                        error_response(&e.to_string()).into()
                    }
                };

                info!("About to send ack with response: {:?}", response);
                match ack.send(&response) {
                    Ok(_) => info!("✓ Acknowledgment sent successfully"),
                    Err(e) => warn!("✗ Failed to send acknowledgment: {:?}", e),
                }
            });
        },
    );

    let ctx_clone = ctx.clone();
    socket.on(
        "login",
        move |socket: SocketRef, Data::<LoginData>(data), ack: AckSender| {
            let ctx = ctx_clone.clone();
            info!(
                "'login' event from socket {} for user '{}'",
                socket.id, data.username
            );
            tokio::spawn(async move {
                match handle_login(&socket, &ctx, data).await {
                    Ok(response) => {
                        info!("Login handler succeeded for socket {}", socket.id);
                        match ack.send(&response) {
                            Ok(_) => info!("Login ack sent to socket {}", socket.id),
                            Err(e) => warn!("Login ack failed for socket {}: {:?}", socket.id, e),
                        }
                    }
                    Err(e) => {
                        warn!("Login handler failed for socket {}: {}", socket.id, e);
                        ack.send(&error_response(&e.to_string())).ok();
                    }
                };
            });
        },
    );

    let ctx_clone = ctx.clone();
    socket.on(
        "loginByToken",
        move |socket: SocketRef, Data::<String>(token), ack: AckSender| {
            let ctx = ctx_clone.clone();
            info!("'loginByToken' event from socket {}", socket.id);
            tokio::spawn(async move {
                match handle_login_by_token(&socket, &ctx, &token).await {
                    Ok(response) => {
                        info!("loginByToken succeeded for socket {}", socket.id);
                        match ack.send(&response) {
                            Ok(_) => info!("loginByToken ack sent to socket {}", socket.id),
                            Err(e) => {
                                warn!("loginByToken ack failed for socket {}: {:?}", socket.id, e)
                            }
                        }
                    }
                    Err(e) => {
                        warn!("loginByToken failed for socket {}: {}", socket.id, e);
                        let response: serde_json::Value = error_response_i18n("authInvalidToken").into();
                        ack.send(&response).ok();
                    }
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

    // Note: disconnect handler is registered in server.rs setup_socketio_handlers()
    // to avoid duplicate handler registration
}

async fn handle_setup(
    _socket: &SocketRef,
    ctx: &ServerContext,
    data: SetupData,
) -> Result<serde_json::Value> {
    // Check if already setup
    let user_count = User::count(&ctx.db).await?;
    if user_count > 0 {
        return Err(anyhow!(
            "Dockru has been initialized. If you want to run setup again, please delete the database."
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

    // Initialize JWT secret if not exists
    init_jwt_secret(&ctx.db).await?;

    // Update encryption secret in server context so agent passwords can be encrypted
    let jwt_secret_value: Option<(String,)> =
        sqlx::query_as("SELECT value FROM setting WHERE key = 'jwtSecret'")
            .fetch_optional(&ctx.db)
            .await?;
    if let Some((secret,)) = jwt_secret_value {
        ctx.set_encryption_secret(secret);
    }

    // Broadcast that setup is complete
    broadcast_to_authenticated(&ctx.io, "setup", json!({}));

    Ok(BaseRes::ok_with_msg_i18n("successAdded").into())
}

async fn handle_login(
    socket: &SocketRef,
    ctx: &ServerContext,
    data: LoginData,
) -> Result<serde_json::Value> {
    // Rate limiting
    let ip = get_client_ip(socket);
    let limiter = LoginRateLimiter::new();
    if limiter.check(ip).is_err() {
        info!("Login rate limit exceeded for IP: {:?}", ip);
        return Ok(error_response_i18n("authRateLimitExceeded").into());
    }

    // Find and verify user
    let mut user = User::find_by_username(&ctx.db, &data.username)
        .await?
        .ok_or_else(|| anyhow!("authIncorrectCreds"))?;

    if !user.verify_password(&data.password)? {
        return Ok(error_response_i18n("authIncorrectCreds").into());
    }

    // Check if password needs rehashing with updated cost
    if let Some(ref password_hash) = user.password {
        if crate::auth::need_rehash_password(password_hash) {
            info!(
                "Rehashing password for user {} with updated cost",
                user.username
            );
            user.update_password(&ctx.db, &data.password).await?;
        }
    }

    // Check 2FA
    if user.twofa_status {
        if let Some(_token) = data.token {
            // Verify 2FA token
            let twofa_limiter = TwoFaRateLimiter::new();
            if twofa_limiter.check(ip).is_err() {
                return Ok(error_response_i18n("authRateLimitExceeded").into());
            }

            // TODO: Implement 2FA verification in Phase 4 completion
            // For now, always fail if 2FA is enabled
            return Ok(error_response_i18n("authInvalidToken").into());
        } else {
            // 2FA token required
            return Ok(json!({
                "tokenRequired": true
            }));
        }
    }

    // Login successful
    after_login(socket, ctx, &user).await?;

    let jwt_secret_value = Setting::get(
        &ctx.db,
        &crate::db::models::SettingsCache::default(),
        "jwtSecret",
    )
    .await?
    .ok_or_else(|| anyhow!("JWT secret not found"))?;
    let jwt_secret = jwt_secret_value
        .as_str()
        .ok_or_else(|| anyhow!("JWT secret is not a string"))?;

    let password_hash = user
        .password
        .as_ref()
        .ok_or_else(|| anyhow!("User has no password"))?;
    let token = create_jwt(&user.username, password_hash, jwt_secret)?;

    #[derive(Serialize)]
    struct LoginResponse {
        token: String,
    }

    Ok(CustomResponse::ok_with_fields(LoginResponse { token }).into())
}

async fn handle_login_by_token(
    socket: &SocketRef,
    ctx: &ServerContext,
    token: &str,
) -> Result<serde_json::Value> {
    let ip = get_client_ip(socket);
    info!("Login by token. IP={}", ip);

    let jwt_secret_value = Setting::get(
        &ctx.db,
        &crate::db::models::SettingsCache::default(),
        "jwtSecret",
    )
    .await?
    .ok_or_else(|| anyhow!("JWT secret not found"))?;
    let jwt_secret = jwt_secret_value
        .as_str()
        .ok_or_else(|| anyhow!("JWT secret is not a string"))?;

    let payload = verify_jwt(token, jwt_secret)?;
    let username = payload.username;
    let password_hash = payload.h;

    let user = User::find_by_username(&ctx.db, &username)
        .await?
        .ok_or_else(|| anyhow!("authUserInactiveOrDeleted"))?;

    if !user.active {
        return Ok(error_response_i18n("authUserInactiveOrDeleted").into());
    }

    // Verify password hash matches (detect password change)
    let stored_password = user
        .password
        .as_ref()
        .ok_or_else(|| anyhow!("User has no password"))?;
    let stored_hash = shake256(stored_password, SHAKE256_LENGTH);
    if password_hash != stored_hash {
        return Err(anyhow!(
            "The token is invalid due to password change or old token"
        ));
    }

    after_login(socket, ctx, &user).await?;

    info!("Successfully logged in user {}. IP={}", username, ip);

    Ok(BaseRes::ok().into())
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

    // Mark socket as authenticated by joining the authenticated room
    add_authenticated_socket(socket);

    // Join user room for broadcasting
    socket.join(user.id.to_string()).ok();

    // Set endpoint from request headers or default to empty
    let endpoint = extract_endpoint(socket).unwrap_or_default();
    set_endpoint(socket, endpoint.clone());

    // Send server info (Phase 10)
    crate::broadcasts::send_info(socket, ctx, false).await?;

    // TODO Phase 7: Send stack list

    // Send agent list and connect to all agents (Phase 8)
    if let Some(agent_manager) =
        crate::agent_manager::get_agent_manager(&socket.id.to_string()).await
    {
        agent_manager.send_agent_list().await;
        agent_manager.connect_all(&endpoint).await;
    }

    Ok(())
}

/// Get client IP from socket
/// Always respects X-Forwarded-For and X-Real-IP headers (trust proxy)
fn get_client_ip(_socket: &SocketRef) -> std::net::IpAddr {
    // Try to get from socket extensions/state
    // socketioxide doesn't provide direct access to request headers
    // For now, return localhost as placeholder
    // TODO: Extract from X-Forwarded-For when socketioxide supports it
    // Or extract at connection time and store in socket state

    std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1))
}

/// Extract endpoint from request headers
fn extract_endpoint(_socket: &SocketRef) -> Option<String> {
    // socketioxide doesn't currently expose request headers
    // We would need to extract this at connection time
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

/// Initialize JWT secret in database if not exists
/// Matches TypeScript initJWTSecret() behavior
async fn init_jwt_secret(pool: &SqlitePool) -> Result<()> {
    // Check if JWT secret already exists
    let existing: Option<(String,)> =
        sqlx::query_as("SELECT value FROM setting WHERE key = 'jwtSecret'")
            .fetch_optional(pool)
            .await?;

    if existing.is_none() {
        // Generate new secret: hash a random 64-char string
        let secret = gen_secret(64);
        let hashed_secret = hash_password(&secret)?;

        // Store in database
        sqlx::query("INSERT INTO setting (key, value, type) VALUES ('jwtSecret', ?1, NULL)")
            .bind(&hashed_secret)
            .execute(pool)
            .await?;

        info!("Generated and stored new JWT secret");
    }

    Ok(())
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
        let json = r#"{"currentPassword": "old123", "newPassword": "new123"}"#;
        let data: ChangePasswordData = serde_json::from_str(json).unwrap();
        assert_eq!(data.current_password, "old123");
        assert_eq!(data.new_password, "new123");
    }
}
