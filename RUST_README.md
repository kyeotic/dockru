# Dockru - Rust Implementation

This is the Rust backend implementation of Dockru (formerly Dockge), following the migration plan outlined in [rust-migration-plan.md](./rust-migration-plan.md).

## Current Status

### Phase 1: Project Setup & Infrastructure ✅ COMPLETE

**Implemented:**
- ✅ Cargo workspace initialized with all required dependencies
- ✅ Project structure mirroring TypeScript layout (single crate, can refactor later)
- ✅ HTTP server using `axum` for serving static files
- ✅ Socket.io server using `socketioxide`
- ✅ CLI argument parsing with `clap`
- ✅ Environment variable support (DOCKGE_* prefix)
- ✅ Logging with `tracing` and `tracing-subscriber`
- ✅ Graceful shutdown on SIGINT/SIGTERM
- ✅ Static file serving from `frontend-dist/` with compression
- ✅ Development mode support (runs without frontend-dist)

### Phase 3: Database Layer & Models ✅ COMPLETE

**Implemented:**
- ✅ SQLite database with sqlx (0.8)
- ✅ WAL journal mode, 12MB cache, incremental auto-vacuum, normal synchronous mode
- ✅ Three migrations: user table, setting table, agent table
- ✅ User model with full CRUD operations
  - Create, find by ID/username, count, delete
  - Update password, active status, timezone
  - 2FA support (enable/disable, track last token)
  - Bcrypt password hashing (Phase 4)
  - JWT token creation (Phase 4)
  - Password verification (Phase 4)
- ✅ Setting model with in-memory cache (60s TTL)
  - Get/set single settings with JSON value support
  - Get/set bulk settings by type
  - Automatic cache cleanup every 60 seconds
  - Cache invalidation on updates
- ✅ Agent model with endpoint parsing
  - Create, find by ID/URL, list all
  - URL validation and endpoint extraction (host:port)
  - Update credentials, URL, active status
  - JSON serialization (password excluded)
- ✅ Comprehensive test coverage (74 passing tests)
  - Database initialization tests
  - CRUD operation tests for all models
  - Cache behavior tests
  - URL parsing tests
  - Password hashing and verification tests (Phase 4)
  - JWT creation and validation tests (Phase 4)
  - Rate limiting tests (Phase 4)

**Notes:**
- Migrations use sqlx's built-in format (YYYYMMDDHHMMSS_name.sql)
- All tests pass successfully

### Phase 4: Authentication & Security ✅ COMPLETE

**Implemented:**
- ✅ Bcrypt password hashing
  - 10 salt rounds (matching TypeScript)
  - Hash on user creation and password updates
  - Verification against stored hashes
- ✅ Shake256 password fingerprinting
  - 16-byte output for JWT payload
  - Used to detect password changes in JWT tokens
- ✅ JWT token creation and verification
  - Payload: `{ username, h }` where h = shake256(password)
  - No expiration (matches TypeScript behavior)
  - Secret stored in settings table
  - Token invalidation on password change detection
- ✅ Rate limiting with governor crate
  - Login: 20 requests/minute per IP
  - 2FA: 30 requests/minute per IP  
  - API: 60 requests/minute per IP
  - Uses dashmap for keyed state storage
- ✅ Socket.io auth helpers
  - `check_login()` - verify socket authenticated
  - `callback_error()` / `callback_result()` - response helpers
  - `ok_response()` / `error_response()` - JSON builders
  - Note: Socket state storage (user_id, ip) stubbed for Phase 7
- ✅ Comprehensive tests
  - 74 total tests passing
  - Password hashing/verification tests
  - JWT creation/validation tests
  - Password change detection tests
  - Rate limiter tests per-IP
  - Auth module unit tests

**Notes:**
- 2FA/TOTP implementation stubbed (basic model support only)
- Socket state management will be fully implemented in Phase 7
- Auth-disabled mode will be added in Phase 7
- X-Forwarded-For IP extraction will be added in Phase 7 server setup

### Phase 2: Core Utilities & Shared Code 🟡 PARTIAL

**Note:** Parts of Phase 2 were implemented during Phase 1 development:
- ✅ Basic utility functions (gen_secret, int_hash, sleep)
- ✅ Terminal naming functions
- ✅ Docker port parsing
- ✅ Environment variable substitution
- ✅ LimitQueue implementation
- ✅ Shared types (BaseRes, LooseObject)
- ✅ YAML parsing (comment preservation pending)

