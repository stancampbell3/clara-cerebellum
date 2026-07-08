# Dagda Tableau Integration — Change Log

Branch: `ghost_watch`
Date: 2026-03-08

---

## What Was Changed / Added

### `clara-dagda`

#### New file: `src/kind.rs`
- `Kind` enum: `Rule | Predicate | Condition`
  - `Rule` — a Prolog rule head (`h :- body.`)
  - `Predicate` — a concrete predicate or body goal
  - `Condition` — a built-in arithmetic or comparison goal (`==`, `<`, `is`, etc.)
- `as_str()` / `from_str()` for DB serialization

#### `src/predicate.rs` — extended `PredicateEntry`
New fields added to the tableau row struct:

| Field | Type | Purpose |
|---|---|---|
| `entry_id` | `Uuid` | Stable per-row identifier (future: parent links) |
| `kind` | `Kind` | Rule, Predicate, or Condition |
| `source` | `Option<String>` | Functor of the rule that introduced this goal |
| `bound_vars` | `Vec<String>` | Variable names appearing in the args pattern |
| `bindings` | `Vec<Binding>` | Discovered variable→value pairs |
| `parent_id` | `Option<Uuid>` | Reserved for future explanation tree |

New struct: `Binding { var: String, val: String }`

#### `src/cache.rs` — extended `Dagda`
- Schema updated: all new columns + `ADD COLUMN IF NOT EXISTS` migration paths
- New methods:
  - `set_entry(&PredicateEntry)` — full-row upsert preserving all fields
  - `update_truth(session_id, functor, args, truth, bindings)` — targeted update
  - `export_session(session_id) -> Vec<PredicateEntry>` — snapshot serialization
  - `import_session(&[PredicateEntry])` — snapshot restore
  - `tableau_changed_since(session_id, since_ms) -> bool` — convergence helper
- All queries updated to select/insert all 12 columns
- 23 unit tests (including new: `set_entry_roundtrip`, `set_entry_updates_on_conflict`, `update_truth_creates_row`, `update_truth_updates_bindings`, `export_import_roundtrip`, `tableau_changed_since_*`)

#### `src/lib.rs`
- Re-exports added: `Kind`, `Binding`

---

### `clara-cycle`

#### `Cargo.toml`
- Added dependency: `clara-dagda = { path = "../clara-dagda" }`

#### `src/session.rs` — `DeductionSession`
- New field: `pub tableau: Dagda`
- `new()` initializes a fresh `Dagda` instance alongside the engine pair
- `seed_prolog()` now also calls `seed_tableau_from_source()` after consulting
- New private method `seed_tableau_from_source(&str)`:
  - Parses clauses via `parse_prolog_rules()`
  - Bare facts → `Kind::Predicate`, `KnownTrue`
  - Rule heads → `Kind::Rule`, `Unknown`
  - Body goals → `Kind::Predicate` or `Kind::Condition`, `Unknown`, with `source` set to the rule head functor and `parent_id` linking to the rule entry
  - Parse failures are logged and silently skipped
- New private helpers: `term_functor()`, `term_args_pattern()`, `concrete_bindings()`, `is_condition_goal()`, `clara_dagda_now_ms()`

#### `src/result.rs` — `DeductionResult`
New fields:

| Field | Type | Notes |
|---|---|---|
| `goal_bindings` | `Option<Vec<Binding>>` | Final bindings for the root goal |
| `tableau` | `Option<Vec<PredicateEntry>>` | Full tableau at convergence |
| `explanation` | `Option<serde_json::Value>` | Reserved — always `None` for now |

#### `src/controller.rs` — `CycleController`
- New private struct `GoalAgenda`: tracks `root_functor` and `last_cycle_ts`
  - `begin_cycle()` — snapshots the timestamp at the start of each cycle
  - `tableau_progressed()` — calls `tableau.tableau_changed_since(last_cycle_ts)`
  - `root_goal_resolved()` — checks if any entry for the root functor is resolved
- `run()` changes:
  - Creates a `GoalAgenda` at start
  - Calls `agenda.begin_cycle()` at the top of each cycle
  - After cycle 0 Prolog pass: calls `update_tableau_from_solutions()` to seed initial truth values from query solutions
  - On exit (converge/interrupt): populates `goal_bindings` and `tableau` in the returned `DeductionResult`
- `has_converged()` — **new criterion** (replaces old snapshot-only check):
  - Both Coire mailboxes empty
  - CLIPS agenda empty
  - **AND** (tableau fixed point since last cycle **OR** root goal resolved)
- New private methods:
  - `update_tableau_from_solutions()` — marks root goal `KnownTrue`/`KnownFalse` from cycle-0 solutions
  - `export_tableau()` — calls `tableau.export_session(prolog_id)`
  - `root_goal_bindings()` — reads root goal's bindings from tableau
