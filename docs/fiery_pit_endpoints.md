# FieryPit REST API Endpoints

Complete reference for all HTTP endpoints implemented in `goat/app/main.py`.

## Core Endpoints

### Health & Status

#### `GET /health`
Basic health check.

**Response:** `HealthCheckResponse`
```json
{
  "status": "ok",
  "timestamp": 1704067200000,
  "version": "1.0.0"
}
```

#### `GET /status`
Current FieryPit status including active evaluator.

**Response:** `StatusResponse`
```json
{
  "status": "ok",
  "current_evaluator": "echo",
  "available_evaluators": 5,
  "timestamp": 1704067200000
}
```

#### `GET /`
API metadata and endpoint listing.

**Response:**
```json
{
  "name": "FieryPit REST API",
  "version": "1.0.0",
  "description": "REST API for FieryPit REPL Engine with Disdomain Registry",
  "endpoints": { ... }
}
```

### Evaluation

#### `POST /evaluate`
Evaluate input using the current active evaluator.

**Request:** `EvaluationRequest`
```json
{
  "data": {
    "prompt": "Hello, world!"
  }
}
```

**Response:** Tephra response (varies by evaluator)
```json
{
  "timestamp": 1704067200000,
  "hohi": {
    "response": { ... },
    "code": 200
  },
  "task_id": "abc12345"
}
```

---

## Evaluation Monitoring

Track and manage in-flight evaluations via the EvaluationRegistry.

### `GET /evaluations/active`
List all currently running evaluations.

**Response:**
```json
{
  "active_evaluations": [
    {
      "task_id": "abc12345",
      "evaluator": "ollama",
      "model": "llama2",
      "prompt_preview": "What is Python?...",
      "started_at": 1704067200.0,
      "status": "running",
      "duration_ms": 5000
    }
  ],
  "count": 1
}
```

### `GET /evaluations/stats`
Get evaluation statistics.

**Response:**
```json
{
  "running": 1,
  "completed": 42,
  "cancelled": 2,
  "failed": 3,
  "total_tracked": 48
}
```

### `GET /evaluations/history`
Get recent evaluation history.

**Query Parameters:**
- `limit` (int, default: 50): Maximum evaluations to return

**Response:**
```json
{
  "evaluations": [...],
  "count": 50
}
```

### `GET /evaluations/{task_id}`
Get details of a specific evaluation by task ID.

**Response:**
```json
{
  "task_id": "abc12345",
  "evaluator": "ollama",
  "model": "llama2",
  "prompt_preview": "What is...",
  "started_at": 1704067200.0,
  "completed_at": 1704067205.0,
  "status": "completed",
  "duration_ms": 5000
}
```

### `DELETE /evaluations/{task_id}`
Cancel an active evaluation. For async-capable evaluators (OllamaEvaluator, ToolifiedOllamaEvaluator), this also cancels the underlying HTTP request to Ollama, freeing GPU resources.

**Response:**
```json
{
  "status": "cancelled",
  "task_id": "abc12345",
  "message": "Evaluation abc12345 has been cancelled"
}
```

---

## Hung Detection

Monitor and handle evaluations that appear stuck.

### `GET /evaluations/hung`
List evaluations that have exceeded the hung threshold.

**Response:**
```json
{
  "hung_evaluations": [...],
  "count": 1,
  "threshold_seconds": 300.0
}
```

### `GET /evaluations/long-running`
List evaluations running longer than a threshold.

**Query Parameters:**
- `threshold` (float, optional): Custom threshold in seconds (uses warn_threshold if not specified)

**Response:**
```json
{
  "long_running_evaluations": [...],
  "count": 2,
  "threshold_seconds": 60.0
}
```

### `POST /evaluations/cancel-hung`
Cancel all evaluations that appear hung, freeing up resources.

**Response:**
```json
{
  "cancelled_count": 3,
  "cancelled_task_ids": ["abc123", "def456", "ghi789"]
}
```

### `GET /hung-detector/status`
Get hung detector status and configuration.