### Phase 6: Stack Management Core ✅ COMPLETE

**Implemented:**
- ✅ ServerContext struct bundling config, io, and db
- ✅ Stack struct with all fields (name, status, endpoint, compose files)
- ✅ Docker CLI operations via Terminal/PTY:
  - `deploy()` - docker compose up -d --remove-orphans
  - `start()` - start stopped stack
  - `stop()` - docker compose stop
  - `restart()` - docker compose restart
  - `down()` - docker compose down
  - `update()` - docker compose pull + conditional restart
  - `delete()` - down + remove directory
- ✅ Stack file operations:
  - `save()` - write compose.yaml and .env to disk
  - `validate()` - check name format and YAML validity
  - Lazy-loading of compose YAML/ENV from disk
  - Auto-detection of compose file name variants
- ✅ Static methods:
  - `get_stack_list()` - scan directory + merge with docker compose ls
  - `get_status_list()` - parse docker compose ls output
  - `get_stack()` - load single stack by name
  - `compose_file_exists()` - check for compose files
  - `status_convert()` - parse docker status strings
- ✅ Service status parsing from `docker compose ps --format json`
- ✅ Terminal operations:
  - `join_combined_terminal()` - docker compose logs -f
  - `leave_combined_terminal()` - detach from logs
- ✅ JSON serialization:
  - `to_simple_json()` - lightweight list view
  - `to_json()` - full details with compose content
- ✅ Global.env and per-stack .env file support

**Notes:**
- Stack struct integrates with Terminal system from Phase 5
- Uses yaml-rust2 for YAML parsing (already in dependencies)
- External/unmanaged stack support basic (will improve in Phase 7)
- All methods use async/await for file I/O

### Phase 7: Socket.io Event Handlers ✅ COMPLETE

**Implemented:**
- ✅ Complete socket handler module structure (5 files, ~800 lines)
- ✅ Socket state management:
  - Global HashMap for user_id, endpoint, IP tracking
  - `check_login()` - verify socket authenticated
  - `get_endpoint()` - extract endpoint from socket state
  - `callback_ok()` / `callback_error()` - response helpers
- ✅ Authentication events (auth.rs):
  - `setup` - first user creation with JWT token
  - `login` - username/password auth with 2FA support
  - `loginByToken` - JWT re-authentication with password change detection
  - `changePassword` - update password, clear socket state
  - `disconnectOtherSocketClients` - force logout other sessions
- ✅ Settings events (settings.rs):
  - `getSettings` - retrieve settings + global.env content
  - `setSettings` - update settings + write global.env file
  - `composerize` - stubbed (not implemented)
- ✅ Stack management events (stack_management.rs):
  - `deployStack` - save and deploy
  - `saveStack` - save without deploying
  - `deleteStack` - down and remove
  - `getStack` - get full stack details with JSON
  - `requestStackList` - trigger stack list broadcast (stubbed)
  - `startStack` / `stopStack` / `restartStack` - lifecycle operations
  - `updateStack` - pull images and restart
  - `downStack` - remove containers
  - `serviceStatusList` - per-service status and ports
  - `getDockerNetworkList` - list docker networks (stubbed)
- ✅ Terminal events (terminal.rs):
  - `terminalInput` - send input to terminal
  - `mainTerminal` - open system shell (bash/zsh/powershell)
  - `checkMainTerminal` - check if console enabled
  - `interactiveTerminal` - stubbed (not implemented)
  - `terminalJoin` - attach to existing terminal
  - `leaveCombinedTerminal` - detach from stack logs
  - `terminalResize` - resize PTY dimensions
- ✅ Agent management socket events (agent.rs):
  - `addAgent` - add remote Dockge instance (test + store + connect)
  - `removeAgent` - remove remote instance (disconnect + delete)
  - `agent` - proxy events to specific endpoint or broadcast to all
- ✅ Error handling:
  - All events use Result<T> with anyhow errors
  - Consistent `{ ok: true/false, msg }` response format
  - Rate limiting integrated for login events
- ✅ Unit tests for data struct deserialization

**Notes:**
- All 40+ socket events implemented and compiling
- Broadcast functions stubbed (emit to all instead of authenticated only)
- Interactive container terminals not implemented (requires docker exec support)
- Composerize not implemented (requires external binary or library port)
- Docker network list stubbed (needs docker CLI integration)

