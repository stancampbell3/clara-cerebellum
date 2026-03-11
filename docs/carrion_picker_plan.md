# Carrion-Picker: CoireStore Maintenance Thread

## Purpose

After each deduction run, `CycleController` saves both engine mailboxes to the
`CoireStore` (if configured), then clears the in-memory `Coire`. The persistent
store accumulates these snapshots over time. Without active cleanup, the store
grows unbounded with entries that are:

- **Stale** — saved from completed deductions whose results have already been
  consumed by the caller
- **Orphaned** — saved from runs that ended when the process crashed before
  the caller could inspect or delete them

The carrion-picker is a background task that periodically sweeps `CoireStore`
and deletes sessions that have exceeded a configured time-to-live (TTL).

---

## Design

### What gets deleted

A stored session (identified by a `Uuid`) is eligible for deletion when the
**newest event** in that session (`MAX(created_at_ms)`) is older than
`ttl_seconds` from the current wall clock. Using the newest event rather than
the oldest correctly handles sessions that accumulated events over multiple
cycles before being saved.

Sessions that are currently active (i.e., their UUIDs belong to a deduction
that is still `Running` in `AppState::deductions`) must never be deleted, even
if they appear old. This requires the picker to consult the active session ID
set at sweep time.

### Stale vs. orphaned distinction

Both cases share the same TTL-based deletion rule — no separate logic is needed.
The TTL is tuned to be comfortably longer than the longest expected deduction
run, so any entry surviving past it is safely considered abandoned.

---

## Configuration

Add to `clara-config/src/schema.rs` `PersistenceConfig`:

```rust
/// Carrion-picker sweep interval in seconds. How often the background task
/// wakes to scan the CoireStore. Default: 3600 (1 hour).
pub coire_store_sweep_interval_seconds: u64,

/// Time-to-live for CoireStore entries in seconds. Sessions whose newest
/// event is older than this are deleted by the carrion-picker.
/// Default: 86400 (24 hours). Set to 0 to disable the picker entirely.
pub coire_store_ttl_seconds: u64,
```

Add to `config/default.toml`:

```toml
[persistence]
coire_store_path = "./data/coire.duckdb"
coire_store_ttl_seconds = 86400          # 24 hours
coire_store_sweep_interval_seconds = 3600 # sweep every hour
```

Setting `coire_store_ttl_seconds = 0` disables the picker even if a store path
is configured.

---

## Active Session Tracking

The picker needs to know which session UUIDs are currently live. This requires
a small addition to `AppState` and `DeductionEntry`:

### `DeductionEntry` additions (`clara-api/src/handlers/session_handler.rs`)

```rust
pub struct DeductionEntry {
    pub status:           CycleStatus,
    pub result:           Option<DeductionResult>,
    pub cycles:           u32,
    pub interrupt:        Arc<AtomicBool>,
    pub created_at:       std::time::Instant,
    // NEW — populated as soon as the DeductionSession is created inside
    // spawn_blocking, before run() is called.
    pub prolog_session_id: Option<Uuid>,
    pub clips_session_id:  Option<Uuid>,
}
```

### `AppState` addition

```rust
pub struct AppState {
    // existing fields ...
    pub coire_store: Option<clara_coire::CoireStore>,
    // NEW — shared with CarrionPicker so it can read active session IDs
    // without taking a full lock on the deductions map.
    pub active_coire_sessions: Arc<RwLock<HashSet<Uuid>>>,
}
```

`active_coire_sessions` is populated when `DeductionEntry` gets its session IDs
and cleared when the entry transitions out of `Running`. The picker reads this
set at sweep time to compute the exclusion list.

---

## CoireStore additions (`clara-coire/src/store.rs`)

The sweep query needs to find sessions with a newest event older than the TTL
cutoff, excluding a provided set of active UUIDs.

```rust
/// Return session IDs whose newest event is older than `older_than_ms`
/// (Unix milliseconds) and are not in `exclude`.
pub fn sessions_older_than(
    &self,
    older_than_ms: i64,
    exclude: &HashSet<Uuid>,
) -> CoireResult<Vec<Uuid>>
```

Implementation:

```sql
SELECT session_id, MAX(created_at_ms) AS newest
FROM coire_events
GROUP BY session_id
HAVING MAX(created_at_ms) < ?
ORDER BY newest ASC
```

