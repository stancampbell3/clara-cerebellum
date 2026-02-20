# Coire-CLIPS Framework Integration

## Overview

This document describes the semantic event framework for CLIPS engines within Clara,
mirroring the Prolog-side framework in `COIRE_PROLOG_FRAMEWORK.md`.

The integration spans three layers:

```
┌─────────────────────────────────────────────────────────┐
│   Application Code  (Rust or CLIPS)                     │
├─────────────────────────────────────────────────────────┤
│   the_coire.clp  —  Semantic CLIPS API                  │
│   ClipsEnvironment  —  Rust session wrapper             │
├─────────────────────────────────────────────────────────┤
│   coire-emit / coire-poll / coire-mark / coire-count    │
│   CLIPS UDFs registered in userfunctions.c              │
├─────────────────────────────────────────────────────────┤
│   clara-coire  —  DuckDB-backed event mailbox           │
└─────────────────────────────────────────────────────────┘
```

---

## Architecture

### Session Ownership

Each `ClipsEnvironment` owns a Coire session identified by a randomly generated UUID
(`session_id: Uuid`). This UUID is:

- Stored in the CLIPS `defglobal` `?*coire-session-id*` when the environment is created.
- Used by `(coire-publish ...)` and its variants to address events to the correct mailbox.
- Used by `consume_coire_events()` on the Rust side to poll and dispatch events.

### Initialization Order

```
clara_coire::init_global()            ← DuckDB mailbox ready
  └─ ClipsEnvironment::new()          ← creates CLIPS environment
       ├─ load_coire_library()        ← builds defglobal, deftemplate, deffunction into env
       └─ eval("(bind ?*coire-session-id* \"uuid\")")  ← seeds session global
```

Unlike the Prolog integration, `the_coire.clp` constructs are compiled per-environment
(via `Build()`), not globally, because each CLIPS environment is independent. There is no
process-level singleton — each `ClipsEnvironment::new()` loads the library fresh.

### How Low-Level UDFs Work

The C file `clips-src/core/userfunctions.c` registers the following CLIPS UDFs via `AddUDF`
at environment creation time (called automatically by CLIPS via `UserFunctions()`):

| CLIPS function | Rust backing | Signature |
|----------------|-------------|-----------|
| `(coire-emit session origin payload)` | `rust_coire_emit` | 3 string args → string |
| `(coire-poll session)` | `rust_coire_poll` | 1 string arg → JSON string |
| `(coire-mark event-id)` | `rust_coire_mark` | 1 string arg → string |
| `(coire-count session)` | `rust_coire_count` | 1 string arg → integer |

`the_coire.clp` builds higher-level `deffunction`s on top of these.

---

## Files Changed

| File | Change |
|------|--------|
| `clara-clips/clp-lib/the_coire.clp` | **NEW** — CLIPS semantic event API constructs |
| `clara-clips/src/backend/ffi/bindings.rs` | Added `Build()` FFI binding |
| `clara-clips/src/backend/ffi/environment.rs` | Added `session_id`, `build()`, `load_coire_library()`, `session_id()`, `consume_coire_events()` |
| `clara-clips/Cargo.toml` | Added `uuid = { version = "1", features = ["v4"] }` |

No changes to `userfunctions.c` or `clara-coire` — the low-level bridge was already complete.

---

## API Reference

### CLIPS API (`clp-lib/the_coire.clp`)

Loaded automatically by `ClipsEnvironment::new()`. All functions are available in any
CLIPS expression or rule RHS after construction.

#### `(coire-session)` → string
Returns the UUID string for this environment's Coire session.

```clips
CLIPS> (coire-session)
"3f7a1c2e-84b0-4f1a-9d3c-000000000001"
```

#### `(coire-publish ?type ?data-str)` → string
Emits a typed event to the Coire mailbox. `?data-str` must not contain unescaped double
quotes (pre-escape with `\\"` if necessary).

