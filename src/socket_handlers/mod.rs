mod helpers;
pub use helpers::*;

mod agent;
mod auth;
mod settings;
mod stack_management;
mod terminal;

pub use agent::setup_agent_handlers;
pub use auth::setup_auth_handlers;
pub use settings::setup_settings_handlers;
pub use stack_management::setup_stack_handlers;
pub use terminal::setup_terminal_handlers;

use crate::server::ServerContext;
use socketioxide::extract::SocketRef;
use std::sync::Arc;

/// Setup all socket event handlers
pub fn setup_all_handlers(socket: SocketRef, ctx: Arc<ServerContext>) {
    setup_auth_handlers(socket.clone(), ctx.clone());
    setup_settings_handlers(socket.clone(), ctx.clone());
    setup_stack_handlers(socket.clone(), ctx.clone());
    setup_terminal_handlers(socket.clone(), ctx.clone());
    setup_agent_handlers(socket.clone(), ctx.clone());
}
