use crate::server::ServerContext;
use crate::socket_handlers::{callback_error, callback_ok, check_login, get_endpoint};
use crate::stack::{ServiceStatus, Stack};
use anyhow::{anyhow, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use socketioxide::extract::{AckSender, Data, SocketRef};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, warn};

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
        move |socket: SocketRef, Data::<serde_json::Value>(data), ack: AckSender| {
            let ctx = ctx_clone.clone();
            tokio::spawn(async move {
                match parse_deploy_stack_args(&data) {
                    Ok(parsed) => match handle_deploy_stack(&socket, &ctx, parsed).await {
                        Ok(_) => {
                            callback_ok(Some(ack), "Deployed", true);
                            broadcast_stack_list(&ctx).await;
                        }
                        Err(e) => callback_error(Some(ack), e),
                    },
                    Err(e) => callback_error(Some(ack), e),
                }
            });
        },
    );

    // saveStack
    let ctx_clone = ctx.clone();
    socket.on(
        "saveStack",
        move |socket: SocketRef, Data::<serde_json::Value>(data), ack: AckSender| {
            let ctx = ctx_clone.clone();
            tokio::spawn(async move {
                match parse_save_stack_args(&data) {
                    Ok(parsed) => match handle_save_stack(&socket, &ctx, parsed).await {
                        Ok(_) => {
                            callback_ok(Some(ack), "Saved", true);
                            broadcast_stack_list(&ctx).await;
                        }
                        Err(e) => callback_error(Some(ack), e),
                    },
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

/// Parse deployStack positional args: [name, composeYAML, composeENV, isAdd]
fn parse_deploy_stack_args(data: &Value) -> Result<DeployStackData> {
    let args = data
        .as_array()
        .ok_or_else(|| anyhow!("Expected array of arguments"))?;
    if args.len() < 4 {
        return Err(anyhow!(
            "deployStack requires 4 arguments: name, composeYAML, composeENV, isAdd"
        ));
    }
    Ok(DeployStackData {
        name: args[0]
            .as_str()
            .ok_or_else(|| anyhow!("name must be a string"))?
            .to_string(),
        compose_yaml: args[1]
            .as_str()
            .ok_or_else(|| anyhow!("composeYAML must be a string"))?
            .to_string(),
        compose_env: args[2]
            .as_str()
            .ok_or_else(|| anyhow!("composeENV must be a string"))?
            .to_string(),
        is_add: args[3]
            .as_bool()
            .ok_or_else(|| anyhow!("isAdd must be a boolean"))?,
    })
}

/// Parse saveStack positional args: [name, composeYAML, composeENV, isAdd]
fn parse_save_stack_args(data: &Value) -> Result<SaveStackData> {
    let args = data
        .as_array()
        .ok_or_else(|| anyhow!("Expected array of arguments"))?;
    if args.len() < 4 {
        return Err(anyhow!(
            "saveStack requires 4 arguments: name, composeYAML, composeENV, isAdd"
        ));
    }
    Ok(SaveStackData {
        name: args[0]
            .as_str()
            .ok_or_else(|| anyhow!("name must be a string"))?
            .to_string(),
        compose_yaml: args[1]
            .as_str()
            .ok_or_else(|| anyhow!("composeYAML must be a string"))?
            .to_string(),
        compose_env: args[2]
            .as_str()
            .ok_or_else(|| anyhow!("composeENV must be a string"))?
            .to_string(),
        is_add: args[3]
            .as_bool()
            .ok_or_else(|| anyhow!("isAdd must be a boolean"))?,
    })
}

/// Dispatch a stack event from the agent proxy (local endpoint).
/// Returns Ok(true) if the event was handled, Ok(false) if not recognized.
pub(crate) async fn dispatch_stack_event(
    socket: &SocketRef,
    ctx: &ServerContext,
    event_name: &str,
    event_args: &[Value],
    ack: &mut Option<AckSender>,
) -> Result<bool> {
    match event_name {
        "deployStack" => {
            let data = parse_deploy_stack_args(&json!(event_args))?;
            match handle_deploy_stack(socket, ctx, data).await {
                Ok(_) => {
                    callback_ok(ack.take(), "Deployed", true);
                    broadcast_stack_list(ctx).await;
                }
                Err(e) => callback_error(ack.take(), e),
            }
            Ok(true)
        }
        "saveStack" => {
            warn!(
                "dispatch_stack_event saveStack: event_args={:?}",
                event_args
            );
            let data = parse_save_stack_args(&json!(event_args))?;
            warn!(
                "dispatch_stack_event saveStack parsed: name={}, is_add={}",
                data.name, data.is_add
            );
            match handle_save_stack(socket, ctx, data).await {
                Ok(_) => {
                    callback_ok(ack.take(), "Saved", true);
                    broadcast_stack_list(ctx).await;
                }
                Err(e) => callback_error(ack.take(), e),
            }
            Ok(true)
        }
        "deleteStack" => {
            let stack_name = event_args
                .first()
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("deleteStack requires a stack name"))?;
            match handle_delete_stack(socket, ctx, stack_name).await {
                Ok(_) => {
                    callback_ok(ack.take(), "Deleted", true);
                    broadcast_stack_list(ctx).await;
                }
                Err(e) => callback_error(ack.take(), e),
            }
            Ok(true)
        }
        "getStack" => {
            let stack_name = event_args
                .first()
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("getStack requires a stack name"))?;
            match handle_get_stack(socket, ctx, stack_name).await {
                Ok(response) => {
                    if let Some(ack) = ack.take() {
                        ack.send(&response).ok();
                    }
                }
                Err(e) => callback_error(ack.take(), e),
            }
            Ok(true)
        }
        "requestStackList" => {
            if check_login(socket).is_ok() {
                broadcast_stack_list(ctx).await;
                callback_ok(ack.take(), "Updated", true);
            }
            Ok(true)
        }
        "startStack" => {
            let stack_name = event_args
                .first()
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("startStack requires a stack name"))?;
            match handle_start_stack(socket, ctx, stack_name).await {
                Ok(_) => {
                    callback_ok(ack.take(), "Started", true);
                    broadcast_stack_list(ctx).await;
                }
                Err(e) => callback_error(ack.take(), e),
            }
            Ok(true)
        }
        "stopStack" => {
            let stack_name = event_args
                .first()
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("stopStack requires a stack name"))?;
            match handle_stop_stack(socket, ctx, stack_name).await {
                Ok(_) => {
                    callback_ok(ack.take(), "Stopped", true);
                    broadcast_stack_list(ctx).await;
                }
                Err(e) => callback_error(ack.take(), e),
            }
            Ok(true)
        }
        "restartStack" => {
            let stack_name = event_args
                .first()
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("restartStack requires a stack name"))?;
            match handle_restart_stack(socket, ctx, stack_name).await {
                Ok(_) => {
                    callback_ok(ack.take(), "Restarted", true);
                    broadcast_stack_list(ctx).await;
                }
                Err(e) => callback_error(ack.take(), e),
            }
            Ok(true)
        }
        "updateStack" => {
            let stack_name = event_args
                .first()
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("updateStack requires a stack name"))?;
            match handle_update_stack(socket, ctx, stack_name).await {
                Ok(_) => {
                    callback_ok(ack.take(), "Updated", true);
                    broadcast_stack_list(ctx).await;
                }
                Err(e) => callback_error(ack.take(), e),
            }
            Ok(true)
        }
        "downStack" => {
            let stack_name = event_args
                .first()
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("downStack requires a stack name"))?;
            match handle_down_stack(socket, ctx, stack_name).await {
                Ok(_) => {
                    callback_ok(ack.take(), "Downed", true);
                    broadcast_stack_list(ctx).await;
                }
                Err(e) => callback_error(ack.take(), e),
            }
            Ok(true)
        }
        "serviceStatusList" => {
            let stack_name = event_args
                .first()
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("serviceStatusList requires a stack name"))?;
            match handle_service_status_list(socket, ctx, stack_name).await {
                Ok(response) => {
                    if let Some(ack) = ack.take() {
                        ack.send(&response).ok();
                    }
                }
                Err(e) => callback_error(ack.take(), e),
            }
            Ok(true)
        }
        "getDockerNetworkList" => {
            match handle_get_docker_network_list(socket, ctx).await {
                Ok(response) => {
                    if let Some(ack) = ack.take() {
                        ack.send(&response).ok();
                    }
                }
                Err(e) => callback_error(ack.take(), e),
            }
            Ok(true)
        }
        _ => Ok(false),
    }
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
        .args(["network", "ls", "--format", "{{.Name}}"])
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
    use crate::stack::Stack;
    use std::collections::HashMap;

    let ctx_arc = Arc::new(ctx.clone());
    match Stack::get_stack_list(ctx_arc, String::new(), false).await {
        Ok(stack_list) => {
            let mut map: HashMap<String, serde_json::Value> = HashMap::new();
            for (name, stack) in stack_list {
                let simple_json = stack.to_simple_json().await;
                if let Ok(json) = serde_json::to_value(simple_json) {
                    map.insert(name, json);
                }
            }

            let response = json!({
                "ok": true,
                "stackList": map,
            });

            // Broadcast wrapped in "agent" protocol
            ctx.io.emit("agent", ("stackList", &response)).ok();
        }
        Err(e) => {
            debug!("Failed to get stack list for broadcast: {}", e);
        }
    }
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
