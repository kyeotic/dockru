use anyhow::Result;
use serde_json::{json, Value};
use socketioxide::extract::SocketRef;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tracing::{debug, warn};

/// Socket state stored per connection
#[derive(Debug, Clone, Default)]
pub struct SocketState {
    pub user_id: Option<i64>,
    pub endpoint: String,
    pub ip_address: Option<String>,
}

/// Global socket state storage
/// Maps socket ID to socket state
static SOCKET_STATE: once_cell::sync::Lazy<Arc<RwLock<HashMap<String, SocketState>>>> =
    once_cell::sync::Lazy::new(|| Arc::new(RwLock::new(HashMap::new())));

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
        .unwrap_or_else(|| "".to_string())
}

/// Set endpoint in socket state
pub fn set_endpoint(socket: &SocketRef, endpoint: String) {
    let socket_id = socket.id.to_string();
    let mut state = get_socket_state(&socket_id).unwrap_or_default();
    state.endpoint = endpoint;
    set_socket_state(&socket_id, state);
}

/// Check if socket is authenticated
pub fn check_login(socket: &SocketRef) -> Result<i64> {
    get_user_id(socket).ok_or_else(|| anyhow::anyhow!("You are not logged in."))
}

/// Create success response
pub fn ok_response<T: serde::Serialize>(data: T) -> Value {
    json!({
        "ok": true,
        "data": data
    })
}

/// Create error response
pub fn error_response(msg: &str) -> Value {
    json!({
        "ok": false,
        "msg": msg
    })
}

/// Create error response with i18n flag
pub fn error_response_i18n(msg: &str) -> Value {
    json!({
        "ok": false,
        "msg": msg,
        "msgi18n": true
    })
}

/// Emit to socket with agent proxy support (stubbed for Phase 7)
/// In Phase 8, this will route events through agent manager if endpoint is set
pub fn emit_agent(socket: &SocketRef, event: &str, data: Value) -> Result<()> {
    let endpoint = get_endpoint(socket);

    if endpoint.is_empty() || endpoint == "local" {
        // Local endpoint - emit directly
        socket.emit(event.to_string(), &data).ok();
        debug!("Emitted {} to local socket {}", event, socket.id);
    } else {
        // Remote endpoint - would proxy through agent manager in Phase 8
        // For now, just log and emit locally
        debug!(
            "Agent proxy not implemented - would route {} to endpoint {}",
            event, endpoint
        );
        socket.emit(event.to_string(), &data).ok();
        warn!("Agent proxy is stubbed - emitting locally instead");
    }

    Ok(())
}

/// Broadcast to all authenticated sockets
pub fn broadcast_to_authenticated(io: &socketioxide::SocketIo, event: &str, data: Value) {
    // TODO:
    // Phase 8: Iterate through all sockets and emit only to authenticated ones
    // For now, broadcast to all
    io.emit(event.to_string(), &data).ok();
    debug!("Broadcasted {} to all sockets", event);
}

/// Handle callback with result
pub fn callback_result<T: serde::Serialize>(
    callback: Option<socketioxide::extract::AckSender>,
    result: Result<T>,
) {
    if let Some(ack) = callback {
        match result {
            Ok(data) => {
                let response = json!({
                    "ok": true,
                    "data": data
                });
                ack.send(&response).ok();
            }
            Err(e) => {
                let response = error_response(&e.to_string());
                ack.send(&response).ok();
            }
        }
    }
}

/// Handle callback with simple ok response
pub fn callback_ok(callback: Option<socketioxide::extract::AckSender>, msg: &str, msgi18n: bool) {
    if let Some(ack) = callback {
        let mut response = json!({
            "ok": true,
            "msg": msg,
        });
        if msgi18n {
            response["msgi18n"] = json!(true);
        }
        ack.send(&response).ok();
    }
}

/// Handle callback with error
pub fn callback_error(callback: Option<socketioxide::extract::AckSender>, error: anyhow::Error) {
    if let Some(ack) = callback {
        let response = error_response(&error.to_string());
        ack.send(&response).ok();
    }
}