**Response:**
```json
{
  "running": true,
  "config": {
    "hung_threshold_seconds": 300.0,
    "auto_cancel_hung": false,
    "check_interval_seconds": 10.0,
    "warn_threshold_seconds": 60.0,
    "critical_threshold_seconds": 180.0
  },
  "current_hung_count": 0,
  "current_long_running_count": 1
}
```

### `POST /hung-detector/configure`
Update hung detector configuration.

**Request:**
```json
{
  "hung_threshold_seconds": 300.0,
  "auto_cancel_hung": false,
  "check_interval_seconds": 10.0,
  "warn_threshold_seconds": 60.0,
  "critical_threshold_seconds": 180.0
}
```

All fields are optional; only specified values are updated.

**Response:**
```json
{
  "status": "configured",
  "config": { ... }
}
```

---

### Evaluator Management

#### `GET /evaluators`
List all available evaluators.

**Response:**
```json
{
  "evaluators": ["echo", "ollama", "clips", "prolog"],
  "current": "echo",
  "count": 4
}
```

#### `GET /evaluators/{evaluator_name}`
Get detailed information about a specific evaluator.

**Response:**
```json
{
  "name": "clips",
  "module": "goat.evaluators.custom",
  "class": "CLIPSEvaluator",
  "description": "CLIPS expert system evaluator",
  "parameters": { ... }
}
```

#### `GET /evaluators/{evaluator_name}/auth-status`
Get authentication configuration status for an evaluator (without exposing secrets).

**Response:**
```json
{
  "auth_configured": true,
  "auth_type": "bearer",
  "auth_token_env_set": true
}
```

#### `POST /evaluators/set`
Set the current active evaluator with optional auth configuration.

**Request:** `EvaluatorSetRequest`
```json
{
  "evaluator": "ollama",
  "params": {
    "ollama_url": "https://remote-ollama.example.com"
  },
  "auth": {
    "auth_type": "bearer",
    "auth_token_env": "OLLAMA_AUTH_TOKEN"
  }
}
```

All fields except `evaluator` are optional. The `auth` object supports:
- `auth_type`: "bearer" or "basic"
- `auth_token`: Direct token value (sensitive - not logged)
- `auth_token_env`: Environment variable name containing the token

**Response:**
```json
{
  "status": "evaluator_changed",
  "evaluator": "ollama"
}
```

#### `POST /evaluators/reset`
Reset to default echo evaluator.

#### `POST /evaluators/{evaluator_name}/load`
Load/verify an evaluator is available with optional parameter overrides.

**Request:** `EvaluatorLoadRequest` (optional)
```json
{
  "params": {
    "timeout": 60.0
  },
  "auth": {
    "auth_type": "bearer",
    "auth_token_env": "MY_TOKEN_VAR"
  }
}
```

**Response:**
```json
{
  "status": "evaluator_loaded",
  "evaluator": "ollama",
  "auth_configured": true
}
```

#### `DELETE /evaluators/{evaluator_name}`
Unload/unregister an evaluator.

---

## Codex Cobbler: Authentication & User Management

These endpoints support the browser-based REPL (Codex Cobbler). All `/repl/*` endpoints require a valid JWT obtained via `/auth/token`.

### Auth Endpoints

#### `POST /auth/register`
Create a new user account.

**Request:** `UserCreate`
```json
{
  "username": "alice",
  "password": "s3cr3tword"
}
```

Constraints: username 3–32 chars (alphanumeric, hyphens, underscores); password ≥ 8 chars.

**Response:** `User` (201 Created)
```json
{
  "user_id": "uuid-here",
  "username": "alice",
  "created_at": "2026-01-01T00:00:00Z",
  "is_active": true,
  "role": "user"
}
```

#### `POST /auth/token`
Login and obtain a JWT. Accepts OAuth2 password form data.

**Request:** `application/x-www-form-urlencoded`
```
username=alice&password=s3cr3tword
```

**Response:** `Token`
```json
{
  "access_token": "<jwt>",
  "token_type": "bearer",
  "expires_in": 3600
}
```

Returns `401` for bad credentials, `403` if account is inactive.

#### `GET /auth/me`
Get the current user's profile. Requires `Authorization: Bearer <jwt>`.

**Response:** `User`
```json
{
  "user_id": "uuid-here",
  "username": "alice",
  "created_at": "2026-01-01T00:00:00Z",
  "is_active": true,
  "role": "user"
}
```

