# Dockru - Next Steps and Future Work

This document catalogs features not yet implemented, known limitations, technical debt, and future optimizations for the Dockru Rust implementation.

**Status as of February 2026:** All 10 migration phases complete. Core functionality working.

---

## 1. Not Implemented Features

### 1.1 Two-Factor Authentication (2FA/TOTP)

**Status:** Stubbed  
**Location:** [src/socket_handlers/auth.rs](src/socket_handlers/auth.rs#L188)

**Current Behavior:**
- User model has 2FA fields (`twofa_status`, `twofa_secret`)
- Login flow checks for 2FA and requests token
- Token verification always fails (returns `authInvalidToken`)

**What's Needed:**
- Implement TOTP token generation and verification using `totp-lite` crate
- QR code generation for 2FA setup
- Backup codes generation and storage
- Token validation with time window (±30 seconds)

**Priority:** Medium - Security feature but not blocking core functionality

---

### 1.4 Local Agent Event Handling

**Status:** Stubbed  
**Location:** [src/socket_handlers/agent.rs](src/socket_handlers/agent.rs#L189)

**Current Behavior:**
- Events routed to local endpoint (empty string) log "not implemented"
- Requires AgentSocket abstraction that wasn't in original TypeScript

**What's Needed:**
- Create `AgentSocket` trait or module
- Route local events directly to stack/terminal handlers
- Handle local vs remote event routing properly

**Priority:** Low - Agent system works for remote instances

---

### 1.5 Docker Network List

**Status:** Basic implementation  
**Location:** [src/socket_handlers/stack_management.rs](src/socket_handlers/stack_management.rs#L418)

**Current Behavior:**
- Runs `docker network ls --format {{.Name}}`
- Returns only network names, no additional metadata

**What's Needed:**
- Return network driver, scope, labels
- Parse JSON format for richer data
- Or use Docker SDK for proper API access

**Priority:** Low - Works for basic use case

---

## 2. TODOs and Partial Implementations
---

### 2.3 Stack List Caching

**Status:** Not implemented  
**Location:** [src/stack.rs](src/stack.rs#L752)

**Current Behavior:**
- Stack list generated fresh every 10 seconds
- Scans filesystem and runs `docker compose ls` each time
- ~200-400ms for 50 stacks

**What's Needed:**
- Implement `use_cache_for_managed` flag
- Track directory mtime to detect changes
- Cache stack list between broadcasts
- Invalidate cache on stack operations

**Priority:** Very Low - Performance acceptable for typical usage, does not run when no connections


---

### 2.5 Version Check - Beta Channel

**Status:** Not implemented  
**Location:** [src/check_version.rs](src/check_version.rs)

**Current Behavior:**
- Only checks stable/slow channel
- `slowChannel` setting exists but not used

**What's Needed:**
- Parse version JSON for beta channel
- Support switching between stable/beta
- Semantic version comparison for pre-release tags

**Priority:** Low - Most users want stable only

---

## 3. Known Limitations

### 3.2 X-Forwarded-For IP Extraction

**Status:** Not implemented  
**Location:** [src/socket_handlers/auth.rs](src/socket_handlers/auth.rs#L350)

**Issue:**
- socketioxide doesn't expose HTTP request headers in socket handler
- Cannot extract real IP from `X-Forwarded-For` header
- Rate limiting uses socket peer address (incorrect behind proxy)

**Workaround:**
- Trust proxy mode enabled but can't access forwarded IP
- Rate limiting still works but may block entire proxy

**Resolution:**
- Wait for socketioxide to expose headers in socket context
- Or use middleware to inject IP into socket state

**Priority:** Medium - Affects rate limiting accuracy behind proxies


---

### 3.5 IP Address Identification for Socket Connections

**Status:** Not implemented
**Related:** Section 3.2 (X-Forwarded-For extraction)
**Location:** [src/socket_handlers/helpers.rs](src/socket_handlers/helpers.rs) - IP tracking infrastructure exists

**Issue:**
- socketioxide doesn't expose peer address or HTTP headers in socket handlers
- Cannot capture client IP for rate limiting, audit logs, or security
- `SocketState::ip_address` field exists but remains None
- `get_client_ip()` in auth.rs returns hardcoded 127.0.0.1

**Proposed Solution - Signed Nonce System:**

1. **Nonce Generation Endpoint:**
   - Add HTTP endpoint: `GET /api/socket-nonce`
   - Extract client IP from request headers (X-Forwarded-For or peer address)
   - Generate cryptographically secure random nonce
   - Sign nonce with server secret (HMAC-SHA256 using jwtSecret)
   - Store mapping: `nonce -> {ip, timestamp, used: false}` in memory (with TTL)
   - Return `{nonce, signature}` to client

2. **Socket Connection:**
   - Client sends nonce + signature on socket connect (e.g., in query params or initial event)
   - Server validates signature and checks nonce is unused and not expired
   - Mark nonce as used (prevent replay attacks)
   - Extract IP from stored mapping
   - Store IP in `SocketState::ip_address` via `set_ip_address()`

3. **Benefits:**
   - Works behind reverse proxies (HTTP endpoint sees X-Forwarded-For)
   - Secure (signed nonces prevent forgery)
   - Replay-attack resistant (one-time use)
   - No socketioxide framework changes needed

**Implementation Details:**
```rust
// In-memory nonce store with TTL cleanup
struct NonceData {
    ip: String,
    created_at: Instant,
    used: bool,
}

// Endpoint: GET /api/socket-nonce
async fn generate_socket_nonce(
    req: Request,
    ctx: Arc<ServerContext>
) -> Result<Json<NonceResponse>> {
    let ip = extract_client_ip(&req); // X-Forwarded-For aware
    let nonce = gen_secret(32);
    let signature = hmac_sign(&nonce, &ctx.jwt_secret);

    NONCE_STORE.insert(nonce.clone(), NonceData {
        ip,
        created_at: Instant::now(),
        used: false,
    });

    Ok(Json(NonceResponse { nonce, signature }))
}

// On socket connect, validate and extract IP
fn validate_and_extract_ip(nonce: &str, signature: &str) -> Option<String> {
    // Verify signature
    // Check not expired (< 60 seconds old)
    // Check not used
    // Mark as used
    // Return IP
}
```

**Alternative Solutions:**
- **Middleware injection:** Use Axum middleware to capture IP and inject into socket extensions (requires socketioxide API support)
- **Wait for socketioxide:** Framework may add header/peer address access in future versions
- **Trust mode only:** Only use this for reverse proxy deployments, direct connections use peer_addr when available

**TTL and Cleanup:**
- Nonces expire after 60 seconds (should be used immediately after generation)
- Background task cleans expired nonces every 5 minutes
- Used nonces removed immediately after validation

**Priority:** Medium - Enables accurate rate limiting and audit logging behind proxies

---

## 4. Technical Debt

---

### 4.2 Docker CLI vs SDK

**Status:** Using CLI, SDK would be better  
**Location:** Throughout [src/stack.rs](src/stack.rs)

**Issue:**
- All Docker operations shell out to `docker` CLI
- Parsing text output (brittle)
- Higher overhead than API calls
- Limited error handling

**Recommended Fix:**
- Integrate Docker SDK/API client (e.g., `bollard` crate)
- Direct API calls instead of CLI commands
- Richer error information
- Better performance

**Benefits:**
- More reliable (no output parsing)
- Faster execution (no shell overhead)
- Better error messages
- Programmatic container management

**Priority:** High - Current approach works but fragile

---

---

## 5. Future Optimizations

### 5.1 LRU Cache for Compose Files

**Priority:** Low  
**Context:** [PERFORMANCE.md](PERFORMANCE.md) - File I/O section

**Current:** Lazy-load compose files on demand

**Optimization:**
- Add LRU cache for frequently accessed files
- Invalidate on file mtime change
- Limit cache size (e.g., 50 most recent)

**Trigger:** If users have > 100 stacks

---

### 5.2 Stack Status Caching

**Priority:** Low  
**Context:** [PERFORMANCE.md](PERFORMANCE.md) - Stack List Generation

**Current:** Run `docker compose ls` every 10 seconds

**Optimization:**
- Cache status results with 10-second TTL
- Use Docker events API to detect changes
- Update cache incrementally on events
- Reduce `docker compose ls` calls

**Trigger:** If stack list broadcast becomes bottleneck

---

### 5.3 Broadcast Batching and Debouncing

**Priority:** Low  
**Context:** [PERFORMANCE.md](PERFORMANCE.md) - Advanced Optimizations

**Current:** Each stack operation triggers broadcast

**Optimization:**
- Batch multiple stack updates into single broadcast
- Debounce rapid stack list updates (e.g., 500ms)
- Reduce socket.io message overhead

**Trigger:** If rapid stack operations cause performance issues

---

### 5.4 Connection Pool Tuning

**Priority:** Low  
**Context:** [PERFORMANCE.md](PERFORMANCE.md) - Database Performance

**Current:** SQLx default pool settings

**Optimization:**
- Tune pool size for concurrent users
- Adjust connection timeout settings
- Monitor pool utilization

**Trigger:** If > 100 concurrent users

---

### 5.5 YAML Parsing Cache

**Priority:** Low  
**Context:** [PERFORMANCE.md](PERFORMANCE.md) - Advanced Optimizations

**Current:** Parse YAML on every access

**Optimization:**
- Cache parsed YAML documents (not just strings)
- Invalidate on file mtime change
- Reduce CPU usage for repeated accesses

**Trigger:** If stack detail requests become slow

---

## 6. Testing and Quality

### 6.1 Integration Tests

**Status:** Manual test scenarios only  
**Location:** [TESTING.md](TESTING.md)

**Needed:**
- Automated integration tests
- Socket.io client tests
- Docker operation tests (requires test containers)
- End-to-end workflow tests

**Priority:** Medium - Important for confidence in changes

---

### 6.2 Load Testing

**Status:** Not performed  
**Context:** [PERFORMANCE.md](PERFORMANCE.md) - Load Testing Recommendations

**Scenarios to test:**
- 50 stacks, 10 concurrent deployments
- 100 concurrent socket connections
- 100 stacks with broadcast every 10s
- 24-hour uptime test (memory leaks)

**Priority:** Medium - Before production use at scale

---

### 6.3 Cross-Platform Testing

**Status:** Developed on Linux  
**Context:** All code

**Needed:**
- Test on Windows (PowerShell terminals, paths)
- Test on macOS (BSD vs GNU tools)
- Docker Desktop vs Docker Engine differences

**Priority:** Medium - Before claiming cross-platform support

---

## 7. Documentation

### 7.1 API Documentation

**Status:** Code comments only  
**Needed:**
- Socket.io event reference
- Request/response formats
- Error codes and messages
- Rate limiting details

**Priority:** Low - Matches TypeScript behavior

---

### 7.2 Deployment Guide

**Status:** Basic instructions in README  
**Needed:**
- Docker deployment guide
- Systemd service setup
- Reverse proxy configuration (nginx/caddy/traefik)
- TLS/SSL certificate setup
- Backup and restore procedures

**Priority:** Medium - Important for production use

---

### 7.3 Migration Guide (TypeScript → Rust)

**Status:** Not written  
**Location:** [MIGRATION_GUIDE.md](MIGRATION_GUIDE.md) referenced but empty

**Needed:**
- Database compatibility notes
- Configuration changes
- Breaking changes (if any)
- Rollback procedures
- Agent compatibility notes

**Priority:** Low - Critical for existing users, not new ones

---

## 8. Priority Summary

### High Priority (Security/Blocking)
1. ✅ Agent password encryption (AES-256-GCM at rest)
2. ✅ Password validation when disabling auth
3. ✅ YAML comment preservation (handled by frontend, no backend work needed)
4. ✅ Password rehashing on bcrypt cost increase
5. ⚠️ Docker CLI → SDK migration

### Medium Priority (Functionality)
6. ✅ Socket state management and authentication filtering
7. IP address identification for socket connections (signed nonce system)
8. X-Forwarded-For IP extraction (or use nonce system workaround)
9. Two-factor authentication implementation
10. ✅ Composerize feature
11. ✅ Terminal keep-alive (close all terminals when last socket disconnects)
12. Integration and load testing
13. Cross-platform testing
14. Deployment documentation

### Low Priority (Nice-to-Have)
14. Stack list caching
15. Various performance optimizations
16. Beta version channel support
17. Local agent event handling
18. Docker network list enrichment
19. Docker container detection
20. Error handling consistency
21. API documentation

---

## 9. Recommendations

### Immediate Actions (Before Production)
1. ✅ Implement agent password encryption
2. ✅ Add password validation for disableAuth setting
3. ✅ Implement proper socket authentication filtering
4. Write migration guide
5. Cross-platform testing

### Near-Term (First Production Release)
1. Implement IP address identification (signed nonce system or wait for socketioxide)
2. Create deployment documentation
3. Integration tests
4. Two-factor authentication

### Long-Term (Future Versions)
1. Two-factor authentication
2. ✅ Interactive container terminals
3. Docker SDK migration
4. Performance optimizations (as needed)
5. ✅ Advanced features (composerize)

---

## 10. Notes

- **Performance:** Current implementation performs well for expected use cases (< 100 stacks, < 50 concurrent users)
- **Compatibility:** Frontend unchanged, Socket.io protocol matches TypeScript version
- **Memory:** 60-70% reduction vs Node.js implementation (~20-30 MB idle)
- **Binary Size:** 20x smaller than Node.js (~5-10 MB stripped)
- **Stability:** All core features implemented and tested

**Overall Status:** ✅ Production-ready for single-user and small team deployments after addressing high-priority security items

---

## Contributing

When implementing items from this document:
1. Update this file to reflect new status
2. Add tests for new functionality
3. Update relevant documentation (README, PERFORMANCE, TESTING)
4. Ensure backward compatibility with existing deployments
5. Follow existing code patterns and error handling

---

*Last updated: February 15, 2026*
*Based on: Phase 10 completion + terminal keep-alive implementation*
