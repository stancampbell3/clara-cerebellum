# Session Lifecycle and Management

## Overview

A Clara deduction session is a paired Prolog + CLIPS reasoning environment that
runs one reasoning cycle under `CycleController`. Sessions are short-lived and
purpose-bound: one session per deduction request. This document describes the
full lifecycle — creation, seeding, execution, termination — and how to use
`CoireStore` to persist and restore a session's Coire mailbox state across runs.

---

## Session Architecture

Each deduction session owns two independent engine instances and two Coire
event mailboxes identified by UUIDs.

```
┌─────────────────────────────────────────────────────────┐
│  CycleController                                        │
│                                                         │
│  ┌─────────────────────────────────────────────────┐   │
│  │  DeductionSession                               │   │
│  │                                                 │   │
│  │  prolog: PrologEnvironment  prolog_id: Uuid ────┼───┼──▶ Coire mailbox (in-memory)
│  │  clips:  ClipsEnvironment   clips_id:  Uuid ────┼───┼──▶ Coire mailbox (in-memory)
│  └─────────────────────────────────────────────────┘   │
│                                                         │
│  max_cycles:   u32                                      │
│  initial_goal: Option<String>                           │
│  interrupt:    Arc<AtomicBool>                          │
│  store:        Option<CoireStore>   ◀── optional        │
└─────────────────────────────────────────────────────────┘
```

The global `Coire` singleton (one per process) stores all session mailboxes in
a single in-memory DuckDB table, keyed by session UUID. `CoireStore` is a
separate file-backed DuckDB that can snapshot and restore those mailboxes.

---

## Lifecycle States

```
  DeductionSession::new()
          │
          ▼
    ┌───────────┐
    │  Created  │  UUIDs assigned, engines initialized
    └─────┬─────┘
          │  seed_prolog() / seed_clips() / seed_clips_file()
          ▼
    ┌───────────┐
    │   Seeded  │  Knowledge loaded into engines
    └─────┬─────┘
          │  (optional) controller.restore_from()
          ▼
    ┌───────────┐
    │  Restored │  Previous Coire state reloaded
    └─────┬─────┘
          │  controller.run()
          ▼
    ┌───────────┐
    │  Running  │  Prolog ↔ CLIPS cycles executing
    └─────┬─────┘
          │
    ┌─────┴──────────────────────────┐
    │                                │
    ▼                                ▼
┌───────────┐               ┌─────────────────┐
│ Converged │               │   Interrupted   │
│           │               │  (or MaxCycles) │
└─────┬─────┘               └────────┬────────┘
      │                              │
      │  (if store configured)       │  (if store configured)
      └──────────┬───────────────────┘
                 │  save_to_store() called automatically
                 ▼
         ┌─────────────┐
         │   Persisted │  Both mailboxes written to CoireStore
         └─────────────┘
```

---

## Creating a Session

```rust
use clara_cycle::{DeductionSession, CycleController, CycleStatus};
use std::sync::{Arc, atomic::AtomicBool};

// 1. Create the paired engine environment
let mut session = DeductionSession::new()?;

// 2. Seed Prolog knowledge
session.seed_prolog(&[
    "man(socrates).".into(),
    "mortal(X) :- man(X).".into(),
])?;

// 3. (Optional) Seed CLIPS knowledge from a file or inline constructs
session.seed_clips_file("/path/to/rules.clp")?;
// or:
session.seed_clips(&[
    "(defrule mortal-clips (man ?x) => (assert (mortal ?x)))".into(),
])?;

// 4. Create the controller
let interrupt = Arc::new(AtomicBool::new(false));
let mut controller = CycleController::new(
    session,
    100,                              // max cycles
    Some("mortal(X)".into()),         // initial Prolog goal (cycle 0 only)
    interrupt,
);
```

`DeductionSession::new()` immediately assigns two fresh UUIDs —
`prolog_id` and `clips_id` — and registers both as Coire mailboxes with the
global `Coire` singleton.

---

## Running the Cycle

```rust
match controller.run() {
    Ok(result) => {
        println!("Status:  {}", result.status);   // Converged | Interrupted
        println!("Cycles:  {}", result.cycles);
        println!("Prolog session: {}", result.prolog_session_id);
        println!("CLIPS  session: {}", result.clips_session_id);
        if let Some(solutions) = result.prolog_solutions {
            println!("Solutions: {}", solutions);  // JSON array from cycle 0
        }
    }
    Err(CycleError::MaxCyclesExceeded(n)) => {
        eprintln!("Did not converge within {} cycles", n);
    }
    Err(e) => eprintln!("Cycle error: {}", e),
}
```

### What each cycle does

