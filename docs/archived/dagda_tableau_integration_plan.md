# Clara-Dagda Tableau Integration Plan

## Overview

Integrate `clara-dagda` as a **live tableau** inside `clara-cycle`, tracking predicate truth
values and variable bindings as the reasoning cycle evolves. Persist/restore the tableau
alongside the deduction snapshot. Replace the current coire-count convergence criterion with
a goal-agenda / tableau-progress criterion.

---

## Phase 1: Extend `clara-dagda`

### 1.1 Schema changes — add columns to `dagda_predicates`

| New Column | Type | Purpose |
|---|---|---|
| `entry_id` | VARCHAR | Stable UUID/hash per entry (for future parent links) |
| `kind` | VARCHAR | `'rule'` \| `'predicate'` \| `'condition'` |
| `source` | VARCHAR NULLABLE | Functor/rule that introduced this entry |
| `bound_vars` | VARCHAR | JSON array of bound variable names (e.g. `["Bastard"]`) |
| `bindings_json` | VARCHAR | JSON array of `{var, val}` (e.g. `[{"var":"Level","val":"4"}]`) |
| `parent_id` | VARCHAR NULLABLE | Reserved for future explanation tree |

`args_json` still serves as part of the primary key; it holds the argument pattern (with
unbound vars as `"*"`).

### 1.2 Add `Kind` enum

`Rule | Predicate | Condition` with serde string serialization.

### 1.3 Extend `PredicateEntry`

Carry all new fields. Derive `serde::Serialize/Deserialize` (needed for snapshot JSON).

### 1.4 New `Dagda` API methods

- `set_entry(entry: PredicateEntry)` — full-row upsert
- `update_truth(session_id, functor, args_json, truth_value, bindings_json)` — targeted update
- `export_session(session_id) -> Vec<PredicateEntry>` — for snapshot persistence
- `import_session(entries: Vec<PredicateEntry>)` — for restore
- `tableau_changed_since(session_id, since_ms: i64) -> bool` — convergence helper

---

## Phase 2: Integrate into `clara-cycle`

### 2.1 `DeductionSession` — add tableau field

```rust
pub struct DeductionSession {
    pub prolog: PrologEnvironment,
    pub clips:  ClipsEnvironment,
    pub prolog_coire_id: Uuid,
    pub clips_coire_id:  Uuid,
    pub tableau: Dagda,           // <-- new
}
```

`Dagda` is already `Clone` (shares an `Arc<Mutex<Connection>>`), so the controller can hold
an independent handle for convergence checks.

### 2.2 Tableau initialization at seed time

When `seed_prolog()` is called, parse the clauses with the existing `parse_prolog_rules()`
from `transduction.rs` and populate the tableau:

- Each **fact** `f(a,b).` → `Kind::Predicate`, `TruthValue::KnownTrue`, bindings filled
- Each **rule head** `h :- ...` → `Kind::Rule`, `TruthValue::Unknown`
- Each **body goal** → `Kind::Predicate` or `Kind::Condition` (arithmetic/comparison goals),
  `TruthValue::Unknown`, bound vars captured

### 2.3 `CycleController` — tableau updates during the cycle

In each cycle iteration, after the Prolog query step:

- Parse solutions from `fiery-pit-client` query results
- For each goal that returned solutions: `update_truth(..., KnownTrue, bindings)`
- For each goal that returned empty solutions: `update_truth(..., KnownFalse, [])`
- For `clara_fy`-style goals pending LLM response: set `KnownUnresolved`
- After relay steps: update truth values for relayed predicates

### 2.4 Goal agenda and revised convergence criterion

Replace the current simple snapshot comparison with:

```
converged = true iff ALL of:
  1. prolog_pending == 0  (coire mailbox empty)
  2. clips_pending == 0
  3. clips agenda empty
  4. !tableau.tableau_changed_since(session_id, last_cycle_timestamp_ms)
     — tableau reached a fixed point (no truth-value or binding changes)
```

If the root goal's tableau entry is `KnownTrue` or `KnownFalse`, convergence is triggered
immediately regardless of agenda emptiness.

A `GoalAgenda` struct tracks:

```rust
struct GoalAgenda {
    root_goal:    String,           // initial goal functor/arity
    active_goals: HashSet<String>,  // functors currently UNKNOWN in tableau
    last_snapshot_ms: i64,
}
```

### 2.5 `DeductionResult` extensions

```rust
pub struct DeductionResult {
    pub status:           CycleStatus,
    pub cycles:           u32,
    pub prolog_session:   Uuid,
    pub clips_session:    Uuid,
    pub prolog_solutions: Option<serde_json::Value>,
    pub goal_bindings:    Option<Vec<Binding>>,        // final bindings for root goal
    pub tableau:          Option<Vec<PredicateEntry>>, // final tableau state
    pub explanation:      Option<serde_json::Value>,   // reserved, always None for now
}

pub struct Binding {
    pub var: String,
    pub val: String,
}
```

---

## Phase 3: Snapshot persistence

### 3.1 Extend `DeductionSnapshot`

```rust
pub struct DeductionSnapshot {
    // existing fields ...
    pub tableau_entries: Vec<PredicateEntry>,  // <-- new
}
```

### 3.2 Save path

On cycle exit (converged, interrupted, or max-cycles-exceeded + persistent):

```
tableau.export_session(session_id) → snapshot.tableau_entries
```

### 3.3 Restore path

In `CycleController::restore_from()`:

```
session.tableau.import_session(snapshot.tableau_entries)
```

Runs alongside the existing Coire event restoration.

---

## Phase 4: API surface changes (minimal)

- `GET /deduce/{id}` response: include `goal_bindings` and `tableau` fields from `DeductionResult`
- `GET /deduce/{id}/snapshot`: `tableau_entries` field present in response JSON (via snapshot struct extension)
- No new endpoints needed

---

## Implementation Order

1. **`clara-dagda`**: schema migration + new fields + new API methods + serde derives + tests
2. **`clara-cycle/session.rs`**: add `tableau: Dagda` field, populate from `seed_prolog()`
3. **`clara-cycle/controller.rs`**: tableau update calls per cycle, `GoalAgenda`, new convergence check, tableau save/restore
4. **`clara-cycle/result.rs`**: extend `DeductionResult` with `goal_bindings`, `tableau`, `explanation` placeholder
5. **`clara-api/handlers/deduce_handler.rs`**: extend `DeductionSnapshot` struct, wire save/restore

---

## Design Decisions & Constraints

- **No separate `Tableau` type** — `Dagda` already is the tableau; we extend it in place
- **In-memory DuckDB** stays as-is; `export`/`import` bridge to JSON in the snapshot blob
- **Explanation deferred** — `parent_id` in schema + `explanation: Option<Value>` in result
  reserves the slot; no logic implemented yet
- **Tableau init is best-effort** — if clause parsing yields nothing (e.g. pure CLIPS run),
  tableau starts empty and fills from relay updates
- **`parse_prolog_rules()` reuse** — `transduction.rs` already has all the parsing machinery;
  no duplication needed