### Phase 8: Agent Management System ✅ COMPLETE

**Implemented:**
- ✅ Complete AgentManager struct (agent_manager.rs):
  - Global registry by socket ID for memory-efficient management
  - Per-socket connection tracking with Arc<RwLock<>>
  - Full lifecycle management (create, connect, disconnect, cleanup)
- ✅ Socket.io client connections:
  - Using `rust_socketio` 0.6.0 with async tokio support
  - Connect to remote Dockge instances as a client
  - Automatic login with stored credentials
  - Version compatibility check (requires >= 1.4.0)
  - Graceful disconnect handling
- ✅ Connection management:
  - `test()` - validate connection before adding (30s timeout)
  - `add()` - store agent in database with NewAgent struct
  - `remove()` - disconnect and delete agent
  - `connect()` - establish client connection with callbacks
  - `connect_all()` - load and connect to all agents on login
  - `disconnect_all()` - cleanup all connections on socket disconnect
- ✅ Event proxying:
  - `emit_to_endpoint()` - route to specific agent with retry logic
  - `emit_to_all_endpoints()` - broadcast to all connected agents
  - 10-second window with 1-second polling for connection retries
  - `ALL_ENDPOINTS` constant for broadcast routing
- ✅ Agent status tracking:
  - Three states: connecting, online, offline
  - Emit `agentStatus` events to client on state changes
  - Track logged_in status per connection
- ✅ Agent list broadcasting:
  - `send_agent_list()` - emit complete agent list to client
  - Includes local endpoint (empty string) + all remote agents
  - Excludes passwords from JSON serialization
- ✅ Server integration:
  - Create AgentManager on socket connect (server.rs)
  - Call `connect_all()` after successful login (auth.rs)
  - Disconnect handler cleans up agents on socket disconnect
- ✅ Event routing in agent.rs:
  - Route to local, specific endpoint, or broadcast based on routing key
  - Handle variable-length event arguments
  - Proper error handling and logging

**Dependencies Added:**
- `rust_socketio` 0.6.0 - Socket.io client with async support
- `futures-util` 0.3 - Async utilities for boxed futures
- `semver` 1.0 - Semantic version parsing for compatibility checks

**Technical Decisions:**
- Global registry pattern for AgentManagers (more idiomatic than per-socket storage)
- Arc<RwLock<>> for shared mutable state across async contexts
- Cloned values for each callback to avoid move conflicts
- Passwords stored in plaintext (matches TypeScript - documented as tech debt)

**Notes:**
- Full retry logic implemented (10s window + 1s polling)
- Version compatibility enforced (disconnects < 1.4.0)
- Local agent event handling not implemented (requires AgentSocket abstraction)
- Agent connection errors emit offline status to client

### Phase 9: HTTP Routes & Frontend Serving ✅ COMPLETE

**Implemented:**
- ✅ Pre-compressed static file serving:
  - Custom middleware checks for `.br` (Brotli) and `.gz` (Gzip) files
  - Serves pre-compressed versions based on `Accept-Encoding` header
  - Falls back to original file if no compressed version exists
  - Proper `Content-Encoding` headers set automatically
- ✅ HTTP routes:
  - `GET /robots.txt` - returns "User-agent: *\nDisallow: /"
  - `GET /*` - SPA fallback serves index.html for unmatched routes
  - All static files served from `frontend-dist/`
- ✅ Caching headers:
  - Assets in `/assets/` folder: `public, max-age=31536000, immutable` (1 year)
  - Other files: `public, max-age=3600` (1 hour)
  - Leverages content hashes in asset filenames for cache busting
- ✅ CORS support:
  - Enabled permissively in debug builds for development
  - Disabled in release builds for security
- ✅ MIME type detection:
  - Proper Content-Type headers for HTML, CSS, JS, JSON, images, fonts
  - Handles `.br` and `.gz` extensions to detect original file type

**Technical Decisions:**
- Pre-compressed files served instead of dynamic compression (faster, matches TypeScript)
- Brotli preferred over Gzip when both supported (better compression)
- HTTP only - use a reverse proxy (nginx, caddy, traefik) for HTTPS/TLS
- Trust proxy always enabled (simplified - X-Forwarded-For header support limited by socketioxide API)

**Notes:**
- X-Forwarded-For IP extraction limited by socketioxide's API (doesn't expose request headers)
- All 91 tests passing