---

### Per-User Configuration

Users can store per-user YAML configs that control which evaluators and fish are available to their REPL sessions. Three independent config slots exist: allowed-evaluators (simple list), full evaluator config, and fish config.

#### `GET /auth/me/config`
Get the user's allowed-evaluator config and parsed evaluator list.

**Response:**
```json
{
  "config_yaml": "allowed_evaluators:\n  - ollama\n  - clips\n",
  "allowed_evaluators": ["ollama", "clips"]
}
```

#### `PUT /auth/me/config`
Upload or replace the allowed-evaluator config YAML.

**Request:**
```json
{
  "config_yaml": "allowed_evaluators:\n  - ollama\n  - clips\n"
}
```

**Response:**
```json
{
  "status": "ok",
  "allowed_evaluators": ["ollama", "clips"]
}
```

Returns `422` if the YAML is invalid.

#### `GET /auth/me/evaluator-config`
Get the user's full evaluators.yaml-format config.

**Response:**
```json
{
  "evaluator_config_yaml": "evaluators:\n  - name: ollama\n    ...\n",
  "evaluator_names": ["ollama"]
}
```

#### `PUT /auth/me/evaluator-config`
Upload or replace the full evaluator config YAML (same format as `config/evaluators.yaml`).

**Request:**
```json
{
  "evaluator_config_yaml": "evaluators:\n  - name: ollama\n    module: goat.evaluators\n    class: OllamaEvaluator\n"
}
```

**Response:**
```json
{
  "status": "ok",
  "evaluator_names": ["ollama"]
}
```

#### `DELETE /auth/me/evaluator-config`
Clear the user's full evaluator config (reverts to server defaults). Returns `204 No Content`.

#### `GET /auth/me/fish-config`
Get the user's fish (input translator) config.

**Response:**
```json
{
  "fish_config_yaml": "fish:\n  - name: stick\n    ...\n",
  "fish_names": ["stick"]
}
```

#### `PUT /auth/me/fish-config`
Upload or replace the fish config YAML (same format as `config/fish.yaml`).

**Request:**
```json
{
  "fish_config_yaml": "fish:\n  - name: stick\n    ...\n"
}
```

**Response:**
```json
{
  "status": "ok",
  "fish_names": ["stick"]
}
```

#### `DELETE /auth/me/fish-config`
Clear the user's fish config (reverts to server defaults). Returns `204 No Content`.

---

## Codex Cobbler: Web REPL Sessions

Server-side REPL sessions for the browser-based interface. Each session owns an isolated `GoatWrangler` with its own evaluator slots, fish assignments, and per-slot `BleatSession` history. All endpoints require `Authorization: Bearer <jwt>`.

Session ownership is enforced: accessing another user's session returns `403`.

### Session Lifecycle

#### `POST /repl/sessions`
Create a new REPL session for the current user. Allowed evaluators are read from the user's config; the request body can override them.

**Request:** (optional)
```json
{
  "allowed_evaluators": ["ollama", "clips"]
}
```

If `allowed_evaluators` is omitted, the server reads the user's stored config.

**Response:** (201 Created) Session state dict
```json
{
  "session_id": "uuid-here",
  "user_id": "alice-uuid",
  "created_at": "2026-01-01T00:00:00Z",
  "last_active_at": "2026-01-01T00:00:00Z",
  "focused_slot": null,
  "active_slots": [],
  "allowed_evaluators": ["ollama", "clips"]
}
```

#### `GET /repl/sessions`
List all active sessions belonging to the current user.

**Response:** Array of session state dicts.

#### `GET /repl/sessions/{session_id}`
Get current state of a session.

**Response:** Session state dict (same shape as create response).

#### `DELETE /repl/sessions/{session_id}`
Delete a session. Returns `204 No Content`.

---

### Sending Messages

#### `POST /repl/sessions/{session_id}/send`
Send a message or `!command` to a session (REST, non-streaming).

**Request:**
```json
{
  "text": "What is 2+2?"
}
```

For commands, prefix with `!`:
```json
{
  "text": "!summon ollama"
}
```

