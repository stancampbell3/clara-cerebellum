# Session Lifecycle and Management

## Overview

Clara Cerebellum supports multiple concurrent CLIPS sessions, each maintaining independent rule sets, facts, and execution state. This document describes the complete session lifecycle from creation to termination.

## Session Architecture

```
┌──────────────────────────────────────┐
│      SessionManager                  │
│  ┌────────────────────────────────┐  │
│  │  sessions: HashMap<ID, Session>│  │
│  └────────────────────────────────┘  │
│           │                          │
│           ├─▶ Session A (id: abc123) │
│           │    ├─ ClipsEnvironment   │
│           │    ├─ Facts              │
│           │    └─ Rules              │
│           │                          │
│           ├─▶ Session B (id: def456) │
│           │    ├─ ClipsEnvironment   │
│           │    ├─ Facts              │
│           │    └─ Rules              │
│           │                          │
│           └─▶ Session C (id: ghi789) │
│                ├─ ClipsEnvironment   │
│                ├─ Facts              │
│                └─ Rules              │
└──────────────────────────────────────┘
```

---

## Lifecycle States

```
               ┌──────────────┐
               │ Initializing │
               └──────┬───────┘
                      │
                      ▼
               ┌─────────────┐
          ┌───▶│   Active    │◀───┐
          │    └──────┬──────┘    │
          │           │           │
          │           ▼           │
          │    ┌─────────────┐   │
          │    │  Evaluating │───┘
          │    └──────┬──────┘
          │           │
          │           ▼
          │    ┌─────────────┐
          └────│    Idle     │
               └──────┬──────┘
                      │
                      ▼
               ┌─────────────┐
               │   Paused*   │
               └──────┬──────┘
                      │
                      ▼
               ┌─────────────┐
               │ Terminated  │
               └─────────────┘

* Paused state reserved for future pause/resume functionality
```

### State Descriptions

**Initializing**: CLIPS environment being created
**Active**: Ready to accept commands, rules, and queries
**Evaluating**: Currently executing CLIPS code (eval or run)
**Idle**: Session exists but not actively used (future: timeout cleanup)
**Paused**: Reserved for future pause/resume functionality
**Terminated**: All resources freed, session no longer accessible

---

## Session Creation

### API Endpoint

```http
POST /sessions
Content-Type: application/json

{
  "user_id": "required_user_id",
  "name": "optional_session_name",
  "config": {
    "max_facts": 1000,
    "max_rules": 500,
    "max_memory_mb": 128
  },
  "preload": ["file1.clp"],
  "metadata": {"key": "value"}
}
```

**Note**: Our implementation requires `user_id` for multi-user session management. The `name` field is optional for human-readable session identification.

### Response

```http
HTTP/1.1 201 Created
Content-Type: application/json

{
  "session_id": "sess-abc123def456",
  "user_id": "user-123",
  "started": "2025-12-21T10:30:00Z",
  "touched": "2025-12-21T10:30:00Z",
  "status": "active",
  "resources": {
    "facts": 0,
    "rules": 0,
    "objects": 0
  },
  "limits": {
    "facts": 1000,
    "rules": 500,
    "objects": 0,
    "memory_mb": 128
  }
}
```

### Implementation Flow

```rust
// clara-session/src/manager.rs
impl SessionManager {
    pub fn create_session(&mut self, config: SessionConfig) -> Result<SessionId> {
        // 1. Generate unique session ID
        let session_id = SessionId::new();

        // 2. Create CLIPS environment
        let env = ClipsEnvironment::new()?;

        // 3. Initialize session state
        let session = Session {
            id: session_id.clone(),
            env,
            created_at: Utc::now(),
            status: SessionStatus::Initialized,
            config,
        };

        // 4. Store in manager
        self.sessions.insert(session_id.clone(), session);

        // 5. Return ID
        Ok(session_id)
    }
}
```

---

## Session Initialization

### Loading Rules

```http
POST /sessions/{session_id}/rules
Content-Type: application/json

{
  "rules": [
    "(defrule example (fact ?x) => (printout t ?x crlf))",
    "(defrule another (data ?y) => (assert (processed ?y)))"
  ]
}
```

### Loading Facts

```http
POST /sessions/{session_id}/facts
Content-Type: application/json

{
  "facts": [
    "(fact value1)",
    "(data value2)"
  ]
}
```

