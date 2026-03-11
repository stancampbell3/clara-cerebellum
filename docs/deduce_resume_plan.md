# Plan: Persistent Deduction Sessions + POST /deduce/resume

Adds opt-in persistence to `POST /deduce` and a matching resume endpoint.
When `persist: true` is set on the initial request, the full session snapshot
(seed knowledge + Coire pending events + result metadata) is saved to
`CoireStore` at cycle end. A subsequent `POST /deduce/resume` can restore and
continue that session without the caller re-specifying seed knowledge.

---

## New Concepts

### DeductionSnapshot

Stored alongside Coire events in the same `coire.duckdb` file (new table
`deduction_snapshots`). Contains everything needed to reconstruct a deduction
run:

| Column | Type | Description |
|--------|------|-------------|
| `deduction_id` | UUID PK | The original `deduction_id` returned to the caller |
| `prolog_clauses` | JSON text | Seed clauses array |
| `clips_constructs` | JSON text | Seed constructs array |
| `clips_file` | text nullable | Server-side `.clp` file path (reference only) |
| `initial_goal` | text nullable | Prolog goal used on cycle 0 |
| `max_cycles` | integer | Cycle budget used for this run |
| `status` | text | Final `CycleStatus` string |
| `cycles_run` | integer | Actual cycles completed |
| `prolog_session_id` | UUID | Links to `coire_events` rows |
| `clips_session_id` | UUID | Links to `coire_events` rows |
| `created_at_ms` | bigint | Unix timestamp (ms) at snapshot creation |
| `expires_at_ms` | bigint | `created_at_ms + snapshot_ttl` — hard expiry |

**What is not captured**: dynamically-asserted Prolog facts (from `assert/1`
called during relay or rule firing) and CLIPS working-memory facts accumulated
during the run. These are ephemeral engine state. Callers that need to carry
derived facts forward must include them in `prolog_clauses` of the next request,
or encode them as pending Coire events before the run ends. This is a known
limitation and should be documented clearly.

---

## API Changes

### POST /deduce — gains `persist` field

```json
{
  "prolog_clauses":    ["man(socrates).", "mortal(X) :- man(X)."],
  "clips_constructs":  [],
  "clips_file":        null,
  "initial_goal":      "mortal(X)",
  "max_cycles":        100,
  "persist":           true
}
```

`persist` defaults to `false`. When `true` and a `coire_store_path` is
configured, the handler saves a `DeductionSnapshot` to the store after
`run()` completes (whether converged, interrupted, or max-cycles).

When `persist: true` and no store is configured, the field is silently
ignored and a warning is logged — callers should not fail hard on a
persistence hint.

### POST /deduce/resume — takes only deduction_id

```json
{
  "deduction_id": "<uuid from original POST /deduce response>",
  "max_cycles":   100
}
```

| Field | Required | Notes |
|-------|----------|-------|
| `deduction_id` | yes | UUID that was returned by the original `POST /deduce` |
| `max_cycles` | no | Override cycle budget; defaults to value stored in snapshot |

The seed knowledge (`prolog_clauses`, `clips_constructs`, `clips_file`) is
loaded from the snapshot — the caller does not re-specify it. `initial_goal`
is NOT re-run on resume (cycle 0 acts as a regular cycle, processing restored
Coire events). If the caller wants to inject a new goal they can use
`POST /cycle/coire/push` before or after resuming.

**Response**: same `202 Accepted` + new `deduction_id` as `POST /deduce`.

### DELETE /deduce/{id}/snapshot — explicit delete

Removes the snapshot row and all associated Coire events (both
`prolog_session_id` and `clips_session_id` mailboxes) from the store
immediately, regardless of TTL.

**Response**: `200 OK` with `{ "deduction_id": "<uuid>", "status": "deleted" }`.
`404` if no snapshot exists for that ID.

---

## Error Cases for POST /deduce/resume

| Condition | HTTP Status | Body |
|-----------|-------------|------|
| `coire_store` not configured | `503` | `{ "error": "persistence not enabled" }` |
| No snapshot for `deduction_id` | `404` | `{ "error": "snapshot not found" }` |
| Either session UUID in `active_coire_sessions` | `409` | `{ "error": "session still active" }` |
| `restore_from` fails inside `spawn_blocking` | `DeductionEntry → Error(...)` | poll via `GET /deduce/{id}` |

---

## Internal Flow: POST /deduce with persist: true