**Response:** WS-message-shaped dict
```json
{
  "type": "done",
  "text": "4",
  "slot": "ollama",
  "tephra": { ... }
}
```

For commands:
```json
{
  "type": "command",
  "text": "Evaluator `ollama` set and focused.",
  "session_state": { ... }
}
```

`session_state` is included when the command mutates session state (`!summon`, `!invoke`, `!focus`, `!close`, `!fish`, `!reset`, `!model`).

#### `POST /repl/sessions/{session_id}/fish`
Set the fish (input translator) for the focused slot in a session. Requires `Authorization: Bearer <jwt>`.

**Request:**
```json
{
  "fish": "stick"
}
```

**Response:** Updated session state dict (same shape as create response).

Returns `400` if no slot is focused or the fish name is not found.

#### `GET /repl/sessions/{session_id}/history`
Fetch message history for a slot.

**Query Parameters:**
- `slot` (optional): Slot name to query. Defaults to the currently focused slot.

**Response:**
```json
{
  "slot": "ollama",
  "history": [
    {"role": "user", "content": "What is 2+2?", "timestamp": 1704067200.0},
    {"role": "assistant", "content": "4", "timestamp": 1704067201.0}
  ]
}
```

---

### WebSocket Streaming

#### `WS /repl/sessions/{session_id}/stream?token=<jwt>`
Persistent WebSocket connection for streaming REPL interaction. The JWT is passed as a query parameter (not a header) since WebSocket connections cannot set custom headers.

**Connection:** `ws://host/repl/sessions/{session_id}/stream?token=<jwt>`

Closes with code `1008` if the token is missing, invalid/expired, the session is not found, or the caller doesn't own the session.

**Client → Server messages:**

| `type` | Fields | Description |
|--------|--------|-------------|
| `send` | `text` | Evaluate text or run a `!command` |
| `ping` | — | Keepalive ping |

```json
{"type": "send", "text": "Hello, Ollama!"}
{"type": "ping"}
```

**Server → Client messages:**

| `type` | Fields | Description |
|--------|--------|-------------|
| `thinking` | `evaluator`, `slot` | Evaluation started (before result arrives) |
| `progress` | `stats` | Periodic stats while evaluation is in flight (every 0.5 s) |
| `done` | `text`, `slot`, `tephra`, `stats`, `elapsed_ms` | Evaluation completed successfully |
| `command` | `text` | Command output (markdown) |
| `error` | `text` | Evaluation or command error |
| `pong` | — | Response to ping |
| `session_state` | `state` | Updated session state (sent after mutating commands) |

`stats` shape (both `progress` and `done`):
```json
{
  "ctx_messages": 10,
  "ctx_chars": 4200,
  "tool_uses_session": 3,
  "think_last": 512,
  "think_session": 1024
}
```

```json
{"type": "thinking", "evaluator": "ollama", "slot": "ollama"}
{"type": "progress", "stats": {"ctx_messages": 5, "ctx_chars": 2000, "tool_uses_session": 0, "think_last": 0, "think_session": 0}}
{"type": "done", "text": "The answer is 42.", "slot": "ollama", "tephra": {...}, "stats": {...}, "elapsed_ms": 3241}
{"type": "session_state", "state": {...}}
```

Client disconnect cancels any in-flight evaluation task.

---

### REPL Session Commands

Commands are prefixed with `!` and work identically over both the REST `/send` endpoint and the WebSocket.

| Command | Description |
|---------|-------------|
| `!help` | Show command reference |
| `!status` | Current evaluator, fish, slot summary |
| `!list` | List evaluators available to this session |
| `!summon <name>` | Spawn evaluator in new slot and focus it |
| `!invoke <name>` | Spawn evaluator in new slot (no focus change) |
| `!focus <name>` | Switch focus to a spawned slot |
| `!close <name>` | Close (unload) a slot |
| `!slots` | List all active slots with fish and history counts |
| `!fish <name>` | Set fish translator for the focused slot |
| `!sonar` | List available fish |
| `!model <name>` | Set model on the focused evaluator |
| `!reset` | Close all slots, revert to echo |
| `!clear [slot]` | Clear message history for a slot (default: focused) |

---

