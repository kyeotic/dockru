# Rust Dead Code Analysis

This document catalogs all `#[allow(dead_code)]` annotations in the Dockru project, grouped by intended feature.

## 2. Socket IP Address Tracking (Future Infrastructure)

**Purpose**: IP address tracking for rate limiting and audit logs.
**Status**: Infrastructure ready, awaiting implementation (see rust-next.md section 3.5)

- `src/socket_handlers/helpers.rs:18` - `SocketState::ip_address` field
  - Stores client IP address per socket connection
  - Currently always None due to socketioxide limitations

- `src/socket_handlers/helpers.rs:82` - `get_ip_address()` function
  - Retrieve IP address from socket state

- `src/socket_handlers/helpers.rs:88` - `set_ip_address()` function
  - Store IP address in socket state

**Planned Implementation:**
- Signed nonce system: client fetches nonce via HTTP (captures IP), uses nonce to connect socket
- See rust-next.md section 3.5 for full design

## 3. Utility Types & Response Helpers

**Purpose**: Common data structures and response builders.

- `src/utils/types.rs:6` - `LooseObject` type alias
  - Flexible JSON object type (HashMap<String, JsonValue>)

- `src/utils/types.rs:20` - `BaseRes::ok()` method
- `src/utils/types.rs:29` - `BaseRes::ok_with_msg()` method
- `src/utils/types.rs:38` - `BaseRes::error()` method
  - Standard API response builders

## 4. Rate Limiting (Future API Endpoints)

**Purpose**: Rate limiting for REST API endpoints (not yet implemented).

- `src/rate_limiter.rs:65` - `ApiRateLimiter` struct
- `src/rate_limiter.rs:72` - `ApiRateLimiter::new()` method
- `src/rate_limiter.rs:82` - `ApiRateLimiter::check()` method
- `src/rate_limiter.rs:92` - `RateLimiters` struct
- `src/rate_limiter.rs:100` - `RateLimiters::new()` method
  - Currently only login and 2FA rate limiters are actively used

## 5. Constants & Status Utilities

**Purpose**: Error types and status display functions.

- `src/utils/constants.rs:18` - `ERROR_TYPE_VALIDATION` constant
  - Error type classification (not yet used)

- `src/utils/constants.rs:33` - `status_name()` function
- `src/utils/constants.rs:45` - `status_name_short()` function
- `src/utils/constants.rs:57` - `status_color()` function
  - Stack status display helpers for UI

## 6. Database Management

**Purpose**: Database maintenance and lifecycle operations.

- `src/db/mod.rs:121` - `Database::close()` method
  - Graceful database shutdown with WAL checkpoint

- `src/db/mod.rs:138` - `Database::get_size()` method
  - Get database file size in bytes

- `src/db/mod.rs:147` - `Database::shrink()` method
  - Run VACUUM to compact database

## 7. Docker Port Parsing

**Purpose**: Parse and display Docker port mappings.

- `src/utils/docker.rs:6` - `DockerPort` struct
- `src/utils/docker.rs:32` - `parse_docker_port()` function
  - Parse various Docker port formats (3000, 8000:8000, 0.0.0.0:8080->8080/tcp, etc.)
  - Convert to URL and display string

## 8. Cryptography Utilities

**Purpose**: Random string generation, hashing, and async sleep utilities.

- `src/utils/crypto.rs:14` - `ALPHANUMERIC` constant
- `src/utils/crypto.rs:24` - `gen_secret()` function
  - Generate cryptographically secure random strings

- `src/utils/crypto.rs:44` - `get_crypto_random_int()` function
  - Secure random number generation

- `src/utils/crypto.rs:57` - `int_hash()` function
  - Simple string hashing for consistent random selection

- `src/utils/crypto.rs:70` - `sleep()` async function
  - Async sleep wrapper

## 9. Version Checking

**Purpose**: Check for software updates.

- `src/check_version.rs:22` - `VersionResponse::beta` field
  - Beta release version (currently only stable/slow channel is used)

## 10. Terminal Naming Utilities

**Purpose**: Generate consistent terminal names for different terminal types.

- `src/utils/terminal.rs:35` - `get_container_terminal_name()` function
  - Format: "container-{endpoint}-{container}"
  - May be used for container attach operations (vs exec which is currently used)

