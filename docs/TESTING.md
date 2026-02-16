# Dockru Phase 10 - Manual Test Scenarios

This document provides comprehensive test scenarios to validate the Phase 10 implementation of Dockru.

## Prerequisites

- Rust environment set up (rustc 1.75+, cargo)
- Docker and Docker Compose installed
- Node.js and npm for frontend build (if testing with frontend)
- Test stacks directory with sample compose files

## Setup

1. **Build the project:**
   ```bash
   cargo build --release
   ```

2. **Prepare test environment:**
   ```bash
   mkdir -p ./data ./test-stacks
   ```

## Test Scenarios

### 1. Version Checking (Phase 10)

**Goal:** Verify version checking fetches and stores the latest version.

**Steps:**
1. Start the server:
   ```bash
   cargo run -- --stacks-dir ./test-stacks --data-dir ./data
   ```

2. Check logs for version check:
   ```bash
   # Look for:
   # INFO dockru::check_version: Checking for updates from https://dockge.kuma.pet/version
   # INFO dockru::check_version: Latest stable version: X.X.X
   ```

3. Connect a socket.io client and check for `info` event:
   - Event should contain: `{ version: "1.5.0", latestVersion: "X.X.X", primaryHostname: null }`

**Expected Results:**
- ✅ Version check runs on startup
- ✅ Latest version is fetched from update server
- ✅ `info` event broadcasts version information

**Known Limitations:**
- Beta channel not supported (stable only)
- `checkUpdate` setting defaults to `true`

---

### 2. Scheduled Stack List Broadcast (Phase 10)

**Goal:** Verify stack list broadcasts every 10 seconds to authenticated clients.

**Steps:**
1. Start the server
2. Create a test stack:
   ```bash
   mkdir -p ./test-stacks/nginx-test
   cat > ./test-stacks/nginx-test/compose.yaml << 'EOF'
   services:
     web:
       image: nginx:alpine
       ports:
         - "8080:80"
   EOF
   ```

3. Connect and authenticate a socket.io client
4. Listen for `stackList` events
5. Wait 10+ seconds

**Expected Results:**
- ✅ Initial `stackList` event received after login
- ✅ Subsequent `stackList` events every 10 seconds
- ✅ Stack list contains the test stack with correct status

**Sample stackList event:**
```json
{
  "ok": true,
  "stackList": {
    "nginx-test": {
      "name": "nginx-test",
      "status": 0,
      "tags": [],
      "isManagedByDockge": true,
      "composeFileName": "compose.yaml",
      "endpoint": ""
    }
  }
}
```

---

### 3. Settings Cache Cleanup (Phase 3/10)

**Goal:** Verify settings cache automatically removes expired entries.

**Steps:**
1. Start the server with `RUST_LOG=debug`
2. Trigger multiple settings reads via socket events (e.g., `getSettings`)
3. Wait 60+ seconds
4. Check logs for cache cleanup

**Expected Results:**
- ✅ Log shows: `DEBUG dockru::db::models::setting: Settings cache cleanup running`
- ✅ Expired entries (older than 60s) are removed
- ✅ Settings are re-fetched from database after expiry

---

### 4. Terminal Cleanup Task (Phase 5/10)

**Goal:** Verify terminal cleanup tasks run every 60 seconds.

**Steps:**
1. Start the server with `RUST_LOG=debug`
2. Open a terminal (e.g., `mainTerminal` event)
3. Wait 60+ seconds
4. Check logs for cleanup checks

**Expected Results:**
- ✅ Log shows: `DEBUG dockru::terminal: Terminal <name> cleanup check complete`
- ✅ Cleanup runs every 60 seconds
- ✅ Terminal remains alive (keep-alive not fully implemented)

**Known Limitations:**
- Keep-alive room member counting not available in socketioxide 0.14
- Disconnected clients handled by socket.io automatically
- Terminals don't auto-close when empty (limitation documented)

---

### 5. Complete Workflow Test

**Goal:** Test a complete user workflow from setup to stack deployment.

**Steps:**
1. **First-time setup:**
   ```bash
   # Reset database
   rm -rf ./data
   cargo run -- --stacks-dir ./test-stacks --data-dir ./data
   ```

2. **Connect with socket.io client:**
   - Event: `setup` → Create first user
   - Expected: `{ ok: true, token: "jwt-token" }`

3. **Login:**
   - Event: `login` with username/password
   - Expected: `{ ok: true, token: "jwt-token" }`
   - Expected: `info` event with version info
   - Expected: `stackList` event

4. **Deploy a stack:**
   - Event: `deployStack` with stack data
   - Expected: Terminal output captured
   - Expected: Stack status updates
   - Expected: `stackList` broadcast after deploy

5. **Check version in info:**
   - Verify `info` event has `version` and `latestVersion`

**Expected Results:**
- ✅ Complete workflow succeeds
- ✅ All broadcasts received
- ✅ Version checking integrated
- ✅ Stack list updates propagate

---

### 6. Performance Baseline Test

**Goal:** Establish baseline performance metrics.

**Test 1: Memory Usage**
```bash
cargo build --release
./target/release/dockru --stacks-dir ./test-stacks --data-dir ./data &
PID=$!

# Monitor memory usage
while true; do
  ps -p $PID -o pid,vsz,rss,comm | tail -1
  sleep 10
done
```

