use crate::server::ServerContext;
use crate::socket_handlers::{callback_error, check_login};
use anyhow::anyhow;
use serde::Deserialize;
use socketioxide::extract::{AckSender, Data, SocketRef};
use std::sync::Arc;
use tracing::warn;

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

#[derive(Debug, Deserialize)]
struct AgentProxyData {
    endpoint: Option<String>,
    event: String,
    #[serde(flatten)]
    args: serde_json::Value,
}

/// Setup agent management event handlers (Phase 8 - stubbed for Phase 7)
pub fn setup_agent_handlers(socket: SocketRef, ctx: Arc<ServerContext>) {
    // addAgent - Add a remote Dockge instance
    let ctx_clone = ctx.clone();
    socket.on(
        "addAgent",
        move |socket: SocketRef, Data::<AddAgentData>(data), ack: AckSender| {
            let ctx = ctx_clone.clone();
            tokio::spawn(async move {
                match handle_add_agent(&socket, &ctx, data).await {
                    Ok(response) => { ack.send(&response).ok(); },
                    Err(e) => callback_error(Some(ack), e),
                };
            });
        },
    );

    // removeAgent - Remove a remote Dockge instance
    let ctx_clone = ctx.clone();
    socket.on(
        "removeAgent",
        move |socket: SocketRef, Data::<RemoveAgentData>(data), ack: AckSender| {
            let ctx = ctx_clone.clone();
            tokio::spawn(async move {
                match handle_remove_agent(&socket, &ctx, data).await {
                    Ok(response) => { ack.send(&response).ok(); },
                    Err(e) => callback_error(Some(ack), e),
                };
            });
        },
    );

    // agent - Proxy event to specific endpoint or broadcast
    let ctx_clone = ctx.clone();
    socket.on(
        "agent",
        move |socket: SocketRef, Data::<AgentProxyData>(data), ack: AckSender| {
            let ctx = ctx_clone.clone();
            tokio::spawn(async move {
                match handle_agent_proxy(&socket, &ctx, data).await {
                    Ok(response) => { ack.send(&response).ok(); },
                    Err(e) => callback_error(Some(ack), e),
                };
            });
        },
    );
}

async fn handle_add_agent(
    socket: &SocketRef,
    _ctx: &ServerContext,
    data: AddAgentData,
) -> Result<serde_json::Value, anyhow::Error> {
    check_login(socket)?;

    warn!(
        "Agent management not implemented - addAgent called for {}",
        data.url
    );

    // TODO Phase 8: Implement agent management
    // 1. Validate URL
    // 2. Test connection
    // 3. Store in database
    // 4. Connect to agent
    // 5. Send agent list update

    Err(anyhow!(
        "Agent management is not yet implemented (Phase 8)"
    ))
}

async fn handle_remove_agent(
    socket: &SocketRef,
    _ctx: &ServerContext,
    data: RemoveAgentData,
) -> Result<serde_json::Value, anyhow::Error> {
    check_login(socket)?;

    warn!(
        "Agent management not implemented - removeAgent called for {}",
        data.url
    );

    // TODO Phase 8: Implement agent removal
    // 1. Find agent in database
    // 2. Disconnect from agent
    // 3. Remove from database
    // 4. Send agent list update

    Err(anyhow!(
        "Agent management is not yet implemented (Phase 8)"
    ))
}

async fn handle_agent_proxy(
    socket: &SocketRef,
    _ctx: &ServerContext,
    data: AgentProxyData,
) -> Result<serde_json::Value, anyhow::Error> {
    check_login(socket)?;

    warn!(
        "Agent proxy not implemented - event {} for endpoint {:?}",
        data.event, data.endpoint
    );

    // TODO Phase 8: Implement agent proxy
    // If endpoint is None or "local":
    //   - Handle locally
    // If endpoint is specific:
    //   - Route to that agent with retry
    // If endpoint is "*":
    //   - Broadcast to all agents

    Err(anyhow!("Agent proxy is not yet implemented (Phase 8)"))
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
        let json = r#"{"url": "http://localhost:5002"}"#;
        let data: RemoveAgentData = serde_json::from_str(json).unwrap();
        assert_eq!(data.url, "http://localhost:5002");
    }

    #[test]
    fn test_agent_proxy_deserialize() {
        let json = r#"{
            "endpoint": "localhost:5002",
            "event": "deployStack",
            "stackName": "test"
        }"#;
        let data: AgentProxyData = serde_json::from_str(json).unwrap();
        assert_eq!(data.endpoint, Some("localhost:5002".to_string()));
        assert_eq!(data.event, "deployStack");
    }
}
