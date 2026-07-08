# Evaluate Cache TTL & CarrionPicker Integration — Implementation Plan

**Status:** Phases 1–5 implemented; Phase 6 pending planning  
**Author:** Stan Campbell  
**Date:** 2026-04-03  
**Last updated:** 2026-04-03

---

## Background

`clara-toolbox/src/ffi.rs` maintains a process-global memoisation cache for LLM evaluation results.
Previously the only eviction mechanism was `clear_evaluate_cache()`, a full wipe with no metadata.
This work adds TTL eviction, per-deduction eviction, and domain attribution to the cache, delegating
sweep responsibility to `CarrionPicker` in `clara-coire`.

---

## Goals

- Attach **creation timestamp** and **source deduction ID** to every cache entry. ✅
- Give `CarrionPicker` the ability to evict entries **older than a configured TTL** on each sweep. ✅
- Evict entries associated with a **specific deduction session** when that session is cleaned up. ✅
- Lay groundwork for **Coire persistence and cross-domain gossip** of cache entries (Phase 6). ✅
- Keep `clara-coire` free of a direct dependency on `clara-toolbox` (wrong dependency direction). ✅

## Non-Goals

- Persistent on-disk evaluation cache (Phase 6).
- LRU or size-based eviction (TTL + session-scoped eviction is sufficient for now).
- Cache invalidation logic (prompts are treated as pure functions of their input).

---

## Dependency Constraints

```
clara-coire  (defines EvaluateCacheEviction trait)
     ↑
clara-toolbox  (implements trait via ToolboxCacheEviction; holds cache + globals)
     ↑
clara-cycle / clara-api  (wiring layer)
```

`clara-coire` has no dependency on `clara-toolbox`. The integration uses a trait object injected
into `CarrionPicker` at construction time by `clara-api`.

---

## Phase 1 — `CacheEntry` Metadata Struct ✅

**Crate:** `clara-toolbox`  
**Files:** `src/ffi.rs`  
**Status:** Implemented and tested

Replaced the raw `String` cache value with:

```rust
pub struct CacheEntry {
    pub value:         String,
    pub created_at_ms: i64,
    pub size_bytes:    usize,
    pub deduction_id:  Option<Uuid>,   // populated in Phase 2
    pub domain_id:     Option<String>, // populated in Phase 5
}
```

Cache type: `HashMap<String, CacheEntry>`.

New public API: `evaluate_cache_stats()`, `evict_cache_older_than(cutoff_ms)`,
`evict_cache_by_deduction(id)`. `clear_evaluate_cache()` retained.

---

## Phase 2 — Thread-Local Deduction Context ✅

**Crates:** `clara-toolbox`, `clara-cycle`  
**Files:** `src/ffi.rs`, `src/controller.rs`, `Cargo.toml`  
**Status:** Implemented and tested

Thread-local `CURRENT_DEDUCTION_ID: Cell<Option<Uuid>>` propagates the active deduction ID
through the FFI call stack without changing any C signatures.

Key API:
- `set_current_deduction_id(id: Option<Uuid>)` — raw set/clear
- `deduction_context(id: Uuid) -> DeductionContextGuard` — RAII guard; restores `None` on drop including unwind

`CycleController::run()` installs the guard at the top of `run()`:
```rust
let _ctx = clara_toolbox::ffi::deduction_context(self.deduction_id);
```
All three exit paths (Converged, Interrupted, MaxCyclesExceeded) are covered automatically.

`clara-toolbox` promoted from `dev-dependencies` to `dependencies` in `clara-cycle/Cargo.toml`.

> **Design note:** `thread_local!` is correct because all FFI callbacks are synchronous
> (same OS thread as the controller pass). If evaluation is ever called from async context,
> migrate to `tokio::task_local!`.

---

## Phase 3 — Cache Eviction Trait ✅

**Crates:** `clara-coire`, `clara-toolbox`  
**Files:** `clara-coire/src/cache_eviction.rs` (new), `clara-toolbox/src/ffi.rs`  
**Status:** Implemented and tested

`EvaluateCacheEviction` trait defined in `clara-coire`:

```rust
pub trait EvaluateCacheEviction: Send + Sync {
    fn evict_older_than(&self, cutoff_ms: i64) -> usize;
    fn evict_by_deduction(&self, deduction_id: Uuid) -> usize;
}
```

`ToolboxCacheEviction` (zero-sized struct) in `clara-toolbox` implements the trait by delegating
to the module-level functions. Re-exported at `clara_toolbox::ToolboxCacheEviction`.

