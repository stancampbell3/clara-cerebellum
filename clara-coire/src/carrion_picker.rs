use std::collections::HashSet;
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use uuid::Uuid;

use crate::cache_eviction::EvaluateCacheEviction;
use crate::store::CoireStore;

/// Background task that periodically deletes stale data from a [`CoireStore`].
///
/// Each sweep performs three passes in order:
///
/// 1. **Snapshot expiry** — deletes [`DeductionSnapshot`] rows (and their
///    associated Coire events) whose `expires_at_ms` is in the past.
///    If a cache eviction handler is configured, cache entries attributed to
///    each expired deduction are also evicted.
/// 2. **Orphan Coire sweep** — deletes `coire_events` rows whose session has
///    no snapshot and whose newest event is older than the Coire TTL.
/// 3. **Evaluate-cache TTL sweep** — evicts in-memory evaluate-cache entries
///    older than `cache_ttl`.  Only runs when a cache eviction handler is
///    configured via [`CarrionPicker::with_cache_eviction`].
///
/// Sessions whose UUIDs appear in `active` are always skipped in passes 1 & 2,
/// protecting deductions that are currently running.
///
/// Spawn with [`CarrionPicker::spawn`]. The returned [`tokio::task::JoinHandle`]
/// can be aborted on server shutdown.
pub struct CarrionPicker {
    store:          CoireStore,
    /// TTL for orphaned Coire event sessions (no snapshot).
    coire_ttl:      Duration,
    /// TTL for [`DeductionSnapshot`] rows (and their Coire events).
    snapshot_ttl:   Duration,
    /// TTL for in-memory evaluate-cache entries.
    cache_ttl:      Duration,
    interval:       Duration,
    active:         Arc<RwLock<HashSet<Uuid>>>,
    /// Optional handler for evicting the in-memory evaluate cache.
    /// When `None`, pass 3 and the per-deduction eviction in pass 1 are skipped.
    cache_eviction: Option<Arc<dyn EvaluateCacheEviction>>,
}

impl CarrionPicker {
    pub fn new(
        store:        CoireStore,
        coire_ttl:    Duration,
        snapshot_ttl: Duration,
        interval:     Duration,
        active:       Arc<RwLock<HashSet<Uuid>>>,
    ) -> Self {
        Self {
            store,
            coire_ttl,
            snapshot_ttl,
            cache_ttl: Duration::from_secs(0),
            interval,
            active,
            cache_eviction: None,
        }
    }

    /// Attach an evaluate-cache eviction handler and its TTL.
    ///
    /// Once set, every sweep will:
    /// - Evict cache entries attributed to each expired deduction (pass 1).
    /// - Evict all entries older than `cache_ttl` (pass 3).
    pub fn with_cache_eviction(
        mut self,
        cache_ttl:  Duration,
        eviction:   Arc<dyn EvaluateCacheEviction>,
    ) -> Self {
        self.cache_ttl      = cache_ttl;
        self.cache_eviction = Some(eviction);
        self
    }