| Step | Action |
|------|--------|
| 1 | **Prolog pass** — dispatch Coire events from Prolog mailbox; on cycle 0 execute `initial_goal` and collect all solutions |
| 2 | **Relay Prolog → CLIPS** — drain Prolog mailbox, transpile, forward to CLIPS |
| 3 | **Evaluator pass** — stub; future LLM/FieryPit hook |
| 4 | **CLIPS pass** — dispatch Coire events from CLIPS mailbox; run `(run)` to saturation |
| 5 | **Relay CLIPS → Prolog** — drain CLIPS mailbox, transpile, forward to Prolog |
| 6 | **Convergence check** — both mailboxes empty + CLIPS agenda empty + snapshot stable |
| 7 | **Interrupt check** — poll `Arc<AtomicBool>` for early termination |

---

## Session Isolation

Each `DeductionSession` is fully isolated:

- **Prolog**: separate SWI-Prolog engine instance with its own heap and database
- **CLIPS**: separate CLIPS environment pointer; no fact/rule sharing
- **Coire**: events are scoped to UUIDs — `prolog_id` and `clips_id` — so
  mailboxes from different sessions never interfere
- **Thread safety**: `CycleController::run()` is blocking; call from
  `tokio::task::spawn_blocking` in async contexts

---

## Coire Event Mailbox

The `Coire` in-memory store is a global DuckDB instance holding all pending
events across all sessions. Key operations used internally by the cycle:

| Operation | Purpose |
|-----------|---------|
| `write_event(&event)` | Enqueue an event to a session's mailbox |
| `poll_pending(session_id)` | Atomically read + mark all pending events processed |
| `count_pending(session_id)` | Count events waiting in a mailbox |
| `drain_session(session_id)` | Mark all pending events drained (soft discard) |
| `clear_session(session_id)` | Hard delete all events for a session |

Events carry a typed JSON `payload`, an `origin` string (`"prolog"`,
`"clips"`, or custom), a timestamp, and an `EventStatus`
(`Pending | Processed | Drained`).

The global Coire is initialized once at process startup:

```rust
clara_coire::init_global()?;
```

---

## Persistent Coire Store

`CoireStore` is a file-backed DuckDB that can snapshot and restore session
mailboxes across process restarts or between deduction runs.

### Enabling persistence via configuration

In `clara-api`, set `coire_store_path` in the `[persistence]` section of your
config file. The server opens the store at startup and attaches it to every
`CycleController` automatically:

```toml
# config/default.toml  (or your environment overlay)
[persistence]
coire_store_path = "./data/coire.duckdb"
```

The path is created if it does not exist. If the path is configured but cannot
be opened the server will refuse to start with a clear error message. Omit the
key (or leave it commented out) to disable persistence.

### Enabling persistence programmatically

When using `clara-cycle` directly, open and attach a store manually:

```rust
use clara_cycle::CoireStore;                  // re-exported from clara-coire

let store = CoireStore::open("/var/lib/clara/coire.duckdb")?;

let mut controller = CycleController::new(session, 100, goal, interrupt)
    .with_store(store.clone());               // attach the store
```

With a store attached either way, **both mailboxes are saved automatically** at
every `run()` exit point — converged, interrupted, and max-cycles exceeded.
Save failures are logged as warnings and do not alter the cycle result.

### CoireStore API

| Method | Description |
|--------|-------------|
| `CoireStore::open(path)` | Open or create a persistent store file |
| `save_session(session_id, coire)` | Upsert all events for a session; safe to call repeatedly |
| `restore_session(session_id, coire)` | Reload stored events back into a live Coire (same UUIDs) |
| `restore_session_as(from_id, into_id, coire)` | Reload stored events, rewriting session UUID (for new sessions) |
| `read_session(session_id)` | Read stored events without modifying state |
| `delete_session(session_id)` | Remove all stored events for a session |
| `list_sessions()` | Return all session UUIDs with stored events |

`CoireStore` is `Clone` — clones share the same underlying connection.

### Resuming a previous deduction

When creating a new `DeductionSession`, fresh UUIDs are assigned. To resume
where a prior run left off, use `restore_from()` to remap stored events to
the new session's IDs before calling `run()`:

```rust
// Previous run returned these IDs in DeductionResult:
let prev_prolog_id: Uuid = result.prolog_session_id;
let prev_clips_id:  Uuid = result.clips_session_id;

// New session gets fresh UUIDs
let mut session = DeductionSession::new()?;
session.seed_prolog(&clauses)?;

let mut controller = CycleController::new(session, 100, None, interrupt)
    .with_store(store.clone());

// Remap stored events from prev UUIDs → new session UUIDs
controller.restore_from(&store, prev_prolog_id, prev_clips_id)?;

controller.run()?;
```

