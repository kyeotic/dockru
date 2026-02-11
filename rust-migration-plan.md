# Dockru Rust Migration Plan

High-level plan for migrating the Dockru backend from TypeScript/Node.js to Rust.

---

## Migration Strategy

**Approach:** Incremental rewrite with parallel operation capability. The Rust backend will serve the existing Vue frontend via the same Socket.io protocol.

**Key Constraint:** Frontend remains unchanged. All Socket.io events and response formats must match exactly.

---

## Phase 1: Project Setup & Infrastructure

**Goal:** Bootstrap Rust project with core dependencies and build pipeline.

### Tasks
- Initialize Cargo workspace
- Set up project structure mirroring TypeScript layout
- Configure dependencies:
  - `tokio` — async runtime
  - `socketioxide` — Socket.io server for Rust
  - `axum` or `warp` — lightweight HTTP server (static file serving only)
  - `tower-http` — static file middleware with compression
  - `serde` + `serde_json` — serialization
  - `anyhow` / `thiserror` — error handling
  - `tracing` / `env_logger` — logging
  - `clap` — CLI argument parsing
  - `config` / `figment` — configuration management
- Set up development tooling (clippy, rustfmt, cargo-watch)
- Create basic HTTP server that serves static files from `frontend-dist/`
- Implement graceful shutdown

### Success Criteria
- Rust server can serve the Vue frontend
- Environment variables and CLI args parsed correctly
- Logging configured

---

## Phase 2: Core Utilities & Shared Code

**Goal:** Implement shared utilities that other modules depend on.

### Tasks
- Port `common/util-common.ts` constants (status codes, terminal dimensions, etc.)
- Implement utility functions:
  - `gen_secret()` — crypto random string generation
  - `sleep()` — async delays
  - `int_hash()` — simple string hash
  - Terminal naming functions
  - YAML comment preservation helpers
  - Docker port parsing
  - `envsubst()` — environment variable substitution
- Create custom error types (`ValidationError`, etc.)
- Port `LimitQueue` (fixed-size circular buffer)
- Set up shared types module (`LooseObject`, `BaseRes`, etc.)

### Success Criteria
- All utility functions have unit tests
- Utilities are idiomatic Rust (using Result, Option, etc.)

---

## Phase 3: Database Layer & Models

**Goal:** Replicate SQLite database access with models.

### Tasks
- Choose ORM or query builder (`sqlx` recommended for compile-time SQL checking)
- Set up database connection with:
  - WAL journal mode
  - 12MB cache
  - Auto-vacuum incremental
  - Synchronous normal
- Implement migration system (sqlx has built-in support)
- Port existing migrations:
  - User table
  - Setting table  
  - Agent table
- Create models:
  - `User` — with bcrypt password methods, JWT creation, 2FA support
  - `Setting` — with in-memory cache (60s TTL)
  - `Agent` — with endpoint parsing
- Implement Settings cache with automatic cleanup
- Write integration tests for DB operations

### Success Criteria
- Database schema matches TypeScript version
- All migrations run successfully
- Models can CRUD operations
- Settings cache works correctly

---

## Phase 4: Authentication & Security

**Goal:** Implement JWT, password hashing, rate limiting, and Socket.io auth middleware.

### Tasks
- Port `password-hash.ts`:
  - bcrypt hashing (use `bcrypt` crate)
  - shake256 password fingerprint (use `sha3` crate)
- Implement JWT token creation/verification:
  - Payload: `{ username, h }` where h = shake256(password)
  - Store secret in settings table
  - Token validation with password hash check
- Implement rate limiting (use `governor` crate):
  - Login rate limiter: 20/min
  - 2FA rate limiter: 30/min
  - API rate limiter: 60/min
- Create Socket.io authentication middleware:
  - `check_login()` — verify socket is authenticated
  - Callback error helper
- Port 2FA/TOTP support (use `totp-lite` crate)
- Implement Socket.io origin check (Host header validation)
- Handle auth-disabled mode (auto-login)

### Success Criteria
- Password hashing matches bcrypt(10) output
- JWT tokens are compatible with existing tokens (if migrating live)
- Rate limiting works per-IP
- Auth middleware properly rejects unauthenticated requests

---

## Phase 5: Terminal/PTY System

**Goal:** Replicate the three-tier terminal system with PTY support.

### Tasks
- Choose PTY library (`portable-pty` recommended)
- Implement base `Terminal` struct:
  - PTY spawning with configurable rows/cols
  - Output buffering (circular buffer, last 100 chunks)
  - Socket client join/leave tracking
  - Broadcast `terminalWrite` and `terminalExit` events to joined clients
  - Auto-kick disconnected clients (60s interval)
  - Optional keep-alive (close if no clients for 60s)
  - Static registry: `HashMap<String, Arc<Mutex<Terminal>>>`
  - `exec()` — one-shot command execution returning exit code