## 11. Settings Cache Management

**Purpose**: Settings caching with TTL and cleanup.

- `src/db/models/setting.rs:133` - `SettingsCache::clear()` method
  - Clear all cached settings

- `src/db/models/setting.rs:239` - `Setting::set_settings()` method
  - Bulk set multiple settings of same type

- `src/db/models/setting.rs:294` - `Setting::delete()` method
  - Delete individual setting by key

## 12. Limit Queue (Circular Buffer)

**Purpose**: Fixed-size queue for terminal output buffering.

- `src/utils/limit_queue.rs:31` - `LimitQueue::on_exceed()` method
  - Set callback for when items are evicted

- `src/utils/limit_queue.rs:56` - `LimitQueue::len()` method
- `src/utils/limit_queue.rs:72` - `LimitQueue::iter_mut()` method
- `src/utils/limit_queue.rs:78` - `LimitQueue::get()` method
- `src/utils/limit_queue.rs:84` - `LimitQueue::clear()` method
- `src/utils/limit_queue.rs:90` - `LimitQueue::limit()` method
  - Utility methods for queue management

## 13. User Management (Admin Features)

**Purpose**: User CRUD operations and authentication features.

**Read Operations:**
- `src/db/models/user.rs:51` - `User::find_all()` method
  - Get all users (admin panel)

**Password Management:**
- `src/db/models/user.rs:119` - `User::reset_password()` static method
  - Admin password reset by user ID

**Profile Updates:**
- `src/db/models/user.rs:135` - `User::update_active()` method
  - Enable/disable user account

- `src/db/models/user.rs:150` - `User::update_timezone()` method
  - Update user timezone preference

**Two-Factor Authentication:**
- `src/db/models/user.rs:169` - `User::enable_twofa()` method
- `src/db/models/user.rs:186` - `User::disable_twofa()` method
- `src/db/models/user.rs:203` - `User::update_twofa_last_token()` method
  - 2FA management (feature exists but handlers may be incomplete)

**Deletion:**
- `src/db/models/user.rs:218` - `User::delete()` method
  - Delete user account

**JWT:**
- `src/db/models/user.rs:232` - `User::create_jwt()` method
  - Create JWT token for user (used in auth flow)

## 14. Terminal System (PTY Management)

**Purpose**: Interactive terminal and shell access.

- `src/terminal.rs:169` - Unknown (needs full file read)
- `src/terminal.rs:500` - Unknown (needs full file read)
- `src/terminal.rs:632` - Unknown (needs full file read)
  - Likely terminal lifecycle or I/O methods

## 15. Agent Management (Remote Dockru Instances)

**Purpose**: Connect to and manage remote Dockru agents.

**Agent Status:**
- `src/agent_manager.rs:21` - `AgentStatus::Online` variant
- `src/agent_manager.rs:23` - `AgentStatus::Offline` variant
  - Status tracking (currently only Connecting is used)

**Agent Client:**
- `src/agent_manager.rs:41` - `AgentClient::endpoint` field
  - Endpoint identifier for connected agent

**Agent CRUD:**
- `src/db/models/agent.rs:127` - `Agent::get_agent_list()` method
  - Get all agents as HashMap keyed by endpoint

- `src/db/models/agent.rs:174` - `Agent::update_url()` method
  - Update agent connection URL

- `src/db/models/agent.rs:192` - `Agent::update_credentials()` method
  - Update agent username/password

- `src/db/models/agent.rs:219` - `Agent::update_active()` method
  - Enable/disable agent connection

---

## Summary by Feature Phase

### Phase 4 - Authentication
- ✅ Password rehashing (implemented - see `need_rehash_password()`)
- JWT utilities

### Phase 5 - Terminal System
- Terminal naming utilities
- PTY management methods

### Phase 6 - Stack Management
- Full path resolution
- Status display functions

### Phase 7 - Agent System
- Agent status tracking
- Agent credential management
- Remote agent operations

### Phase 10 - Updates
- Beta version channel support

### Future/Utilities
- API rate limiting (REST API not implemented)
- Docker port parsing
- Cryptography utilities
- Database maintenance
- Admin user management
- Settings bulk operations