```
spawn_blocking → run() completes
async wrapper removes from active_coire_sessions
async wrapper updates DeductionEntry with final result

if persist && coire_store is Some:
    save_snapshot(DeductionSnapshot {
        deduction_id,
        prolog_clauses, clips_constructs, clips_file, initial_goal, max_cycles,
        status:           final_status.to_string(),
        cycles_run:       final_cycles,
        prolog_session_id: tracked_ids.0,
        clips_session_id:  tracked_ids.1,
        created_at_ms:    now_ms,
        expires_at_ms:    now_ms + config.snapshot_ttl_ms,
    })
    // Coire pending events were already saved by controller's save_to_store()
```

Note: `save_snapshot` happens in the async wrapper after `bg_handle.await` —
it is a brief blocking call on `Arc<Mutex<Connection>>` and acceptable in
this context (same pattern as other DuckDB calls in the codebase).

---

## Internal Flow: POST /deduce/resume

```
POST /deduce/resume
  │
  ├─ 503 if coire_store is None
  ├─ load snapshot(deduction_id) → 404 if not found
  ├─ 409 if snapshot.prolog_session_id or .clips_session_id in active_coire_sessions
  │
  ├─ Insert new DeductionEntry { status: Running, ... }
  │
  └─ tokio::spawn(async move {
         let (ids_tx, ids_rx) = oneshot::channel();
         let bg = spawn_blocking(move || {
             let mut session = DeductionSession::new()?;
             session.seed_prolog(&snapshot.prolog_clauses)?;
             if let Some(ref path) = snapshot.clips_file {
                 session.seed_clips_file(path)?;
             }
             session.seed_clips(&snapshot.clips_constructs)?;
             let _ = ids_tx.send((session.prolog_id, session.clips_id));
             let max = req.max_cycles.unwrap_or(snapshot.max_cycles);
             let mut controller = CycleController::new(session, max, None, interrupt)
                 .with_store(store.clone());
             controller.restore_from(&store, snapshot.prolog_session_id, snapshot.clips_session_id)?;
             controller.run()
         });
         // identical active-session tracking + DeductionEntry update as start_deduce
         // if persist flag carried forward: save new snapshot on completion
     });
  │
  └─ 202 DeduceStartResponse { deduction_id: new_id, status: "running" }
```

### Should a resumed run also persist?

A resumed run does **not** automatically persist unless the caller explicitly
wants it to. Simplest approach: the resume request gains an optional
`persist: bool` field with the same semantics as the original request.

---

## TTL and Carrion-Picker Integration

### Snapshot TTL

Config key added to `PersistenceConfig`:
```toml
[persistence]
deduction_snapshot_ttl_seconds = 604800  # 7 days (default)
```

Separate from `coire_store_ttl_seconds` (which governs orphaned Coire entries
not associated with any snapshot). Snapshots have a longer default TTL because
they represent explicit user-created state.

### Extended Carrion-Picker sweep

Each sweep already deletes orphaned Coire sessions (no snapshot, newest event
older than `coire_store_ttl`). Extended behavior:

1. **Snapshot expiry sweep**: query `deduction_snapshots WHERE expires_at_ms < now_ms`.
   For each expired snapshot:
   - Delete `coire_events WHERE session_id = prolog_session_id`
   - Delete `coire_events WHERE session_id = clips_session_id`
   - Delete the snapshot row
   - Skip if `prolog_session_id` or `clips_session_id` is in `active_coire_sessions`

2. **Orphaned Coire sweep** (existing): unchanged — catches events not linked
   to any snapshot.

The sweep order matters: do snapshot expiry first, then orphan sweep, so that
Coire events deleted by snapshot expiry don't also appear in the orphan list.

---

## Storage Layer Changes (clara-coire)

### New `deduction_snapshots` table in CoireStore::open()

```sql
CREATE TABLE IF NOT EXISTS deduction_snapshots (
    deduction_id      VARCHAR NOT NULL PRIMARY KEY,
    prolog_clauses    VARCHAR NOT NULL,
    clips_constructs  VARCHAR NOT NULL,
    clips_file        VARCHAR,
    initial_goal      VARCHAR,
    max_cycles        INTEGER NOT NULL,
    status            VARCHAR NOT NULL,
    cycles_run        INTEGER NOT NULL,
    prolog_session_id VARCHAR NOT NULL,
    clips_session_id  VARCHAR NOT NULL,
    created_at_ms     BIGINT  NOT NULL,
    expires_at_ms     BIGINT  NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_snapshots_expires
    ON deduction_snapshots (expires_at_ms);
```

### New struct: DeductionSnapshot (in clara-coire)