### REPL Discovery Endpoints

These endpoints list the evaluators and fish available to the authenticated user, respecting per-user and per-session `allowed_evaluators` filters. They live under the `/repl` prefix.

#### `GET /repl/evaluators`
List available evaluators, filtered by the user's (or session's) allowed list.

**Query Parameters:**
- `session_id` (optional): If provided, filter by that session's allowed list instead of the user-level allowed list.

**Response:**
```json
{
  "evaluators": {
    "ollama": {
      "name": "ollama",
      "description": "Ollama LLM evaluator",
      "evaluator_class": "OllamaEvaluator"
    }
  },
  "count": 1
}
```

#### `GET /repl/fish`
List available fish (input translators).

**Response:**
```json
{
  "fish": {
    "stick": {"name": "stick"},
    "default": {"name": "default"}
  },
  "count": 2
}
```

---

## Ritual: Autonomous Kafka Consumers

Rituals attach a lildaemon to a Kafka topic as an autonomous evaluator. Each Ritual runs a persistent consumer loop that polls for Offering messages, evaluates them, and publishes Tephra responses back to the topic. All endpoints require `Authorization: Bearer <jwt>`.

#### `GET /ritual`
List all Ritual IDs this server is currently participating in.

**Response:**
```json
{
  "ritual_ids": ["ritual-abc", "ritual-xyz"],
  "count": 2
}
```

#### `POST /ritual/join`
Register this lildaemon as a participant in a Ritual. Starts an autonomous Kafka consumer. Returns `202 Accepted`.

Returns `409 Conflict` if already joined to `ritual_id`. Returns `400` if the specified evaluator is not registered.

**Request:** `RitualJoinRequest`
```json
{
  "ritual_id": "ritual-abc",
  "topic": "my-kafka-topic",
  "bootstrap_servers": "kafka:9092",
  "dis_domain": "default",
  "evaluator": "ollama",
  "session_stateful": false,
  "eval_timeout_s": 30.0
}
```

Fields:
- `ritual_id` — unique identifier for this ritual participation
- `topic` — Kafka topic to consume
- `bootstrap_servers` — Kafka bootstrap server address(es)
- `dis_domain` — Disdomain context label
- `evaluator` (optional) — evaluator name; must be registered in Disdomain
- `session_stateful` (default: `false`) — whether to maintain session state across messages
- `eval_timeout_s` (default: `30.0`) — per-evaluation timeout in seconds

**Response:**
```json
{
  "ritual_id": "ritual-abc",
  "status": "joined",
  "evaluator": "ollama"
}
```

#### `DELETE /ritual/{ritual_id}`
Stop the Kafka consumer for a Ritual and discard its state.

Returns `404` if not currently joined to `ritual_id`.

**Response:**
```json
{
  "ritual_id": "ritual-abc",
  "status": "left"
}
```

---

## Fish (Input Translator) Endpoints

Fish are input translators that preprocess text before it reaches an evaluator. They are registered in `CrystalBowl` and configured in `config/fish.yaml`.

#### `GET /fish`
List all available fish (input translators).

**Response:**
```json
{
  "fish": ["default", "stick", "json"],
  "current_assignments": {
    "ollama": "default",
    "clips": "stick"
  }
}
```

#### `POST /evaluators/{evaluator_name}/fish`
Set the fish (input translator) for a specific evaluator.

**Request:** `FishSetRequest`
```json
{
  "fish": "stick"
}
```

**Response:**
```json
{
  "status": "fish_set",
  "evaluator": "ollama",
  "fish": "stick"
}
```

---

## Global WebSocket Evaluation

#### `WS /ws/evaluate`
Persistent WebSocket connection for multi-turn evaluation against the server's globally-active evaluator (no auth required, no per-user session). Suitable for driving a REPL session from a web frontend when per-user isolation is not needed.

**Connection:** `ws://host/ws/evaluate`

Closes with code `1011` if GoatWrangler is not initialized.

**Client → Server messages:**

| `type` | Fields | Description |
|--------|--------|-------------|
| `send` (default) | `offering` | Evaluate an offering dict |
| `ping` | — | Keepalive ping |

