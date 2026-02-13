use crate::server::ServerContext;
use crate::socket_handlers::{callback_error, callback_ok, check_login, get_endpoint};
use crate::stack::{ServiceStatus, Stack};
use anyhow::{anyhow, Result};
use serde::Deserialize;
use serde_json::json;
use socketioxide::extract::{AckSender, Data, SocketRef};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::debug;

#[derive(Debug, Deserialize)]
struct DeployStackData {
    name: String,
    #[serde(rename = "composeYAML")]
    compose_yaml: String,
    #[serde(rename = "composeENV")]
    compose_env: String,
    #[serde(rename = "isAdd")]
    is_add: bool,
}

#[derive(Debug, Deserialize)]
struct SaveStackData {
    name: String,
    #[serde(rename = "composeYAML")]
    compose_yaml: String,
    #[serde(rename = "composeENV")]
    compose_env: String,
    #[serde(rename = "isAdd")]
    is_add: bool,
}

/// Setup stack management event handlers
pub fn setup_stack_handlers(socket: SocketRef, ctx: Arc<ServerContext>) {
    // deployStack
    let ctx_clone = ctx.clone();
    socket.on(
        "deployStack",
        move |socket: SocketRef, Data::<DeployStackData>(data), ack: AckSender| {
            let ctx = ctx_clone.clone();
            tokio::spawn(async move {
                match handle_deploy_stack(&socket, &ctx, data).await {
                    Ok(_) => {
                        callback_ok(Some(ack), "Deployed", true);
                        broadcast_stack_list(&ctx).await;
                    }
                    Err(e) => callback_error(Some(ack), e),
                }
            });
        },
    );

    // saveStack
    let ctx_clone = ctx.clone();
    socket.on(
        "saveStack",
        move |socket: SocketRef, Data::<SaveStackData>(data), ack: AckSender| {
            let ctx = ctx_clone.clone();
            tokio::spawn(async move {
                match handle_save_stack(&socket, &ctx, data).await {
                    Ok(_) => {
                        callback_ok(Some(ack), "Saved", true);
                        broadcast_stack_list(&ctx).await;
                    }
                    Err(e) => callback_error(Some(ack), e),
                }
            });
        },
    );

    // deleteStack
    let ctx_clone = ctx.clone();
    socket.on(
        "deleteStack",
        move |socket: SocketRef, Data::<String>(stack_name), ack: AckSender| {
            let ctx = ctx_clone.clone();
            tokio::spawn(async move {
                match handle_delete_stack(&socket, &ctx, &stack_name).await {
                    Ok(_) => {
                        callback_ok(Some(ack), "Deleted", true);
                        broadcast_stack_list(&ctx).await;
                    }
                    Err(e) => callback_error(Some(ack), e),
                }
            });
        },
    );

    // getStack
    let ctx_clone = ctx.clone();
    socket.on(
        "getStack",
        move |socket: SocketRef, Data::<String>(stack_name), ack: AckSender| {
            let ctx = ctx_clone.clone();
            tokio::spawn(async move {
                match handle_get_stack(&socket, &ctx, &stack_name).await {
                    Ok(response) => {
                        ack.send(&response).ok();
                    }
                    Err(e) => callback_error(Some(ack), e),
                };
            });
        },
    );

    // requestStackList
    let ctx_clone = ctx.clone();
    socket.on(
        "requestStackList",
        move |socket: SocketRef, ack: AckSender| {
            let ctx = ctx_clone.clone();
            tokio::spawn(async move {
                if check_login(&socket).is_ok() {
                    broadcast_stack_list(&ctx).await;
                    callback_ok(Some(ack), "Updated", true);
                }
            });
        },
    );

    // startStack
    let ctx_clone = ctx.clone();
    socket.on(
        "startStack",
        move |socket: SocketRef, Data::<String>(stack_name), ack: AckSender| {
            let ctx = ctx_clone.clone();
            tokio::spawn(async move {
                match handle_start_stack(&socket, &ctx, &stack_name).await {
                    Ok(_) => {
                        callback_ok(Some(ack), "Started", true);
                        broadcast_stack_list(&ctx).await;
                    }
                    Err(e) => callback_error(Some(ack), e),
                }
            });
        },
    );

    // stopStack
    let ctx_clone = ctx.clone();
    socket.on(
        "stopStack",
        move |socket: SocketRef, Data::<String>(stack_name), ack: AckSender| {
            let ctx = ctx_clone.clone();
            tokio::spawn(async move {
                match handle_stop_stack(&socket, &ctx, &stack_name).await {
                    Ok(_) => {
                        callback_ok(Some(ack), "Stopped", true);
                        broadcast_stack_list(&ctx).await;
                    }
                    Err(e) => callback_error(Some(ack), e),
                }
            });
        },
    );

    // restartStack
    let ctx_clone = ctx.clone();
    socket.on(
        "restartStack",
        move |socket: SocketRef, Data::<String>(stack_name), ack: AckSender| {
            let ctx = ctx_clone.clone();
            tokio::spawn(async move {
                match handle_restart_stack(&socket, &ctx, &stack_name).await {
                    Ok(_) => {
                        callback_ok(Some(ack), "Restarted", true);
                        broadcast_stack_list(&ctx).await;
                    }
                    Err(e) => callback_error(Some(ack), e),
                }
            });
        },
    );

    // updateStack
    let ctx_clone = ctx.clone();
    socket.on(
        "updateStack",
        move |socket: SocketRef, Data::<String>(stack_name), ack: AckSender| {
            let ctx = ctx_clone.clone();
            tokio::spawn(async move {
                match handle_update_stack(&socket, &ctx, &stack_name).await {
                    Ok(_) => {
                        callback_ok(Some(ack), "Updated", true);
                        broadcast_stack_list(&ctx).await;
                    }
                    Err(e) => callback_error(Some(ack), e),
                }
            });
        },
    );

    // downStack
    let ctx_clone = ctx.clone();
    socket.on(
        "downStack",
        move |socket: SocketRef, Data::<String>(stack_name), ack: AckSender| {
            let ctx = ctx_clone.clone();
            tokio::spawn(async move {
                match handle_down_stack(&socket, &ctx, &stack_name).await {
                    Ok(_) => {
                        callback_ok(Some(ack), "Downed", true);
                        broadcast_stack_list(&ctx).await;
                    }
                    Err(e) => callback_error(Some(ack), e),
                }
            });
        },
    );

    // serviceStatusList
    let ctx_clone = ctx.clone();
    socket.on(
        "serviceStatusList",
        move |socket: SocketRef, Data::<String>(stack_name), ack: AckSender| {
            let ctx = ctx_clone.clone();
            tokio::spawn(async move {
                match handle_service_status_list(&socket, &ctx, &stack_name).await {
                    Ok(response) => {
                        ack.send(&response).ok();
                    }
                    Err(e) => callback_error(Some(ack), e),
                };
            });
        },
    );

    // getDockerNetworkList
    let ctx_clone = ctx.clone();
    socket.on(
        "getDockerNetworkList",
        move |socket: SocketRef, ack: AckSender| {
            let ctx = ctx_clone.clone();
            tokio::spawn(async move {
                match handle_get_docker_network_list(&socket, &ctx).await {
                    Ok(response) => {
                        ack.send(&response).ok();
                    }
                    Err(e) => callback_error(Some(ack), e),
                };
            });
        },
    );
}