```clips
(coire-publish "signal" "pressure_high")
(coire-publish "assert" "user_authenticated(alice)")
(coire-publish "goal" "(run)")
```

Returns the result of `(coire-emit ...)` — `"ok"` on success or an error JSON string.

#### `(coire-publish-assert ?fact-str)` → string
Shorthand for `(coire-publish "assert" ?fact-str)`.

For **Prolog consumers**: `?fact-str` must be valid Prolog term syntax.
For **CLIPS consumers**: `?fact-str` must be a valid CLIPS ordered or template fact.

```clips
(coire-publish-assert "user_authenticated(alice)")   ; → Prolog: assertz(user_authenticated(alice))
(coire-publish-assert "(system-initialized)")        ; → CLIPS: (assert (system-initialized))
```

#### `(coire-publish-retract ?fact-str)` → string
Shorthand for `(coire-publish "retract" ?fact-str)`.

For **Prolog consumers**: `?fact-str` is a Prolog term; the consumer calls `retract/1`.
For **CLIPS consumers**: delivered as a `(coire-event (ev-type "retract") ...)` template
fact — write a `defrule` to handle it.

```clips
(coire-publish-retract "session_open(old_session)")
```

#### `(coire-publish-goal ?goal-str)` → string
Shorthand for `(coire-publish "goal" ?goal-str)`.

For **Prolog consumers**: `?goal-str` is called via `call/1`.
For **CLIPS consumers**: `?goal-str` is eval'd directly as a CLIPS expression.

```clips
(coire-publish-goal "run_diagnostics")        ; Prolog: call(run_diagnostics)
(coire-publish-goal "(run)")                  ; CLIPS: eval (run)
(coire-publish-goal "(assert (alarm on))")    ; CLIPS: asserts (alarm on)
```

#### `?*coire-session-id*` — defglobal
The session UUID string. Set by Rust at construction. Treat as read-only; reading via
`(coire-session)` is preferred. Used internally by all `coire-publish-*` functions.

#### `(deftemplate coire-event ...)` — incoming event template
Asserted by `consume_coire_events()` for events with non-builtin types. Write
`defrule`s matching this template to implement custom event handling.

Slots:
- `event-id` (STRING) — UUID of the originating event
- `origin` (STRING) — `"prolog"`, `"clips"`, or other engine identifier
- `ev-type` (STRING) — the event type string
- `data` (STRING) — the payload data string

---

### Rust API (`ClipsEnvironment`)

#### `ClipsEnvironment::new() -> Result<Self, String>`
Creates a new isolated CLIPS environment with its own Coire session UUID. Automatically:
- Creates the CLIPS engine
- Loads `the_coire.clp` constructs via `Build()`
- Seeds `?*coire-session-id*` with the generated UUID

```rust
let mut env = ClipsEnvironment::new()?;
println!("Session: {}", env.session_id());
```

#### `env.session_id() -> Uuid`
Returns the Coire session UUID for this environment.

#### `env.consume_coire_events() -> Result<usize, String>`
Polls the Coire mailbox for this environment's session and dispatches all pending events.
Returns the count of events that were pending before dispatch.

Dispatch rules:
| Event type | Action |
|------------|--------|
| `"assert"` | `(assert <data>)` — data must be valid CLIPS fact syntax |
| `"goal"` | `<data>` is eval'd as a CLIPS expression |
| anything else | `(assert (coire-event ...))` then `(run)` |

```rust
let n = env.consume_coire_events()?;
println!("Dispatched {} incoming events", n);
```

#### `env.build(&mut self, construct: &str) -> Result<(), String>`
Compiles a single CLIPS construct definition into the environment.

```rust
env.build("(deftemplate my-fact (slot value (type STRING)))")?;
```

#### `env.load_coire_library(&mut self) -> Result<(), String>`
Re-loads the `the_coire.clp` constructs. Useful after `clear()` to restore the event API.
Called automatically by `new()`.