Filter the `exclude` set in Rust after fetching (avoids constructing a dynamic
SQL IN-clause).

---

## CarrionPicker struct (`clara-coire/src/carrion_picker.rs`)

```rust
pub struct CarrionPicker {
    store:    CoireStore,
    ttl:      Duration,
    interval: Duration,
    active:   Arc<RwLock<HashSet<Uuid>>>,
}

impl CarrionPicker {
    pub fn new(
        store:    CoireStore,
        ttl:      Duration,
        interval: Duration,
        active:   Arc<RwLock<HashSet<Uuid>>>,
    ) -> Self

    /// Spawn the picker as a tokio background task.
    /// Returns a JoinHandle that can be used to abort on shutdown.
    pub fn spawn(self) -> tokio::task::JoinHandle<()>

    /// Run one sweep: find stale sessions, delete them, log results.
    async fn sweep(&self) -> usize  // returns count deleted
}
```

The `spawn` implementation:

```rust
pub fn spawn(self) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(self.interval).await;
            let deleted = self.sweep().await;
            if deleted > 0 {
                log::info!("CarrionPicker: deleted {} stale CoireStore sessions", deleted);
            } else {
                log::debug!("CarrionPicker: sweep complete, nothing to delete");
            }
        }
    })
}
```

The `sweep` implementation:

```rust
async fn sweep(&self) -> usize {
    use std::time::{SystemTime, UNIX_EPOCH};

    let cutoff_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
        - self.ttl.as_millis() as i64;

    let exclude = self.active.read().unwrap().clone();

    let stale = match self.store.sessions_older_than(cutoff_ms, &exclude) {
        Ok(ids) => ids,
        Err(e) => {
            log::warn!("CarrionPicker: sweep query failed: {}", e);
            return 0;
        }
    };

    let mut deleted = 0;
    for id in stale {
        match self.store.delete_session(id) {
            Ok(n) => {
                deleted += n;
                log::debug!("CarrionPicker: deleted session {} ({} events)", id, n);
            }
            Err(e) => log::warn!("CarrionPicker: failed to delete session {}: {}", id, e),
        }
    }
    deleted
}
```

---

## Wiring into `clara-api`

In `server.rs`, after opening the `CoireStore`, spawn the picker if TTL > 0:

```rust
if let Some(store) = &coire_store {
    let ttl      = config.persistence.coire_store_ttl_seconds;
    let interval = config.persistence.coire_store_sweep_interval_seconds;

    if ttl > 0 {
        let picker = CarrionPicker::new(
            store.clone(),
            Duration::from_secs(ttl),
            Duration::from_secs(interval),
            app_state.active_coire_sessions.clone(),
        );
        picker.spawn();  // fire-and-forget; aborts when server stops
        info!("CarrionPicker spawned (ttl={}s, interval={}s)", ttl, interval);
    }
}
```

---

## Implementation Order

1. **`clara-config`** — add `coire_store_ttl_seconds` and `coire_store_sweep_interval_seconds` to `PersistenceConfig` and `defaults.rs`; update `config/default.toml`
2. **`clara-api/DeductionEntry`** — add `prolog_session_id: Option<Uuid>` and `clips_session_id: Option<Uuid>`; populate them in `start_deduce` before calling `run()`; maintain `active_coire_sessions` set in `AppState`
3. **`clara-coire/src/store.rs`** — add `sessions_older_than(cutoff_ms, exclude)`
4. **`clara-coire/src/carrion_picker.rs`** — implement `CarrionPicker` struct and `spawn`/`sweep`
5. **`clara-coire/src/lib.rs`** — export `CarrionPicker`
6. **`clara-api/src/server.rs`** — spawn the picker after `CoireStore` init; add `active_coire_sessions` to `AppState`
7. **`docs/SESSION_LIFECYCLE.md`** — update Planned section

---

## Open Questions

- **Shutdown grace period**: should the picker finish its in-progress sweep
  before the server stops, or is an abrupt abort acceptable? Recommend a
  tokio `CancellationToken` for clean shutdown.
- **Partial sweeps**: if the store has many stale sessions, should the picker
  delete in batches (e.g. 100 at a time) to avoid long lock holds?
- **Metrics**: emit a counter for deleted sessions so operators can observe
  accumulation rate.
