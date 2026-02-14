use crate::db::models::{Setting, SettingsCache, User};
use crate::server::ServerContext;
use crate::socket_handlers::{callback_error, callback_ok, check_login, emit_agent};
use anyhow::{anyhow, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use socketioxide::extract::{AckSender, Data, SocketRef};
use std::sync::Arc;
use tokio::fs;
use tracing::debug;

#[derive(Debug, Deserialize)]
struct SetSettingsData {
    #[serde(flatten)]
    settings: serde_json::Map<String, Value>,
    #[serde(rename = "globalENV")]
    global_env: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ComposerizeData {
    command: String,
}

/// Setup settings event handlers
pub fn setup_settings_handlers(socket: SocketRef, ctx: Arc<ServerContext>) {
    let ctx_clone = ctx.clone();
    socket.on("getSettings", move |socket: SocketRef, ack: AckSender| {
        let ctx = ctx_clone.clone();
        tokio::spawn(async move {
            match handle_get_settings(&socket, &ctx).await {
                Ok(response) => {
                    ack.send(&response).ok();
                }
                Err(e) => callback_error(Some(ack), e),
            };
        });
    });

    let ctx_clone = ctx.clone();
    socket.on(
        "setSettings",
        move |socket: SocketRef, Data::<serde_json::Value>(data), ack: AckSender| {
            let ctx = ctx_clone.clone();
            tokio::spawn(async move {
                match parse_set_settings_args(&data) {
                    Ok((settings_data, current_password)) => {
                        if let Err(e) =
                            handle_set_settings(&socket, &ctx, settings_data, current_password)
                                .await
                        {
                            callback_error(Some(ack), e);
                        } else {
                            callback_ok(Some(ack), "Saved", false);

                            // Re-send info after settings change
                            if let Err(e) = send_info_after_settings(&socket, &ctx).await {
                                debug!("Failed to send info: {}", e);
                            }
                        }
                    }
                    Err(e) => callback_error(Some(ack), e),
                }
            });
        },
    );

    let ctx_clone = ctx.clone();
    socket.on(
        "composerize",
        move |socket: SocketRef, Data::<String>(docker_run_command), ack: AckSender| {
            let ctx = ctx_clone.clone();
            tokio::spawn(async move {
                match handle_composerize(&socket, &ctx, docker_run_command).await {
                    Ok(response) => {
                        ack.send(&response).ok();
                    }
                    Err(e) => callback_error(Some(ack), e),
                };
            });
        },
    );
}

/// Parse setSettings positional args: [settingsObj, currentPassword?]
fn parse_set_settings_args(data: &serde_json::Value) -> Result<(SetSettingsData, Option<String>)> {
    let args = data
        .as_array()
        .ok_or_else(|| anyhow!("Expected array of arguments"))?;
    if args.is_empty() {
        return Err(anyhow!("setSettings requires at least a settings argument"));
    }
    let settings_data: SetSettingsData = serde_json::from_value(args[0].clone())?;
    let current_password = args.get(1).and_then(|v| v.as_str()).map(|s| s.to_string());
    Ok((settings_data, current_password))
}

async fn handle_get_settings(socket: &SocketRef, ctx: &ServerContext) -> Result<serde_json::Value> {
    check_login(socket)?;

    // Get all general settings
    let settings = Setting::get_settings(&ctx.db, "general").await?;

    // Read global.env if it exists
    let global_env_path = ctx.config.stacks_dir.join("global.env");
    let global_env = if global_env_path.exists() {
        fs::read_to_string(&global_env_path).await?
    } else {
        "# VARIABLE=value #comment".to_string()
    };

    let mut data = settings;
    data.insert("globalENV".to_string(), json!(global_env));

    Ok(json!({
        "ok": true,
        "data": data
    }))
}

async fn handle_set_settings(
    socket: &SocketRef,
    ctx: &ServerContext,
    data: SetSettingsData,
    current_password: Option<String>,
) -> Result<()> {
    let user_id = check_login(socket)?;
    debug!("User {} updating settings", user_id);

    // Handle global.env
    let global_env_path = ctx.config.stacks_dir.join("global.env");

    if let Some(global_env) = &data.global_env {
        if global_env != "# VARIABLE=value #comment" && !global_env.is_empty() {
            // Write global.env
            fs::write(&global_env_path, global_env).await?;
        } else {
            // Delete global.env if it's the default/empty
            if global_env_path.exists() {
                fs::remove_file(&global_env_path).await.ok();
            }
        }
    }

    // Save settings (excluding globalENV)
    let mut settings_to_save = data.settings;
    settings_to_save.remove("globalENV");

    let cache = SettingsCache::default();

    // Check for disableAuth change - require current password when enabling disableAuth
    if let Some(new_disable_auth) = settings_to_save.get("disableAuth") {
        let wants_disable = new_disable_auth.as_bool().unwrap_or(false)
            || new_disable_auth
                .as_str()
                .map(|s| s == "true")
                .unwrap_or(false);

        if wants_disable {
            // Check current setting value
            let current_value = Setting::get(&ctx.db, &cache, "disableAuth").await?;
            let currently_disabled = current_value
                .as_ref()
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
                || current_value
                    .as_ref()
                    .and_then(|v| v.as_str())
                    .map(|s| s == "true")
                    .unwrap_or(false);

            if !currently_disabled {
                // Changing from auth enabled to auth disabled - require password
                let password = current_password
                    .as_deref()
                    .filter(|p| !p.is_empty())
                    .ok_or_else(|| {
                        anyhow!("Current password is required to disable authentication")
                    })?;

                let user = User::find_by_id(&ctx.db, user_id)
                    .await?
                    .ok_or_else(|| anyhow!("User not found"))?;

                if !user.verify_password(password)? {
                    return Err(anyhow!("Incorrect password"));
                }
            }
        }
    }

    for (key, value) in settings_to_save {
        Setting::set(&ctx.db, &cache, &key, &value, Some("general")).await?;
    }

    Ok(())
}

async fn handle_composerize(
    _socket: &SocketRef,
    _ctx: &ServerContext,
    _docker_run_command: String,
) -> Result<serde_json::Value> {
    // TODO Phase 7: Implement composerize
    // Options:
    // 1. Shell out to Node.js composerize package
    // 2. Port the composerize logic to Rust
    // 3. Use an external service

    Err(anyhow!("composerize is not yet implemented"))
}

/// Send updated info after settings change
async fn send_info_after_settings(socket: &SocketRef, ctx: &ServerContext) -> Result<()> {
    let cache = SettingsCache::default();
    let primary_hostname_value = Setting::get(&ctx.db, &cache, "primaryHostname").await?;
    let primary_hostname = primary_hostname_value.and_then(|v| v.as_str().map(|s| s.to_string()));

    emit_agent(
        socket,
        "info",
        json!({
            "version": env!("CARGO_PKG_VERSION"),
            "latestVersion": null,
            "isContainer": std::env::var("DOCKRU_IS_CONTAINER").unwrap_or_default() == "1",
            "primaryHostname": primary_hostname,
        }),
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_settings_deserialize() {
        let json = r#"{
            "primaryHostname": "localhost",
            "globalENV": "FOO=bar\n",
            "disableAuth": false
        }"#;
        let data: SetSettingsData = serde_json::from_str(json).unwrap();
        assert_eq!(data.settings.get("primaryHostname").unwrap(), "localhost");
        assert_eq!(data.global_env.as_ref().unwrap(), "FOO=bar\n");
    }
}
