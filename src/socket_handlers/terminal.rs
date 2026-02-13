use crate::server::ServerContext;
use crate::socket_handlers::{callback_error, callback_ok, check_login, get_endpoint};
use crate::stack::Stack;
use crate::terminal::{Terminal, TerminalType};
use anyhow::{anyhow, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use socketioxide::extract::{AckSender, Data, SocketRef};
use std::sync::Arc;
use tracing::{debug, info};

#[derive(Debug, Deserialize)]
struct TerminalInputData {
    #[serde(rename = "terminalName")]
    terminal_name: String,
    cmd: String,
}

#[derive(Debug, Deserialize)]
struct InteractiveTerminalData {
    #[serde(rename = "stackName")]
    stack_name: String,
    #[serde(rename = "serviceName")]
    service_name: String,
    shell: String,
}

#[derive(Debug, Deserialize)]
struct TerminalResizeData {
    #[serde(rename = "terminalName")]
    terminal_name: String,
    rows: u16,
    cols: u16,
}

/// Setup terminal event handlers
pub fn setup_terminal_handlers(socket: SocketRef, ctx: Arc<ServerContext>) {
    // terminalInput
    let ctx_clone = ctx.clone();
    socket.on(
        "terminalInput",
        move |socket: SocketRef, Data::<serde_json::Value>(data), ack: AckSender| {
            let ctx = ctx_clone.clone();
            tokio::spawn(async move {
                match parse_terminal_input_args(&data) {
                    Ok(parsed) => {
                        if let Err(e) = handle_terminal_input(&socket, &ctx, parsed).await {
                            callback_error(Some(ack), e);
                        }
                    }
                    Err(e) => callback_error(Some(ack), e),
                }
            });
        },
    );

    // mainTerminal
    let ctx_clone = ctx.clone();
    socket.on(
        "mainTerminal",
        move |socket: SocketRef, Data::<String>(terminal_name), ack: AckSender| {
            let ctx = ctx_clone.clone();
            tokio::spawn(async move {
                match handle_main_terminal(&socket, &ctx, terminal_name).await {
                    Ok(response) => {
                        ack.send(&response).ok();
                    }
                    Err(e) => callback_error(Some(ack), e),
                };
            });
        },
    );

    // checkMainTerminal
    let ctx_clone = ctx.clone();
    socket.on(
        "checkMainTerminal",
        move |socket: SocketRef, ack: AckSender| {
            let ctx = ctx_clone.clone();
            tokio::spawn(async move {
                match handle_check_main_terminal(&socket, &ctx).await {
                    Ok(response) => {
                        ack.send(&response).ok();
                    }
                    Err(e) => callback_error(Some(ack), e),
                };
            });
        },
    );

    // interactiveTerminal
    let ctx_clone = ctx.clone();
    socket.on(
        "interactiveTerminal",
        move |socket: SocketRef, Data::<serde_json::Value>(data), ack: AckSender| {
            let ctx = ctx_clone.clone();
            tokio::spawn(async move {
                match parse_interactive_terminal_args(&data) {
                    Ok(parsed) => match handle_interactive_terminal(&socket, &ctx, parsed).await {
                        Ok(response) => {
                            ack.send(&response).ok();
                        }
                        Err(e) => callback_error(Some(ack), e),
                    },
                    Err(e) => callback_error(Some(ack), e),
                }
            });
        },
    );

    // terminalJoin
    let ctx_clone = ctx.clone();
    socket.on(
        "terminalJoin",
        move |socket: SocketRef, Data::<String>(terminal_name), ack: AckSender| {
            let ctx = ctx_clone.clone();
            tokio::spawn(async move {
                match handle_terminal_join(&socket, &ctx, terminal_name).await {
                    Ok(response) => {
                        ack.send(&response).ok();
                    }
                    Err(e) => callback_error(Some(ack), e),
                };
            });
        },
    );

    // leaveCombinedTerminal
    let ctx_clone = ctx.clone();
    socket.on(
        "leaveCombinedTerminal",
        move |socket: SocketRef, Data::<String>(stack_name), ack: AckSender| {
            let ctx = ctx_clone.clone();
            tokio::spawn(async move {
                match handle_leave_combined_terminal(&socket, &ctx, &stack_name).await {
                    Ok(response) => {
                        ack.send(&response).ok();
                    }
                    Err(e) => callback_error(Some(ack), e),
                };
            });
        },
    );

    // terminalResize
    let ctx_clone = ctx.clone();
    socket.on(
        "terminalResize",
        move |socket: SocketRef, Data::<serde_json::Value>(data)| {
            let ctx = ctx_clone.clone();
            tokio::spawn(async move {
                match parse_terminal_resize_args(&data) {
                    Ok(parsed) => {
                        if let Err(e) = handle_terminal_resize(&socket, &ctx, parsed).await {
                            debug!("terminalResize error: {}", e);
                        }
                    }
                    Err(e) => debug!("terminalResize parse error: {}", e),
                }
            });
        },
    );
}

/// Parse terminalInput positional args: [terminalName, cmd]
fn parse_terminal_input_args(data: &Value) -> Result<TerminalInputData> {
    let args = data
        .as_array()
        .ok_or_else(|| anyhow!("Expected array of arguments"))?;
    if args.len() < 2 {
        return Err(anyhow!(
            "terminalInput requires 2 arguments: terminalName, cmd"
        ));
    }
    Ok(TerminalInputData {
        terminal_name: args[0]
            .as_str()
            .ok_or_else(|| anyhow!("terminalName must be a string"))?
            .to_string(),
        cmd: args[1]
            .as_str()
            .ok_or_else(|| anyhow!("cmd must be a string"))?
            .to_string(),
    })
}

/// Parse interactiveTerminal positional args: [stackName, serviceName, shell]
fn parse_interactive_terminal_args(data: &Value) -> Result<InteractiveTerminalData> {
    let args = data
        .as_array()
        .ok_or_else(|| anyhow!("Expected array of arguments"))?;
    if args.len() < 3 {
        return Err(anyhow!(
            "interactiveTerminal requires 3 arguments: stackName, serviceName, shell"
        ));
    }
    Ok(InteractiveTerminalData {
        stack_name: args[0]
            .as_str()
            .ok_or_else(|| anyhow!("stackName must be a string"))?
            .to_string(),
        service_name: args[1]
            .as_str()
            .ok_or_else(|| anyhow!("serviceName must be a string"))?
            .to_string(),
        shell: args[2]
            .as_str()
            .ok_or_else(|| anyhow!("shell must be a string"))?
            .to_string(),
    })
}

/// Parse terminalResize positional args: [terminalName, rows, cols]
fn parse_terminal_resize_args(data: &Value) -> Result<TerminalResizeData> {
    let args = data
        .as_array()
        .ok_or_else(|| anyhow!("Expected array of arguments"))?;
    if args.len() < 3 {
        return Err(anyhow!(
            "terminalResize requires 3 arguments: terminalName, rows, cols"
        ));
    }
    Ok(TerminalResizeData {
        terminal_name: args[0]
            .as_str()
            .ok_or_else(|| anyhow!("terminalName must be a string"))?
            .to_string(),
        rows: args[1]
            .as_u64()
            .ok_or_else(|| anyhow!("rows must be a number"))? as u16,
        cols: args[2]
            .as_u64()
            .ok_or_else(|| anyhow!("cols must be a number"))? as u16,
    })
}

/// Dispatch a terminal event from the agent proxy (local endpoint).
/// Returns Ok(true) if the event was handled, Ok(false) if not recognized.
pub(crate) async fn dispatch_terminal_event(
    socket: &SocketRef,
    ctx: &ServerContext,
    event_name: &str,
    event_args: &[Value],
    ack: &mut Option<AckSender>,
) -> Result<bool> {
    match event_name {
        "terminalInput" => {
            let data = parse_terminal_input_args(&json!(event_args))?;
            if let Err(e) = handle_terminal_input(socket, ctx, data).await {
                callback_error(ack.take(), e);
            }
            Ok(true)
        }
        "mainTerminal" => {
            let terminal_name = event_args
                .first()
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            match handle_main_terminal(socket, ctx, terminal_name).await {
                Ok(response) => {
                    if let Some(ack) = ack.take() {
                        ack.send(&response).ok();
                    }
                }
                Err(e) => callback_error(ack.take(), e),
            }
            Ok(true)
        }
        "checkMainTerminal" => {
            match handle_check_main_terminal(socket, ctx).await {
                Ok(response) => {
                    if let Some(ack) = ack.take() {
                        ack.send(&response).ok();
                    }
                }
                Err(e) => callback_error(ack.take(), e),
            }
            Ok(true)
        }
        "interactiveTerminal" => {
            let data = parse_interactive_terminal_args(&json!(event_args))?;
            match handle_interactive_terminal(socket, ctx, data).await {
                Ok(response) => {
                    if let Some(ack) = ack.take() {
                        ack.send(&response).ok();
                    }
                }
                Err(e) => callback_error(ack.take(), e),
            }
            Ok(true)
        }
        "terminalJoin" => {
            let terminal_name = event_args
                .first()
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("terminalJoin requires a terminal name"))?
                .to_string();
            match handle_terminal_join(socket, ctx, terminal_name).await {
                Ok(response) => {
                    if let Some(ack) = ack.take() {
                        ack.send(&response).ok();
                    }
                }
                Err(e) => callback_error(ack.take(), e),
            }
            Ok(true)
        }
        "leaveCombinedTerminal" => {
            let stack_name = event_args
                .first()
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("leaveCombinedTerminal requires a stack name"))?;
            match handle_leave_combined_terminal(socket, ctx, stack_name).await {
                Ok(response) => {
                    if let Some(ack) = ack.take() {
                        ack.send(&response).ok();
                    }
                }
                Err(e) => callback_error(ack.take(), e),
            }
            Ok(true)
        }
        "terminalResize" => {
            let data = parse_terminal_resize_args(&json!(event_args))?;
            if let Err(e) = handle_terminal_resize(socket, ctx, data).await {
                debug!("terminalResize error: {}", e);
            }
            Ok(true)
        }
        _ => Ok(false),
    }
}

