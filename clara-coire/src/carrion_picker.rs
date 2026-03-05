use std::collections::HashSet;
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use uuid::Uuid;

use crate::store::CoireStore;

/// Background task that periodically deletes stale sessions from a
/// [`CoireStore`].
///
/// A session is eligible for deletion when the timestamp of its newest event
/// (`MAX(created_at_ms)`) is older than the configured TTL. Sessions whose
/// UUIDs appear in `active` are always skipped, protecting any deduction that
/// is still running.
///
/// Spawn with [`CarrionPicker::spawn`]. The returned [`tokio::task::JoinHandle`]
/// can be aborted on server shutdown.
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
    ) -> Self {
        Self { store, ttl, interval, active }
    }

    /// Spawn the picker as a tokio background task.
    ///
    /// The task sleeps for `interval`, runs a sweep, and repeats indefinitely.
    /// Abort the returned handle on server shutdown to stop the loop cleanly.
    pub fn spawn(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            log::info!(
                "CarrionPicker: started (ttl={}s, interval={}s)",
                self.ttl.as_secs(),
                self.interval.as_secs()
            );
            loop {
                tokio::time::sleep(self.interval).await;
                let deleted = self.sweep();
                if deleted > 0 {
                    log::info!(
                        "CarrionPicker: deleted {} stale CoireStore event(s)",
                        deleted
                    );
                } else {
                    log::debug!("CarrionPicker: sweep complete, nothing to delete");
                }
            }
        })
    }

    /// Run one sweep: find stale sessions, delete them, return total event
    /// count deleted across all removed sessions.
    fn sweep(&self) -> usize {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let cutoff_ms = now_ms - self.ttl.as_millis() as i64;

        let exclude = self.active.read().unwrap().clone();

        let stale = match self.store.sessions_older_than(cutoff_ms, &exclude) {
            Ok(ids) => ids,
            Err(e) => {
                log::warn!("CarrionPicker: sweep query failed: {}", e);
                return 0;
            }
        };

        let mut total_deleted = 0;
        for id in stale {
            match self.store.delete_session(id) {
                Ok(n) => {
                    total_deleted += n;
                    log::debug!(
                        "CarrionPicker: deleted session {} ({} event(s))",
                        id,
                        n
                    );
                }
                Err(e) => {
                    log::warn!("CarrionPicker: failed to delete session {}: {}", id, e);
                }
            }
        }
        total_deleted
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

    #[test]
    fn sweep_deletes_stale_not_fresh() {
        let (store, _dir) = tmp_store();
        let active = Arc::new(RwLock::new(HashSet::new()));

        let ttl = Duration::from_secs(3600);
        // Stale: 2 hours old
        let stale_sid = make_old_session(&store, 7_200_000);
        // Fresh: 30 minutes old
        let fresh_sid = make_old_session(&store, 1_800_000);

        let picker = CarrionPicker::new(store.clone(), ttl, Duration::from_secs(9999), active);
        let deleted = picker.sweep();

        assert!(deleted > 0, "should have deleted stale events");
        assert_eq!(store.read_session(stale_sid).unwrap().len(), 0, "stale session should be gone");
        assert!(!store.read_session(fresh_sid).unwrap().is_empty(), "fresh session should remain");
    }

    #[test]
    fn sweep_skips_active_sessions() {
        let (store, _dir) = tmp_store();

        // Stale but active
        let stale_sid = make_old_session(&store, 7_200_000);
        let active = Arc::new(RwLock::new(HashSet::from([stale_sid])));

        let picker = CarrionPicker::new(
            store.clone(),
            Duration::from_secs(3600),
            Duration::from_secs(9999),
            active,
        );
        let deleted = picker.sweep();

        assert_eq!(deleted, 0, "active session must not be deleted");
        assert!(!store.read_session(stale_sid).unwrap().is_empty());
    }

    #[test]
    fn sweep_empty_store_is_noop() {
        let (store, _dir) = tmp_store();
        let active = Arc::new(RwLock::new(HashSet::new()));
        let picker = CarrionPicker::new(store, Duration::from_secs(3600), Duration::from_secs(9999), active);
        assert_eq!(picker.sweep(), 0);
    }
}
