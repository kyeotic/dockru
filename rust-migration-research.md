# Dockru Backend — Rust Migration Research

Research document cataloging every component of the existing TypeScript backend.

---

## Table of Contents

1. [Architecture Overview](#architecture-overview)
2. [Runtime & Dependencies](#runtime--dependencies)
3. [HTTP Routes](#http-routes)
4. [Socket.io Events — Authentication](#socketio-events--authentication)
5. [Socket.io Events — Settings](#socketio-events--settings)
6. [Socket.io Events — Stack Management](#socketio-events--stack-management)
7. [Socket.io Events — Terminal](#socketio-events--terminal)
8. [Socket.io Events — Agent Management](#socketio-events--agent-management)
9. [Socket.io Events — Agent Proxy](#socketio-events--agent-proxy)
10. [Server-to-Client Broadcasts](#server-to-client-broadcasts)
11. [Stack Management Core](#stack-management-core)
12. [Terminal / PTY System](#terminal--pty-system)
13. [Database Layer](#database-layer)
14. [Settings System](#settings-system)
15. [Agent Manager (Multi-Instance Federation)](#agent-manager-multi-instance-federation)
16. [Authentication & Security](#authentication--security)
17. [Scheduled Tasks](#scheduled-tasks)
18. [Shared/Common Utilities](#sharedcommon-utilities)
19. [Data Models & Types](#data-models--types)
20. [Configuration](#configuration)
21. [File Layout](#file-layout)

---

## Architecture Overview

Dockru (fork of Dockge) is a Docker Compose stack manager. The backend is a monolithic Node.js server built on:

- **Express** — serves the compiled Vue frontend as static files, plus a robots.txt
- **Socket.io** — the *real* API layer; all client-server interaction is via websocket events
- **SQLite** — stores users, settings, and agent credentials via RedBean ORM + Knex
- **node-pty** — spawns pseudo-terminals for docker commands and interactive shells
- **Child processes** — runs `docker compose` CLI commands for stack operations

There are essentially **zero REST API endpoints** for application logic. The entire API is Socket.io events with callback-based request/response patterns.

### Request/Response Convention

All socket event handlers follow the same pattern:

```
socket.on("eventName", async (params..., callback) => {
  try {
    checkLogin(socket);       // throws if not authenticated
    // ... do work ...
    callback({ ok: true, ...data });
  } catch (e) {
    callbackError(e, callback);  // { ok: false, msg: e.message }
  }
});
```

### Key Architectural Concerns for Rust Migration

- The entire API is Socket.io — Rust has `socketioxide` or could switch to raw WebSocket
- PTY spawning via `node-pty` — Rust can use `portable-pty` or raw `openpty`/`forkpty`
- SQLite via RedBean ORM — Rust can use `rusqlite` or `sqlx`
- Docker interaction is 100% CLI-based (`docker compose` commands), not Docker API
- The frontend expects Socket.io protocol and specific event names/shapes

---

## Runtime & Dependencies

**Runtime:** Node.js >= 22.14.0, ES Modules, TypeScript via `tsx`

### Key Backend Dependencies

| Package | Purpose |
|---|---|
| `express` ~4.21 | HTTP server, static file serving |
| `socket.io` ~4.8 | WebSocket API layer |
| `@louislam/sqlite3` ~15.1 | SQLite native bindings |
| `redbean-node` ~0.3 | ORM layer over SQLite |
| `knex` ~2.5 | SQL query builder (used for migrations) |
| `@homebridge/node-pty-prebuilt-multiarch` | PTY spawning for terminals |
| `jsonwebtoken` ~9.0 | JWT token creation/verification |
| `bcryptjs` ~2.4 | Password hashing |
| `yaml` ~2.3 | YAML parse/stringify (compose files) |
| `croner` ~8.1 | Cron job scheduling |
| `dayjs` ~1.11 | Date/time with timezone |
| `composerize` | Convert `docker run` to compose YAML |
| `dotenv` ~16.3 | .env file parsing |
| `semver` | Version comparison (agent compat check) |
| `limiter-es6-compat` | Rate limiting |
| `promisify-child-process` | Promisified child_process.spawn |
| `http-graceful-shutdown` | Graceful server shutdown |
| `ts-command-line-args` | CLI argument parsing |
| `express-static-gzip` | Serve pre-compressed static files |
| `command-exists` | Check if shell commands exist |
| `@inventage/envsubst` | Environment variable substitution |

---

## HTTP Routes

**File:** `backend/routers/main-router.ts`

Only 3 routes exist, all for serving the frontend:

| Method | Path | Description |
|---|---|---|
| GET | `/` | Serves `frontend-dist/index.html` |
| GET | `/robots.txt` | Returns `User-agent: *\nDisallow: /` |
| GET | `*` | Catch-all SPA fallback — serves index.html |

Static files are served from `frontend-dist/` with brotli/gzip support via `express-static-gzip`.

---

## Socket.io Events — Authentication

**File:** `backend/socket-handlers/main-socket-handler.ts`

### `setup`

- **Auth:** None (only works when no users exist)
- **Params:** `username: string, password: string`
- **Response:** `{ ok, msg, msgi18n }`
- **Intention:** First-time admin account creation
- **Implementation:** Validates password strength, creates user with bcrypt hash, auto-logs in, disables `needSetup` flag

### `login`

- **Auth:** None
- **Rate Limited:** Yes (20/min via `loginRateLimiter`)
- **Params:** `{ username, password, token? }` (token is for 2FA)
- **Response:** `{ ok, token?, tokenRequired?, msg? }`
- **Intention:** Authenticate with username/password, optionally 2FA
- **Implementation:** Finds user by username, verifies bcrypt password, checks 2FA if enabled (TOTP via `notp`), returns signed JWT containing `{ username, h: shake256(password) }`

### `loginByToken`

- **Auth:** None
- **Params:** `token: string` (JWT)
- **Response:** `{ ok, msg? }`
- **Intention:** Re-authenticate with an existing JWT token
- **Implementation:** Verifies JWT signature, checks user still exists and is active, validates password hash in token matches current password (invalidates tokens on password change)

### `changePassword`

- **Auth:** Required
- **Params:** `{ currentPassword, newPassword }`
- **Response:** `{ ok, msg }`
- **Intention:** Change the authenticated user's password
- **Implementation:** Verifies current password, validates new password strength, updates hash, regenerates JWT secret (invalidates all tokens), disconnects all other sessions

### `disconnectOtherSocketClients`

- **Auth:** Required
- **Params:** None
- **Response:** None
- **Intention:** Force-logout all other sessions for the current user
- **Implementation:** Iterates all sockets, emits "refresh" and disconnects any matching `userID` except current socket

---

## Socket.io Events — Settings

**File:** `backend/socket-handlers/main-socket-handler.ts`

### `getSettings`

- **Auth:** Required
- **Params:** None
- **Response:** `{ ok, data: { ...settings, globalENV } }`
- **Intention:** Retrieve all general settings plus the global.env file content
- **Implementation:** Reads settings from DB via `Settings.getSettings("general")`, reads `global.env` file from stacks directory, merges them

### `setSettings`

- **Auth:** Required
- **Params:** `data: { globalENV?, ...otherSettings }, currentPassword: string`
- **Response:** `{ ok, msg }`
- **Intention:** Update application settings and global.env
- **Implementation:** If disabling auth, requires password confirmation via `doubleCheckPassword`. Writes `global.env` file if `globalENV` field present. Persists remaining settings to DB via `Settings.setSettings("general", data)`

### `composerize`

- **Auth:** Required
- **Params:** `dockerRunCommand: string`
- **Response:** `{ ok, composeTemplate: string }`
- **Intention:** Convert a `docker run` command into docker-compose YAML
- **Implementation:** Calls the `composerize` library, returns YAML string

---

## Socket.io Events — Stack Management

**File:** `backend/agent-socket-handlers/docker-socket-handler.ts`

All stack events go through the agent socket system (can be proxied to remote instances).

### `deployStack`

- **Auth:** Required
- **Params:** `name, composeYAML, composeENV, isAdd: boolean`
- **Response:** `{ ok, msg }`
- **Intention:** Save compose files and run `docker compose up -d`
- **Implementation:** Creates `Stack` object, calls `stack.save(isAdd)` (writes compose.yaml + .env to disk), calls `stack.deploy(socket)` (runs `docker compose up -d --remove-orphans` via PTY), then joins combined terminal (log stream), sends updated stack list

### `saveStack`

- **Auth:** Required
- **Params:** `name, composeYAML, composeENV, isAdd: boolean`
- **Response:** `{ ok, msg }`
- **Intention:** Save compose files to disk *without* deploying
- **Implementation:** Creates `Stack`, calls `stack.save(isAdd)`, sends updated stack list

### `deleteStack`

- **Auth:** Required
- **Params:** `name: string`
- **Response:** `{ ok, msg }`
- **Intention:** Stop and remove a stack, delete its files
- **Implementation:** Loads stack via `Stack.getStack()`, runs `docker compose down --remove-orphans` via PTY, then `rm -rf` the stack directory, sends updated stack list

### `getStack`

- **Auth:** Required
- **Params:** `stackName: string`
- **Response:** `{ ok, stack: { name, status, composeYAML, composeENV, primaryHostname, ... } }`
- **Intention:** Get full details of a single stack (including compose file contents)
- **Implementation:** Loads stack, calls `stack.toJSON(endpoint)` which reads compose.yaml and .env from disk, resolves primary hostname. If managed by Dockge, also joins the combined terminal (live log stream)

### `requestStackList`

- **Auth:** Required
- **Params:** None
- **Response:** `{ ok, msg }`
- **Intention:** Request server to broadcast updated stack list to all clients
- **Implementation:** Calls `server.sendStackList()` which scans stacks directory + runs `docker compose ls --all --format json`

### `startStack`

- **Auth:** Required
- **Params:** `stackName: string`
- **Response:** `{ ok, msg }`
- **Intention:** Start a stopped stack
- **Implementation:** Loads stack, runs `docker compose up -d --remove-orphans` via PTY, joins combined terminal, sends updated stack list

### `stopStack`

- **Auth:** Required
- **Params:** `stackName: string`
- **Response:** `{ ok, msg }`
- **Intention:** Stop a running stack (keeps containers)
- **Implementation:** Loads stack, runs `docker compose stop` via PTY, sends updated stack list

### `restartStack`

- **Auth:** Required
- **Params:** `stackName: string`
- **Response:** `{ ok, msg }`
- **Intention:** Restart a stack
- **Implementation:** Loads stack, runs `docker compose restart` via PTY, sends updated stack list

### `updateStack`

- **Auth:** Required
- **Params:** `stackName: string`
- **Response:** `{ ok, msg }`
- **Intention:** Pull latest images and restart if running
- **Implementation:** Loads stack, runs `docker compose pull` via PTY. If stack was running, also runs `docker compose up -d --remove-orphans`. Sends updated stack list

### `downStack`

- **Auth:** Required
- **Params:** `stackName: string`
- **Response:** `{ ok, msg }`
- **Intention:** Remove stack containers (`docker compose down`)
- **Implementation:** Loads stack, runs `docker compose down` via PTY, sends updated stack list

### `serviceStatusList`

- **Auth:** Required
- **Params:** `stackName: string`
- **Response:** `{ ok, serviceStatusList: Record<string, { state, ports[] }> }`
- **Intention:** Get per-service status and exposed ports for a stack
- **Implementation:** Runs `docker compose ps --format json`, parses JSON output per line, extracts `Service`, `State`/`Health`, and `Ports` (filtered to only port mappings with `->`)

### `getDockerNetworkList`

- **Auth:** Required
- **Params:** None
- **Response:** `{ ok, dockerNetworkList: string[] }`
- **Intention:** List available Docker networks
- **Implementation:** Runs `docker network ls --format "{{.Name}}"`, splits by newline, sorts alphabetically

---

## Socket.io Events — Terminal

**File:** `backend/agent-socket-handlers/terminal-socket-handler.ts`

### `terminalInput`

- **Auth:** Required
- **Params:** `terminalName: string, cmd: string`
- **Response:** Error callback only
- **Intention:** Send user input to an interactive terminal session
- **Implementation:** Looks up terminal by name, validates it's an `InteractiveTerminal`, writes input to PTY

### `mainTerminal`

- **Auth:** Required
- **Params:** `terminalName: string` (ignored, forced to "console")
- **Response:** `{ ok, msg? }`
- **Intention:** Open a system shell (bash/powershell) in the stacks directory
- **Implementation:** Requires `enableConsole` config flag. Creates or gets a `MainTerminal` (which spawns bash/pwsh), stores reference on socket as `socket.consoleTerminal`, joins socket to terminal, starts terminal

### `checkMainTerminal`

- **Auth:** Required
- **Params:** None
- **Response:** `{ ok: boolean }`
- **Intention:** Check if the console terminal feature is enabled
- **Implementation:** Returns `server.config.enableConsole`

### `interactiveTerminal`

- **Auth:** Required
- **Params:** `stackName, serviceName, shell: string` (e.g. "/bin/bash")
- **Response:** `{ ok, msg? }`
- **Intention:** Open an interactive shell inside a running container
- **Implementation:** Generates terminal name from endpoint+stack+service+index, creates `InteractiveTerminal` running `docker compose exec <service> <shell>`, joins socket, starts

### `terminalJoin`

- **Auth:** Required
- **Params:** `terminalName: string`
- **Response:** `{ ok, buffer: string }`
- **Intention:** Attach to an existing terminal and receive its buffered output
- **Implementation:** Looks up terminal by name, joins socket, returns `terminal.getBuffer()` (last 100 output chunks)

### `leaveCombinedTerminal`

- **Auth:** Required
- **Params:** `stackName: string`
- **Response:** `{ ok, msg? }`
- **Intention:** Detach from a stack's combined log stream
- **Implementation:** Computes combined terminal name, calls `terminal.leave(socket)`

### `terminalResize`

- **Auth:** Required
- **Params:** `terminalName: string, rows: number, cols: number`
- **Response:** None (no callback)
- **Intention:** Resize a terminal's PTY dimensions
- **Implementation:** Looks up terminal, sets `terminal.rows` and `terminal.cols` which triggers `ptyProcess.resize()`

---

## Socket.io Events — Agent Management

**File:** `backend/socket-handlers/manage-agent-socket-handler.ts`

### `addAgent`

- **Auth:** Required
- **Params:** `{ url, username, password }`
- **Response:** `{ ok, msg }`
- **Intention:** Register a remote Dockge instance as an agent
- **Implementation:** Tests connection by connecting via Socket.io client and attempting login. On success, stores credentials in DB via `AgentManager.add()`, connects to agent, disconnects all other socket clients to force UI refresh

### `removeAgent`

- **Auth:** Required
- **Params:** `url: string`
- **Response:** `{ ok, msg }`
- **Intention:** Unregister a remote Dockge agent
- **Implementation:** Calls `AgentManager.remove(url)` which deletes from DB and disconnects. Disconnects other clients for UI refresh

---

## Socket.io Events — Agent Proxy

**File:** `backend/socket-handlers/agent-proxy-socket-handler.ts`

### `agent`

- **Auth:** Required
- **Params:** `endpoint: string, eventName: string, ...args: unknown[]`
- **Response:** Depends on proxied event
- **Intention:** Route a request to a specific agent endpoint or broadcast to all
- **Implementation:**
  - If `endpoint === "##ALL_DOCKGE_ENDPOINTS##"` — broadcast to all agents AND call locally
  - If `endpoint` is empty or matches this server's endpoint — call local `agentSocket`
  - Otherwise — proxy via `instanceManager.emitToEndpoint(endpoint, eventName, ...args)`

---

## Server-to-Client Broadcasts

Events the server pushes to clients (not request/response):

| Event | Payload | Trigger |
|---|---|---|
| `info` | `{ version, latestVersion, isContainer, primaryHostname }` | On connect, after login |
| `stackList` | `{ ok, stackList: Record<string, StackSimpleJSON> }` | Every 10s cron, after stack operations |
| `setup` | (none) | On connect when `needSetup = true` |
| `refresh` | (none) | Before force-disconnecting a socket |
| `autoLogin` | (none) | On connect when auth is disabled |
| `agentList` | `{ ok, agentList: Record<string, AgentJSON> }` | After login, after agent add/remove |
| `agentStatus` | `{ endpoint, status: "connecting"\|"online"\|"offline", msg? }` | On agent connection state change |
| `terminalWrite` | `terminalName, data: string` | When PTY produces output |
| `terminalExit` | `terminalName, exitCode: number` | When PTY process exits |

---

## Stack Management Core

**File:** `backend/stack.ts` (556 lines)

The `Stack` class is the central abstraction for Docker Compose projects.

### Data

- `name` — stack name (directory name, validated as `[a-z0-9_-]+`)
- `_status` — numeric status: UNKNOWN(0), CREATED_FILE(1), CREATED_STACK(2), RUNNING(3), EXITED(4)
- `_composeYAML` — lazy-loaded contents of compose.yaml
- `_composeENV` — lazy-loaded contents of .env
- `_composeFileName` — detected compose file name (supports compose.yaml, docker-compose.yaml, docker-compose.yml, compose.yml)
- `path` — `{stacksDir}/{name}`

### Docker CLI Interaction

All docker operations shell out to the `docker` CLI via PTY. The `getComposeOptions()` method builds args:

```
docker compose [--env-file ../global.env] [--env-file ./.env] <command> [options]
```

Global env file is included if `{stacksDir}/global.env` exists. Per-stack `.env` is included if present.

### Operations

| Method | Docker Command | Notes |
|---|---|---|
| `deploy()` | `docker compose up -d --remove-orphans` | Via PTY |
| `start()` | `docker compose up -d --remove-orphans` | Same as deploy |
| `stop()` | `docker compose stop` | Via PTY |
| `restart()` | `docker compose restart` | Via PTY |
| `down()` | `docker compose down` | Via PTY |
| `update()` | `docker compose pull` then `up -d` if running | Via PTY |
| `delete()` | `docker compose down --remove-orphans` then `rm -rf dir` | Via PTY + fs |
| `ps()` | `docker compose ps --format json` | Via child_process |
| `getServiceStatusList()` | `docker compose ps --format json` | Parses per-service status |
| `joinCombinedTerminal()` | `docker compose logs -f --tail 100` | Persistent PTY with keep-alive |
| `joinContainerTerminal()` | `docker compose exec <service> <shell>` | Interactive PTY |

### Static Methods

- `getStackList(server, useCacheForManaged?)` — Scans stacks dir for compose files + runs `docker compose ls --all --format json` to merge managed and unmanaged stacks
- `getStatusList()` — Runs `docker compose ls --all --format json`, returns `Map<name, status>`
- `getStack(server, name)` — Loads a single stack from disk or from cached list
- `composeFileExists(dir, name)` — Checks if any accepted compose filename exists in directory
- `statusConvert(statusString)` — Converts docker status strings like "running(2)" to numeric constants

### Stack Serialization

- `toJSON(endpoint)` — Full representation with `composeYAML`, `composeENV`, `primaryHostname`
- `toSimpleJSON(endpoint)` — Lightweight: `name`, `status`, `tags:[]`, `isManagedByDockge`, `composeFileName`, `endpoint`

---

## Terminal / PTY System

**File:** `backend/terminal.ts` (294 lines)

Three terminal classes form a hierarchy:

### `Terminal` (base)

Non-interactive PTY for running docker commands (deploy, stop, logs, etc.)

- Spawns PTY via `node-pty` (`pty.spawn(file, args, { cwd, cols, rows })`)
- Maintains output buffer (`LimitQueue<string>` capped at 100 items)
- Supports multiple socket clients joining/leaving the same terminal
- Auto-kicks disconnected clients every 60s
- Optional keep-alive: closes terminal if no clients for 60s
- Broadcasts `terminalWrite` events to all joined sockets on PTY output
- Broadcasts `terminalExit` events on PTY exit
- Static registry: `terminalMap: Map<string, Terminal>`
- `Terminal.exec()` — one-shot command execution, returns exit code via Promise

### `InteractiveTerminal` (extends Terminal)

Adds `write(input)` method for sending user input to the PTY. Used for `docker compose exec` (container shells).

### `MainTerminal` (extends InteractiveTerminal)

System shell access. Spawns `bash` (Linux) or `pwsh.exe`/`powershell.exe` (Windows) in the stacks directory. Gated behind `enableConsole` config flag.

### Terminal Naming Convention

- Compose operations: `compose-{endpoint}-{stackName}`
- Combined logs: `combined-{endpoint}-{stackName}`
- Container exec: `container-exec-{endpoint}-{stackName}-{serviceName}-{index}`
- Main console: `"console"`

---

## Database Layer

**File:** `backend/database.ts` (259 lines)

### Engine

SQLite by default (also has MySQL support code, but SQLite is primary). Database file stored at `{dataDir}/dockge.db`.

### ORM

Uses `redbean-node` (RedBean ORM for Node.js) which wraps `knex`. Models extend `BeanModel`.

### Migrations

Run automatically on startup via Knex migrations in `backend/migrations/`:

| Migration | Tables Created |
|---|---|
| `2023-10-20-0829-user-table.ts` | `user` |
| `2023-10-20-0829-setting-table.ts` | `setting` |
| `2023-12-20-2117-agent-table.ts` | `agent` |

### Schema

**`user`** table:
| Column | Type | Notes |
|---|---|---|
| id | integer | Auto-increment PK |
| username | varchar(255) | Unique |
| password | varchar(255) | bcrypt hash |
| active | boolean | Default true |
| timezone | varchar(150) | |
| twofa_secret | varchar(64) | TOTP secret |
| twofa_status | boolean | Default false |
| twofa_last_token | varchar(6) | Replay protection |

**`setting`** table:
| Column | Type | Notes |
|---|---|---|
| id | integer | Auto-increment PK |
| key | varchar(200) | Unique |
| value | text | JSON-serialized |
| type | varchar(20) | Grouping (e.g. "general") |

**`agent`** table:
| Column | Type | Notes |
|---|---|---|
| id | integer | Auto-increment PK |
| url | varchar(255) | Unique, full URL of remote instance |
| username | varchar(255) | Plaintext |
| password | varchar(255) | Plaintext (not hashed) |
| active | boolean | Default true |

### SQLite Configuration

Applied on init: WAL journal mode, 12MB cache, auto_vacuum=INCREMENTAL, synchronous=NORMAL

---

## Settings System

**File:** `backend/settings.ts` (175 lines)

Key-value store backed by the `setting` database table.

- **In-memory cache** with 60-second TTL per entry
- Cache cleaner runs every 60 seconds
- Settings grouped by `type` field (e.g. "general")
- Used for: `jwtSecret`, `primaryHostname`, `disableAuth`, `trustProxy`, `serverTimezone`, `checkUpdate`

### Known Setting Keys

| Key | Type | Purpose |
|---|---|---|
| `jwtSecret` | - | JWT signing secret (auto-generated) |
| `primaryHostname` | general | Hostname shown in UI for port links |
| `disableAuth` | general | Skip authentication (auto-login) |
| `trustProxy` | general | Respect X-Forwarded-For headers |
| `serverTimezone` | general | Server timezone setting |
| `checkUpdate` | general | Enable version update checking |

---

## Agent Manager (Multi-Instance Federation)

**File:** `backend/agent-manager.ts` (294 lines)

Allows one Dockge instance to manage stacks across multiple remote Dockge instances.

- **One `AgentManager` per client socket connection**
- Maintains Socket.io client connections to remote instances
- Stores agent credentials in the `agent` DB table (plaintext passwords)
- Connection retry: 10-second window with 1s polling on first connect

### Flow

1. On login, `connectAll()` loads all agents from DB and establishes Socket.io client connections
2. Each agent connection authenticates via `login` event with stored credentials
3. Agent events are relayed: remote agent emits → `client.on("agent")` → `socket.emit("agent")` to frontend
4. `emitToEndpoint()` proxies events to specific agents with retry logic
5. Version check: disconnects agents running < 1.4.0

### Agent Status Events

Emits `agentStatus` to the client with states: `"connecting"`, `"online"`, `"offline"`

---

## Authentication & Security

### Password Hashing (`backend/password-hash.ts`)

- **Hash:** bcrypt with 10 salt rounds
- **Verify:** `bcryptjs.compare()`
- **shake256:** Used to embed password fingerprint in JWT tokens (16 bytes)

### JWT Tokens

- Payload: `{ username: string, h: string }` where `h` is shake256 of password
- Secret: generated once on first run, stored in settings table
- Token invalidation: changing password regenerates JWT secret (invalidates ALL tokens for ALL users)

### Rate Limiting (`backend/rate-limiter.ts`)

| Limiter | Rate | Applied To |
|---|---|---|
| `loginRateLimiter` | 20/minute | `login` event |
| `twoFaRateLimiter` | 30/minute | 2FA verification |
| `apiRateLimiter` | 60/minute | (available but not actively used on specific events) |

### Socket.io Origin Check

In production, validates that the WebSocket `Origin` header matches the `Host` header. Bypassable via `UPTIME_KUMA_WS_ORIGIN_CHECK=bypass` env var.

### Auth Disable Mode

When `disableAuth` setting is true, auto-logs in the first user on socket connect.

---

## Scheduled Tasks

| Task | Interval | Implementation |
|---|---|---|
| Stack list broadcast | Every 10 seconds | Cron via `croner`, calls `sendStackList(true)` with cache |
| Version update check | Every 48 hours | `checkVersion.startInterval()`, fetches from `https://dockge.kuma.pet/version` |
| Settings cache cleanup | Every 60 seconds | `setInterval` in Settings class |
| Terminal client cleanup | Every 60 seconds | `setInterval` in Terminal.start(), kicks disconnected sockets |
| Terminal keep-alive check | Every 60 seconds | `setInterval` in Terminal.start(), closes terminals with 0 clients |

---

## Shared/Common Utilities

**File:** `common/util-common.ts` (430 lines)

Shared between frontend and backend:

### Constants

- Stack statuses: `UNKNOWN=0, CREATED_FILE=1, CREATED_STACK=2, RUNNING=3, EXITED=4`
- Terminal dimensions: `TERMINAL_COLS=105, TERMINAL_ROWS=10, PROGRESS_TERMINAL_ROWS=8, COMBINED_TERMINAL_COLS=58, COMBINED_TERMINAL_ROWS=20`
- `ERROR_TYPE_VALIDATION = 1`
- `ALL_ENDPOINTS = "##ALL_DOCKGE_ENDPOINTS##"`
- `acceptedComposeFileNames = ["compose.yaml", "docker-compose.yaml", "docker-compose.yml", "compose.yml"]`

### Functions

| Function | Purpose |
|---|---|
| `statusName(status)` | Numeric status to display name ("draft", "running", etc.) |
| `statusNameShort(status)` | Short status name ("active", "inactive", "exited") |
| `statusColor(status)` | Bootstrap color class for status |
| `intHash(str, length)` | Simple string hash to integer |
| `sleep(ms)` | Promise-based delay |
| `genSecret(length=64)` | Cryptographically random alphanumeric string |
| `getCryptoRandomInt(min, max)` | CSPRNG integer |
| `getComposeTerminalName(endpoint, stack)` | Terminal naming: `compose-{endpoint}-{stack}` |
| `getCombinedTerminalName(endpoint, stack)` | Terminal naming: `combined-{endpoint}-{stack}` |
| `getContainerTerminalName(endpoint, container)` | Terminal naming |
| `getContainerExecTerminalName(endpoint, stack, container, index)` | Terminal naming |
| `copyYAMLComments(doc, src)` | Preserves YAML comments when re-serializing |
| `parseDockerPort(input, hostname)` | Parses docker port strings to `{ url, display }` |
| `envsubst(string, variables)` | Shell-style variable substitution |
| `envsubstYAML(content, env)` | Traverse YAML values and substitute env vars |

### AgentSocket (`common/agent-socket.ts`)

Simple event emitter: `Map<string, callback>` with `on(event, cb)` and `call(event, ...args)`. Used to dispatch events to the correct agent socket handler locally.

---

## Data Models & Types

### DockgeSocket (extends Socket.io Socket)

```typescript
interface DockgeSocket extends Socket {
    userID: number;
    consoleTerminal?: Terminal;
    instanceManager: AgentManager;
    endpoint: string;
    emitAgent: (eventName: string, ...args: unknown[]) => void;
}
```

### Config / Arguments

```typescript
interface Arguments {
    sslKey?: string;
    sslCert?: string;
    sslKeyPassphrase?: string;
    port?: number;
    hostname?: string;
    dataDir?: string;
    stacksDir?: string;
    enableConsole?: boolean;
}

interface Config extends Arguments {
    dataDir: string;     // required
    stacksDir: string;   // required
}
```

### JWT Decoded

```typescript
interface JWTDecoded {
    username: string;
    h?: string;  // shake256 of password
}
```

### Stack JSON Shapes

**Simple (for list):**
```json
{
    "name": "mystack",
    "status": 3,
    "tags": [],
    "isManagedByDockge": true,
    "composeFileName": "compose.yaml",
    "endpoint": ""
}
```

**Full (for detail):**
```json
{
    "name": "mystack",
    "status": 3,
    "tags": [],
    "isManagedByDockge": true,
    "composeFileName": "compose.yaml",
    "endpoint": "",
    "composeYAML": "services:\n  ...",
    "composeENV": "FOO=bar",
    "primaryHostname": "localhost"
}
```

### Service Status

```json
{
    "serviceName": {
        "state": "running",
        "ports": ["0.0.0.0:8080->80/tcp"]
    }
}
```

### Agent JSON

```json
{
    "url": "https://remote:5001",
    "username": "admin",
    "endpoint": "remote:5001"
}
```

### Utility Types

```typescript
interface LooseObject { [key: string]: any }
interface BaseRes { ok: boolean; msg?: string }
class ValidationError extends Error {}
```

---

## Configuration

### CLI Arguments / Environment Variables

| Argument | Env Var | Default | Description |
|---|---|---|---|
| `--port` | `DOCKGE_PORT` | 5001 | Server listen port |
| `--hostname` | `DOCKGE_HOSTNAME` | (all interfaces) | Bind hostname |
| `--dataDir` | `DOCKGE_DATA_DIR` | `./data/` | Data directory (DB, config) |
| `--stacksDir` | `DOCKGE_STACKS_DIR` | `/opt/stacks` (Linux), `./stacks` (Windows) | Stacks directory |
| `--sslKey` | `DOCKGE_SSL_KEY` | (none) | Path to SSL key file |
| `--sslCert` | `DOCKGE_SSL_CERT` | (none) | Path to SSL cert file |
| `--sslKeyPassphrase` | `DOCKGE_SSL_KEY_PASSPHRASE` | (none) | SSL key passphrase |
| `--enableConsole` | `DOCKGE_ENABLE_CONSOLE` | false | Enable main terminal console |

### Other Environment Variables

| Var | Purpose |
|---|---|
| `NODE_ENV` | development/production |
| `DOCKGE_IS_CONTAINER` | Set to "1" when running in Docker |
| `DOCKGE_HIDE_LOG` | Filter log output by module:level |
| `SQL_LOG` | Enable SQL query logging when set to "1" |
| `UPTIME_KUMA_WS_ORIGIN_CHECK` | Set to "bypass" to skip origin validation |

---

## File Layout

```
backend/
├── index.ts                          # Entry point: creates DockgeServer, calls serve()
├── dockge-server.ts                  # Main server class (Express + Socket.io + lifecycle)
├── database.ts                       # SQLite connection, migrations, ORM init
├── stack.ts                          # Docker Compose stack operations
├── terminal.ts                       # PTY terminal management (3 classes)
├── settings.ts                       # Key-value settings with cache
├── agent-manager.ts                  # Multi-instance agent connections
├── log.ts                            # Colored console logger
├── rate-limiter.ts                   # Rate limiting (login, 2FA, API)
├── password-hash.ts                  # bcrypt + shake256
├── check-version.ts                  # Periodic version update checker
├── util-server.ts                    # Types (DockgeSocket, Config, JWTDecoded) + helpers
├── router.ts                         # Abstract Router base class
├── socket-handler.ts                 # Abstract SocketHandler base class
├── agent-socket-handler.ts           # Abstract AgentSocketHandler base class
├── routers/
│   └── main-router.ts               # GET /, GET /robots.txt, GET *
├── socket-handlers/
│   ├── main-socket-handler.ts        # Auth, settings, composerize
│   ├── manage-agent-socket-handler.ts # Add/remove agents
│   └── agent-proxy-socket-handler.ts  # Route events to agents
├── agent-socket-handlers/
│   ├── docker-socket-handler.ts      # Stack CRUD + lifecycle operations
│   └── terminal-socket-handler.ts    # Terminal management events
├── models/
│   ├── user.ts                       # User model (resetPassword, createJWT)
│   └── agent.ts                      # Agent model (getAgentList, endpoint)
├── migrations/
│   ├── 2023-10-20-0829-user-table.ts
│   ├── 2023-10-20-0829-setting-table.ts
│   └── 2023-12-20-2117-agent-table.ts
└── utils/
    └── limit-queue.ts                # Fixed-size queue (Array subclass)

common/
├── util-common.ts                    # Shared constants, status helpers, YAML utils, crypto
└── agent-socket.ts                   # Simple event dispatcher (Map-based)
```
