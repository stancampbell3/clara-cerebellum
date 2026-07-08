# Source Registry — Architecture Plan

**Feature:** DuckDB-backed source and artifact registry for Prolog/CLIPS source files and
their derived artifacts (DOT graphs, parsed rule structures, decorated sources)  
**Motivation:** Eliminate repeated Prolog re-parsing on trace visualization requests; make
source and metadata generally available across deductions, the Cobbler GUI, and future tooling  
**Depends on / enables:** `trace_visualization_plan.md`  
**Date:** 2026-04-10

---

## Problem

Two related issues converge here:

**1. Expensive reconstruction on every trace request.**  
`GET /deduce/{id}/trace/{change_id}/dot` must produce a colorized DOT graph. The current plan
re-joins `prolog_clauses: Vec<String>` from the snapshot, calls `parse_prolog_rules`, then
calls `generate_dot` on every request. Parsing is expensive; the rules don't change between
calls. There is no cache.

**2. Source managed as server-side file paths.**  
`DeduceRequest.clips_file: Option<String>` names a path that must exist on the server at
request time. There is no content-addressed identity, no deduplication across runs, and no
way for the Cobbler GUI (or anything else) to register, browse, or reuse knowledge-base
sources independently of a deduction run.

---

## Goals

1. **Cache derived artifacts** — parse once, store the result, serve from cache on subsequent
   requests. Zero re-parsing overhead for colorized DOT on trace replays.
2. **Content-addressed source storage** — SHA-256 dedup: the same source uploaded twice stores
   one row. Deductions referencing the same source share one cache entry.
3. **Source-first request model** — callers may submit a `source_id` in `DeduceRequest` instead
   of inline clauses or a server path. Inline and file-path modes auto-register on first use.
4. **General availability** — sources and artifacts are queryable by the Cobbler GUI, the
   transduction CLI, and future tooling without requiring an active deduction.
5. **GC integration** — sources and artifacts expire alongside their associated snapshots via
   the existing `CarrionPicker` sweep mechanism.
6. **In-memory trace path** — trace visualization works even without a persistent store, via
   a lightweight cache in `CycleController`'s lifetime.

---

## What Already Exists (Don't Rebuild)

| Location | Relevance |
|---|---|
| `clara-coire/src/store.rs` | `CoireStore` with `Arc<Mutex<Connection>>`, migration pattern, `CarrionPicker` sweep |
| `store.rs:141–167` | Column migration via `information_schema.columns` — extend this for new columns |
| `clara-cycle/src/transduction.rs:474` | `generate_dot(rules, coloring, opts)` — unchanged; we just stop re-parsing rules |
| `transduction.rs:57` | `PrologRule { head: Term, body: Vec<BodyGoal> }` — needs `Serialize/Deserialize` added |
| `clara-coire/src/cache_eviction.rs` | `CarrionPicker` — extend sweep to cover new tables |

---

## New Storage: Two Tables in `CoireStore`

Both tables live in the same on-disk DuckDB file as `coire_events`, `deduction_snapshots`,
and `tableau_changes`. The `CoireStore::open()` schema block is extended; the migration
loop handles upgrades to existing stores.

### `source_registry`

Content-addressed storage for raw source files.

```sql
CREATE TABLE IF NOT EXISTS source_registry (
    source_id     TEXT    NOT NULL PRIMARY KEY,  -- UUID
    content_hash  TEXT    NOT NULL,              -- SHA-256 hex of content
    source_type   TEXT    NOT NULL,              -- 'prolog' | 'clips'
    label         TEXT,                          -- optional human name, e.g. "ex1_clara"
    content       TEXT    NOT NULL,              -- raw source text
    created_at_ms BIGINT  NOT NULL,
    expires_at_ms BIGINT                         -- NULL = no expiry
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_source_hash_type
    ON source_registry (content_hash, source_type);
```