- `restore_from()` — **signature changed**: now takes extra `prev_tableau: &[PredicateEntry]` and calls `tableau.import_session()`

#### `src/lib.rs`
- Re-exports added: `Binding`, `Dagda`, `Kind`, `PredicateEntry`, `TruthValue`

---

### `clara-coire`

#### `src/store.rs` — `DeductionSnapshot`
- New field: `tableau_entries: serde_json::Value` (`#[serde(default)]`, stored as JSON text column)
- DuckDB schema: `tableau_entries VARCHAR NOT NULL DEFAULT '[]'` added
- Migration: `ADD COLUMN IF NOT EXISTS tableau_entries ...` runs on `open()`
- `save_snapshot()` — writes `tableau_entries` column
- `load_snapshot()` — reads `tableau_entries` column
- `snapshots_expired()` — fills `tableau_entries` with `json!([])` (not needed for expiry logic)
- Test helpers and `carrion_picker.rs` test updated to supply the new field

---

### `clara-api`

#### `src/handlers/deduce_handler.rs`
- `start_deduce`:
  - Extracts `deduction_result.tableau` after the blocking task completes
  - Serializes it to `serde_json::Value` before building `DeductionSnapshot`
  - Passes as `tableau_entries` in `DeductionSnapshot`
- `resume_deduce`:
  - Reads `snap.tableau_entries` from the loaded snapshot
  - Deserializes to `Vec<PredicateEntry>` (best-effort, empty on failure)
  - Passes to `controller.restore_from(...)` — updated call signature

---

## Items Yet to be Addressed

### High priority

1. **Cycle-by-cycle tableau updates from relay and CLIPS**
   Currently the tableau is only updated from cycle-0 Prolog query solutions.
   Future work: after each relay pass and each CLIPS inference pass, parse the
   asserted/retracted facts and call `update_truth()` for the corresponding
   tableau entries.  This is where the tableau becomes truly "live" rather than
   just a seed-time snapshot.

2. **`clara_fy` / evaluator integration**
   The `evaluator_pass()` in `CycleController` is a stub.  When a body goal
   calls `clara_fy(Question, R)` and the LLM is invoked, the tableau entry for
   that goal should be set to `KnownUnresolved` while waiting and updated to
   `KnownTrue`/`KnownFalse` when the response arrives.  This requires wiring
   the FieryPit/LilDaemon LLM client into the cycle controller.

3. **Forward-chaining goal discovery**
   The `GoalAgenda`'s `active` goal set was scaffolded but not yet populated.
   As CLIPS forward-chaining fires rules and new goals are relayed back to
   Prolog, those goal functors should be added to the agenda so convergence
   waits for all of them — not just the initial root goal.

4. **API response — expose `tableau` in `GET /deduce/{id}`**
   `DeductionResult` now carries `tableau: Option<Vec<PredicateEntry>>` and it
   is serialized by `poll_deduce` already, but the API response model
   (`DeduceStatusResponse`) may need review / documentation updates.

### Medium priority

5. **Tableau persistence for max-cycles-exceeded path**
   When `CycleError::MaxCyclesExceeded` is returned, the tableau is currently
   not included in the error result (there is no `DeductionResult` to carry it).
   The in-progress tableau should still be saved via the snapshot so that the
   resume path can pick it up.  Currently the snapshot is saved (via
   `save_to_store`) but without the tableau.

6. **Explanation tree**
   `parent_id` in `PredicateEntry` and `explanation: Option<Value>` in
   `DeductionResult` are reserved but not populated.  Future: walk the
   `parent_id` chain to build a proof tree and return it in `explanation`.

7. **Tableau-aware convergence tuning**
   The current fixed-point check is conservative (any row update = progress).
   A more precise criterion would distinguish meaningful progress (a truth value
   moving from `Unknown` toward resolution) from noise (e.g. repeated relay
   of already-known facts).

8. **Dagda session ID alignment (Prolog vs CLIPS)**
   The tableau is keyed on `prolog_id` only.  CLIPS-originated facts relayed
   back to Prolog update the same session, but if we want per-engine tableau
   views in the future, we may want to store under both IDs or maintain a
   separate CLIPS-side view.

### Low priority / future

9. **`GET /deduce/{id}/tableau`** — dedicated endpoint to stream the live
   tableau during a running session (would require the Dagda instance to be
   accessible from `AppState`).

10. **Tableau diffing** — expose which entries changed between cycles as part
    of the result or streaming API, useful for debugging and visualization.

11. **Persist Dagda to a file-backed DuckDB** — currently always in-memory.
    For very large or long-running deductions an on-disk Dagda may be preferable
    to JSON-serializing the whole tableau into the snapshot blob.
