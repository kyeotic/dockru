use crate::utils::types::BaseRes;
use anyhow::Result;
use serde::Serialize;
use serde_json::{json, Value};
use socketioxide::extract::SocketRef;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tracing::debug;

/// Socket state stored per connection
#[derive(Debug, Clone, Default)]
pub struct SocketState {
    pub user_id: Option<i64>,
    pub endpoint: String,
    /// IP address of the socket connection.
    /// Note: Currently always None due to socketioxide not exposing peer address.
    /// See rust-next.md section 3.5 for implementation plan (signed nonce system).
    #[allow(dead_code)]
    pub ip_address: Option<String>,
}

/// Global socket state storage
/// Maps socket ID to socket state
static SOCKET_STATE: once_cell::sync::Lazy<Arc<RwLock<HashMap<String, SocketState>>>> =
    once_cell::sync::Lazy::new(|| Arc::new(RwLock::new(HashMap::new())));

/// Room name for all authenticated sockets
const AUTHENTICATED_ROOM: &str = "authenticated";

/// Set socket state
pub fn set_socket_state(socket_id: &str, state: SocketState) {
    if let Ok(mut map) = SOCKET_STATE.write() {
        map.insert(socket_id.to_string(), state);
    }
}

/// Get socket state
pub fn get_socket_state(socket_id: &str) -> Option<SocketState> {
    SOCKET_STATE
        .read()
        .ok()
        .and_then(|map| map.get(socket_id).cloned())
}

/// Remove socket state (on disconnect)
pub fn remove_socket_state(socket_id: &str) {
    if let Ok(mut map) = SOCKET_STATE.write() {
        map.remove(socket_id);
    }
    // Note: Socket rooms are automatically cleaned up on disconnect
}

/// Get user ID from socket state
pub fn get_user_id(socket: &SocketRef) -> Option<i64> {
    get_socket_state(&socket.id.to_string()).and_then(|s| s.user_id)
}

/// Set user ID in socket state
pub fn set_user_id(socket: &SocketRef, user_id: i64) {
    let socket_id = socket.id.to_string();
    let mut state = get_socket_state(&socket_id).unwrap_or_default();
    state.user_id = Some(user_id);
    set_socket_state(&socket_id, state);
}

/// Get endpoint from socket state
pub fn get_endpoint(socket: &SocketRef) -> String {
    get_socket_state(&socket.id.to_string())
        .map(|s| s.endpoint)
        .unwrap_or_default()
}

/// Set endpoint in socket state
pub fn set_endpoint(socket: &SocketRef, endpoint: String) {
    let socket_id = socket.id.to_string();
    let mut state = get_socket_state(&socket_id).unwrap_or_default();
    state.endpoint = endpoint;
    set_socket_state(&socket_id, state);
}

/// Get IP address from socket state
/// Infrastructure for future use - see rust-next.md section 3.5
#[allow(dead_code)]
pub fn get_ip_address(socket: &SocketRef) -> Option<String> {
    get_socket_state(&socket.id.to_string()).and_then(|s| s.ip_address)
}

/// Set IP address in socket state
/// Infrastructure for future use - see rust-next.md section 3.5
#[allow(dead_code)]
pub fn set_ip_address(socket: &SocketRef, ip_address: Option<String>) {
    let socket_id = socket.id.to_string();
    let mut state = get_socket_state(&socket_id).unwrap_or_default();
    state.ip_address = ip_address;
    set_socket_state(&socket_id, state);
}

/// Mark a socket as authenticated by joining it to the authenticated room
pub fn add_authenticated_socket(socket: &SocketRef) {
    socket.join(AUTHENTICATED_ROOM);
    debug!("Socket {} joined authenticated room", socket.id);
}

/// Check if socket is authenticated
pub fn check_login(socket: &SocketRef) -> Result<i64> {
    get_user_id(socket).ok_or_else(|| anyhow::anyhow!("You are not logged in."))
}

/// Create success response with data
pub fn ok_response<T: Serialize>(data: T) -> BaseRes {
    BaseRes::ok_with_data(data)
}

/// Create error response
pub fn error_response(msg: &str) -> BaseRes {
    BaseRes::error(msg)
}

/// Create error response with i18n flag
pub fn error_response_i18n(msg: &str) -> BaseRes {
    BaseRes::error_i18n(msg)
}

/// Emit to socket with agent proxy support (stubbed for Phase 7)
/// In Phase 8, this will route events through agent manager if endpoint is set
/// Emit an event to the socket, wrapped in the "agent" protocol.
/// The TypeScript equivalent is `dockgeSocket.emitAgent(event, data)` which sends
/// `socket.emit("agent", event, { ...data, endpoint })`.
/// The frontend listens: `socket.on("agent", (eventName, ...args) => agentSocket.call(eventName, ...args))`
pub fn emit_agent(socket: &SocketRef, event: &str, data: Value) -> Result<()> {
    let endpoint = get_endpoint(socket);

    // Inject endpoint into the data object, matching TypeScript behavior
    let mut agent_data = data;
    if let Some(obj) = agent_data.as_object_mut() {
        obj.insert("endpoint".to_string(), json!(endpoint));
    }

    // Wrap in "agent" event: emit("agent", eventName, data)
    socket
        .emit("agent", &(event, &agent_data))
        .map_err(|e| anyhow::anyhow!("Failed to emit agent event: {}", e))?;
    debug!("Emitted agent/{} to socket {}", event, socket.id);

    Ok(())
}

/// Broadcast to all authenticated sockets, wrapped in the "agent" protocol.
pub async fn broadcast_to_authenticated(
    io: &socketioxide::SocketIo,
    event: &str,
    data: Value,
) -> Result<()> {
    // Emit to the authenticated room
    io.to(AUTHENTICATED_ROOM)
        .emit("agent", &(event, &data))
        .await
        .map_err(|e| anyhow::anyhow!("Failed to broadcast to authenticated sockets: {}", e))?;
    debug!("Broadcasted agent/{} to authenticated sockets", event);
    Ok(())
}

/// Handle callback with simple ok response
pub fn callback_ok(callback: Option<socketioxide::extract::AckSender>, msg: &str, msgi18n: bool) {
    if let Some(ack) = callback {
        let response = if msgi18n {
            BaseRes::ok_with_msg_i18n(msg)
        } else {
            BaseRes::ok_with_msg(msg)
        };
        ack.send(&response).ok();
    }
}

/// Handle callback with error
pub fn callback_error(callback: Option<socketioxide::extract::AckSender>, error: anyhow::Error) {
    if let Some(ack) = callback {
        let response = BaseRes::error(error.to_string());
        ack.send(&response).ok();
    }
}
