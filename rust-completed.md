---

### 1.2 Interactive Container Terminals

**Status:** ✅ Implemented  
**Location:** [src/stack.rs](src/stack.rs#L685) | [src/socket_handlers/terminal.rs](src/socket_handlers/terminal.rs#L218)

**Current Behavior:**
- `interactiveTerminal` socket event creates interactive shell in running containers
- Can open shell in running containers via `docker compose exec`
- Supports multiple shell types (bash, sh, ash, etc.)
- Reuses existing terminal sessions when reconnecting

**Implementation:**
- `Stack::join_container_terminal(socket, service_name, shell, index)` method
- Executes `docker compose exec <service> <shell>` in interactive PTY
- Defaults to "sh" if no shell specified
- Supports multiple concurrent terminals via index parameter
- Terminal input/output piping via Socket.io events

**Testing Notes:**
- Test with different shells (bash, sh, ash for Alpine)
- Test reconnection to existing terminal sessions
- Test multiple simultaneous connections to different services