async fn handle_deploy_stack(
    socket: &SocketRef,
    ctx: &ServerContext,
    data: DeployStackData,
) -> Result<()> {
    check_login(socket)?;

    let endpoint = get_endpoint(socket);
    let mut stack = Stack::new_with_content(
        ctx.clone().into(),
        data.name.clone(),
        endpoint,
        data.compose_yaml,
        data.compose_env,
    );

    // Validate YAML is parseable
    stack.compose_yaml().await?;
    stack.save(data.is_add).await?;
    stack.deploy(Some(socket.clone())).await?;

    // Join combined terminal to see logs
    stack.join_combined_terminal(socket.clone()).await?;

    Ok(())
}

async fn handle_save_stack(
    socket: &SocketRef,
    ctx: &ServerContext,
    data: SaveStackData,
) -> Result<()> {
    check_login(socket)?;

    let endpoint = get_endpoint(socket);
    let mut stack = Stack::new_with_content(
        ctx.clone().into(),
        data.name,
        endpoint,
        data.compose_yaml,
        data.compose_env,
    );

    // Validate YAML is parseable
    stack.compose_yaml().await?;
    stack.save(data.is_add).await?;

    Ok(())
}

async fn handle_delete_stack(
    socket: &SocketRef,
    ctx: &ServerContext,
    stack_name: &str,
) -> Result<()> {
    check_login(socket)?;

    let endpoint = get_endpoint(socket);
    let stack = Stack::get_stack(ctx.clone().into(), stack_name, endpoint).await?;
    stack.delete(Some(socket.clone())).await?;

    Ok(())
}

