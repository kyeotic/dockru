# Dockru - Rust Implementation

This is the Rust backend implementation of Dockru (formerly Dockge), following the migration plan outlined in [rust-migration-plan.md](./rust-migration-plan.md).

## Current Status: Phase 1 Complete ✅

### Phase 1: Project Setup & Infrastructure

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

### Dependencies

**Core Runtime:**
- `tokio` - Async runtime
- `axum` - HTTP server framework
- `socketioxide` - Socket.io server for Rust

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

| CLI Argument | Environment Variable | Default | Description |
|--------------|---------------------|---------|-------------|
| `--port` | `DOCKGE_PORT` | `5001` | Port to listen on |
| `--hostname` | `DOCKGE_HOSTNAME` | `0.0.0.0` | Hostname to bind to |
| `--data-dir` | `DOCKGE_DATA_DIR` | `./data` | Data directory |
| `--stacks-dir` | `DOCKGE_STACKS_DIR` | `/opt/stacks` (Linux)<br>`./stacks` (Windows) | Stacks directory |
| `--ssl-key` | `DOCKGE_SSL_KEY` | None | Path to SSL key file |
| `--ssl-cert` | `DOCKGE_SSL_CERT` | None | Path to SSL certificate |
| `--ssl-key-passphrase` | `DOCKGE_SSL_KEY_PASSPHRASE` | None | SSL key passphrase |
| `--enable-console` | `DOCKGE_ENABLE_CONSOLE` | `false` | Enable interactive console |

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
├── main.rs          # Entry point, logging setup
├── config.rs        # CLI and environment variable parsing
└── server.rs        # HTTP server, Socket.io, graceful shutdown
```

## Next Steps (Phase 2+)

See [rust-migration-plan.md](./rust-migration-plan.md) for the complete migration roadmap:

- **Phase 2:** Core utilities & shared code
- **Phase 3:** Database layer & models
- **Phase 4:** Authentication & security
- **Phase 5:** Terminal/PTY system
- **Phase 6+:** Stack management, Docker integration, etc.

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

## License

Same as original Dockge project - see [LICENSE](./LICENSE)