```json
{"offering": {"prompt": "Hello!"}}
{"type": "ping"}
```

**Server → Client messages:**

| `type` | Fields | Description |
|--------|--------|-------------|
| `thinking` | `evaluator` | Evaluation started |
| `done` | `tephra` | Evaluation completed successfully |
| `error` | `message` | Error occurred |
| `pong` | — | Response to ping |

```json
{"type": "thinking", "evaluator": "ollama"}
{"type": "done", "tephra": {...}}
```

> **Note:** For per-user isolated sessions with streaming progress, use `WS /repl/sessions/{session_id}/stream` instead.

---

## Prolog Endpoints

Direct access to the clara-cerebellum LilDevils (Prolog) API.

### Session Management

#### `POST /prolog/sessions`
Create a new Prolog session.

**Request:** `PrologCreateSessionRequest`
```json
{
  "user_id": "demo",
  "name": "my-session",
  "config": {
    "max_facts": 1000,
    "max_rules": 500,
    "max_memory_mb": 128
  }
}
```

**Response:**
```json
{
  "session_id": "uuid-here",
  "user_id": "demo",
  "status": "active",
  "created_at": "2024-01-01T00:00:00Z"
}
```

#### `GET /prolog/sessions`
List all Prolog sessions.

#### `GET /prolog/sessions/{session_id}`
Get details of a specific session.

#### `DELETE /prolog/sessions/{session_id}`
Terminate a Prolog session.

### Query Operations

#### `POST /prolog/sessions/{session_id}/query`
Execute a Prolog query.

**Request:** `PrologQueryRequest`
```json
{
  "goal": "member(X, [1,2,3])",
  "all_solutions": true
}
```

**Response:**
```json
{
  "result": "[{\"X\": 1}, {\"X\": 2}, {\"X\": 3}]",
  "success": true,
  "runtime_ms": 5
}
```

#### `POST /prolog/sessions/{session_id}/consult`
Load Prolog clauses into a session.

**Request:** `PrologConsultRequest`
```json
{
  "clauses": [
    "parent(tom, mary).",
    "parent(mary, ann).",
    "ancestor(X, Y) :- parent(X, Y).",
    "ancestor(X, Y) :- parent(X, Z), ancestor(Z, Y)."
  ]
}
```

**Response:**
```json
{
  "status": "clauses_loaded",
  "count": 4
}
```

### Using Prolog via `/evaluate`

When the `prolog` evaluator is active, you can use the unified `/evaluate` endpoint:

```json
POST /evaluate
{
  "data": {
    "consult": ["parent(tom, mary).", "ancestor(X,Y) :- parent(X,Y)."],
    "goal": "ancestor(tom, Who)",
    "all_solutions": true
  }
}
```

**Supported operations:**
- `goal` - Execute a Prolog query
- `consult` - Load clauses (can be combined with goal)
- `all_solutions` - Return all solutions (default: false)

---

## CLIPS Endpoints

Direct access to the clara-cerebellum CLIPS (expert system) API.

### Session Management

#### `POST /clips/sessions`
Create a new CLIPS session.

**Request:** `CLIPSCreateSessionRequest`
```json
{
  "user_id": "demo",
  "name": "my-session",
  "config": {
    "max_facts": 1000,
    "max_rules": 500,
    "max_memory_mb": 128
  }
}
```

**Response:**
```json
{
  "session_id": "uuid-here",
  "user_id": "demo",
  "status": "active",
  "resources": {
    "facts": 0,
    "rules": 0
  }
}
```

#### `GET /clips/sessions`
List all CLIPS sessions.

#### `GET /clips/sessions/{session_id}`
Get details of a specific session.

#### `DELETE /clips/sessions/{session_id}`
Terminate a CLIPS session.

### Rule & Fact Operations

#### `POST /clips/sessions/{session_id}/evaluate`
Execute raw CLIPS code.

**Request:** `CLIPSEvalRequest`
```json
{
  "script": "(printout t \"Hello from CLIPS\" crlf)",
  "timeout_ms": 2000
}
```

**Response:**
```json
{
  "stdout": "Hello from CLIPS\n",
  "stderr": "",
  "exit_code": 0,
  "metrics": {
    "elapsed_ms": 5
  }
}
```