async fn handle_get_stack(
    socket: &SocketRef,
    ctx: &ServerContext,
    stack_name: &str,
) -> Result<serde_json::Value> {
    check_login(socket)?;

    let endpoint = get_endpoint(socket);
    let mut stack = Stack::get_stack(ctx.clone().into(), stack_name, endpoint.clone()).await?;

    // Join combined terminal if managed by dockru
    if stack.is_managed_by_dockru().await {
        stack.join_combined_terminal(socket.clone()).await.ok();
    }

    let stack_json = stack.to_json().await?;

    Ok(json!({
        "ok": true,
        "stack": stack_json
    }))
}

async fn handle_start_stack(
    socket: &SocketRef,
    ctx: &ServerContext,
    stack_name: &str,
) -> Result<()> {
    check_login(socket)?;

    let endpoint = get_endpoint(socket);
    let stack = Stack::get_stack(ctx.clone().into(), stack_name, endpoint).await?;
    stack.start(Some(socket.clone())).await?;
    stack.join_combined_terminal(socket.clone()).await?;

    Ok(())
}

async fn handle_stop_stack(
    socket: &SocketRef,
    ctx: &ServerContext,
    stack_name: &str,
) -> Result<()> {
    check_login(socket)?;

    let endpoint = get_endpoint(socket);
    let stack = Stack::get_stack(ctx.clone().into(), stack_name, endpoint).await?;
    stack.stop(Some(socket.clone())).await?;

    Ok(())
}

async fn handle_restart_stack(
    socket: &SocketRef,
    ctx: &ServerContext,
    stack_name: &str,
) -> Result<()> {
    check_login(socket)?;

    let endpoint = get_endpoint(socket);
    let stack = Stack::get_stack(ctx.clone().into(), stack_name, endpoint).await?;
    stack.restart(Some(socket.clone())).await?;

    Ok(())
}

async fn handle_update_stack(
    socket: &SocketRef,
    ctx: &ServerContext,
    stack_name: &str,
) -> Result<()> {
    check_login(socket)?;

    let endpoint = get_endpoint(socket);
    let mut stack = Stack::get_stack(ctx.clone().into(), stack_name, endpoint).await?;
    stack.update(Some(socket.clone())).await?;

    Ok(())
}

async fn handle_down_stack(
    socket: &SocketRef,
    ctx: &ServerContext,
    stack_name: &str,
) -> Result<()> {
    check_login(socket)?;

    let endpoint = get_endpoint(socket);
    let stack = Stack::get_stack(ctx.clone().into(), stack_name, endpoint).await?;
    stack.down(Some(socket.clone())).await?;

    Ok(())
}

async fn handle_service_status_list(
    socket: &SocketRef,
    ctx: &ServerContext,
    stack_name: &str,
) -> Result<serde_json::Value> {
    check_login(socket)?;

    let endpoint = get_endpoint(socket);
    let stack = Stack::get_stack(ctx.clone().into(), stack_name, endpoint).await?;
    let service_status_list = stack.get_service_status_list().await?;

    // Convert HashMap to JSON
    let status_map: HashMap<String, ServiceStatus> = service_status_list;

    Ok(json!({
        "ok": true,
        "serviceStatusList": status_map
    }))
}

async fn handle_get_docker_network_list(
    socket: &SocketRef,
    _ctx: &ServerContext,
) -> Result<serde_json::Value> {
    check_login(socket)?;

    // Run docker network ls command
    let output = tokio::process::Command::new("docker")
        .args(&["network", "ls", "--format", "{{.Name}}"])
        .output()
        .await?;

    if !output.status.success() {
        return Err(anyhow!("Failed to get docker network list"));
    }

    let networks: Vec<String> = String::from_utf8(output.stdout)?
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| line.to_string())
        .collect();

    Ok(json!({
        "ok": true,
        "dockerNetworkList": networks
    }))
}

/// Broadcast stack list to all authenticated sockets
async fn broadcast_stack_list(ctx: &ServerContext) {
    // TODO Phase 7: Implement proper broadcasting to all authenticated sockets
    // For now, just log
    debug!("Broadcasting stack list (stubbed)");

    // In Phase 8, we'll iterate through all sockets, check authentication,
    // and emit stackList to each with their specific endpoint
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deploy_stack_data_deserialize() {
        let json = r#"{
            "name": "test-stack",
            "composeYAML": "version: '3'\nservices:\n  web:\n    image: nginx",
            "composeENV": "FOO=bar",
            "isAdd": true
        }"#;
        let data: DeployStackData = serde_json::from_str(json).unwrap();
        assert_eq!(data.name, "test-stack");
        assert!(data.is_add);
    }
}
