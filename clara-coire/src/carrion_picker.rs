use std::collections::HashSet;
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use uuid::Uuid;

use crate::store::CoireStore;

/// Background task that periodically deletes stale data from a [`CoireStore`].
///
/// Each sweep performs two passes in order:
///
/// 1. **Snapshot expiry** — deletes [`DeductionSnapshot`] rows (and their
///    associated Coire events) whose `expires_at_ms` is in the past.
/// 2. **Orphan Coire sweep** — deletes `coire_events` rows whose session has
///    no snapshot and whose newest event is older than the Coire TTL.
///
/// Sessions whose UUIDs appear in `active` are always skipped in both passes,
/// protecting deductions that are currently running.
///
/// Spawn with [`CarrionPicker::spawn`]. The returned [`tokio::task::JoinHandle`]
/// can be aborted on server shutdown.
pub struct CarrionPicker {
    store:        CoireStore,
    /// TTL for orphaned Coire event sessions (no snapshot).
    coire_ttl:    Duration,
    /// TTL for [`DeductionSnapshot`] rows (and their Coire events).
    snapshot_ttl: Duration,
    interval:     Duration,
    active:       Arc<RwLock<HashSet<Uuid>>>,
}

impl CarrionPicker {
    pub fn new(
        store:        CoireStore,
        coire_ttl:    Duration,
        snapshot_ttl: Duration,
        interval:     Duration,
        active:       Arc<RwLock<HashSet<Uuid>>>,
    ) -> Self {
        Self { store, coire_ttl, snapshot_ttl, interval, active }
    }

    /// Spawn the picker as a tokio background task.
    ///
    /// The task sleeps for `interval`, runs a sweep, and repeats indefinitely.
    /// Abort the returned handle on server shutdown to stop the loop cleanly.
    pub fn spawn(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            log::info!(
                "CarrionPicker: started (coire_ttl={}s, snapshot_ttl={}s, interval={}s)",
                self.coire_ttl.as_secs(),
                self.snapshot_ttl.as_secs(),
                self.interval.as_secs()
            );
            loop {
                tokio::time::sleep(self.interval).await;
                let (snaps, events) = self.sweep();
                if snaps > 0 || events > 0 {
                    log::info!(
                        "CarrionPicker: deleted {} snapshot(s), {} orphan event(s)",
                        snaps,
                        events
                    );
                } else {
                    log::debug!("CarrionPicker: sweep complete, nothing to delete");
                }
            }
        })
    }

    /// Run one sweep. Returns `(snapshots_deleted, orphan_events_deleted)`.
    ///
    /// Pass 1: expire snapshots (and their Coire events) whose `expires_at_ms`
    /// is in the past.
    /// Pass 2: delete orphaned Coire sessions (no snapshot) older than
    /// `coire_ttl`. Session IDs freed by pass 1 are excluded from the orphan
    /// query automatically since they have already been deleted.
    fn sweep(&self) -> (usize, usize) {
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

        let mut snaps_deleted = 0;
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
                return (snaps_deleted, 0);
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

        (snaps_deleted, events_deleted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coire::Coire;
    use crate::event::ClaraEvent;
    use serde_json::json;

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
        // Backdated timestamp
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

    fn make_picker(store: CoireStore, coire_ttl: Duration, snapshot_ttl: Duration, active: Arc<RwLock<HashSet<Uuid>>>) -> CarrionPicker {
        CarrionPicker::new(store, coire_ttl, snapshot_ttl, Duration::from_secs(9999), active)
    }

    #[test]
    fn sweep_deletes_stale_not_fresh() {
        let (store, _dir) = tmp_store();
        let active = Arc::new(RwLock::new(HashSet::new()));

        // Stale: 2 hours old
        let stale_sid = make_old_session(&store, 7_200_000);
        // Fresh: 30 minutes old
        let fresh_sid = make_old_session(&store, 1_800_000);

        let picker = make_picker(store.clone(), Duration::from_secs(3600), Duration::from_secs(86400), active);
        let (_, events) = picker.sweep();

        assert!(events > 0, "should have deleted stale events");
        assert_eq!(store.read_session(stale_sid).unwrap().len(), 0, "stale session should be gone");
        assert!(!store.read_session(fresh_sid).unwrap().is_empty(), "fresh session should remain");
    }

    #[test]
    fn sweep_skips_active_sessions() {
        let (store, _dir) = tmp_store();

        // Stale but active
        let stale_sid = make_old_session(&store, 7_200_000);
        let active = Arc::new(RwLock::new(HashSet::from([stale_sid])));

        let picker = make_picker(store.clone(), Duration::from_secs(3600), Duration::from_secs(86400), active);
        let (snaps, events) = picker.sweep();

        assert_eq!(snaps + events, 0, "active session must not be deleted");
        assert!(!store.read_session(stale_sid).unwrap().is_empty());
    }

    #[test]
    fn sweep_empty_store_is_noop() {
        let (store, _dir) = tmp_store();
        let active = Arc::new(RwLock::new(HashSet::new()));
        let picker = make_picker(store, Duration::from_secs(3600), Duration::from_secs(86400), active);
        assert_eq!(picker.sweep(), (0, 0));
    }

    #[test]
    fn sweep_expires_snapshot_and_coire_events() {
        use crate::store::DeductionSnapshot;
        let (store, _dir) = tmp_store();
        let active = Arc::new(RwLock::new(HashSet::new()));

        // Build a snapshot that is already expired
        let did        = Uuid::new_v4();
        let prolog_id  = Uuid::new_v4();
        let clips_id   = Uuid::new_v4();
        let now_ms = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i64;

        // Save some pending Coire events
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
            expires_at_ms:     now_ms - 1,   // already expired
            context:           vec![],
        };
        store.save_snapshot(&snap).unwrap();

        let picker = make_picker(store.clone(), Duration::from_secs(3600), Duration::from_secs(86400), active);
        let (snaps, _) = picker.sweep();

        assert_eq!(snaps, 1, "expired snapshot should be deleted");
        assert!(store.load_snapshot(did).unwrap().is_none(), "snapshot gone");
        assert!(store.read_session(prolog_id).unwrap().is_empty(), "coire events gone");
    }
}