async fn handle_terminal_input(
    socket: &SocketRef,
    _ctx: &ServerContext,
    data: TerminalInputData,
) -> Result<()> {
    check_login(socket)?;

    let terminal = Terminal::get_terminal(&data.terminal_name)
        .await
        .ok_or_else(|| anyhow!("Terminal not found or it is not an Interactive Terminal."))?;

    // Check if it's an interactive terminal and write to it
    if terminal.terminal_type() == TerminalType::Interactive
        || terminal.terminal_type() == TerminalType::Main
    {
        terminal.write(&data.cmd).await?;
    } else {
        return Err(anyhow!("Terminal is not interactive"));
    }

    Ok(())
}

async fn handle_main_terminal(
    socket: &SocketRef,
    ctx: &ServerContext,
    _terminal_name: String,
) -> Result<serde_json::Value> {
    check_login(socket)?;

    // Check if console is enabled
    if !ctx.config.enable_console {
        return Err(anyhow!("Console is not enabled."));
    }

    // Force one main terminal for now
    let terminal_name = "console";
    debug!("Main terminal name: {}", terminal_name);

    // Get or create main terminal
    let terminal = if let Some(term) = Terminal::get_terminal(terminal_name).await {
        term
    } else {
        // Create new main terminal
        let term = Terminal::new_main(
            ctx.io.clone(),
            terminal_name.to_string(),
            ctx.config.stacks_dir.to_string_lossy().to_string(),
        )?;
        term.set_rows(50).await?;
        debug!("Main terminal created");

        // Detect shell and start terminal
        let shell = detect_shell();
        term.clone()
            .start(
                shell.clone(),
                vec![],
                ctx.config.stacks_dir.to_string_lossy().to_string(),
            )
            .await?;

        term
    };

    terminal.join(socket.clone()).await?;

    Ok(json!({
        "ok": true
    }))
}