**Expected Results:**
- ✅ Idle memory: < 50MB RSS
- ✅ Memory stable over time (no leaks)

**Test 2: Stack List Broadcast Performance**
```bash
# Create 50 test stacks
for i in {1..50}; do
  mkdir -p ./test-stacks/test-$i
  echo "services:" > ./test-stacks/test-$i/compose.yaml
  echo "  web:" >> ./test-stacks/test-$i/compose.yaml
  echo "    image: nginx:alpine" >> ./test-stacks/test-$i/compose.yaml
done

# Start server and monitor broadcast timing
# Check logs for broadcast duration
```

**Expected Results:**
- ✅ Stack list generation: < 500ms for 50 stacks
- ✅ Broadcast interval: ~ 10 seconds (consistent)

**Test 3: Concurrent Connections**
```bash
# Use socket.io-client to create multiple connections
# Test 10 concurrent authenticated clients
# Verify all receive broadcasts
```

**Expected Results:**
- ✅ All clients receive `stackList` broadcasts
- ✅ No broadcast delays or drops
- ✅ Memory scales linearly with connections

---

## Integration Tests

### Authentication Flow
1. ✅ Setup creates first user
2. ✅ Login succeeds with valid credentials
3. ✅ Login fails with invalid credentials
4. ✅ JWT token authentication works
5. ✅ Password change invalidates old tokens
6. ✅ `info` event sent after login

### Stack Management
1. ✅ Stack list includes all managed stacks
2. ✅ Deploy stack updates status
3. ✅ Stack list broadcasts after operations
4. ✅ Multiple endpoints supported (agents)

### Terminal System
1. ✅ Terminals can be created and joined
2. ✅ Terminal output broadcasts to joined sockets
3. ✅ Terminal cleanup runs periodically
4. ✅ Terminals exit cleanly

### Agent Management
1. ✅ Agents can be added
2. ✅ Agents connect and authenticate
3. ✅ Events proxy to remote agents
4. ✅ Agent list broadcasts

---

## Regression Tests

Run existing tests to ensure no functionality broken:

```bash
cargo test
```

**Expected:** All 91+ tests pass

---

## Manual Frontend Testing

If testing with the Vue frontend:

1. **Build frontend:**
   ```bash
   npm install
   npm run build:frontend
   ```

2. **Start Rust backend:**
   ```bash
   cargo run --release -- --stacks-dir /opt/stacks
   ```

3. **Open browser:** http://localhost:5001

4. **Test UI:**
   - ✅ Setup page appears on first run
   - ✅ Login works
   - ✅ Stack list populates
   - ✅ Stack operations work
   - ✅ Version shown in footer/about
   - ✅ No console errors

---

## Known Issues / Limitations

### Phase 10 Limitations

1. **Terminal keep-alive:**
   - Room member counting not available in socketioxide 0.14
   - Terminals don't auto-close when empty
   - Workaround: Manual terminal cleanup or upgrade socketioxide when API available

2. **Stack list broadcast:**
   - Broadcasts to all connected sockets (not filtered by authentication)
   - Clients should ignore if not authenticated
   - Full implementation requires socket iteration API

3. **Docker container detection:**
   - `DOCKGE_IS_CONTAINER` not implemented
   - Can be added later if needed

4. **Beta version channel:**
   - Only stable/slow channel supported
   - `checkBeta` setting not implemented

### General Limitations

- X-Forwarded-For IP extraction limited by socketioxide API
- 2FA/TOTP basic support only (not fully tested)
- Interactive container terminals stubbed
- Composerize not implemented

---

## Success Criteria

Phase 10 is considered complete when:

- ✅ Version checking runs every 48 hours
- ✅ Stack list broadcasts every 10 seconds
- ✅ Settings cache cleanup runs every 60 seconds
- ✅ Terminal cleanup runs every 60 seconds
- ✅ `sendInfo` broadcasts version information
- ✅ All scheduled tasks start on server startup
- ✅ No crashes or memory leaks
- ✅ Performance acceptable (< 50MB idle, broadcasts < 500ms)
- ✅ All existing tests pass
- ✅ Known limitations documented

---

## Troubleshooting

### Server won't start
- Check port 5001 not in use: `netstat -tuln | grep 5001`
- Verify stacks directory exists and is readable
- Check logs: `RUST_LOG=debug cargo run -- --stacks-dir ./stacks`

### Version check fails
- Verify internet connectivity
- Check firewall allows HTTPS to dockge.kuma.pet
- Disable version check: Set `checkUpdate` to `false` in settings

### Stack list not broadcasting
- Check logs for errors: `RUST_LOG=debug`
- Verify stacks directory readable
- Ensure docker compose command available

### Memory leak suspected
- Monitor RSS with: `watch -n 1 "ps aux | grep dockru"`
- Run with memory profiler: `valgrind --leak-check=full`
- Check for unclosed file descriptors: `lsof -p <pid>`

---

## Next Steps

After validating all test scenarios:

1. Run full test suite: `cargo test`
2. Run frontend integration test
3. Performance profiling with real workloads
4. Update documentation (RUST_README.md)
5. Create migration guide
6. Tag release: Phase 10 complete