### Implementation

```rust
impl Session {
    pub fn load_rules(&mut self, rules: Vec<String>) -> Result<()> {
        for rule in rules {
            self.env.eval(&rule)?;
        }
        Ok(())
    }

    pub fn load_facts(&mut self, facts: Vec<String>) -> Result<()> {
        for fact in facts {
            self.env.eval(&format!("(assert {})", fact))?;
        }
        Ok(())
    }
}
```

---

## Active Session Operations

### Evaluate Expression

```http
POST /sessions/{session_id}/evaluate
Content-Type: application/json

{
  "script": "(printout t \"Hello\" crlf)",
  "timeout_ms": 2000
}
```

**Response**:
```json
{
  "stdout": "Hello\n",
  "stderr": "",
  "exit_code": 0,
  "metrics": {
    "elapsed_ms": 125,
    "facts_added": null,
    "rules_fired": null
  },
  "session": null
}
```

### Run Rules

```http
POST /sessions/{session_id}/run
Content-Type: application/json

{
  "max_iterations": 100
}
```

**Response**:
```json
{
  "rules_fired": 42,
  "status": "completed",
  "runtime_ms": 125
}
```

### Query Facts

```http
GET /sessions/{session_id}/facts?pattern=(data%20?x)
```

**Response**:
```json
{
  "matches": [
    "(data value1)",
    "(data value2)"
  ],
  "count": 2
}
```

**Note**: Pattern matching is simplified in current implementation. Use `(find-all-facts ...)` in evaluate endpoint for advanced queries.

---

## Session State Management

### Get Session Info

```http
GET /sessions/{session_id}
```

**Response**:
```json
{
  "session_id": "sess-abc123",
  "user_id": "user-123",
  "started": "2025-12-21T10:30:00Z",
  "touched": "2025-12-21T10:35:22Z",
  "status": "active",
  "resources": {
    "facts": 127,
    "rules": 15,
    "objects": 0
  },
  "limits": {
    "facts": 1000,
    "rules": 500,
    "objects": 0,
    "memory_mb": 128
  }
}
```

**Implementation Note**: Session statistics (rules_fired_total, evaluations_total, last_activity) are tracked in the session metadata but not yet exposed in this endpoint response. Will be added in a future update.

### List All Sessions

```http
GET /sessions
```

**Response**:
```json
{
  "sessions": [
    {
      "session_id": "sess-abc123",
      "user_id": "user-123",
      "started": "2025-12-21T10:30:00Z",
      "touched": "2025-12-21T10:35:00Z",
      "status": "active",
      "resources": {
        "facts": 127,
        "rules": 15,
        "objects": 0
      },
      "limits": {
        "facts": 1000,
        "rules": 500,
        "objects": 0,
        "memory_mb": 128
      }
    },
    {
      "session_id": "sess-def456",
      "user_id": "user-456",
      "started": "2025-12-21T09:15:00Z",
      "touched": "2025-12-21T09:20:00Z",
      "status": "idle",
      "resources": {
        "facts": 45,
        "rules": 8,
        "objects": 0
      },
      "limits": {
        "facts": 1000,
        "rules": 500,
        "objects": 0,
        "memory_mb": 128
      }
    }
  ],
  "total": 2
}
```

### List User Sessions

```http
GET /sessions/user/{user_id}
```

Returns sessions for a specific user only. Same response format as listing all sessions.

---

## Session Persistence

### Save Session State

```http
POST /sessions/{session_id}/save
Content-Type: application/json

{
  "user_id": "user-123",
  "session_id": "sess-abc123",
  "metadata": {"note": "checkpoint_before_changes"}
}
```

**Response**:
```json
{
  "status": "saved"
}
```

**Current Implementation**: Session save updates the session metadata but does not yet create versioned snapshots. Persistence is in-memory only.

### Restore Session (Planned)

```http
POST /sessions/restore
Content-Type: application/json

{
  "snapshot_id": "snap_xyz789"
}
```

**Status**: Not yet implemented. Planned for future release.

### Persistence Backends

**Memory** (current):
- Session state kept in RAM via `SessionStore`
- Lost on server restart
- Fast access
- No disk persistence

**Disk** (planned):
- CLIPS save/load to `.clp` files
- Survives server restart
- Slower than memory