#### `POST /clips/sessions/{session_id}/rules`
Load CLIPS rules into a session.

**Request:** `CLIPSLoadRulesRequest`
```json
{
  "rules": [
    "(defrule adult (person (age ?a&:(>= ?a 18))) => (assert (is-adult)))",
    "(defrule greeting (initial-fact) => (printout t \"Hello\" crlf))"
  ]
}
```

**Response:**
```json
{
  "status": "rules_loaded",
  "count": 2
}
```

#### `POST /clips/sessions/{session_id}/facts`
Assert CLIPS facts into a session.

**Request:** `CLIPSLoadFactsRequest`
```json
{
  "facts": [
    "(person (name \"John\") (age 30))",
    "(person (name \"Jane\") (age 25))"
  ]
}
```

**Response:**
```json
{
  "status": "facts_loaded",
  "count": 2
}
```

#### `GET /clips/sessions/{session_id}/facts`
Query facts from a session.

**Query Parameters:**
- `pattern` (optional) - Pattern to filter facts

**Response:**
```json
{
  "matches": [
    "(person (name \"John\") (age 30))",
    "(person (name \"Jane\") (age 25))"
  ],
  "count": 2
}
```

#### `POST /clips/sessions/{session_id}/run`
Run the CLIPS rule engine.

**Request:** `CLIPSRunRequest`
```json
{
  "max_iterations": -1
}
```

**Parameters:**
- `max_iterations`: `-1` for unlimited, positive integer for limit

**Response:**
```json
{
  "rules_fired": 5,
  "status": "completed",
  "runtime_ms": 10
}
```

### Using CLIPS via `/evaluate`

When the `clips` evaluator is active, you can use the unified `/evaluate` endpoint:

```json
POST /evaluate
{
  "data": {
    "facts": ["(person (name \"John\") (age 30))"],
    "rules": ["(defrule adult (person (age ?a&:(>= ?a 18))) => (assert (is-adult)))"],
    "run": true,
    "query_facts": true
  }
}
```

**Supported operations (executed in order):**
1. `facts` - Assert facts
2. `rules` - Load rules
3. `eval` - Execute raw CLIPS code
4. `run` - Run rule engine (true for unlimited, integer for limit)
5. `query_facts` - Query resulting facts
6. `save` - Save session state

---

## Evaluator Authentication

Ollama-based evaluators (OllamaEvaluator, ToolifiedOllamaEvaluator, ClaraMindSplinter) support authentication for remote/secured endpoints.

### Authentication Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `auth_type` | string | "bearer" or "basic" |
| `auth_token` | string | Direct token value (sensitive - never logged or returned in responses) |
| `auth_token_env` | string | Environment variable name containing the token |

### Configuring Auth via YAML

In `config/evaluators.yaml`:

```yaml
evaluators:
  - name: remote_ollama
    module: goat.evaluators
    class: OllamaEvaluator
    description: "Remote Ollama with bearer auth"
    parameters:
      ollama_url: https://remote-ollama.example.com
      auth_type: bearer
      auth_token_env: OLLAMA_AUTH_TOKEN
```

### Configuring Auth via API

**Set evaluator with auth:**
```bash
curl -X POST http://localhost:8000/evaluators/set \
  -H "Content-Type: application/json" \
  -d '{
    "evaluator": "ollama",
    "auth": {
      "auth_type": "bearer",
      "auth_token_env": "OLLAMA_AUTH_TOKEN"
    }
  }'
```

**Load evaluator with auth override:**
```bash
curl -X POST http://localhost:8000/evaluators/ollama/load \
  -H "Content-Type: application/json" \
  -d '{
    "auth": {
      "auth_type": "bearer",
      "auth_token": "your-secret-token"
    }
  }'
```

### Querying Auth Status

```bash
curl http://localhost:8000/evaluators/ollama/auth-status
```

**Response:**
```json
{
  "auth_configured": true,
  "auth_type": "bearer",
  "auth_token_env_set": true
}
```

**Note:** Actual token values are never exposed in API responses.

---

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `OLLAMA_URL` | `http://localhost:11434` | Default Ollama server URL |
| `CEREBELLUM_URL` | `http://localhost:8080` | clara-cerebellum server URL for CLIPS/Prolog |