---

## Event Payload Schema

Events published from CLIPS via `(coire-publish ...)`:

```json
{ "type": "assert",  "data": "user_authenticated(alice)" }
{ "type": "retract", "data": "session_open(old_session)" }
{ "type": "goal",    "data": "run_diagnostics" }
{ "type": "signal",  "data": "pressure_high" }
```

These are stored as `ClaraEvent.payload`. The full DuckDB record:

```json
{
  "event_id":      "e3a1f7b2-...",
  "session_id":    "3f7a1c2e-...",
  "origin":        "clips",
  "created_at_ms": 1708380000000,
  "payload":       { "type": "assert", "data": "user_authenticated(alice)" },
  "status":        "Pending"
}
```

---

## Usage Patterns

### Pattern 1: CLIPS Publishing Facts for Prolog

CLIPS publishes events whenever it modifies important state, and Prolog subscribes:

```clips
;;; In CLIPS:
(defrule user-login
  (login-request (user ?u))
  =>
  (assert (user-authenticated ?u))
  (coire-publish-assert (str-cat "user_authenticated(" ?u ")")))
```

```rust
// In Rust — Prolog side:
let prolog_env = PrologEnvironment::new()?;
let n = prolog_env.consume_coire_events()?;
// → assertz(user_authenticated(alice)) is executed in Prolog
```

### Pattern 2: Prolog Publishing Facts for CLIPS

Prolog publishes events; CLIPS consumes them via `consume_coire_events()`:

```prolog
% In Prolog:
handle_pressure_alert(Level) :-
    coire_publish(assert, (alarm on)),          % CLIPS fact syntax
    coire_publish(assert, pressure_level(Level)). % also Prolog syntax for Prolog consumers
```

```rust
// In Rust — CLIPS side:
let mut clips_env = ClipsEnvironment::new()?;
let n = clips_env.consume_coire_events()?;
// → (assert (alarm on)) is eval'd in CLIPS
// → (assert pressure_level(high)) — Prolog fact, treated as ordered CLIPS fact
```

```clips
;;; CLIPS rule fires after (run) triggered by consume_coire_events:
(defrule handle-alarm
  (alarm on)
  =>
  (printout t "ALARM: system alert active" crlf))
```

### Pattern 3: Custom Signal Events via Template Facts

For events that don't map directly to assert/retract/goal, use the template fact pattern:

```prolog
% Prolog publishes a custom signal:
coire_publish(signal, pressure_high).
```

```clips
;;; CLIPS receives it as a (coire-event ...) template fact:
(defrule handle-pressure-signal
  (coire-event (ev-type "signal") (origin "prolog") (data ?d))
  =>
  (printout t "Signal from Prolog: " ?d crlf)
  (assert (alert ?d)))
```

```rust
// Rust drives consumption:
let n = clips_env.consume_coire_events()?;
// → asserts (coire-event (ev-type "signal") (data "pressure_high")), then (run)
// → rule fires, asserts (alert pressure_high)
```

### Pattern 4: Rust-Driven Event Cycle

```rust
use clara_clips::ClipsEnvironment;

let mut env = ClipsEnvironment::new()?;

// Load your domain rules
env.load("rules/domain.clp")?;
env.reset()?;

loop {
    // Drive CLIPS reasoning cycle
    env.eval("(run)")?;

    // Process incoming events from other engines (Prolog, etc.)
    let processed = env.consume_coire_events()?;
    if processed > 0 {
        log::info!("Dispatched {} incoming events", processed);
    }

    std::thread::sleep(std::time::Duration::from_millis(100));
}
```

### Pattern 5: Cross-Engine RPC via Goal Events

CLIPS can invoke a Prolog goal by sending a goal event. Prolog's `consume_coire_events()`
receives it and calls the goal:

