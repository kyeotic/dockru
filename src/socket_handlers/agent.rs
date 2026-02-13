use crate::agent_manager;
use crate::server::ServerContext;
use crate::socket_handlers::{
    callback_error, check_login, error_response_i18n, get_endpoint, ok_response,
};
use crate::utils::ALL_ENDPOINTS;
use anyhow::anyhow;
use serde::Deserialize;
use serde_json::json;
use socketioxide::extract::{AckSender, Data, SocketRef};
use std::sync::Arc;
use tracing::{debug, info, warn};

use super::stack_management::dispatch_stack_event;
use super::terminal::dispatch_terminal_event;

#[derive(Debug, Deserialize)]
struct AddAgentData {
    url: String,
    username: String,
    password: String,
}

#[derive(Debug, Deserialize)]
struct RemoveAgentData {
    url: String,
}

/// Setup agent management event handlers
pub fn setup_agent_handlers(socket: SocketRef, ctx: Arc<ServerContext>) {
    // addAgent - Add a remote Dockru instance
    let ctx_clone = ctx.clone();
    socket.on(
        "addAgent",
        move |socket: SocketRef, Data::<AddAgentData>(data), ack: AckSender| {
            let ctx = ctx_clone.clone();
            tokio::spawn(async move {
                match handle_add_agent(&socket, &ctx, data).await {
                    Ok(response) => {
                        ack.send(&response).ok();
                    }
                    Err(e) => callback_error(Some(ack), e),
                }
            });
        },
    );

    // removeAgent - Remove a remote Dockru instance
    let ctx_clone = ctx.clone();
    socket.on(
        "removeAgent",
        move |socket: SocketRef, Data::<String>(url), ack: AckSender| {
            let ctx = ctx_clone.clone();
            tokio::spawn(async move {
                match handle_remove_agent(&socket, &ctx, &url).await {
                    Ok(response) => {
                        ack.send(&response).ok();
                    }
                    Err(e) => callback_error(Some(ack), e),
                }
            });
        },
    );

    // agent - Proxy event to specific endpoint or broadcast
    // Format: agent(endpoint: string, eventName: string, ...args)
    let ctx_clone = ctx;
    socket.on(
        "agent",
        move |socket: SocketRef, Data::<serde_json::Value>(data), ack: AckSender| {
            let ctx = ctx_clone.clone();
            tokio::spawn(async move {
                warn!(
                    "Agent event received, data type: {:?}",
                    std::mem::discriminant(&data)
                );
                if let Err(e) = handle_agent_proxy(&socket, &ctx, data, ack).await {
                    warn!("Agent proxy error: {}", e);
                }
            });
        },
    );
}

async fn handle_add_agent(
    socket: &SocketRef,
    ctx: &ServerContext,
    data: AddAgentData,
) -> Result<serde_json::Value, anyhow::Error> {
    check_login(socket)?;

    info!("Adding agent: {}", data.url);

    // Get agent manager
    let manager = agent_manager::get_agent_manager(&socket.id.to_string())
        .await
        .ok_or_else(|| anyhow!("Agent manager not found"))?;

    // Test connection first
    manager
        .test(&data.url, &data.username, &data.password)
        .await?;

    // Add to database
    manager
        .add(&data.url, &data.username, &data.password)
        .await?;

    // Connect to the agent
    manager
        .connect(&data.url, &data.username, &data.password)
        .await;

    // Broadcast to force refresh other clients
    // TODO: Implement disconnectAllSocketClients except current socket

    // Send updated agent list
    manager.send_agent_list().await;

    Ok(ok_response(json!({
        "msg": "agentAddedSuccessfully",
        "msgi18n": true,
    })))
}

async fn handle_remove_agent(
    socket: &SocketRef,
    _ctx: &ServerContext,
    url: &str,
) -> Result<serde_json::Value, anyhow::Error> {
    check_login(socket)?;

    info!("Removing agent: {}", url);

    // Get agent manager
    let manager = agent_manager::get_agent_manager(&socket.id.to_string())
        .await
        .ok_or_else(|| anyhow!("Agent manager not found"))?;

    // Remove agent
    manager.remove(url).await?;

    // TODO: Broadcast to force refresh other clients

    Ok(ok_response(json!({
        "msg": "agentRemovedSuccessfully",
        "msgi18n": true,
    })))
}