### Custom Auth Token Variables

When using `auth_token_env`, specify any environment variable name:

```yaml
parameters:
  auth_type: bearer
  auth_token_env: MY_CUSTOM_TOKEN_VAR
```

The evaluator reads `os.environ.get("MY_CUSTOM_TOKEN_VAR")` at runtime. This allows:
- Different tokens for different evaluator configurations
- Secure token management without storing secrets in config files
- Environment-specific configurations (dev vs. prod)

---

## Error Handling

### Error Response Format

```json
{
  "error": "Error message here",
  "timestamp": 1704067200000,
  "path": "/evaluate"
}
```

### Common Error Codes

| Code | Meaning |
|------|---------|
| 400 | Bad request (invalid input) |
| 404 | Resource not found (session, evaluator) |
| 408 | Request timeout (Ollama took too long) |
| 499 | Client disconnected (evaluation cancelled) |
| 500 | Internal server error |
| 503 | Service unavailable (backend not reachable) |

### Tephra Error Response

When using `/evaluate`, errors are returned in Tephra format:

```json
{
  "timestamp": 1704067200000,
  "tabu": {
    "message": "Session not found",
    "code": 404,
    "details": { ... }
  }
}
```

---

## Implementation Notes

### Response Shapes
- Health and status endpoints return Pydantic models
- Evaluator management endpoints return manager-specific dicts
- Backend endpoints (CLIPS, Prolog) return backend response formats

### Initialization
- GoatWrangler initializes on startup
- Evaluators load from `config/evaluators.yaml`
- Backend connections are lazy-initialized

### Middleware
- Request/response logging enabled
- CORS middleware enabled (all origins)
- Custom exception handlers for HTTP and general exceptions

---

### Changelog

**2026-04 — Ritual support, command renames, WS progress, session fish endpoint**
- **Ritual endpoints**: `GET /ritual`, `POST /ritual/join`, `DELETE /ritual/{id}` — join/leave autonomous Kafka consumer loops (JWT required)
- **Global WS evaluator**: `WS /ws/evaluate` — persistent WebSocket for multi-turn evaluation against the global active evaluator
- **Session fish endpoint**: `POST /repl/sessions/{id}/fish` — set fish on the focused slot via REST (complements `!fish` command)
- **Command renames**: `!set` → `!summon`, `!spawn` → `!invoke`, `!list-evaluators` → `!list`
- **WS `progress` messages**: streaming per-slot stats (`ctx_messages`, `ctx_chars`, `tool_uses_session`, `think_last`, `think_session`) emitted every 0.5 s during evaluation
- **WS `done` enhancements**: `stats` and `elapsed_ms` fields added to all `done` messages over the session stream

**2026-04 — Codex Cobbler web REPL support**
- **JWT Auth**: `/auth/register`, `/auth/token`, `/auth/me` for user accounts and bearer tokens
- **Per-user config**: `/auth/me/config`, `/auth/me/evaluator-config`, `/auth/me/fish-config` — store allowed evaluator lists and full YAML configs per user
- **Server-side REPL sessions**: `/repl/sessions/*` — isolated `GoatWrangler` per session with ownership enforcement (403 on cross-user access)
- **REST send**: `POST /repl/sessions/{id}/send` for non-streaming message dispatch and `!command` execution
- **WebSocket streaming**: `WS /repl/sessions/{id}/stream?token=<jwt>` with `thinking`/`done`/`session_state` message protocol
- **Session history**: `GET /repl/sessions/{id}/history` for per-slot message retrieval
- **Filtered discovery**: `GET /repl/evaluators` and `GET /repl/fish` respect per-user/per-session allowed lists

**2025-01**
- **Evaluation Monitoring**: Track in-flight evaluations via `/evaluations/*` endpoints
- **Hung Detection**: Automatically detect and cancel stuck evaluations via `/hung-detector/*`
- **Authentication Configuration**: Pass auth config (bearer/basic tokens) via API when setting evaluators
- **Client Disconnection Handling**: Evaluations are cancelled when clients disconnect

*Last updated: 2026-04-16*