- Implement `InteractiveTerminal` (extends Terminal):
  - Add `write(input)` method for user input to PTY
- Implement `MainTerminal` (extends InteractiveTerminal):
  - Spawn bash/pwsh in stacks directory
  - Gate behind `enableConsole` config
- Implement terminal naming convention helpers
- Add PTY resize support

### Success Criteria
- Terminals can run docker commands and capture output
- Multiple sockets can join same terminal
- Interactive terminals accept user input
- Terminal output buffer works correctly
- Cleanup tasks run properly

---

## Phase 6: Stack Management Core

**Goal:** Port the Stack class and all Docker Compose operations.

### Tasks
- Implement `Stack` struct with:
  - name, status, path, compose file detection
  - Lazy-loaded composeYAML and composeENV
  - Validation (name must be `[a-z0-9_-]+`)
- Implement Docker CLI operations via PTY:
  - `deploy()` — `docker compose up -d --remove-orphans`
  - `start()` — same as deploy
  - `stop()` — `docker compose stop`
  - `restart()` — `docker compose restart`
  - `down()` — `docker compose down`
  - `update()` — `docker compose pull` then `up -d` if running
  - `delete()` — `docker compose down --remove-orphans` + `rm -rf`
  - `ps()` — `docker compose ps --format json` (via child process, not PTY)
  - `get_service_status_list()` — parse JSON output per-service
  - `join_combined_terminal()` — `docker compose logs -f --tail 100`
  - `join_container_terminal()` — `docker compose exec <service> <shell>`
- Implement static methods:
  - `get_stack_list()` — scan directory + merge with `docker compose ls`
  - `get_status_list()` — run `docker compose ls --all --format json`
  - `get_stack()` — load single stack
  - `compose_file_exists()` — check for accepted filenames
  - `status_convert()` — parse docker status strings
- Implement Stack serialization:
  - `to_json()` — full representation
  - `to_simple_json()` — lightweight list view
- Handle global.env and per-stack .env files
- Parse and preserve YAML comments

### Success Criteria
- All Docker operations work correctly
- Stack list merges managed and unmanaged stacks
- Status parsing handles all docker status strings
- YAML parsing preserves comments and formatting

---

## Phase 7: Socket.io Event Handlers

**Goal:** Implement all Socket.io events matching TypeScript behavior exactly.

### Tasks

#### Authentication Events (main-socket-handler)
- `setup` — first-time user creation
- `login` — username/password + optional 2FA
- `loginByToken` — JWT re-authentication
- `changePassword` — update password, invalidate all tokens
- `disconnectOtherSocketClients` — force logout other sessions

#### Settings Events (main-socket-handler)
- `getSettings` — retrieve settings + global.env
- `setSettings` — update settings + global.env file
- `composerize` — convert docker run to compose (use external binary or port JS library)

#### Stack Management Events (docker-socket-handler)
- `deployStack` — save and deploy
- `saveStack` — save without deploying
- `deleteStack` — down and remove
- `getStack` — get full stack details
- `requestStackList` — trigger stack list broadcast
- `startStack` — start stopped stack
- `stopStack` — stop running stack
- `restartStack` — restart stack
- `updateStack` — pull images and restart
- `downStack` — remove containers
- `serviceStatusList` — per-service status and ports
- `getDockerNetworkList` — list docker networks

#### Terminal Events (terminal-socket-handler)
- `terminalInput` — send input to interactive terminal
- `mainTerminal` — open system shell
- `checkMainTerminal` — check if console enabled
- `interactiveTerminal` — open container shell
- `terminalJoin` — attach to existing terminal
- `leaveCombinedTerminal` — detach from logs
- `terminalResize` — resize PTY dimensions

#### Agent Management Events
- `addAgent` — register remote instance
- `removeAgent` — unregister remote instance
- `agent` — proxy event to specific endpoint or broadcast

#### Server-to-Client Broadcasts
- `info` — version, hostname info
- `stackList` — broadcast stack list
- `setup` — trigger setup UI
- `refresh` — force disconnect
- `autoLogin` — auth disabled
- `agentList` — broadcast agent list
- `agentStatus` — agent connection state
- `terminalWrite` — PTY output
- `terminalExit` — PTY exit

### Success Criteria
- All events respond with exact same JSON shape as TypeScript
- Error handling matches TypeScript behavior
- All events properly authenticated
- Callbacks follow `{ ok, ...data }` / `{ ok: false, msg }` pattern

---

## Phase 8: Agent Management System

**Goal:** Replicate multi-instance federation for managing remote Dockge instances.

### Tasks
- Implement `AgentManager` struct:
  - One per client socket connection
  - Load agents from database on login
  - Establish Socket.io client connections to remote instances
  - Authenticate with stored credentials
  - Relay events bidirectionally
  - Version compatibility check (>= 1.4.0)
  - Connection retry with 10s window + 1s polling