```rust
pub struct DeductionSnapshot {
    pub deduction_id:       Uuid,
    pub prolog_clauses:     Vec<String>,
    pub clips_constructs:   Vec<String>,
    pub clips_file:         Option<String>,
    pub initial_goal:       Option<String>,
    pub max_cycles:         u32,
    pub status:             String,
    pub cycles_run:         u32,
    pub prolog_session_id:  Uuid,
    pub clips_session_id:   Uuid,
    pub created_at_ms:      i64,
    pub expires_at_ms:      i64,
}
```

### New CoireStore methods

| Method | Description |
|--------|-------------|
| `save_snapshot(&DeductionSnapshot)` | Insert or replace snapshot row |
| `load_snapshot(deduction_id) -> Option<DeductionSnapshot>` | Fetch by deduction_id |
| `delete_snapshot(deduction_id) -> usize` | Delete snapshot + its Coire events |
| `snapshots_expired(now_ms, exclude: &HashSet<Uuid>) -> Vec<DeductionSnapshot>` | Find expired, non-active snapshots |

`delete_snapshot` deletes in order: Coire events (both session IDs) then the
snapshot row. All three DELETEs happen under the same mutex lock.

---

## Files to Change

| Crate / File | Change |
|---|---|
| `clara-coire/src/store.rs` | `deduction_snapshots` table in schema; `DeductionSnapshot` struct; 4 new methods |
| `clara-coire/src/lib.rs` | Re-export `DeductionSnapshot` |
| `clara-coire/src/carrion_picker.rs` | Extend `sweep()` to expire snapshots before orphan sweep |
| `clara-config/src/schema.rs` | Add `deduction_snapshot_ttl_seconds: u64` to `PersistenceConfig` |
| `clara-config/src/defaults.rs` | Default `deduction_snapshot_ttl_seconds: 604800` |
| `config/default.toml` | Add `deduction_snapshot_ttl_seconds = 604800` |
| `clara-api/src/models/request.rs` | Add `persist: bool` to `DeduceRequest`; add `DeduceResumeRequest`; add `DeduceDeleteSnapshotResponse` |
| `clara-api/src/models/response.rs` | Add `DeduceDeleteSnapshotResponse` |
| `clara-api/src/models/mod.rs` | Re-export new types |
| `clara-api/src/handlers/deduce_handler.rs` | `start_deduce`: save snapshot when `persist && store is Some`; add `resume_deduce`; add `delete_snapshot` |
| `clara-api/src/routes/deduce.rs` | Re-export `resume_deduce`, `delete_snapshot` |
| `clara-api/src/routes/mod.rs` | Wire `POST /deduce/resume`, `DELETE /deduce/{id}/snapshot` |
| `clara-api/src/server.rs` | Pass `deduction_snapshot_ttl_seconds` to `CarrionPicker` |
| `docs/SESSION_LIFECYCLE.md` | Move resume + snapshot from Planned → Implemented |

---

## Implementation Order

1. `DeductionSnapshot` struct + `deduction_snapshots` table in `CoireStore`
2. `save_snapshot`, `load_snapshot`, `delete_snapshot`, `snapshots_expired` methods
3. Unit tests for new CoireStore methods
4. `deduction_snapshot_ttl_seconds` in `clara-config`
5. Extend `CarrionPicker::sweep()` for snapshot expiry
6. `persist: bool` field on `DeduceRequest`; snapshot save in `start_deduce`
7. `DeduceResumeRequest`; `resume_deduce` handler + route
8. `delete_snapshot` handler + `DELETE /deduce/{id}/snapshot` route
9. Manual integration test: deduce with persist, interrupt, resume, verify result
10. Update `docs/SESSION_LIFECYCLE.md`

---

## Open Questions

1. **Should a resumed run automatically persist?**
   Proposed: honor an explicit `persist: bool` on `DeduceResumeRequest` —
   same semantics. Default `false`. This lets callers chain resume → resume
   without accumulating snapshots unintentionally.

2. **Should `DELETE /deduce/{id}/snapshot` require the run to be finished?**
   Proposed: yes — return `409` if the session IDs are in `active_coire_sessions`.
   Deleting a live run's mailboxes would corrupt the in-flight state.

3. **Should we expose `GET /deduce/{id}/snapshot` for inspection?**
   Useful for debugging. Low implementation cost (one `load_snapshot` call).
   Can be added later without breaking anything.

4. **Derived Prolog facts**: if the caller wants Prolog-derived facts to survive
   a resume, they need to encode them into the `prolog_clauses` of the next
   request or relay them as Coire events. A future `GET /deduce/{id}/snapshot`
   endpoint plus a `prolog_clauses_override` field on the resume request would
   cover this cleanly.
