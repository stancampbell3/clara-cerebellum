# Demonic Voice API

## Overview

The **Demonic Voice** is the HTTP API exposed by `clara-api`. It provides
session-managed rule engine and hybrid rule engine processing for evaluate and
deduce requests — the symbolic-reasoning analogue of an LLM chat/response
interface.

The Demonic Voice is distinct from the **Fiery Pit API** (port 6666), which
is the internal REST interface used by clara-cerebrum components to interact
with lildaemon. The Demonic Voice is the public-facing gateway through which
external callers drive CLIPS sessions, Prolog (LilDevils) sessions, and
full hybrid deduction cycles.

**Default port:** `8080`

---

## Common Structures

### SessionResponse

Returned by all session creation, retrieval, and listing endpoints.

```json
{
  "session_id": "a1b2c3d4-...",
  "user_id":    "alice",
  "started":    "2026-04-01T10:00:00Z",
  "touched":    "2026-04-01T10:05:00Z",
  "status":     "active",
  "resources": { "facts": 12, "rules": 5, "objects": 0 },
  "limits":    { "facts": 1000, "rules": 500, "objects": 0, "memory_mb": 128 }
}
```

`limits` is omitted when not set.

### TerminateResponse

```json
{
  "session_id": "a1b2c3d4-...",
  "status":     "terminated",
  "saved":      false
}
```

---

## Health Endpoints

These endpoints carry no authentication requirements and are suitable for
load-balancer and orchestrator probes.

### GET /healthz

Basic health check.

**Response `200`:**
```json
{ "status": "ok" }
```

### GET /readyz

Readiness check.

**Response `200`:**
```json
{ "status": "ready" }
```

### GET /livez

Liveness check with uptime.

**Response `200`:**
```json
{ "status": "alive", "uptime_seconds": 3820 }
```

---

## CLIPS Sessions

CLIPS sessions provide forward-chaining rule engine processing. Each session
maintains an isolated CLIPS environment with its own fact base, rules, and
resource counters.

### POST /sessions

Create a new CLIPS session.

**Request:**
```json
{
  "user_id": "alice",
  "name":    "my-session",
  "config": {
    "max_facts":     1000,
    "max_rules":     500,
    "max_memory_mb": 128
  }
}
```

`name` and `config` are optional. Default limits: 1000 facts, 500 rules, 128 MB.

**Response `201`:** `SessionResponse`

---

### GET /sessions

List all CLIPS sessions.

**Response `200`:**
```json
{
  "sessions": [ /* SessionResponse, ... */ ],
  "total":    3
}
```

---

### GET /sessions/user/{user_id}

List all CLIPS sessions for a user.

**Response `200`:** Array of `SessionResponse`.

---

### GET /sessions/{session_id}

Get details for a specific CLIPS session.

**Response `200`:** `SessionResponse`

---

### DELETE /sessions/{session_id}

Terminate a CLIPS session and release its resources.

**Response `200`:** `TerminateResponse`

---

### POST /sessions/{session_id}/evaluate

Evaluate a raw CLIPS expression in the session's environment.

**Request:**
```json
{
  "script":     "(assert (temperature 72))",
  "timeout_ms": 2000
}
```

`timeout_ms` defaults to `2000`.

**Response `200`:**
```json
{
  "stdout":    "",
  "stderr":    "",
  "exit_code": 0,
  "metrics": {
    "elapsed_ms":  4,
    "facts_added": null,
    "rules_fired": null
  }
}
```

---

### POST /sessions/{session_id}/rules

Load CLIPS constructs (`defrule`, `deftemplate`, etc.) into the session.

**Request:**
```json
{
  "rules": [
    "(defrule high-temp (temperature ?t&:(> ?t 80)) => (assert (alert high)))"
  ]
}
```

**Response `200`:**
```json
{ "status": "rules_loaded", "count": 1 }
```

---

### POST /sessions/{session_id}/facts

Assert facts into the session.

**Request:**
```json
{
  "facts": ["(temperature 85)", "(humidity 60)"]
}
```

Each element is wrapped with `(assert ...)` before execution.

**Response `200`:**
```json
{ "status": "facts_loaded", "count": 2 }
```

---

### GET /sessions/{session_id}/facts

Query facts currently in the session's fact base.

**Query parameter:** `pattern` (optional, default `?f`) — currently unused
in filtering; all facts are returned.

