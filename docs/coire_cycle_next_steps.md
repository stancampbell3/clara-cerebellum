# Integration Plan: CoireStore in clara-cycle

## Context

Each `DeductionSession` owns two Coire mailboxes — `prolog_id` and `clips_id` — both fresh UUIDs assigned at construction. Currently, when a session ends all mailbox state is lost. The goal is to optionally persist and restore it.

## Design Decisions

### 1. Store is optional, injected via builder

Not every use of `CycleController` needs persistence. Make it opt-in:

```rust
pub struct CycleController {
    session:      DeductionSession,
    max_cycles:   u32,
    initial_goal: Option<String>,
    interrupt:    Arc<AtomicBool>,
    store:        Option<CoireStore>,   // NEW
}

impl CycleController {
    pub fn with_store(mut self, store: CoireStore) -> Self { ... }
}
```

### 2. Auto-save on run() exit

When `store` is `Some`, save both mailboxes at every natural exit point of `run()` — converged, interrupted, and max-cycles-exceeded. This is unconditional: no result-type filtering at the cycle layer (callers can delete if they don't want it).

```
run() → converge      → save prolog_id + clips_id → return result
run() → interrupt     → save prolog_id + clips_id → return result
run() → max cycles    → save prolog_id + clips_id → return Err
```

### 3. Restore is explicit with ID remapping

Session IDs change between runs (new UUIDs on each `DeductionSession::new()`). A restore maps events from a stored session into a new live session, rewriting `session_id` in transit. Add a method to `CoireStore`:

```rust
/// Read stored events for `from_id`, rewrite their session_id to `into_id`,
/// and write them into `coire`. Returns event count.
pub fn restore_session_as(
    &self,
    from_id: Uuid,
    into_id: Uuid,
    coire: &Coire,
) -> CoireResult<usize>
```

Then add a convenience method on `CycleController`:

```rust
/// Reload a previous session's Coire state into the current session's mailboxes.
pub fn restore_from(
    &mut self,
    store: &CoireStore,
    prev_prolog_id: Uuid,
    prev_clips_id: Uuid,
) -> Result<(), CycleError>
```

### 4. Error variant

Add `CycleError::Store(#[from] CoireError)` so store errors propagate cleanly through the cycle result type.

---

## Files to Change

| File | Change |
|------|--------|
| `clara-coire/src/store.rs` | Add `restore_session_as(from_id, into_id, coire)` |
| `clara-cycle/Cargo.toml` | No change needed (`clara-coire` already a dep) |
| `clara-cycle/src/error.rs` | Add `Store(#[from] CoireError)` variant |
| `clara-cycle/src/controller.rs` | Add `store` field, `with_store()` builder, auto-save in `run()`, `restore_from()` method |
| `clara-cycle/src/lib.rs` | Re-export `CoireStore` from `clara-coire` for caller convenience |

---

## Call Flow

### Save (happens automatically inside `run()`)

```
run() exit
  └─ if let Some(store) = &self.store
       store.save_session(self.session.prolog_id, coire_global)?
       store.save_session(self.session.clips_id,  coire_global)?
```

### Restore (caller-driven, before `run()`)

```rust
// Caller has a DeductionResult from a previous run:
let prev_prolog = result.prolog_session_id;
let prev_clips  = result.clips_session_id;

let mut controller = CycleController::new(session, 100, None, interrupt)
    .with_store(store.clone());

controller.restore_from(&store, prev_prolog, prev_clips)?;
controller.run()?;
```

---

## Implementation Order

1. `CoireStore::restore_session_as` — small addition to `clara-coire/src/store.rs`
2. `CycleError::Store` variant — `clara-cycle/src/error.rs`
3. `CycleController::store` field + `with_store` builder — `clara-cycle/src/controller.rs`
4. Auto-save logic at all three exit points in `run()` — `clara-cycle/src/controller.rs`
5. `CycleController::restore_from` — `clara-cycle/src/controller.rs`
6. Re-export `CoireStore` from `clara-cycle/src/lib.rs`
