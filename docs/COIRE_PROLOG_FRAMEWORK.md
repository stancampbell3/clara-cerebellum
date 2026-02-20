# Coire-Prolog Framework Integration

## Overview

This document describes the semantic event framework built on top of the low-level Coire
primitives, enabling Prolog engines within Clara to publish and consume typed events without
managing UUIDs or JSON manually.

The integration spans three layers:

```
┌─────────────────────────────────────────────────────────┐
│   Application Code  (Rust or Prolog)                    │
├─────────────────────────────────────────────────────────┤
│   the_coire.pl  —  Semantic Prolog API                  │
│   PrologEnvironment  —  Rust session wrapper            │
├─────────────────────────────────────────────────────────┤
│   coire_emit/3, coire_poll/2, coire_mark/1,             │
│   coire_count/2  —  Low-level foreign predicates        │
├─────────────────────────────────────────────────────────┤
│   clara-coire  —  DuckDB-backed event mailbox           │
└─────────────────────────────────────────────────────────┘
```

---

## Architecture

### Session Ownership

Each `PrologEnvironment` owns a Coire session identified by a randomly generated UUID
(`session_id: Uuid`). This UUID is:

- Stored as a SWI-Prolog `thread_local` fact (`the_coire:coire_session_id/1`) inside the
  engine when it is created.
- Used by `coire_publish/*` predicates to address events to the right mailbox.
- Used by `consume_coire_events()` to poll and dispatch only the events belonging to this
  engine.

Because `coire_session_id/1` is `thread_local`, SWI-Prolog engines are fully isolated: each
engine sees only its own UUID even when multiple engines are active in the same process.

### Initialization Order

```
clara_coire::init_global()          ← DuckDB mailbox ready
  └─ clara_prolog::init_global()    ← registers foreign predicates first,
                                        then loads the_coire.pl library
       └─ PrologEnvironment::new()  ← creates engine, asserts session UUID
```

This ordering is required: `the_coire.pl` calls `coire_emit/3` etc., which must already
be registered as foreign predicates before `use_module(library(the_coire))` succeeds.

---

## Files Changed

| File | Change |
|------|--------|
| `clara-prolog/prolog-lib/the_coire.pl` | **NEW** — Prolog semantic event API module |
| `clara-prolog/src/backend/ffi/environment.rs` | Added `session_id`, `load_coire_library()`, `session_id()`, `consume_coire_events()` |
| `clara-prolog/src/lib.rs` | Exports `load_coire_library`, calls it in `init_global()` |

---

## API Reference

### Prolog API (`library(the_coire)`)

Load with:
```prolog
:- use_module(library(the_coire)).
```

#### `coire_session(-SessionId)`
Retrieves the UUID string for the current engine's Coire session. Set by Rust at engine
creation; do not assert `coire_session_id/1` manually.

```prolog
?- coire_session(S).
S = '3f7a1c2e-84b0-4f1a-9d3c-000000000001'.
```

#### `coire_publish(+EventType, +DataTerm)`
Publishes a typed event. `DataTerm` is any Prolog term; it is serialised with
`term_to_atom/2` before being stored.

```prolog
coire_publish(assert, user_authenticated(alice)).
coire_publish(retract, session_open(old_session)).
coire_publish(goal, run_cleanup).
```

#### `coire_publish_assert(+Fact)`
Shorthand for `coire_publish(assert, Fact)`.

```prolog
coire_publish_assert(user_role(alice, admin)).
```

#### `coire_publish_retract(+Fact)`
Shorthand for `coire_publish(retract, Fact)`.

```prolog
coire_publish_retract(user_role(alice, guest)).
```

#### `coire_publish_goal(+Goal)`
Shorthand for `coire_publish(goal, Goal)`.

```prolog
coire_publish_goal(invalidate_cache(alice)).
```

#### `coire_consume`
Polls the Coire mailbox for all pending events addressed to this engine's session,
then dispatches each one via the built-in handlers (or user hooks). Called automatically
by `consume_coire_events/0` on the Rust side.

```prolog
?- coire_consume.
true.
```

#### `coire_on_event(+EventDict)` — User Hook
Declared `:- discontiguous`. Define clauses in your own module to intercept events before
built-in dispatch. If a clause succeeds, built-in handling is skipped for that event.

```prolog
% Log all incoming goal events before executing them
coire_on_event(Payload) :-
    get_dict(type, Payload, goal),
    get_dict(data, Payload, GoalStr),
    format(atom(Msg), "Incoming goal: ~w", [GoalStr]),
    log_info(Msg),
    fail.   % fail to fall through to built-in handler
```

### Rust API (`PrologEnvironment`)

#### `PrologEnvironment::new() -> PrologResult<Self>`
Creates a new isolated Prolog engine with its own Coire session UUID. Automatically:
- Registers coire foreign predicates (idempotent)
- Loads `library(the_coire)` (idempotent)
- Generates a UUID and asserts `the_coire:coire_session_id/1` into the engine

```rust
let env = PrologEnvironment::new()?;
println!("Session: {}", env.session_id());
```

#### `env.session_id() -> Uuid`
Returns the Coire session UUID for this environment.

#### `env.consume_coire_events() -> PrologResult<usize>`
Polls the Coire mailbox for this engine's session and dispatches all pending events via
`coire_consume/0`. Returns the count of events that were pending before dispatch.