Uniqueness is on `(content_hash, source_type)`: the same Prolog source uploaded twice
returns the same `source_id` without a second row.

### `source_artifacts`

Derived artifacts keyed to a source.

```sql
CREATE TABLE IF NOT EXISTS source_artifacts (
    artifact_id      TEXT    NOT NULL PRIMARY KEY,  -- UUID
    source_id        TEXT    NOT NULL,              -- FK → source_registry.source_id
    artifact_type    TEXT    NOT NULL,              -- 'dot' | 'parsed_rules' | 'decorated_pl'
    content          TEXT    NOT NULL,              -- artifact text content
    generated_at_ms  BIGINT  NOT NULL,
    expires_at_ms    BIGINT                         -- NULL = inherit from source TTL
);
CREATE INDEX IF NOT EXISTS idx_artifact_source_type
    ON source_artifacts (source_id, artifact_type);
```

**Artifact types:**

| `artifact_type` | Content | Producer |
|---|---|---|
| `parsed_rules` | `serde_json::to_string(&Vec<PrologRule>)` | `parse_prolog_rules` + new serialize derive |
| `dot` | raw DOT text (uncolored) | `generate_dot(rules, None, opts)` |
| `decorated_pl` | decorated Prolog source | `decorate_source` |

The `parsed_rules` artifact is the critical one: it eliminates re-parsing on every
colorized DOT request.

---

## Prerequisite: `PrologRule` Serialization

**File:** `clara-cycle/src/transduction.rs`

`PrologRule`, `Term`, and `BodyGoal` are currently `Debug + Clone` only.
Add `Serialize + Deserialize` from `serde`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrologRule { ... }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Term { ... }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BodyGoal { ... }
```

This is a non-breaking change. No behavior changes; it only enables round-trip
through JSON for artifact caching.

---

## New `SourceRegistry` Struct

**New file:** `clara-coire/src/source.rs`

`SourceRegistry` shares the same `Arc<Mutex<Connection>>` as `CoireStore` — no
second DuckDB connection, no second file.

```rust
#[derive(Clone)]
pub struct SourceRegistry {
    conn: Arc<Mutex<Connection>>,
}
```

### Key methods

```rust
impl SourceRegistry {
    /// Register source content. Upserts by (content_hash, source_type).
    /// Returns the source_id (existing if already present, new if not).
    pub fn register(
        &self,
        source_type: &str,
        label: Option<&str>,
        content: &str,
        expires_at_ms: Option<i64>,
    ) -> CoireResult<Uuid>;

    /// Retrieve source entry by ID.
    pub fn get(&self, source_id: Uuid) -> CoireResult<Option<SourceEntry>>;

    /// Find source_id by content hash + type. Used to detect duplicates
    /// before registering inline clauses from a DeduceRequest.
    pub fn find_by_hash(
        &self,
        content_hash: &str,
        source_type: &str,
    ) -> CoireResult<Option<Uuid>>;

    /// Get an artifact, generating and caching it on miss.
    /// `generator` receives the source content string and returns the artifact text.
    pub fn get_or_create_artifact(
        &self,
        source_id: Uuid,
        artifact_type: &str,
        expires_at_ms: Option<i64>,
        generator: impl FnOnce(&str) -> CoireResult<String>,
    ) -> CoireResult<ArtifactEntry>;

    /// Get artifact by ID.
    pub fn get_artifact(&self, artifact_id: Uuid) -> CoireResult<Option<ArtifactEntry>>;

    /// Delete source and cascade-delete all its artifacts.
    pub fn delete(&self, source_id: Uuid) -> CoireResult<()>;