async fn handle_check_main_terminal(
    socket: &SocketRef,
    ctx: &ServerContext,
) -> Result<serde_json::Value> {
    check_login(socket)?;

    let enabled = ctx.config.enable_console;
    Ok(json!({
        "ok": enabled
    }))
}

async fn handle_interactive_terminal(
    socket: &SocketRef,
    ctx: &ServerContext,
    data: InteractiveTerminalData,
) -> Result<serde_json::Value> {
    check_login(socket)?;

    debug!(
        "Interactive terminal - Stack: {}, Service: {}, Shell: {}",
        data.stack_name, data.service_name, data.shell
    );

    let endpoint = get_endpoint(socket);
    let stack = Stack::get_stack(ctx.clone().into(), &data.stack_name, endpoint).await?;

    // Default shell if empty
    let shell = if data.shell.is_empty() {
        "sh".to_string()
    } else {
        data.shell.clone()
    };

    // Join container terminal (index 0 for first connection)
    stack
        .join_container_terminal(socket.clone(), &data.service_name, &shell, 0)
        .await?;

    Ok(json!({
        "ok": true
    }))
}

async fn handle_terminal_join(
    socket: &SocketRef,
    _ctx: &ServerContext,
    terminal_name: String,
) -> Result<serde_json::Value> {
    check_login(socket)?;

    let buffer = if let Some(terminal) = Terminal::get_terminal(&terminal_name).await {
        // Join the socket to the terminal's room so it receives live broadcasts
        terminal.join(socket.clone()).await?;
        terminal.get_buffer().await
    } else {
        debug!("No terminal found: {}", terminal_name);
        String::new()
    };

    Ok(json!({
        "ok": true,
        "buffer": buffer
    }))
}