async fn handle_agent_proxy(
    socket: &SocketRef,
    ctx: &ServerContext,
    data: serde_json::Value,
    ack: AckSender,
) -> Result<(), anyhow::Error> {
    check_login(socket)?;

    // Parse arguments: [endpoint, eventName, ...args]
    let args_array = data
        .as_array()
        .ok_or_else(|| anyhow!("Agent event data must be an array"))?;

    if args_array.len() < 2 {
        return Err(anyhow!(
            "Agent event must have at least endpoint and eventName"
        ));
    }

    let endpoint = args_array[0]
        .as_str()
        .ok_or_else(|| anyhow!("Endpoint must be a string"))?;

    let event_name = args_array[1]
        .as_str()
        .ok_or_else(|| anyhow!("Event name must be a string"))?;

    // Remaining args (after endpoint and eventName)
    let event_args: Vec<serde_json::Value> = if args_array.len() > 2 {
        args_array[2..].to_vec()
    } else {
        vec![]
    };

    let socket_endpoint = get_endpoint(socket);

    // Get agent manager
    let manager = agent_manager::get_agent_manager(&socket.id.to_string())
        .await
        .ok_or_else(|| anyhow!("Agent manager not found"))?;

    if endpoint == ALL_ENDPOINTS {
        // Send to all endpoints
        debug!("Sending to all endpoints: {}", event_name);

        // Handle locally first
        let mut local_ack = Some(ack);
        dispatch_local_event(socket, ctx, event_name, &event_args, &mut local_ack).await;

        // Forward to remote endpoints
        manager
            .emit_to_all_endpoints(event_name, json!(event_args))
            .await;
    } else if endpoint.is_empty() || endpoint == socket_endpoint {
        // Direct connection or matching endpoint - handle locally
        debug!("Handling local event: {}", event_name);
        let mut local_ack = Some(ack);
        dispatch_local_event(socket, ctx, event_name, &event_args, &mut local_ack).await;
    } else {
        // Proxy to specific remote endpoint
        debug!("Proxying request to {} for {}", endpoint, event_name);
        // TODO: Forward ack to remote endpoint
        manager
            .emit_to_endpoint(endpoint, event_name, json!(event_args))
            .await?;
    }

    Ok(())
}

/// Dispatch a local agent event to the appropriate handler.
async fn dispatch_local_event(
    socket: &SocketRef,
    ctx: &ServerContext,
    event_name: &str,
    event_args: &[serde_json::Value],
    ack: &mut Option<AckSender>,
) {
    // Try stack handlers first
    match dispatch_stack_event(socket, ctx, event_name, event_args, ack).await {
        Ok(true) => return,
        Ok(false) => {} // Not a stack event, try next
        Err(e) => {
            warn!("Stack event dispatch error for {}: {}", event_name, e);
            callback_error(ack.take(), e);
            return;
        }
    }

    // Try terminal handlers
    match dispatch_terminal_event(socket, ctx, event_name, event_args, ack).await {
        Ok(true) => return,
        Ok(false) => {} // Not a terminal event either
        Err(e) => {
            warn!("Terminal event dispatch error for {}: {}", event_name, e);
            callback_error(ack.take(), e);
            return;
        }
    }

    // No handler found
    warn!("Unknown local agent event: {}", event_name);
    callback_error(ack.take(), anyhow!("Unknown event: {}", event_name));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_agent_deserialize() {
        let json = r#"{
            "url": "http://localhost:5002",
            "username": "admin",
            "password": "secret"
        }"#;
        let data: AddAgentData = serde_json::from_str(json).unwrap();
        assert_eq!(data.url, "http://localhost:5002");
        assert_eq!(data.username, "admin");
    }

    #[test]
    fn test_remove_agent_deserialize() {
        let json = r#""http://localhost:5002""#;
        let url: String = serde_json::from_str(json).unwrap();
        assert_eq!(url, "http://localhost:5002");
    }

    #[test]
    fn test_agent_proxy_parse() {
        let json = json!(["localhost:5002", "deployStack", {"stackName": "test"}]);
        let args_array = json.as_array().unwrap();
        assert_eq!(args_array.len(), 3);
        assert_eq!(args_array[0].as_str(), Some("localhost:5002"));
        assert_eq!(args_array[1].as_str(), Some("deployStack"));
    }
}