**Response `200`:**
```json
{
  "matches": ["(temperature 85)", "(humidity 60)"],
  "count":   2
}
```

---

### POST /sessions/{session_id}/run

Run the CLIPS rule engine for up to `max_iterations` activations.

**Request:**
```json
{
  "max_iterations": -1
}
```

`max_iterations: -1` runs until the agenda is empty (equivalent to `(run)`).
Any positive integer limits rule firings (equivalent to `(run N)`).

**Response `200`:**
```json
{
  "rules_fired": 3,
  "status":      "completed",
  "runtime_ms":  12
}
```

---

### POST /sessions/{session_id}/save

Persist the current session state.

**Request:**
```json
{
  "user_id":    "alice",
  "session_id": "a1b2c3d4-..."
}
```

**Response `200`:**
```json
{ "status": "saved" }
```

---

## LilDevils Sessions (Prolog)

LilDevils sessions provide backward-chaining logic programming via SWI-Prolog.
They share the same session lifecycle model as CLIPS sessions but operate over
a separate Prolog environment.

### POST /devils/sessions

Create a new Prolog session.

**Request:** Same schema as `POST /sessions` (`user_id`, optional `config`).

**Response `201`:** `SessionResponse`

---

### GET /devils/sessions

List all Prolog sessions.

**Response `200`:**
```json
{
  "sessions": [ /* SessionResponse, ... */ ],
  "total":    2
}
```

---

### GET /devils/sessions/{session_id}

Get a specific Prolog session. Returns `400` if the session ID belongs to a
non-Prolog session.

**Response `200`:** `SessionResponse`

---

### DELETE /devils/sessions/{session_id}

Terminate a Prolog session.

**Response `200`:** `TerminateResponse`

---

### POST /devils/sessions/{session_id}/consult

Load Prolog clauses (facts and rules) into the session's knowledge base.

Regular clauses are asserted via `assertz`. Directives (`:-`, `use_module`,
`ensure_loaded`, etc.) are executed as goals rather than asserted.

**Request:**
```json
{
  "clauses": [
    "parent(tom, mary).",
    "parent(tom, john).",
    "ancestor(X, Z) :- parent(X, Y), ancestor(Y, Z).",
    "ancestor(X, Y) :- parent(X, Y)."
  ]
}
```

**Response `200`:**
```json
{ "status": "clauses_loaded", "count": 4 }
```

---

### POST /devils/sessions/{session_id}/query

Execute a Prolog goal in the session.

**Request:**
```json
{
  "goal":         "ancestor(tom, X)",
  "all_solutions": true
}
```

`all_solutions: false` (default) returns the first solution only.

**Response `200`:**
```json
{
  "result":     "X = mary ;\nX = john",
  "success":    true,
  "runtime_ms": 2
}
```

---

## Deduction Cycles

Deduction cycle endpoints drive the hybrid Prolog↔CLIPS reasoning loop
managed by `clara-cycle`. Each cycle run is asynchronous: the caller receives
a `deduction_id` immediately and polls for completion.

A cycle converges when the Coire mailboxes are empty, the CLIPS agenda is
empty, and the Dagda tableau has reached a fixed point — or when the root
goal is resolved.

### POST /deduce

Start an asynchronous deduction run. Returns `202 Accepted` immediately.

**Request:**
```json
{
  "prolog_clauses": [
    "eligible(X) :- age(X, A), A >= 18.",
    "age(alice, 22)."
  ],
  "clips_constructs": [
    "(defrule flag-eligible (eligible ?p) => (assert (approved ?p)))"
  ],
  "clips_file":   null,
  "initial_goal": "eligible(alice)",
  "max_cycles":   100,
  "persist":      false,
  "context": [
    { "role": "user", "content": "Is alice eligible?" }
  ]
}
```

| Field | Type | Default | Description |
|---|---|---|---|
| `prolog_clauses` | `string[]` | `[]` | Prolog facts and rules to seed the Prolog engine |
| `clips_constructs` | `string[]` | `[]` | CLIPS constructs to seed the CLIPS engine |
| `clips_file` | `string\|null` | `null` | Server-side path to a `.clp` file loaded before `clips_constructs` |
| `initial_goal` | `string\|null` | `null` | Prolog goal executed on the first cycle |
| `max_cycles` | `uint\|null` | `100` | Maximum Prolog↔CLIPS cycles before aborting |
| `persist` | `bool` | `false` | Save a `DeductionSnapshot` at completion (requires Coire store) |
| `context` | `object[]` | `[]` | Conversational context injected into the session; available to Prolog via `current_context/1` and forwarded to LLM evaluate calls |

