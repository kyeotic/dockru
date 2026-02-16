# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Dockru is a Rust implementation of Dockge - a self-hosted Docker Compose stack manager with a reactive web interface. This is a complete rewrite of the original TypeScript/Node.js version, offering 60-70% less memory usage, 2x faster operations, and a 20x smaller binary.

**Key Features:**
- Manage Docker Compose stacks through a web UI
- Real-time terminal output via Socket.io
- Multi-agent support for managing multiple Docker hosts
- JWT-based authentication
- Interactive web terminal with PTY support

## Development Commands

This project uses `just` as the command runner. Key commands:

```bash
# Development
just dev              # Run backend with cargo watch (serves pre-built frontend)
just dev-all         # Run both backend and frontend dev servers

# Building
just build           # Build both frontend and backend
just build-backend   # Build Rust backend only
just build-frontend  # Build frontend only

# Testing
just test            # Run all tests
just test-verbose    # Run tests with output
just test-watch      # Watch and run tests on changes

# Code Quality
just lint            # Run cargo clippy
just lint-frontend   # Run frontend linter
just fmt             # Format all code
just check           # Check compilation without building

# Docker
just docker-build    # Build Docker image locally
just docker-up       # Run with docker-compose

# Installation
just install         # Install all dependencies (cargo + npm)
```

**Important Environment Variables:**
- `DOCKRU_STACKS_DIR` - Directory containing Docker Compose stacks (default: /opt/stacks)
- `DOCKRU_DATA_DIR` - Directory for database and config (default: ./data)
- `DOCKRU_ENABLE_CONSOLE` - Enable console output (default: false)
- `RUST_LOG` - Set logging level (e.g., `debug`, `info`, `warn`)

## Architecture

### Backend Structure

The Rust backend is organized into focused modules:

**Core Modules:**
- `main.rs` - Application entry point
- `server.rs` - HTTP and Socket.io server setup
- `config.rs` - Configuration parsing from CLI args and env vars

**Domain Logic:**
- `stack.rs` - Docker Compose stack management (deploy, stop, delete, status)
- `docker.rs` - Docker operations and Bollard SDK integration
- `terminal.rs` - PTY/terminal system with output buffering (LimitQueue)
- `agent_manager.rs` - Multi-agent system for remote Docker host management
- `auth.rs` - JWT token generation and validation
- `socket_auth.rs` - Socket.io authentication middleware

**Socket.io Event Handlers (`src/socket_handlers/`):**
- `auth.rs` - Login, setup, password management
- `stack_management.rs` - Stack operations (deploy, stop, delete, etc.)
- `terminal.rs` - Terminal creation, input/output
- `agent.rs` - Agent management (add, remove, status)
- `settings.rs` - Settings management
- `helpers.rs` - Utility handlers

**Database (`src/db/`):**
- `mod.rs` - Database connection and migration runner
- `models/user.rs` - User authentication (bcrypt password hashing)
- `models/setting.rs` - Settings with 60-second cache TTL
- `models/agent.rs` - Remote agent configuration

**Utilities:**
- `broadcasts.rs` - Scheduled broadcasts (stack list every 10s, version check every 48h)
- `check_version.rs` - Version checking against update server
- `rate_limiter.rs` - Governor-based rate limiting for auth endpoints
- `static_files.rs` - Pre-compressed static file serving (brotli/gzip)

### Frontend Structure

The frontend is a Vue 3 application with Socket.io client:

- `frontend/src/` - Vue components, pages, and routing
- `frontend/src/components/` - Reusable Vue components
- `frontend/src/pages/` - Page components (Dashboard, Console, Settings)
- `frontend/common/agent-socket.ts` - Shared Socket.io types

**Build Output:**
- `frontend-dist/` - Production build output served by Rust backend

### Database Schema

SQLite database with three tables:
- `user` - User accounts with bcrypt password hashes
- `setting` - Key-value settings with caching
- `agent` - Remote Dockge agent configurations

Migrations are in `migrations/` and run automatically on startup.

## Testing

**Run tests:**
```bash
cargo test                    # All tests
cargo test -- --nocapture    # With output
cargo test test_name         # Single test
```