`restore_from` calls `restore_session_as` for each mailbox, rewriting each
event's `session_id` in transit so they arrive in the correct new mailbox.

### Manual save / selective persistence

You can also save or restore without attaching a store to the controller:

```rust
let coire = clara_coire::global();

// Save after a run:
store.save_session(result.prolog_session_id, coire)?;
store.save_session(result.clips_session_id,  coire)?;

// Inspect without restoring:
let events = store.read_session(result.prolog_session_id)?;

// Clean up stored state:
store.delete_session(result.prolog_session_id)?;
store.delete_session(result.clips_session_id)?;
```

---

## HTTP API

The `clara-api` crate exposes deduction and Coire management endpoints.

### Deduction

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/deduce` | Submit a deduction request; returns a deduction ID |
| `GET` | `/deduce/{id}` | Poll the status and result of a deduction |
| `DELETE` | `/deduce/{id}` | Cancel a running deduction (sets the interrupt flag) |

### Coire inspection

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/cycle/coire/snapshot` | Pending event counts per active session |
| `POST` | `/cycle/coire/push` | Inject a synthetic event into any session's mailbox |

---

## Error Handling

| Error | Cause |
|-------|-------|
| `CycleError::Prolog(e)` | SWI-Prolog engine error |
| `CycleError::Clips(msg)` | CLIPS engine error |
| `CycleError::Coire(e)` | DuckDB error in Coire or CoireStore |
| `CycleError::MaxCyclesExceeded(n)` | No convergence within cycle budget |
| `CycleError::SessionCreationFailed(msg)` | Engine initialization failure |

Store errors (`CoireError`) surface as `CycleError::Coire` when returned from
`restore_from`. Save errors inside `run()` are logged as warnings only.

---

## Concurrency

- `CycleController::run()` is **blocking** — never call it on an async thread
- Use `tokio::task::spawn_blocking` in async handlers
- Multiple `CycleController` instances run concurrently without interference
  (each has its own `DeductionSession` with isolated Coire UUIDs)
- `Coire` uses `Arc<Mutex<Connection>>` — all mailbox operations are serialized
  but non-blocking for callers using `spawn_blocking`
- `CoireStore` is likewise `Arc<Mutex<Connection>>` and safe to share across
  threads via `Clone`

---

## Implementation Status

### Implemented

- `DeductionSession` — paired Prolog + CLIPS engines with Coire UUIDs
- `CycleController` — full Prolog ↔ CLIPS cycle with convergence detection
- `Coire` — in-memory event mailbox with atomic `poll_pending`
- `CoireStore` — file-backed persistent snapshot store
  - `save_session`, `restore_session`, `restore_session_as`
  - `delete_session`, `list_sessions`, `read_session`
  - Upsert semantics (safe to save repeatedly)
  - `CycleController::with_store` — auto-save on all exit paths
  - `CycleController::restore_from` — resume from a previous run
  - `persistence.coire_store_path` config key wired into `clara-api`
- Prolog ↔ CLIPS relay with bidirectional term/fact transpilation
- Prolog → CLIPS transduction (speculative forward chaining)
- HTTP API: `/deduce` (POST/GET/DELETE), `/cycle/coire/snapshot`, `/cycle/coire/push`

- In-memory Coire eviction at `run()` exit — `CycleController::evict_coire_sessions()` calls `clear_session()` for both mailboxes after every run, freeing in-memory DuckDB rows immediately
- `CarrionPicker` background task — sweeps `CoireStore` on a configurable interval and deletes sessions whose newest event is older than the TTL; skips any session UUID currently in `AppState::active_coire_sessions`
  - Active session tracking: `DeductionEntry` stores `prolog_session_id` / `clips_session_id`; a oneshot channel communicates them from `spawn_blocking` to the async wrapper before `run()` begins
  - Config keys: `persistence.coire_store_ttl_seconds` (default 86400), `persistence.coire_store_sweep_interval_seconds` (default 3600); set TTL to 0 to disable

### Planned

- Session resume exposed as a dedicated HTTP endpoint (`POST /deduce/resume`)
- Carrion-picker cancellation token for clean shutdown grace period
- Per-sweep metrics (sessions deleted, sweep duration)

---

## Related Documentation

- `ARCHITECTURE.md` — Overall system design
- `docs/coire_cycle_next_steps.md` — Integration design notes for CoireStore
- `clara-coire/src/coire.rs` — In-memory Coire implementation
- `clara-coire/src/store.rs` — CoireStore persistent store implementation
- `clara-cycle/src/controller.rs` — CycleController implementation
- `clara-cycle/src/session.rs` — DeductionSession creation and seeding