    /// Sweep sources with expires_at_ms <= now_ms.
    /// Cascades to source_artifacts. Returns rows deleted.
    pub fn sweep_expired(&self, now_ms: i64) -> CoireResult<usize>;
}
```

### Supporting types

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceEntry {
    pub source_id:     Uuid,
    pub content_hash:  String,
    pub source_type:   String,
    pub label:         Option<String>,
    pub content:       String,
    pub created_at_ms: i64,
    pub expires_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactEntry {
    pub artifact_id:      Uuid,
    pub source_id:        Uuid,
    pub artifact_type:    String,
    pub content:          String,
    pub generated_at_ms:  i64,
    pub expires_at_ms:    Option<i64>,
}
```

---

## `CoireStore` Integration

**File:** `clara-coire/src/store.rs`

Add `SourceRegistry` as a field of `CoireStore`, sharing the same connection:

```rust
pub struct CoireStore {
    conn: Arc<Mutex<Connection>>,
    pub sources: SourceRegistry,
}
```

`CoireStore::open()` initializes both sets of tables in the same `execute_batch` call.
`SourceRegistry::new(conn.clone())` takes the same Arc — no extra connection overhead.

### Migration additions

Extend the migration loop to add new columns to `deduction_snapshots`:

```rust
for (col, definition) in [
    ("context",              "VARCHAR NOT NULL DEFAULT '[]'"),
    ("tableau_entries",      "VARCHAR NOT NULL DEFAULT '[]'"),
    ("prolog_source_id",     "VARCHAR"),          // FK → source_registry
    ("clips_source_id",      "VARCHAR"),          // FK → source_registry
    ("dot_artifact_id",      "VARCHAR"),          // FK → source_artifacts
] { ... }
```

### `DeductionSnapshot` struct additions

```rust
/// FK to `source_registry` for the Prolog source used in this run.
/// When present, use this instead of re-parsing `prolog_clauses`.
#[serde(default)]
pub prolog_source_id: Option<Uuid>,

/// FK to `source_registry` for the CLIPS source.
#[serde(default)]
pub clips_source_id: Option<Uuid>,

/// FK to `source_artifacts` for the pre-generated base DOT (uncolored).
/// Populated on first trace request if not set at deduction time.
#[serde(default)]
pub dot_artifact_id: Option<Uuid>,
```

---

## `DeduceRequest` Changes

**File:** `clara-api/src/models/request.rs`

Add optional source ID fields alongside the existing inline fields. Priority order in the
handler: `source_id` → inline content → file path (auto-registers in each case).

```rust
/// Pre-registered Prolog source ID from `POST /source`.
/// When present, `prolog_clauses` is ignored.
#[serde(default)]
pub prolog_source_id: Option<Uuid>,

/// Pre-registered CLIPS source ID from `POST /source`.
/// When present, `clips_file` and `clips_constructs` are ignored.
#[serde(default)]
pub clips_source_id: Option<Uuid>,

/// Existing fields stay unchanged for backwards compatibility.
/// prolog_clauses, clips_constructs, clips_file, ...
```

### Handler resolution logic (deduce_handler.rs)

```
if prolog_source_id given:
    load source from registry → use content
else if prolog_clauses non-empty:
    hash content → register_or_get source_id
    (now have source_id for artifact caching)
else:
    no prolog source (bare goal run)

if persist=true and trace=true:
    ensure dot_artifact_id populated: get_or_create_artifact("dot", ...)
    store prolog_source_id + dot_artifact_id in snapshot
```

---

## Colorized DOT Flow (Optimized)

**Old flow (every trace request):**
```
join prolog_clauses → string
  → parse_prolog_rules (expensive)
  → generate_dot(rules, coloring, opts)
  → return DOT text
```

**New flow (with source registry):**
```
load snapshot → get dot_artifact_id (or prolog_source_id)
  → get_or_create_artifact("parsed_rules", source_id, || {
        parse_prolog_rules(source_content)  ← one-time only
        serde_json::to_string(rules)
     })
  → serde_json::from_str::<Vec<PrologRule>>(artifact.content)  ← fast
  → load tableau_change.entries_json → coloring_from_entries
  → generate_dot(&rules, Some(&coloring), opts)
  → return DOT text
```