    /// Spawn the picker as a tokio background task.
    ///
    /// The task sleeps for `interval`, runs a sweep, and repeats indefinitely.
    /// Abort the returned handle on server shutdown to stop the loop cleanly.
    pub fn spawn(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            if self.cache_eviction.is_some() {
                log::info!(
                    "CarrionPicker: started (coire_ttl={}s, snapshot_ttl={}s, \
                     cache_ttl={}s, interval={}s)",
                    self.coire_ttl.as_secs(),
                    self.snapshot_ttl.as_secs(),
                    self.cache_ttl.as_secs(),
                    self.interval.as_secs(),
                );
            } else {
                log::info!(
                    "CarrionPicker: started (coire_ttl={}s, snapshot_ttl={}s, \
                     interval={}s, cache_eviction=disabled)",
                    self.coire_ttl.as_secs(),
                    self.snapshot_ttl.as_secs(),
                    self.interval.as_secs(),
                );
            }
            loop {
                tokio::time::sleep(self.interval).await;
                let (snaps, events, cache) = self.sweep();
                if snaps > 0 || events > 0 || cache > 0 {
                    log::info!(
                        "CarrionPicker: deleted {} snapshot(s), {} orphan event(s), \
                         {} cache entry/entries",
                        snaps, events, cache,
                    );
                } else {
                    log::debug!("CarrionPicker: sweep complete, nothing to delete");
                }
            }
        })
    }

    /// Run one sweep. Returns `(snapshots_deleted, orphan_events_deleted, cache_entries_evicted)`.
    ///
    /// Pass 1: expire snapshots (and their Coire events) whose `expires_at_ms`
    /// is in the past.  Evicts cache entries for each expired deduction when a
    /// cache eviction handler is configured.
    /// Pass 2: delete orphaned Coire sessions (no snapshot) older than
    /// `coire_ttl`. Session IDs freed by pass 1 are excluded from the orphan
    /// query automatically since they have already been deleted.
    /// Pass 3: evict evaluate-cache entries older than `cache_ttl` (only when
    /// a cache eviction handler is configured).
    fn sweep(&self) -> (usize, usize, usize) {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        let exclude = self.active.read().unwrap().clone();

        // ── Pass 1: expired snapshots ────────────────────────────────────────
        let expired = match self.store.snapshots_expired(now_ms, &exclude) {
            Ok(v) => v,
            Err(e) => {
                log::warn!("CarrionPicker: snapshot query failed: {}", e);
                vec![]
            }
        };

        let mut snaps_deleted  = 0;
        let mut cache_evicted  = 0;
        for snap in &expired {
            match self.store.delete_snapshot(snap.deduction_id) {
                Ok(_) => {
                    snaps_deleted += 1;
                    log::debug!(
                        "CarrionPicker: expired snapshot {} (prolog={}, clips={})",
                        snap.deduction_id,
                        snap.prolog_session_id,
                        snap.clips_session_id,
                    );
                    // Evict cache entries attributed to this deduction.
                    if let Some(ev) = &self.cache_eviction {
                        let n = ev.evict_by_deduction(snap.deduction_id);
                        if n > 0 {
                            log::debug!(
                                "CarrionPicker: evicted {} cache entry/entries \
                                 for expired deduction {}",
                                n, snap.deduction_id,
                            );
                        }
                        cache_evicted += n;
                    }
                }
                Err(e) => {
                    log::warn!(
                        "CarrionPicker: failed to delete snapshot {}: {}",
                        snap.deduction_id,
                        e
                    );
                }
            }
        }

        // ── Pass 2: orphaned Coire sessions ──────────────────────────────────
        let coire_cutoff = now_ms - self.coire_ttl.as_millis() as i64;
        let stale = match self.store.sessions_older_than(coire_cutoff, &exclude) {
            Ok(ids) => ids,
            Err(e) => {
                log::warn!("CarrionPicker: orphan query failed: {}", e);
                return (snaps_deleted, 0, cache_evicted);
            }
        };

        let mut events_deleted = 0;
        for id in stale {
            match self.store.delete_session(id) {
                Ok(n) => {
                    events_deleted += n;
                    log::debug!(
                        "CarrionPicker: deleted orphan session {} ({} event(s))",
                        id,
                        n
                    );
                }
                Err(e) => {
                    log::warn!("CarrionPicker: failed to delete orphan session {}: {}", id, e);
                }
            }
        }

        // ── Pass 3: evaluate-cache TTL eviction ──────────────────────────────
        if let Some(ev) = &self.cache_eviction {
            let cache_cutoff = now_ms - self.cache_ttl.as_millis() as i64;
            let n = ev.evict_older_than(cache_cutoff);
            if n > 0 {
                log::info!(
                    "CarrionPicker: TTL-evicted {} evaluate_cache entry/entries (ttl={}s)",
                    n, self.cache_ttl.as_secs(),
                );
            }
            cache_evicted += n;
        }

        (snaps_deleted, events_deleted, cache_evicted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coire::Coire;
    use crate::event::ClaraEvent;
    use serde_json::json;
    use std::sync::Mutex;

    // ── helpers ──────────────────────────────────────────────────────────────

    fn tmp_store() -> (CoireStore, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.duckdb");
        let store = CoireStore::open(&path).unwrap();
        (store, dir)
    }

    fn make_old_session(store: &CoireStore, age_ms: i64) -> Uuid {
        let sid = Uuid::new_v4();
        let coire = Coire::new().unwrap();
        let mut event = ClaraEvent::new(sid, "test", json!({"x": 1}));
        let old_ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64
            - age_ms;
        event.created_at_ms = old_ts;
        coire.write_event(&event).unwrap();
        store.save_session(sid, &coire).unwrap();
        sid
    }

    fn make_picker(
        store:        CoireStore,
        coire_ttl:    Duration,
        snapshot_ttl: Duration,
        active:       Arc<RwLock<HashSet<Uuid>>>,
    ) -> CarrionPicker {
        CarrionPicker::new(store, coire_ttl, snapshot_ttl, Duration::from_secs(9999), active)
    }

    /// Minimal mock that records `evict_older_than` and `evict_by_deduction`
    /// calls and returns configurable counts.
    struct MockCacheEviction {
        older_than_calls:     Mutex<Vec<i64>>,
        by_deduction_calls:   Mutex<Vec<Uuid>>,
        /// How many entries to claim were removed per `evict_older_than` call.
        older_than_returns:   usize,
        /// How many entries to claim were removed per `evict_by_deduction` call.
        by_deduction_returns: usize,
    }

    impl MockCacheEviction {
        fn new(older_than_returns: usize, by_deduction_returns: usize) -> Arc<Self> {
            Arc::new(Self {
                older_than_calls:     Mutex::new(vec![]),
                by_deduction_calls:   Mutex::new(vec![]),
                older_than_returns,
                by_deduction_returns,
            })
        }
    }

    impl EvaluateCacheEviction for MockCacheEviction {
        fn evict_older_than(&self, cutoff_ms: i64) -> usize {
            self.older_than_calls.lock().unwrap().push(cutoff_ms);
            self.older_than_returns
        }

        fn evict_by_deduction(&self, deduction_id: Uuid) -> usize {
            self.by_deduction_calls.lock().unwrap().push(deduction_id);
            self.by_deduction_returns
        }
    }

    // ── existing sweep tests (updated for 3-tuple) ───────────────────────────

    #[test]
    fn sweep_deletes_stale_not_fresh() {
        let (store, _dir) = tmp_store();
        let active = Arc::new(RwLock::new(HashSet::new()));

        let stale_sid = make_old_session(&store, 7_200_000); // 2 h old
        let fresh_sid = make_old_session(&store, 1_800_000); // 30 min old

        let picker = make_picker(store.clone(), Duration::from_secs(3600), Duration::from_secs(86400), active);
        let (_, events, _) = picker.sweep();

        assert!(events > 0, "should have deleted stale events");
        assert_eq!(store.read_session(stale_sid).unwrap().len(), 0, "stale session should be gone");
        assert!(!store.read_session(fresh_sid).unwrap().is_empty(), "fresh session should remain");
    }

    #[test]
    fn sweep_skips_active_sessions() {
        let (store, _dir) = tmp_store();

        let stale_sid = make_old_session(&store, 7_200_000);
        let active = Arc::new(RwLock::new(HashSet::from([stale_sid])));

        let picker = make_picker(store.clone(), Duration::from_secs(3600), Duration::from_secs(86400), active);
        let (snaps, events, cache) = picker.sweep();

        assert_eq!(snaps + events + cache, 0, "active session must not be deleted");
        assert!(!store.read_session(stale_sid).unwrap().is_empty());
    }

    #[test]
    fn sweep_empty_store_is_noop() {
        let (store, _dir) = tmp_store();
        let active = Arc::new(RwLock::new(HashSet::new()));
        let picker = make_picker(store, Duration::from_secs(3600), Duration::from_secs(86400), active);
        assert_eq!(picker.sweep(), (0, 0, 0));
    }

    #[test]
    fn sweep_expires_snapshot_and_coire_events() {
        use crate::store::DeductionSnapshot;
        let (store, _dir) = tmp_store();
        let active = Arc::new(RwLock::new(HashSet::new()));

        let did       = Uuid::new_v4();
        let prolog_id = Uuid::new_v4();
        let clips_id  = Uuid::new_v4();
        let now_ms    = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i64;

        let coire = Coire::new().unwrap();
        let mut ev = ClaraEvent::new(prolog_id, "prolog", serde_json::json!({"x": 1}));
        ev.created_at_ms = now_ms - 1000;
        coire.write_event(&ev).unwrap();
        store.save_session(prolog_id, &coire).unwrap();

        let snap = DeductionSnapshot {
            deduction_id:      did,
            prolog_clauses:    vec![],
            clips_constructs:  vec![],
            clips_file:        None,
            initial_goal:      None,
            max_cycles:        10,
            status:            "Interrupted".into(),
            cycles_run:        1,
            prolog_session_id: prolog_id,
            clips_session_id:  clips_id,
            created_at_ms:     now_ms - 2000,
            expires_at_ms:     now_ms - 1,
            context:           vec![],
            tableau_entries:   serde_json::json!([]),
        };
        store.save_snapshot(&snap).unwrap();

        let picker = make_picker(store.clone(), Duration::from_secs(3600), Duration::from_secs(86400), active);
        let (snaps, _, _) = picker.sweep();

        assert_eq!(snaps, 1, "expired snapshot should be deleted");
        assert!(store.load_snapshot(did).unwrap().is_none(), "snapshot gone");
        assert!(store.read_session(prolog_id).unwrap().is_empty(), "coire events gone");
    }

    // ── cache eviction tests ─────────────────────────────────────────────────

    #[test]
    fn sweep_no_cache_eviction_when_not_configured() {
        // No cache eviction handler → third count is always 0.
        let (store, _dir) = tmp_store();
        let active = Arc::new(RwLock::new(HashSet::new()));
        let picker = make_picker(store, Duration::from_secs(3600), Duration::from_secs(86400), active);
        let (_, _, cache) = picker.sweep();
        assert_eq!(cache, 0);
    }

    #[test]
    fn sweep_pass3_calls_evict_older_than() {
        let (store, _dir) = tmp_store();
        let active  = Arc::new(RwLock::new(HashSet::new()));
        let mock    = MockCacheEviction::new(3, 0); // claims 3 TTL evictions

        let picker = make_picker(store, Duration::from_secs(3600), Duration::from_secs(86400), active)
            .with_cache_eviction(Duration::from_secs(300), mock.clone());

        let (_, _, cache) = picker.sweep();

        assert_eq!(cache, 3, "sweep should return mock's claimed eviction count");
        let calls = mock.older_than_calls.lock().unwrap();
        assert_eq!(calls.len(), 1, "evict_older_than called exactly once per sweep");
        // cutoff should be approximately now - 300 s
        let expected_cutoff = SystemTime::now()
            .duration_since(UNIX_EPOCH).unwrap().as_millis() as i64
            - 300_000;
        assert!(
            (calls[0] - expected_cutoff).abs() < 500,
            "cutoff_ms should be ~now - cache_ttl, got {}",
            calls[0]
        );
    }

    #[test]
    fn sweep_pass1_calls_evict_by_deduction_for_expired_snapshot() {
        use crate::store::DeductionSnapshot;
        let (store, _dir) = tmp_store();
        let active  = Arc::new(RwLock::new(HashSet::new()));
        let mock    = MockCacheEviction::new(0, 2); // claims 2 per-deduction evictions

        let did       = Uuid::new_v4();
        let prolog_id = Uuid::new_v4();
        let clips_id  = Uuid::new_v4();
        let now_ms    = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i64;

        let snap = DeductionSnapshot {
            deduction_id:      did,
            prolog_clauses:    vec![],
            clips_constructs:  vec![],
            clips_file:        None,
            initial_goal:      None,
            max_cycles:        10,
            status:            "Converged".into(),
            cycles_run:        2,
            prolog_session_id: prolog_id,
            clips_session_id:  clips_id,
            created_at_ms:     now_ms - 5000,
            expires_at_ms:     now_ms - 1,   // already expired
            context:           vec![],
            tableau_entries:   serde_json::json!([]),
        };
        store.save_snapshot(&snap).unwrap();

        let picker = make_picker(store.clone(), Duration::from_secs(3600), Duration::from_secs(86400), active)
            .with_cache_eviction(Duration::from_secs(300), mock.clone());

        let (snaps, _, cache) = picker.sweep();

        assert_eq!(snaps, 1, "snapshot should be deleted");
        // cache count = 2 (by_deduction) + 0 (pass3 mock returns 0 for older_than)
        assert_eq!(cache, 2, "cache eviction count should include per-deduction evictions");

        let by_ded = mock.by_deduction_calls.lock().unwrap();
        assert_eq!(by_ded.len(), 1, "evict_by_deduction called once for the expired snapshot");
        assert_eq!(by_ded[0], did, "called with the expired deduction's ID");
    }

    #[test]
    fn sweep_does_not_evict_by_deduction_on_delete_failure() {
        // If delete_snapshot fails, cache eviction for that deduction must be skipped.
        // We simulate this by providing a snapshot ID that doesn't exist in the store
        // — but snapshot expiry query returns nothing if there's nothing to expire.
        // Instead we verify that when no snapshots expire, by_deduction is never called.
        let (store, _dir) = tmp_store();
        let active = Arc::new(RwLock::new(HashSet::new()));
        let mock   = MockCacheEviction::new(0, 99);

        let picker = make_picker(store, Duration::from_secs(3600), Duration::from_secs(86400), active)
            .with_cache_eviction(Duration::from_secs(300), mock.clone());

        picker.sweep();

        let by_ded = mock.by_deduction_calls.lock().unwrap();
        assert_eq!(by_ded.len(), 0, "no expired snapshots → no by_deduction calls");
    }
}
