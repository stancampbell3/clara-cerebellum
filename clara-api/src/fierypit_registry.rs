//! In-memory registry of FieryPits that have registered/heartbeated with
//! this Dis instance.
//!
//! Distinct from `clara_ritual::RitualRegistry` in one deliberate way: no
//! `CoireStore` persistence. A live Ritual needs persistence to survive a
//! server restart (nothing else can recreate it); a FieryPit registration
//! is inherently ephemeral and heartbeat-repopulated — the FieryPit itself
//! re-registers within one heartbeat interval of Dis coming back up, so
//! persisting stale entries across a restart would only risk serving up
//! addresses nobody has heartbeated in a long time. See
//! `docs/fierypit_registration_plan.md` for the full design.
//!
//! One `FieryPitRegistry` lives in `AppState` (`session_handler.rs`),
//! alongside `ritual_registry`.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use uuid::Uuid;

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// One FieryPit's self-reported registration. `fiery_pit_id` is derived
/// client-side (lildaemon: `uuid.uuid5(uuid.NAMESPACE_URL, base_url)`) —
/// Dis treats it as an opaque map key and performs no derivation or
/// verification of its own.
#[derive(Debug, Clone, Serialize)]
pub struct FieryPitRegistration {
    pub fiery_pit_id: Uuid,
    pub base_url: String,
    pub dis_domain: String,
    pub evaluators: Vec<String>,
    /// Wall-clock epoch millis of the last heartbeat — for API consumers.
    pub last_heartbeat_epoch_ms: i64,
    /// Monotonic instant of the last heartbeat — for staleness checks.
    /// Not serialized: epoch millis above is the public-facing timestamp,
    /// this is purely internal bookkeeping immune to system clock changes.
    #[serde(skip)]
    last_heartbeat_instant: Instant,
}

pub struct FieryPitRegistry {
    entries: Arc<RwLock<HashMap<Uuid, FieryPitRegistration>>>,
    ttl: Duration,
}

impl FieryPitRegistry {
    pub fn new(ttl: Duration) -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
            ttl,
        }
    }

    pub fn ttl_seconds(&self) -> u64 {
        self.ttl.as_secs()
    }

    /// Idempotent upsert — registration and heartbeat are the same call.
    /// Re-registering with the same `id` updates in place; it never
    /// errors or duplicates.
    pub fn upsert(
        &self,
        id: Uuid,
        base_url: String,
        dis_domain: String,
        evaluators: Vec<String>,
    ) -> FieryPitRegistration {
        let reg = FieryPitRegistration {
            fiery_pit_id: id,
            base_url,
            dis_domain,
            evaluators,
            last_heartbeat_epoch_ms: now_ms(),
            last_heartbeat_instant: Instant::now(),
        };
        self.entries.write().unwrap().insert(id, reg.clone());
        reg
    }

    /// Non-stale registrations, optionally filtered by evaluator name
    /// and/or dis_domain. Staleness is evaluated lazily here on every
    /// call rather than via a background sweep — simplest correct option
    /// at the cardinality this registry expects (a handful of FieryPits
    /// per Dis domain, not thousands), and avoids a second background
    /// task to manage the lifecycle of.
    pub fn list(&self, evaluator: Option<&str>, dis_domain: Option<&str>) -> Vec<FieryPitRegistration> {
        let now = Instant::now();
        self.entries
            .read()
            .unwrap()
            .values()
            .filter(|r| now.duration_since(r.last_heartbeat_instant) < self.ttl)
            .filter(|r| evaluator.map_or(true, |e| r.evaluators.iter().any(|x| x == e)))
            .filter(|r| dis_domain.map_or(true, |d| r.dis_domain == d))
            .cloned()
            .collect()
    }

    /// Best-effort deregister. Removing an id that isn't present (already
    /// expired, or never existed) is not an error — matches
    /// `coire_topics_handler::delete_topic`'s idempotent-delete idiom.
    pub fn remove(&self, id: Uuid) {
        self.entries.write().unwrap().remove(&id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    fn make_registry(ttl: Duration) -> FieryPitRegistry {
        FieryPitRegistry::new(ttl)
    }

    #[test]
    fn upsert_then_list_roundtrips() {
        let registry = make_registry(Duration::from_secs(90));
        let id = Uuid::new_v4();
        registry.upsert(
            id,
            "http://pineal:6666".to_string(),
            "dis.local".to_string(),
            vec!["clara_mind_splinter".to_string()],
        );
        let listed = registry.list(None, None);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].fiery_pit_id, id);
        assert_eq!(listed[0].base_url, "http://pineal:6666");
    }

    #[test]
    fn upsert_twice_updates_not_duplicates() {
        let registry = make_registry(Duration::from_secs(90));
        let id = Uuid::new_v4();
        registry.upsert(id, "http://a:6666".to_string(), "dis.local".to_string(), vec![]);
        registry.upsert(
            id,
            "http://a:6666".to_string(),
            "dis.local".to_string(),
            vec!["ollama".to_string()],
        );
        let listed = registry.list(None, None);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].evaluators, vec!["ollama".to_string()]);
    }

    #[test]
    fn filters_by_evaluator() {
        let registry = make_registry(Duration::from_secs(90));
        registry.upsert(
            Uuid::new_v4(),
            "http://a:6666".to_string(),
            "dis.local".to_string(),
            vec!["ollama".to_string()],
        );
        registry.upsert(
            Uuid::new_v4(),
            "http://b:6666".to_string(),
            "dis.local".to_string(),
            vec!["clara_mind_splinter".to_string()],
        );
        assert_eq!(registry.list(Some("ollama"), None).len(), 1);
        assert_eq!(registry.list(Some("nonexistent"), None).len(), 0);
        assert_eq!(registry.list(None, None).len(), 2);
    }

    #[test]
    fn filters_by_dis_domain() {
        let registry = make_registry(Duration::from_secs(90));
        registry.upsert(Uuid::new_v4(), "http://a:6666".to_string(), "dis.local".to_string(), vec![]);
        registry.upsert(Uuid::new_v4(), "http://b:6666".to_string(), "dis.other".to_string(), vec![]);
        assert_eq!(registry.list(None, Some("dis.local")).len(), 1);
        assert_eq!(registry.list(None, Some("nonexistent.domain")).len(), 0);
    }

    #[test]
    fn stale_entries_excluded_from_list() {
        let registry = make_registry(Duration::from_millis(20));
        registry.upsert(Uuid::new_v4(), "http://a:6666".to_string(), "dis.local".to_string(), vec![]);
        assert_eq!(registry.list(None, None).len(), 1);
        sleep(Duration::from_millis(50));
        assert_eq!(registry.list(None, None).len(), 0);
    }

    #[test]
    fn remove_then_list_no_longer_shows_it() {
        let registry = make_registry(Duration::from_secs(90));
        let id = Uuid::new_v4();
        registry.upsert(id, "http://a:6666".to_string(), "dis.local".to_string(), vec![]);
        registry.remove(id);
        assert_eq!(registry.list(None, None).len(), 0);
    }

    #[test]
    fn remove_of_nonexistent_is_a_noop() {
        let registry = make_registry(Duration::from_secs(90));
        registry.remove(Uuid::new_v4()); // must not panic
        assert_eq!(registry.list(None, None).len(), 0);
    }
}