After the first trace request for a given source, all subsequent colorized DOT calls
skip parsing entirely. `Vec<PrologRule>` deserialization + `generate_dot` is
O(n) string building with no I/O.

---

## In-Memory Trace Path (No Persistent Store)

**File:** `clara-cycle/src/controller.rs`

Addressing Clara's feedback: trace should work without a store for local/CI use.

Add to `CycleController`:

```rust
/// Populated when trace_mode=true and no store is configured.
/// Lives only for the duration of the run; included in DeductionResult.
trace_log: Option<Vec<InMemoryTraceEntry>>,
```

```rust
pub struct InMemoryTraceEntry {
    pub cycle_num:       u32,
    pub phase:           String,
    pub recorded_at_ms:  i64,
    pub entries:         Vec<PredicateEntry>,
}
```

`DeductionResult` gains:

```rust
/// Populated when trace_mode=true and no store was configured.
/// None when store-backed trace was used (query via GET /deduce/{id}/trace).
#[serde(default)]
pub trace: Option<Vec<InMemoryTraceEntry>>,
```

This means:
- `trace=true` + store configured → trace written to `tableau_changes`, queryable via API
- `trace=true` + no store → trace in `DeductionResult.trace` in the response body
- `trace=false` → no trace overhead in either case

---

## New Source API Endpoints

**New file:** `clara-api/src/handlers/source_handler.rs`

### `POST /source`

Register source content. Returns immediately; artifact generation is lazy (on first use).

Request:
```json
{
    "source_type": "prolog",
    "label": "ex1_clara",
    "content": "unlocked :- tumbler(1,set), ...",
    "ttl_seconds": null
}
```

Response `201 Created`:
```json
{
    "source_id": "uuid",
    "content_hash": "sha256hex",
    "is_new": true
}
```

`is_new: false` when the content hash matched an existing entry — same `source_id` returned.

### `GET /source/{id}`

Returns `SourceEntry` (without `content` by default; add `?include_content=true` for full text).

### `GET /source/{id}/artifact/{type}`

Returns or generates the named artifact (`dot`, `parsed_rules`, `decorated_pl`).
Content-type: `text/plain` for DOT and decorated_pl; `application/json` for parsed_rules.

### `DELETE /source/{id}`

Explicit delete. Cascades to artifacts. GC also sweeps expired sources automatically.

### `GET /source` (optional, Phase 2)

List registered sources with optional `?label=`, `?source_type=` filters. Useful for the
Cobbler rule editor's source browser.

---

## GC Integration (`CarrionPicker`)

**File:** `clara-coire/src/cache_eviction.rs`

Extend the sweep to call `store.sources.sweep_expired(now_ms)` alongside the existing
snapshot and event sweeps. Source TTL is typically tied to the associated snapshot TTL:

- Sources registered via auto-register from a `DeduceRequest` inherit
  `snapshot_ttl_ms` from the server config.
- Sources registered via `POST /source` with `ttl_seconds: null` are permanent
  until explicitly deleted.
- Artifact `expires_at_ms` inherits from its source when `None`.

Cascade behavior: deleting a source deletes its artifacts. The `CarrionPicker` calls
`sweep_expired` on sources first, which cascades; then a second pass cleans orphaned
artifacts (source deleted manually while artifact still referenced by snapshot).

---

## Files Changed Summary