**Response `202`:**
```json
{
  "deduction_id": "f7a3e2b1-...",
  "status":       "running"
}
```

---

### GET /deduce/{id}

Poll the status of a deduction run.

**Response `200`:**
```json
{
  "deduction_id": "f7a3e2b1-...",
  "status":       "Converged",
  "cycles":       4,
  "result": {
    "status":        "Converged",
    "cycles":        4,
    "goal_bindings": "X = alice",
    "tableau":       [ /* PredicateEntry objects */ ],
    "explanation":   null
  }
}
```

`result` is `null` while the run is still in progress.

**Status values:** `running` · `Converged` · `Interrupted` · `Error(<msg>)`

**Response `404`** if the `deduction_id` is unknown.

---

### DELETE /deduce/{id}

Request interrupt of a running deduction. Sets the interrupt flag; the
background task observes it at the end of its next cycle and returns
`Interrupted`.

**Response `200`:**
```json
{
  "deduction_id": "f7a3e2b1-...",
  "status":       "interrupted"
}
```

**Response `404`** if the `deduction_id` is unknown.

---

### POST /deduce/resume

Resume a previously persisted deduction from a stored snapshot. Requires
persistence to be configured. Produces a new `deduction_id` for the resumed
run.

**Response `409 Conflict`** if the original session's engines are still active.
**Response `503 Service Unavailable`** if no Coire store is configured.

**Request:**
```json
{
  "deduction_id": "f7a3e2b1-...",
  "max_cycles":   50,
  "persist":      true,
  "context":      null
}
```

| Field | Type | Default | Description |
|---|---|---|---|
| `deduction_id` | UUID | required | `deduction_id` from the original run's response |
| `max_cycles` | `uint\|null` | snapshot value | Override the cycle budget for the resumed run |
| `persist` | `bool` | `false` | Save a new snapshot at completion to allow further chained resumes |
| `context` | `object[]\|null` | snapshot value | Override the conversational context; uses the snapshot's context if omitted |

**Response `202`:**
```json
{
  "deduction_id": "9c1d4e8f-...",
  "status":       "running"
}
```

---

### GET /deduce/{id}/snapshot

Inspect the persisted `DeductionSnapshot` for a completed deduction.
Requires persistence to be configured.

**Response `200`:** Full `DeductionSnapshot` object including seed knowledge,
Coire session IDs, cycle count, final status, and serialized tableau entries.

**Response `404`** if no snapshot exists for the given ID.
**Response `503`** if no Coire store is configured.

---

### DELETE /deduce/{id}/snapshot

Explicitly delete a persisted snapshot and its associated Coire events.

**Response `409 Conflict`** if the session is still active.
**Response `404`** if no snapshot exists.

**Response `200`:**
```json
{
  "deduction_id": "f7a3e2b1-...",
  "status":       "deleted"
}
```

---

## Coire Observability

Coire is the inter-engine event relay mailbox used internally by deduction
cycles to pass messages between the Prolog and CLIPS engines. These endpoints
expose observability and a testing hook.

### GET /cycle/coire/snapshot

Returns pending event counts for all sessions associated with completed or
in-flight deductions.

**Response `200`:**
```json
{
  "sessions": [
    { "session_id": "a1b2c3d4-...", "pending_count": 0 },
    { "session_id": "b2c3d4e5-...", "pending_count": 2 }
  ]
}
```

---

### POST /cycle/coire/push

Inject a synthetic event into a Coire session. Primarily useful for testing
the relay pipeline or triggering engine behaviour from outside a deduction
cycle.

**Request:**
```json
{
  "session_id": "a1b2c3d4-...",
  "origin":     "test-harness",
  "event_type": "prolog_fact",
  "data":       "eligible(alice)"
}
```

**Response `200`:**
```json
{ "event_id": "e9f0a1b2-..." }
```

---

## Error Responses

All error responses use the following envelope:

```json
{
  "error":   "session not found",
  "details": "optional extended description"
}
```

| HTTP status | Meaning |
|---|---|
| `400` | Validation error or type mismatch (e.g. session ID belongs to wrong engine) |
| `404` | Session, deduction, or snapshot not found |
| `409` | Conflict — session still active |
| `503` | Feature requires persistence (Coire store) which is not configured |
| `500` | Internal error |