```clips
;;; CLIPS triggers a Prolog goal:
(coire-publish-goal "run_sensor_calibration")
```

```prolog
% Prolog receives it and executes:
% coire_dispatch_type(goal, "run_sensor_calibration")
%   → call(run_sensor_calibration)
run_sensor_calibration :-
    format("Calibrating sensors...~n"),
    calibrate_all_sensors,
    coire_publish_assert(sensors_calibrated).  % publishes back to CLIPS
```

### Pattern 6: After `clear()`, Re-Initialize Coire API

After calling `clear()`, CLIPS removes all constructs including those from `the_coire.clp`.
Re-load them:

```rust
env.clear()?;
env.load_coire_library()?;
// Re-seed the session global (clear() removes defglobals too)
env.eval(&format!("(bind ?*coire-session-id* \"{}\")", env.session_id()))?;
```

---

## Design Notes

### Per-Environment vs. Per-Process

Unlike the Prolog integration, where `load_coire_library()` uses a process-level `OnceLock`
(SWI-Prolog modules are global), each `ClipsEnvironment` is completely independent. The
`the_coire.clp` constructs are compiled into each environment separately via `Build()`.

### Why `Build()` and not `Load()`?

CLIPS's `Load()` reads from a file path at runtime. Using `Build()` with `include_str!()`
embeds the library content in the binary at compile time, so the environment can be
initialized without filesystem access and without deploying `.clp` files alongside the
binary. The `split_clips_constructs()` helper parses the embedded source into individual
top-level construct strings for sequential `Build()` calls.

### Data Escaping in `(coire-publish ...)`

The `coire-publish` deffunction builds a JSON payload by string concatenation. If `?data-str`
contains double quotes, the resulting JSON will be malformed. Pre-escape quotes before
publishing:

```clips
;;; Safe: no quotes in data
(coire-publish-assert "user_authenticated(alice)")

;;; Unsafe: will break JSON
; (coire-publish-assert "fact(\"quoted\")")

;;; Workaround: use single quotes where possible (Prolog atoms don't need quotes for simple names)
(coire-publish-assert "fact(simple_atom)")
```

For complex data, consider encoding as a compact identifier and looking up the full value
on the consumer side.

### CLIPS Retract Events

CLIPS does not support pattern-based retract (unlike Prolog's `retract/1`). The `"retract"`
event type is useful for **Prolog consumers** but has no built-in handler on the CLIPS side.
For CLIPS-side retract, use a `"goal"` event with `(do-for-all-facts ...)`:

```clips
;;; Tell a CLIPS consumer to retract alarm facts:
(coire-publish-goal "(do-for-all-facts ((?f alarm)) (eq ?f:state on) (retract ?f))")
```

---

## Verification Checklist

1. `cargo build -p clara-clips` — must compile clean
2. `cargo run --bin clips-repl`:
   - `(coire-session)` → returns a UUID string
   - `(coire-publish-assert "test_fact(42)")` → `"ok"`
   - In another session: `(coire-poll "uuid-from-above")` → JSON array with event
3. `cargo build -p clara-prolog` — must still compile (no Prolog changes)

## Comparison: Prolog vs. CLIPS Integration

| Aspect | Prolog (`the_coire.pl`) | CLIPS (`the_coire.clp`) |
|--------|------------------------|------------------------|
| Session storage | `:- thread_local coire_session_id/1` | `(defglobal ?*coire-session-id*)` |
| Session isolation | Per-engine (SWI thread-local) | Per-environment (CLIPS independent) |
| Library scope | Global (module, loaded once per process) | Per-environment (Build'd into each env) |
| Consume direction | Prolog-native (JSON parsed by SWI) | Rust-driven (JSON parsed by Rust) |
| Extensible dispatch | `coire_on_event/1` hook predicate | `defrule` on `(coire-event ...)` template |
| Retract support | Native (`retract/1`) | Via goal event + `do-for-all-facts` |