| File | Change |
|---|---|
| `clara-cycle/src/transduction.rs` | Add `#[derive(Serialize, Deserialize)]` to `PrologRule`, `Term`, `BodyGoal` |
| `clara-coire/src/source.rs` | **New** — `SourceRegistry`, `SourceEntry`, `ArtifactEntry` |
| `clara-coire/src/store.rs` | Add `source_registry` + `source_artifacts` tables; migration columns; `sources: SourceRegistry` field |
| `clara-coire/src/cache_eviction.rs` | Extend `CarrionPicker` sweep to call `sources.sweep_expired` |
| `clara-coire/src/lib.rs` | `pub mod source;` |
| `clara-api/src/models/request.rs` | Add `prolog_source_id`, `clips_source_id` to `DeduceRequest`; add `trace: bool` |
| `clara-api/src/handlers/deduce_handler.rs` | Source resolution logic; wire `.with_trace()`; populate `dot_artifact_id` in snapshot |
| `clara-api/src/handlers/source_handler.rs` | **New** — CRUD endpoints for source registry |
| `clara-api/src/handlers/trace_handler.rs` | **New** — use `get_or_create_artifact("parsed_rules")` instead of re-parsing |
| `clara-api/src/server.rs` | Register source + trace routes |
| `clara-cycle/src/controller.rs` | `trace_mode`, `trace_log`, `InMemoryTraceEntry`; `.with_trace()`; `DeductionResult.trace` |
| `clara-coire/src/store.rs` | `DeductionSnapshot` gains `prolog_source_id`, `clips_source_id`, `dot_artifact_id` |

---

## Implementation Order

The dependency chain is shallow. Recommended order:

1. **`PrologRule` serialization** — unblocks everything else; one-line derive change
2. **`source_registry` + `source_artifacts` tables + `SourceRegistry` struct** — core storage
3. **`CoireStore` integration + migration** — wires registry into existing store
4. **`DeduceRequest` source fields + handler resolution** — source registration on deduce
5. **In-memory trace path** — `CycleController.trace_log` + `DeductionResult.trace`
6. **`trace_handler.rs`** — uses `get_or_create_artifact` for fast colorized DOT
7. **`source_handler.rs` + routes** — exposes registry to external callers
8. **`CarrionPicker` extension** — GC for new tables

Steps 1–4 are prerequisite for the trace visualization plan. Steps 5–8 are in parallel
with trace handler work.

---

## Decisions — All Questions Resolved

1. **Auto-register TTL for inline clauses.** **Decision: snapshot TTL.**
   When a `DeduceRequest` arrives with inline `prolog_clauses`, the auto-registered source
   inherits `snapshot_ttl_ms` from the server config. Simplest option; prevents accumulation
   without requiring callers to manage TTLs explicitly.

2. **`POST /source` permanence.** **Decision: document clearly; use snapshot TTL as
   default; `persist` flag keeps sources around.**
   `POST /source` with `ttl_seconds: null` uses the server's snapshot TTL by default.
   When a deduction runs with `persist: true`, its associated sources also persist for the
   snapshot lifetime. A `GET /source` list endpoint is deferred to Phase 2 so the Cobbler
   GUI can browse and manage named sources.

3. **Artifact invalidation.** **Decision: new source = new identity.**
   Re-registering the same label with different content produces a new `source_id` and
   new (empty) artifact cache. The old source_id is swept by GC when its TTL expires.
   The Cobbler rule editor displays the label-to-hash mapping so users can track versions.
   This also keeps the identity of reasoning inputs unambiguous — the source used in a
   deduction is immutably linked to a specific `source_id`.

4. **CLIPS source artifacts.** **Decision: register for identity/dedup only; no Rust-side
   artifacts for now.**
   CLIPS parsing is handled by the CLIPS engine, not our Rust code, so there is no
   parse structure to cache. The CLIPS source should be versioned and tracked alongside
   the generated Prolog (both registered in `source_registry`), keeping the pair of
   sources coherent for replay and audit. Artifact generation for CLIPS (e.g. integrating
   defrule structure into `generate_dot`) is future work, likely driven by the event-fired
   side of the reasoning cycle.

5. **`clips_file` deprecation path.** **Decision: integrate into Cobbler GUI first, then
   deprecate.**
   Once `clips_source_id` is exercised through the full GUI → API → store path, mark
   `clips_file: Option<String>` deprecated in the API docs. Keep it functional for
   backwards compatibility until the GUI no longer uses it.