**Frontend tests:**
```bash
cd frontend && npm run check-ts  # TypeScript type checking
cd frontend && npm run lint      # ESLint
```

See `TESTING.md` for comprehensive test scenarios and manual testing procedures.

## Key Implementation Details

### Socket.io Integration

The backend uses `socketioxide` (v0.18) for Socket.io server functionality. Authentication is handled via JWT tokens passed in the auth payload or query parameters.

**Authentication Flow:**
1. Client emits `setup` (first-time) or `login` event
2. Server validates credentials and returns JWT token
3. Subsequent socket connections include token in auth
4. Middleware validates token and attaches user to socket extensions

### Terminal System

Terminals use `portable-pty` for cross-platform PTY support:
- Each terminal has a unique name and room for Socket.io broadcasting
- Output is buffered in a `LimitQueue` (max 100 chunks, ~100KB per terminal)
- Terminal cleanup runs every 60 seconds
- Terminals support resize events

**Known Limitation:** Terminal keep-alive (auto-close when no clients) is not fully implemented due to socketioxide API limitations.

### Stack Management

Stacks are Docker Compose projects stored in the stacks directory. Each stack is a directory containing:
- `compose.yaml` (or `docker-compose.yml`)
- Optional `.env` file
- Optional additional files

**Stack operations:**
- Deploy: `docker compose up -d`
- Stop: `docker compose stop`
- Down: `docker compose down`
- Update: `docker compose pull && docker compose up -d`
- Status: Parsed from `docker compose ls` output

### Agent System

Agents allow managing Docker hosts on remote machines:
- Primary server acts as proxy for agent operations
- Agents connect via Socket.io client (rust_socketio)
- Events are routed to agents based on endpoint parameter
- Agent passwords encrypted at rest with AES-GCM

### Performance Characteristics

See `PERFORMANCE.md` for detailed analysis. Key metrics:
- Idle memory: 20-30 MB RSS
- Stack list generation: 200-400ms for 50 stacks
- Startup time: ~100-150ms
- Binary size: 5-10 MB (stripped release)

## Migration from TypeScript Version

See `MIGRATION_GUIDE.md` for detailed migration instructions from the original Node.js/TypeScript version.

**Key compatibility notes:**
- Database schema is identical - no migration needed
- Environment variables have `DOCKRU_` prefix instead of `DOCKGE_`
- Feature parity achieved in Phase 10
- JWT tokens remain valid after migration

## Common Development Patterns

### Adding a New Socket.io Event Handler

1. Add handler function in appropriate file under `src/socket_handlers/`
2. Register handler in the module's `register_handlers()` function
3. Use `helpers::with_server_state()` to access shared state
4. Emit responses with appropriate error handling

### Adding a New Setting

1. Define setting key constant in `src/db/models/setting.rs`
2. Add getter/setter methods if needed
3. Cache invalidation is automatic (60s TTL)

### Adding Database Fields

1. Create new migration file in `migrations/`
2. Update model structs in `src/db/models/`
3. Migrations run automatically on startup

## Known Limitations

- Beta version channel not supported (stable only)
- Interactive container exec terminals not implemented
- Docker run-to-compose converter (composerize) not implemented
- Terminal auto-close when empty not implemented (socketioxide limitation)
- Stack list broadcasts to all sockets (clients filter based on auth state)

## Code Style

- Use `async/await` for I/O operations
- Prefer `anyhow::Result` for error handling in application code
- Use `thiserror` for domain-specific error types
- Follow Rust naming conventions (snake_case for functions/variables)
- Run `just fmt` before committing
- Run `just lint` to check for warnings

## Debugging

**Enable debug logging:**
```bash
RUST_LOG=debug cargo run -- --stacks-dir ./stacks
```

**Debug specific modules:**
```bash
RUST_LOG=dockru::stack=debug,dockru::terminal=trace cargo run
```

**Check database:**
```bash
sqlite3 data/kuma.db "SELECT * FROM user;"
```

## Additional Documentation

- `README.md` - Project overview and installation
- `TESTING.md` - Comprehensive testing guide
- `MIGRATION_GUIDE.md` - Migration from TypeScript version
- `PERFORMANCE.md` - Performance analysis and profiling
- `old-ts-backend/` - Original TypeScript implementation for reference