**Database** (future):
- PostgreSQL or SQLite
- Full query capabilities
- Versioned snapshots

---

## Session Isolation

### Resource Separation

Each session has independent:
- **CLIPS environment** - Separate C environment pointer
- **Fact base** - No fact sharing between sessions
- **Rule base** - No rule sharing between sessions
- **Execution state** - Independent agenda and activations

### Memory Isolation

```rust
impl SessionManager {
    pub fn get_session(&self, id: &SessionId) -> Option<&Session> {
        self.sessions.get(id)
    }

    pub fn get_session_mut(&mut self, id: &SessionId) -> Option<&mut Session> {
        self.sessions.get_mut(id)
    }
}
```

Sessions are accessed via HashMap, ensuring:
- No cross-session interference
- Thread-safe access via manager lock
- Independent cleanup on termination

---

## Session Termination

### Explicit Termination

```http
DELETE /sessions/{session_id}
```

**Response**:
```http
HTTP/1.1 204 No Content
```

**Implementation**:
```rust
impl SessionManager {
    pub fn terminate_session(&mut self, id: &SessionId) -> Result<()> {
        // 1. Get session
        let mut session = self.sessions.remove(id)
            .ok_or(ManagerError::SessionNotFound)?;

        // 2. Change status
        session.status = SessionStatus::Terminating;

        // 3. Clear CLIPS environment
        session.env.clear()?;

        // 4. Drop session (RAII cleanup)
        drop(session);

        Ok(())
    }
}
```

### Automatic Cleanup (Planned)

**Timeout-based cleanup**: Not yet implemented. Future enhancement will add:
- Configurable idle timeout
- Background cleanup task
- Automatic termination of stale sessions

**Resource-based cleanup**: Planned features:
- Sessions exceeding memory limits
- Sessions with too many facts/rules
- Sessions in error state

**Current Status**: Resource limits are tracked but not enforced. Sessions must be explicitly terminated via `DELETE /sessions/{session_id}`.

---

## Concurrency and Thread Safety

### Manager-Level Locking

```rust
lazy_static! {
    static ref SESSION_MANAGER: Mutex<SessionManager> =
        Mutex::new(SessionManager::new());
}

pub fn with_session<F, R>(id: &SessionId, f: F) -> Result<R>
where
    F: FnOnce(&mut Session) -> Result<R>,
{
    let mut manager = SESSION_MANAGER.lock().unwrap();
    let session = manager.get_session_mut(id)
        .ok_or(ManagerError::SessionNotFound)?;
    f(session)
}
```

### Concurrent Session Access

**Safe**: Multiple sessions can execute simultaneously
```
Session A (thread 1) ─▶ CLIPS env A
Session B (thread 2) ─▶ CLIPS env B
```

**Unsafe**: Single session accessed by multiple threads
```
Session A (thread 1) ─┐
                      ├─▶ CLIPS env A  ❌ RACE CONDITION
Session A (thread 2) ─┘
```

**Solution**: Manager-level mutex ensures sequential access per session

---

## Session Lifecycle Hooks (Planned)

### Pre-Creation Hook (Not Implemented)

Planned feature to add validation and logging hooks before session creation.

**Planned use cases**:
- Validate session configuration
- Check resource limits
- Log session creation
- Enforce quotas

### Post-Termination Hook (Not Implemented)

Planned feature to add cleanup and notification hooks after session termination.

**Planned use cases**:
- Persist final state
- Collect metrics
- Notify external systems
- Archive session data

---

## Error Handling

### Session Creation Errors

```json
{
  "error": "ResourceLimitExceeded",
  "message": "Maximum concurrent sessions (100) reached",
  "details": {
    "current_sessions": 100,
    "limit": 100
  }
}
```

### Session Not Found

```http
HTTP/1.1 404 Not Found

{
  "error": "SessionNotFound",
  "message": "Session abc123 does not exist",
  "hint": "Session may have been terminated or expired"
}
```

### Session In Use

```http
HTTP/1.1 409 Conflict

{
  "error": "SessionInUse",
  "message": "Session abc123 is currently evaluating",
  "hint": "Wait for evaluation to complete or use async API"
}
```

---

## Monitoring and Metrics

### Session Metrics