```rust
let n = env.consume_coire_events()?;
println!("Processed {} events", n);
```

#### `load_coire_library() -> PrologResult<()>`
Loads `the_coire.pl` into the global Prolog system (called once per process). Must be
called after `register_coire_predicates()`. Exposed via `clara_prolog::load_coire_library`.

---

## Event Payload Schema

Events published from Prolog via `coire_publish/*` use this JSON payload structure,
stored as the `payload` field of a `ClaraEvent`:

```json
{ "type": "assert",  "data": "user_authenticated(alice)" }
{ "type": "retract", "data": "session_open(old_session)" }
{ "type": "goal",    "data": "run_cleanup" }
```

The full `ClaraEvent` stored in DuckDB:

```json
{
  "event_id":    "e3a1f7b2-...",
  "session_id":  "3f7a1c2e-...",
  "origin":      "prolog",
  "created_at_ms": 1708380000000,
  "payload":     { "type": "assert", "data": "user_authenticated(alice)" },
  "status":      "Pending"
}
```

---

## Usage Patterns

### Pattern 1: Publishing Facts on State Change

Prolog code publishes events whenever it modifies important state, so that other engines
(CLIPS or another Prolog instance) can react:

```prolog
:- use_module(library(the_coire)).

handle_login(User) :-
    assertz(user_authenticated(User)),
    coire_publish_assert(user_authenticated(User)).

handle_logout(User) :-
    retractall(user_authenticated(User)),
    coire_publish_retract(user_authenticated(User)).
```

### Pattern 2: Rust-Driven Event Cycle

```rust
use clara_prolog::PrologEnvironment;

let env = PrologEnvironment::new()?;

// Load your domain rules
env.consult_file("rules/domain.pl")?;

loop {
    // Drive Prolog reasoning cycle
    env.query_once("run_cycle")?;

    // Process any events sent to this engine from other engines
    let processed = env.consume_coire_events()?;
    if processed > 0 {
        log::info!("Processed {} incoming events", processed);
    }

    std::thread::sleep(std::time::Duration::from_millis(100));
}
```

### Pattern 3: Cross-Engine Communication (Prolog → CLIPS)

Engine A (Prolog) publishes an event. Engine B (CLIPS, using `coire_poll`) reads it.

**Engine A — Prolog:**
```prolog
handle_sensor(valve_pressure, Value) :-
    Value > 200,
    coire_publish_assert(high_pressure_alert(Value)).
```

**Engine B — CLIPS (reading Engine A's session):**
```clips
(defrule process-prolog-events
  =>
  (bind ?events (coire-poll "3f7a1c2e-..."))
  ; dispatch events...
)
```

In a proper multi-engine setup, Engine B's session ID would be passed during configuration
rather than hard-coded.

### Pattern 4: Custom Event Hook

Override the built-in dispatch for specific event types:

```prolog
:- use_module(library(the_coire)).

% Intercept assert events — validate before asserting
coire_on_event(Payload) :-
    get_dict(type, Payload, assert),
    get_dict(data, Payload, DataStr),
    term_to_atom(Fact, DataStr),
    \+ forbidden_fact(Fact),   % security check
    assertz(Fact).             % succeed → skip built-in handler

forbidden_fact(admin(_)).
forbidden_fact(root(_)).
```

### Pattern 5: Goal Execution via Events

Trigger Prolog goals remotely from another engine:

```prolog
% Publisher (another Prolog engine or CLIPS)
coire_publish_goal(reindex_knowledge_base).
coire_publish_goal(notify_user(alice, 'System updated')).
```

The consumer engine runs `consume_coire_events/0`, which calls the goals via
`coire_dispatch_type(goal, ...)` → `call(Goal)`.

---

## Design Notes

### Why `thread_local`?

SWI-Prolog engines maintain their own clause databases for `thread_local` predicates,
even when multiple engines run on the same OS thread (by switching engine contexts with
`PL_set_engine`). This gives each `PrologEnvironment` a private `coire_session_id/1`
without any Rust-side bookkeeping beyond the initial `assertz`.

### Why a Separate `OnceLock` for Library Loading?

`ensure_prolog_initialized()` runs before foreign predicates are registered. If `the_coire`
module were loaded inside `INIT_RESULT`, `use_module` would attempt to call `coire_emit/3`
etc. before they exist, causing an error. The separate `COIRE_LIB_LOADED` lock lets us
load the library at the correct moment (after predicate registration).

### Module Qualification for `assertz`

The `coire_session_id/1` fact is declared `:- thread_local` inside the `the_coire` module.
To assert into the correct module's thread-local storage, Rust uses:
```
assertz((the_coire:coire_session_id('uuid')))
```
An unqualified `assertz(coire_session_id('uuid'))` would land in `user:coire_session_id/1`,
which `the_coire:coire_session/1` would not find.

---

## Verification Checklist

1. `cargo build -p clara-prolog` — must compile clean
2. `cargo run --bin prolog-repl`:
   - `coire_session(S).` → returns a UUID string
   - `coire_publish_assert(test_fact(42)).` → succeeds
3. In a second REPL session, `coire_poll('<uuid-from-above>', Json).` → JSON array
   containing the event
4. `cargo build -p clara-clips` — must still compile (no CLIPS changes)