- Implement agent event proxying:
  - `emit_to_endpoint()` — proxy to specific agent with retry
  - Handle broadcast to all endpoints
  - Handle local endpoint routing
- Emit `agentStatus` events (connecting/online/offline)
- Store agent credentials in database (plaintext passwords)
- Handle agent add/remove operations
- Implement connection cleanup on disconnect

### Success Criteria
- Can connect to remote Dockge instances
- Events proxy correctly to remote agents
- Agent list updates propagate to clients
- Connection state tracked accurately

---

## Phase 9: HTTP Routes & Frontend Serving

**Goal:** Serve the Vue frontend and handle SPA routing.

### Tasks
- Implement HTTP routes:
  - `GET /` — serve index.html
  - `GET /robots.txt` — return robots.txt content
  - `GET /*` — SPA fallback, serve index.html
- Serve static files from `frontend-dist/` with:
  - Brotli compression support
  - Gzip compression support
  - Proper MIME types
  - Caching headers
- Implement SSL/TLS support:
  - Load SSL cert and key from config
  - Support passphrase-protected keys
- Configure server binding (hostname + port)
- Implement trust proxy support (X-Forwarded-For)

### Success Criteria
- Frontend loads and renders correctly
- All frontend assets served properly
- SPA routing works (all routes serve index.html)
- robots.txt returns correct content
- Compression works for supported browsers
- SSL works when configured

---

## Phase 10: Scheduled Tasks & Final Integration

**Goal:** Implement cron jobs and complete the migration.

### Tasks
- Implement scheduled tasks:
  - Stack list broadcast every 10 seconds (use `tokio::time::interval`)
  - Version check every 48 hours
  - Settings cache cleanup every 60 seconds
  - Terminal client cleanup every 60 seconds
  - Terminal keep-alive check every 60 seconds
- Implement version checking:
  - Fetch from `https://dockge.kuma.pet/version`
  - Compare with current version
  - Broadcast to clients
- Integration testing:
  - Test complete workflows (create, deploy, update, delete stacks)
  - Test authentication flows
  - Test terminal interactions
  - Test agent management
  - Load testing
- Performance optimization:
  - Profile memory usage
  - Optimize hot paths
  - Add connection pooling if needed
- Documentation:
  - Update README with Rust build instructions
  - Document configuration changes
  - Create migration guide for users
- Migration cutover plan:
  - Database compatibility verification
  - Backup procedures
  - Rollback strategy
  - Monitoring setup

### Success Criteria
- All scheduled tasks run on time
- Complete end-to-end workflows work
- Performance meets or exceeds TypeScript version
- No memory leaks
- Clean shutdown works properly
- Documentation complete

---

## Technical Decisions to Make

### 1. Async Runtime
**Recommendation:** `tokio` — most mature, best ecosystem support

### 2. HTTP Server
**Recommendation:** `axum` — minimal, just need static file serving and Socket.io

### 3. Socket.io Library
**Recommendation:** `socketioxide` — most complete Rust Socket.io implementation

### 4. Database Library  
**Recommendation:** `sqlx` — compile-time query checking, async, good SQLite support

### 5. PTY Library
**Recommendation:** `portable-pty` — cross-platform, actively maintained

### 6. YAML Library
**Recommendation:** `serde_yaml` — de-facto standard, but comment preservation may require custom handling

### 7. Composerize
**Decision needed:** 
- Option A: Shell out to Node.js `composerize` package
- Option B: Port composerize logic to Rust
- Option C: Find/create Rust equivalent library

---

## Risk Mitigation

### Compatibility Risks
- **Risk:** Socket.io protocol differences between socketioxide and socket.io
- **Mitigation:** Extensive integration testing with actual frontend, fallback to raw WebSocket if needed

### Comment Preservation
- **Risk:** YAML libraries may not preserve comments
- **Mitigation:** Research custom YAML handling, consider line-by-line manipulation for edits

### PTY Cross-Platform
- **Risk:** PTY behavior differs between Linux/Mac/Windows
- **Mitigation:** Test on all platforms early, use portable-pty abstractions

### Migration Downtime
- **Risk:** Database schema changes could cause downtime
- **Mitigation:** Ensure backward compatibility, add migration validation

---

## Success Metrics

1. **Functional Parity:** All TypeScript features work identically in Rust
2. **Performance:** Response times ≤ TypeScript version, memory usage reduced
3. **Stability:** No crashes, proper error handling, clean shutdown
4. **Code Quality:** Full test coverage, clippy-clean, well-documented
5. **User Impact:** Zero breaking changes for existing users

---

## Next Steps

1. Review and approve this plan
2. Expand Phase 1 into detailed tasks
3. Set up development environment
4. Begin Phase 1 implementation