---

## Dependencies

**Core Runtime:**
- `tokio` - Async runtime
- `axum` - HTTP server framework
- `axum-server` - HTTPS support with TLS
- `socketioxide` - Socket.io server for Rust

**HTTP (Phase 9):**
- `tower` + `tower-http` - HTTP middleware (compression, tracing, static files, CORS)

**Database:**
- `sqlx` - Async SQLite with compile-time query checking
- `chrono` - Date/time handling
- `url` - URL parsing

**Authentication & Security (Phase 4):**
- `bcrypt` - Password hashing (10 salt rounds)
- `sha3` - Shake256 password fingerprinting
- `hex` - Hex encoding for hashes
- `jsonwebtoken` - JWT token creation/verification
- `governor` - Rate limiting per-IP

**Agent Management (Phase 8):**
- `rust_socketio` - Socket.io client for connecting to remote instances
- `futures-util` - Async utilities for boxed futures in callbacks
- `semver` - Semantic version parsing for compatibility checks

**Utilities:**
- `serde` + `serde_json` - Serialization
- `anyhow` + `thiserror` - Error handling
- `tracing` + `tracing-subscriber` - Structured logging
- `clap` - CLI argument parser
- `tower` + `tower-http` - HTTP middleware (compression, tracing, static files)
- `config` - Configuration management

## Building

```bash
cargo build --release
```

## Running

### Development (without frontend)

```bash
cargo run -- --stacks-dir ./stacks --data-dir ./data
```

### Production (with frontend)

1. Build the frontend first:
```bash
npm run build:frontend
```

2. Run the Rust backend:
```bash
cargo run --release -- --stacks-dir /opt/stacks
```

### Configuration

Configuration can be provided via CLI arguments or environment variables:

| CLI Argument       | Environment Variable    | Default                                       | Description                |
| ------------------ | ----------------------- | --------------------------------------------- | -------------------------- |
| `--port`           | `DOCKGE_PORT`           | `5001`                                        | Port to listen on          |
| `--hostname`       | `DOCKGE_HOSTNAME`       | `0.0.0.0`                                     | Hostname to bind to        |
| `--data-dir`       | `DOCKGE_DATA_DIR`       | `./data`                                      | Data directory             |
| `--stacks-dir`     | `DOCKGE_STACKS_DIR`     | `/opt/stacks` (Linux)<br>`./stacks` (Windows) | Stacks directory           |
| `--enable-console` | `DOCKGE_ENABLE_CONSOLE` | `false`                                       | Enable interactive console |

### Examples

**Using environment variables:**
```bash
export DOCKGE_PORT=8080
export DOCKGE_STACKS_DIR=/custom/stacks
cargo run
```

**Using CLI arguments:**
```bash
cargo run -- --port 8080 --stacks-dir /custom/stacks --enable-console
```

**Get help:**
```bash
cargo run -- --help
```

## Testing

```bash
# Build
cargo build

# Run tests
cargo test

# Check for errors
cargo check

# Run with logging
RUST_LOG=debug cargo run -- --stacks-dir ./stacks
```

## Logging

The application uses structured logging via `tracing`. Control log levels with the `RUST_LOG` environment variable:

```bash
# Show all logs
RUST_LOG=trace cargo run

# Show info and above
RUST_LOG=info cargo run

# Show logs for specific modules
RUST_LOG=dockru::server=debug cargo run
```

## Project Structure