`clara-coire` added as a dependency of `clara-toolbox`.

---

## Phase 4 — CarrionPicker Integration ✅

**Crate:** `clara-coire`  
**Files:** `src/carrion_picker.rs`  
**Status:** Implemented and tested

`CarrionPicker` gains two new fields (`cache_ttl`, `cache_eviction`) and a builder method:

```rust
pub fn with_cache_eviction(mut self, cache_ttl: Duration, eviction: Arc<dyn EvaluateCacheEviction>) -> Self
```

`sweep()` return type extended to `(usize, usize, usize)` — `(snapshots, orphan_events, cache_entries)`.

**Pass 1** (snapshot expiry): after each successful `delete_snapshot`, calls
`evict_by_deduction(snap.deduction_id)`. Skipped if the delete itself failed.

**Pass 3** (new): calls `evict_older_than(now_ms - cache_ttl_ms)`. Only runs when a handler
is configured. Result folded into the third tuple member along with pass-1 deduction evictions.

Both passes are no-ops when `cache_eviction` is `None` (default), so existing deployments
that don't configure a store are unaffected.

---

## Phase 5 — Domain ID Metadata & Wiring ✅

**Crates:** `clara-config`, `clara-toolbox`, `clara-api`  
**Files:** `clara-config/src/schema.rs`, `clara-config/src/defaults.rs`,
`clara-toolbox/src/ffi.rs`, `clara-toolbox/src/lib.rs`, `clara-api/src/server.rs`  
**Status:** Implemented and tested

### Config additions (`clara-config`)

`ServerConfig::dis_domain_id: Option<String>` — optional Dis domain identifier stamped on cache entries.

`PersistenceConfig::evaluate_cache_ttl_seconds: u64` — controls `CarrionPicker` pass 3 TTL.
Default: `14400` (4 h). Set to `0` to disable TTL eviction (per-deduction eviction still runs).

### Domain-ID global (`clara-toolbox`)

`DOMAIN_ID: OnceLock<String>` process-global. First writer wins; subsequent calls log and no-op.

```rust
pub fn set_domain_id(id: String);
pub fn domain_id() -> Option<&'static str>;
```

`CacheEntry::domain_id` now reads `domain_id().map(str::to_string)` at insertion time.

### Wiring (`clara-api/src/server.rs`)

At startup:
1. Calls `set_domain_id(id)` if `config.server.dis_domain_id` is set.
2. When `coire_store` is open and `evaluate_cache_ttl_seconds > 0`, attaches
   `ToolboxCacheEviction` to `CarrionPicker` via `.with_cache_eviction(...)`.
   When TTL is `0`, logs `cache_eviction=disabled` and skips the handler.

---

## Phase 6 — Coire Persistence & Gossip (Pending)

> **Prerequisite:** Draft an ADR sketching the `evaluate_cache_entry` Coire event shape and
> the receiving-domain hydration path before writing any code.

Proposed approach:

1. On cache insertion, write a `ClaraEvent` of type `evaluate_cache_entry` into the Coire
   under the active deduction's prolog session ID (or a dedicated well-known session).
2. The Feathers/Kafka relay fans these events out to peer Dis domains.
3. Receiving domains hydrate their own in-memory cache from inbound events.
4. Each domain's `CarrionPicker` applies its own TTL independently — no distributed coordination.

**Open design questions for Phase 6:**

- What is the canonical Coire session ID for cache events that have no deduction context?
- Should receiving domains apply the sender's `domain_id` as a filter (skip entries from self)?
- Maximum event payload size: should large cache values be truncated or stored by reference?
- Should the hydration path bypass the normal `evaluate_json_string` call counter so
  cache-warmed entries are not counted as real executions?

---

## Resolved Questions (from original open questions)

| # | Question | Resolution |
|---|----------|------------|
| 1 | `cache_ttl` default value | **14400 s (4 h)** — set in `PersistenceConfig` defaults |
| 2 | Sweep return type arity change | **Updated to 3-tuple** — all callers in `clara-coire` updated; no external callers existed |
| 3 | Thread-local vs. explicit parameter | **`thread_local!`** — correct for synchronous FFI; revisit if async evaluate calls are added |
| 4 | Trait object vs. generic | **`Arc<dyn EvaluateCacheEviction>`** — vtable cost negligible vs. LLM call cost |
| 5 | Domain ID config key | **`server.dis_domain_id: Option<String>`** in `clara-config`; optional |