**Per-session**:
- Creation timestamp
- Last activity timestamp
- Total evaluations
- Total rules fired
- Current fact count
- Current rule count
- Memory usage

**Manager-level**:
- Total sessions created (lifetime)
- Active session count
- Total memory usage
- Average session duration
- Session creation/termination rate

### Health Check Endpoint

```http
GET /health/sessions
```

**Response**:
```json
{
  "status": "healthy",
  "active_sessions": 42,
  "total_memory_mb": 1234,
  "oldest_session_age_seconds": 86400,
  "sessions_created_last_hour": 15
}
```

---

## Best Practices

### For API Users

1. **Always delete sessions when done**:
```javascript
const session_id = await createSession();
try {
  await loadRules(session_id, rules);
  await runRules(session_id);
} finally {
  await deleteSession(session_id);  // Ensure cleanup
}
```

2. **Use session names for debugging**:
```json
{
  "name": "user_12345_weather_query",
  "config": {...}
}
```

3. **Check session status before operations**:
```javascript
const session = await getSession(session_id);
if (session.status !== "active") {
  throw new Error("Session not ready");
}
```

### For Server Operators

1. **Configure session timeout**:
```toml
[sessions]
idle_timeout_seconds = 3600
max_concurrent = 100
```

2. **Monitor session churn**:
- High creation rate might indicate leaks
- Long-lived sessions might indicate unused resources

3. **Regular cleanup**:
```rust
// Run every 5 minutes
setInterval(() => {
    manager.cleanup_idle_sessions(Duration::from_secs(3600));
}, Duration::from_secs(300));
```

---

## Implementation Status

### Implemented (Priority 1 & 2)

✅ Session creation with user_id and optional name
✅ Session configuration with resource limits (tracked, not yet enforced)
✅ Session termination
✅ Rule loading via `POST /sessions/{session_id}/rules`
✅ Fact loading via `POST /sessions/{session_id}/facts`
✅ Expression evaluation via `POST /sessions/{session_id}/evaluate`
✅ Run rules via `POST /sessions/{session_id}/run`
✅ Query facts via `GET /sessions/{session_id}/facts`
✅ List all sessions via `GET /sessions`
✅ List user sessions via `GET /sessions/user/{user_id}`
✅ Session status tracking (Initializing, Active, Evaluating, Idle, Paused, Terminated)
✅ Session statistics (rules_fired_total, evaluations_total, last_activity)
✅ Basic session isolation with separate CLIPS environments
✅ In-memory state management via SessionStore

### Planned (Priority 3)

📋 Snapshot versioning system
📋 Restore from snapshot
📋 Disk persistence (CLIPS save/load)
📋 Database persistence backend

### Planned (Priority 4)

📋 Resource limits enforcement
📋 Automatic idle timeout cleanup
📋 Resource-based eviction policies

### Planned (Priority 5)

📋 Lifecycle hooks (pre-creation, post-termination)
📋 Session templates
📋 Session migration between servers
📋 Distributed session management

---

## Troubleshooting

### Session Creation Fails

**Symptom**: 500 error on `POST /sessions`

**Causes**:
- CLIPS environment initialization failure
- Memory allocation failure
- Resource limits exceeded

**Debug**:
```bash
RUST_LOG=debug cargo run
# Check logs for CLIPS initialization errors
```

### Session State Inconsistent

**Symptom**: Facts disappear or rules don't fire

**Causes**:
- Concurrent access without proper locking
- Unhandled errors during evaluation
- CLIPS environment corruption

**Debug**:
1. Check session status
2. Verify single-threaded access
3. Enable CLIPS debug output

### Memory Leak

**Symptom**: Memory usage grows continuously

**Causes**:
- Sessions not terminated
- CLIPS environments not freed
- String allocations not freed (FFI callbacks)

**Debug**:
```bash
# Monitor session count
curl http://localhost:8080/health/sessions

# Check for orphaned sessions
# All sessions should eventually be terminated
```

---

## Reference

### Related Documentation

- `ARCHITECTURE.md` - Overall system design
- `CLIPS_CALLBACKS.md` - Callback system details
- `fiery_pit_endpoints.md` - Complete API reference

### Source Code

- Session manager: `clara-session/src/manager.rs`
- Session struct: `clara-session/src/session.rs`
- API handlers: `clara-api/src/handlers/`
- Tests: `clara-session/tests/`
