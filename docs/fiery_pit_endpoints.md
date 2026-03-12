# FieryPit REST API Endpoints

Client-side reference for all HTTP endpoints exposed by lildaemon (`goat/app/main.py`).
Used to keep the Rust `LilDaemonClient` (`fiery-pit-client`) in sync with the Python implementation.

Default base URL: `http://localhost:8000`

---

## Health & Status

### `GET /health`
Basic health check.

**Response:** `HealthCheckResponse`
```json
{ "status": "ok", "timestamp": 1704067200000, "version": "1.0.0" }
```

### `GET /status`
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
Errors: `500` if GoatWrangler not initialized; propagates evaluator manager errors.

### `GET /`
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

---

## Evaluation

### `POST /evaluate`
Evaluate input using the current active evaluator.

**Request:** `EvaluationRequest`
```json
{ "data": { "prompt": "Hello, world!" } }
```

**Response:** Tephra (varies by evaluator)
```json
{
  "timestamp": 1704067200000,
  "hohi": { "response": { ... }, "code": 200 },
  "task_id": "abc12345"
}
```
Errors: `500` if uninitialized; manager errors propagated.

---

## Evaluation Monitoring

Track in-flight evaluations via the `EvaluationRegistry`.

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
- `limit` (int, default: 50)

**Response:**
```json
{ "evaluations": [...], "count": 50 }
```

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
- `threshold` (float, optional): seconds; uses `warn_threshold` if omitted

**Response:**
```json
{ "long_running_evaluations": [...], "count": 2, "threshold_seconds": 60.0 }
```

### `GET /evaluations/{task_id}`
Get details of a specific evaluation.

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
Cancel an active evaluation. For `OllamaEvaluator` / `ToolifiedOllamaEvaluator` this also cancels
the underlying HTTP request to Ollama, freeing GPU resources.

**Response:**
```json
{
  "status": "cancelled",
  "task_id": "abc12345",
  "message": "Evaluation abc12345 has been cancelled"
}
```

### `POST /evaluations/cancel-hung`
Cancel all evaluations that appear hung.

**Response:**
```json
{ "cancelled_count": 3, "cancelled_task_ids": ["abc123", "def456", "ghi789"] }
```

---

## Hung Detector

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
Update hung detector configuration. All fields optional; only specified values are updated.

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

**Response:**
```json
{ "status": "configured", "config": { ... } }
```

---

## Evaluator Management

### `GET /evaluators`
List all available evaluators.

**Response:**
```json
{ "evaluators": ["echo", "ollama", "clips", "prolog"], "current": "echo", "count": 4 }
```

### `GET /evaluators/{evaluator_name}`
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

### `GET /evaluators/{evaluator_name}/auth-status`
Get authentication configuration status (no secrets exposed).

**Response:**
```json
{ "auth_configured": true, "auth_type": "bearer", "auth_token_env_set": true }
```

### `POST /evaluators/set`
Set the current active evaluator with optional auth configuration.

**Request:** `EvaluatorSetRequest`
```json
{
  "evaluator": "ollama",
  "params": { "ollama_url": "https://remote-ollama.example.com" },
  "auth": { "auth_type": "bearer", "auth_token_env": "OLLAMA_AUTH_TOKEN" }
}
```
`params` and `auth` are optional. Auth fields: `auth_type` ("bearer"/"basic"), `auth_token`
(direct value, never logged), `auth_token_env` (env var name).

**Response:**
```json
{ "status": "evaluator_changed", "evaluator": "ollama" }
```

### `POST /evaluators/reset`
Reset to the default echo evaluator.

### `POST /evaluators/{evaluator_name}/load`
Load/verify an evaluator with optional parameter overrides.

**Request:** `EvaluatorLoadRequest` (optional)
```json
{
  "params": { "timeout": 60.0 },
  "auth": { "auth_type": "bearer", "auth_token_env": "MY_TOKEN_VAR" }
}
```

**Response:**
```json
{ "status": "evaluator_loaded", "evaluator": "ollama", "auth_configured": true }
```

### `DELETE /evaluators/{evaluator_name}`
Unload/unregister an evaluator.

---

## Fish (Input Translators)

### `GET /fish`
List all available fish (input translators).

**Response:**
```json
{
  "fish": ["default", "stick", "json"],
  "current_assignments": { "ollama": "default", "clips": "stick" }
}
```

### `POST /evaluators/{evaluator_name}/fish`
Set the fish (input translator) for a specific evaluator.

**Request:** `FishSetRequest`
```json
{ "fish": "stick" }
```

**Response:**
```json
{ "status": "fish_set", "evaluator": "ollama", "fish": "stick" }
```

---

## Prolog Endpoints

Direct proxy to the clara-cerebellum LilDevils (Prolog) API at `/devils/sessions`.

### `POST /prolog/sessions`
Create a new Prolog session.