async fn handle_leave_combined_terminal(
    socket: &SocketRef,
    ctx: &ServerContext,
    stack_name: &str,
) -> Result<serde_json::Value> {
    check_login(socket)?;

    debug!("Leave combined terminal - Stack: {}", stack_name);

    let endpoint = get_endpoint(socket);
    let stack = Stack::get_stack(ctx.clone().into(), stack_name, endpoint).await?;
    stack.leave_combined_terminal(socket.clone()).await?;

    Ok(json!({
        "ok": true
    }))
}

async fn handle_terminal_resize(
    socket: &SocketRef,
    _ctx: &ServerContext,
    data: TerminalResizeData,
) -> Result<()> {
    check_login(socket)?;

    info!(
        "Terminal resize: {} ({}x{})",
        data.terminal_name, data.rows, data.cols
    );

    if let Some(terminal) = Terminal::get_terminal(&data.terminal_name).await {
        terminal.set_rows(data.rows).await?;
        terminal.set_cols(data.cols).await?;
    } else {
        return Err(anyhow!("Terminal {} not found", data.terminal_name));
    }

    Ok(())
}

/// Detect the appropriate shell for the system
fn detect_shell() -> String {
    // On Unix, use SHELL env var or default to bash
    #[cfg(unix)]
    {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string())
    }

    // On Windows, use PowerShell
    #[cfg(windows)]
    {
        "powershell.exe".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_terminal_input_deserialize() {
        let json = r#"{"terminalName": "test-term", "cmd": "ls -la\n"}"#;
        let data: TerminalInputData = serde_json::from_str(json).unwrap();
        assert_eq!(data.terminal_name, "test-term");
        assert_eq!(data.cmd, "ls -la\n");
    }

    #[test]
    fn test_interactive_terminal_deserialize() {
        let json = r#"{
            "stackName": "my-stack",
            "serviceName": "web",
            "shell": "/bin/bash"
        }"#;
        let data: InteractiveTerminalData = serde_json::from_str(json).unwrap();
        assert_eq!(data.stack_name, "my-stack");
        assert_eq!(data.service_name, "web");
        assert_eq!(data.shell, "/bin/bash");
    }

    #[test]
    fn test_terminal_resize_deserialize() {
        let json = r#"{"terminalName": "console", "rows": 50, "cols": 120}"#;
        let data: TerminalResizeData = serde_json::from_str(json).unwrap();
        assert_eq!(data.terminal_name, "console");
        assert_eq!(data.rows, 50);
        assert_eq!(data.cols, 120);
    }
}