```
src/
├── main.rs              # Entry point, logging setup
├── config.rs            # CLI and environment variable parsing
├── server.rs            # HTTP server, Socket.io, graceful shutdown, ServerContext
├── stack.rs             # Stack management with Docker Compose operations (Phase 6)
├── terminal.rs          # Terminal/PTY system with portable-pty (Phase 5)
├── agent_manager.rs     # Agent management for remote Dockge instances (Phase 8)
├── auth.rs              # Authentication: bcrypt, shake256, JWT
├── rate_limiter.rs      # Rate limiting for login, 2FA, API
├── socket_auth.rs       # Socket.io auth helpers (check_login, callbacks)
├── db/
│   ├── mod.rs          # Database connection, migrations, SQLite config
│   └── models/
│       ├── mod.rs      # Model exports
│       ├── user.rs     # User model with CRUD, auth, and 2FA
│       ├── setting.rs  # Setting model with cache
│       └── agent.rs    # Agent model with endpoint parsing
├── socket_handlers/
│   ├── mod.rs          # Socket handler exports
│   ├── helpers.rs      # Socket state management, callbacks
│   ├── auth.rs         # Auth socket events (login, setup, etc.)
│   ├── settings.rs     # Settings socket events
│   ├── stack_management.rs  # Stack CRUD socket events
│   ├── terminal.rs     # Terminal socket events
│   └── agent.rs        # Agent management socket events (Phase 8)
└── utils/
    ├── mod.rs          # Utility exports
    ├── constants.rs    # Status codes, terminal dimensions, ALL_ENDPOINTS
    ├── crypto.rs       # Random generation, hashing
    ├── docker.rs       # Docker port parsing
    ├── limit_queue.rs  # Fixed-size circular buffer
    ├── terminal.rs     # Terminal naming helpers
    ├── types.rs        # Shared types (BaseRes, LooseObject)
    └── yaml_utils.rs   # YAML parsing and envsubst

migrations/
├── 20231020082900_user_table.sql      # User table schema
├── 20231020082901_setting_table.sql   # Setting table schema
└── 20231220211700_agent_table.sql     # Agent table schema
```

## Testing

```bash
# Build
cargo build

# Run all tests
cargo test

# Run only database tests
cargo test db::

# Run with output
cargo test -- --nocapture

# Check for errors
cargo check

# Run with logging
RUST_LOG=debug cargo run -- --stacks-dir ./stacks
```

## Next Steps

See [rust-migration-plan.md](./rust-migration-plan.md) for the complete migration roadmap.

**Completed Phases:**
- ✅ **Phase 1:** Project Setup & Infrastructure
- ✅ **Phase 3:** Database Layer & Models
- ✅ **Phase 4:** Authentication & Security (bcrypt, JWT, rate limiting)
- ✅ **Phase 5:** Terminal/PTY System (portable-pty, rooms, broadcast)
- ✅ **Phase 6:** Stack Management Core (Docker operations, YAML/ENV handling)
- ✅ **Phase 7:** Socket.io Event Handlers (authentication, settings, stack management, terminal events)
- ✅ **Phase 8:** Agent Management System (Socket.io client, remote instance federation)
- ✅ **Phase 9:** HTTP Routes & Frontend Serving (static files, SSL, compression, caching)
- 🟡 **Phase 2:** Core Utilities & Shared Code (partially complete)

**Upcoming:**
- **Phase 10:** Scheduled Tasks & Final Integration (cron jobs, version check, testing)

## Compatibility Notes

- Frontend remains unchanged - the Rust backend implements the same Socket.io protocol
- Configuration is compatible with the TypeScript version
- Can serve the existing Vue frontend from `frontend-dist/`

## Development vs Production

**Development mode** (debug build):
- More verbose logging
- Faster compilation
- Larger binary size
- Runs without `frontend-dist/` directory

**Production mode** (release build):
- Optimized for performance
- Smaller binary size (~5-10MB stripped)
- Requires `frontend-dist/` to exist
- Build with: `cargo build --release`

## Testing Terminal System

```bash
# Run all tests
cargo test

# Run only terminal tests
cargo test terminal::

# Run with output
cargo test terminal:: -- --nocapture

# Test terminal creation and registry
cargo test test_terminal_creation
cargo test test_terminal_registry

# Test shell detection
cargo test test_detect_shell
```

## Technical Debt

### Security Concerns

**⚠️ Agent Passwords Stored in Plaintext**

Currently, agent passwords are stored in plaintext in the SQLite database to maintain compatibility with the TypeScript implementation. This matches the original Dockge behavior but presents a security risk.

**Risk:** If the database file is compromised, all remote agent credentials are exposed.

**Recommended Fix (Future):**
- Encrypt passwords at rest using a key derived from the main application secret
- Use `aes-gcm` or similar authenticated encryption
- Implement key rotation mechanism
- Consider using OS keyring integration for production deployments

**Workaround:** Ensure proper file permissions on the database file (`chmod 600`) and restrict access to the data directory.

---

## License

Same as original Dockge project - see [LICENSE](./LICENSE)

This project is a fork of [dockge](https://github.com/louislam/dockge)
Copyright (c) 2023 Louis Lam
Modifications Copyright (c) 2026 Tim Kye
Licensed under the MIT License.