**Request:** `PrologCreateSessionRequest`
```json
{
  "user_id": "demo",
  "name": "my-session",
  "config": { "max_facts": 1000, "max_rules": 500, "max_memory_mb": 128 }
}
```

**Response:**
```json
{
  "session_id": "uuid",
  "user_id": "demo",
  "status": "active",
  "created_at": "2024-01-01T00:00:00Z"
}
```

### `GET /prolog/sessions`
List all Prolog sessions.

### `GET /prolog/sessions/{session_id}`
Get a specific Prolog session.

### `DELETE /prolog/sessions/{session_id}`
Terminate a Prolog session.

### `POST /prolog/sessions/{session_id}/query`
Execute a Prolog goal.

**Request:** `PrologQueryRequest`
```json
{ "goal": "member(X, [1,2,3])", "all_solutions": true }
```

**Response:**
```json
{ "result": "[{\"X\": 1}, {\"X\": 2}, {\"X\": 3}]", "success": true, "runtime_ms": 5 }
```

### `POST /prolog/sessions/{session_id}/consult`
Load Prolog clauses into a session.

**Request:** `PrologConsultRequest`
```json
{
  "clauses": [
    "parent(tom, mary).",
    "ancestor(X, Y) :- parent(X, Y).",
    "ancestor(X, Y) :- parent(X, Z), ancestor(Z, Y)."
  ]
}
```

**Response:**
```json
{ "status": "clauses_loaded", "count": 3 }
```

---

## CLIPS Endpoints

Direct proxy to the clara-cerebellum CLIPS API at `/sessions`.

### `POST /clips/sessions`
Create a new CLIPS session.

**Request:** `CLIPSCreateSessionRequest`
```json
{
  "user_id": "demo",
  "name": "my-session",
  "config": { "max_facts": 1000, "max_rules": 500, "max_memory_mb": 128 }
}
```

**Response:**
```json
{
  "session_id": "uuid",
  "user_id": "demo",
  "status": "active",
  "resources": { "facts": 0, "rules": 0 }
}
```

### `GET /clips/sessions`
List all CLIPS sessions.

### `GET /clips/sessions/{session_id}`
Get a specific CLIPS session.

### `DELETE /clips/sessions/{session_id}`
Terminate a CLIPS session.

### `POST /clips/sessions/{session_id}/evaluate`
Execute raw CLIPS code.

**Request:** `CLIPSEvalRequest`
```json
{ "script": "(printout t \"Hello from CLIPS\" crlf)", "timeout_ms": 2000 }
```

**Response:**
```json
{
  "stdout": "Hello from CLIPS\n",
  "stderr": "",
  "exit_code": 0,
  "metrics": { "elapsed_ms": 5 }
}
```

### `POST /clips/sessions/{session_id}/rules`
Load CLIPS rules into a session.

**Request:** `CLIPSLoadRulesRequest`
```json
{ "rules": ["(defrule adult (person (age ?a&:(>= ?a 18))) => (assert (is-adult)))"] }
```

**Response:** `{ "status": "rules_loaded", "count": 1 }`

### `POST /clips/sessions/{session_id}/facts`
Assert CLIPS facts into a session.

**Request:** `CLIPSLoadFactsRequest`
```json
{ "facts": ["(person (name \"John\") (age 30))"] }
```

**Response:** `{ "status": "facts_loaded", "count": 1 }`

### `GET /clips/sessions/{session_id}/facts`
Query facts from a session.

**Query Parameters:**
- `pattern` (optional) — filter pattern

**Response:**
```json
{ "matches": ["(person (name \"John\") (age 30))"], "count": 1 }
```

### `POST /clips/sessions/{session_id}/run`
Run the CLIPS rule engine.

**Request:** `CLIPSRunRequest`
```json
{ "max_iterations": -1 }
```
`-1` = unlimited; positive integer = cap.

**Response:**
```json
{ "rules_fired": 5, "status": "completed", "runtime_ms": 10 }
```

---

## Error Handling

### Error Response Format

```json
{ "error": "Error message here", "timestamp": 1704067200000, "path": "/evaluate" }
```

### Tephra Error Format (from `/evaluate`)

```json
{
  "timestamp": 1704067200000,
  "tabu": { "message": "Session not found", "code": 404, "details": { ... } }
}
```

### Common Status Codes

| Code | Meaning |
|------|---------|
| 400  | Bad request (invalid input) |
| 404  | Resource not found (session, evaluator) |
| 408  | Request timeout (Ollama took too long) |
| 499  | Client disconnected (evaluation cancelled) |
| 500  | Internal server error |
| 503  | Service unavailable (backend not reachable) |

---

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `OLLAMA_URL` | `http://localhost:11434` | Ollama server URL |
| `CEREBELLUM_URL` | `http://localhost:8080` | clara-cerebellum URL (CLIPS/Prolog backend) |

---

*Last updated: 2026-03-11*
