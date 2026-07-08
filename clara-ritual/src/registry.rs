use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use clara_coire::{CoireStore, RitualRow};
use serde::Serialize;
use uuid::Uuid;

use crate::broker::KafkaBridge;
use crate::envelope::RitualConfig;
use crate::error::RitualError;
use crate::handle::RitualHandle;
use crate::ritual::{Ritual, RitualState};
use crate::topic::topic_name;

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// Snapshot of one active Ritual returned by `list_active`.
#[derive(Debug, Clone, Serialize)]
pub struct RitualSummary {
    pub ritual_id: Uuid,
    pub name:      String,
    pub state:     String,
    pub topic:     String,
}

/// Server-level registry of all active and terminated Rituals.
///
/// One `RitualRegistry` lives in `AppState`. It owns the broker connection
/// (shared across all Rituals) and hands out `RitualHandle`s to callers.
///
/// With a [`CoireStore`] attached (see [`with_store`](Self::with_store)),
/// every mutation is written through to the store's `rituals` table and
/// [`restore_from_store`](Self::restore_from_store) reloads them on boot,
/// so Rituals survive server restarts. Persistence failures are logged and
/// never fail the in-memory operation — worst case is today's
/// memory-only behavior.
pub struct RitualRegistry {
    dis_domain: String,
    broker:     Arc<dyn KafkaBridge>,
    rituals:    Arc<RwLock<HashMap<Uuid, Ritual>>>,
    store:      Option<CoireStore>,
}

impl RitualRegistry {
    pub fn new(dis_domain: impl Into<String>, broker: Arc<dyn KafkaBridge>) -> Self {
        Self {
            dis_domain: dis_domain.into(),
            broker,
            rituals: Arc::new(RwLock::new(HashMap::new())),
            store: None,
        }
    }

    /// Attach a persistent store; mutations write through to its `rituals`
    /// table and [`restore_from_store`](Self::restore_from_store) reloads
    /// them on boot.
    pub fn with_store(mut self, store: CoireStore) -> Self {
        self.store = Some(store);
        self
    }

    /// Create a new Ritual and return its ID.
    ///
    /// The Kafka topic is computed from `dis_domain` and the new `ritual_id`
    /// and validated against Kafka naming constraints.
    pub fn create(&self, config: RitualConfig) -> Result<Uuid, RitualError> {
        let ritual_id = Uuid::new_v4();
        let topic     = topic_name(&self.dis_domain, ritual_id)?;
        // Ensure the Kafka topic exists before any publish can happen.
        // No-op for InMemoryBroker; calls ControllerClient::create_topic for RsKafkaClient.
        self.broker.ensure_topic(&topic, 1, 1)?;
        self.rituals.write().unwrap().insert(
            ritual_id,
            Ritual {
                ritual_id,
                config: config.clone(),
                state: RitualState::Active,
                topic: topic.clone(),
                participants: std::collections::HashMap::new(),
            },
        );
        if let Some(ref store) = self.store {
            let ts = now_ms();
            let row = RitualRow {
                ritual_id,
                name:              config.name.clone(),
                config_json:       serde_json::to_string(&config).unwrap_or_default(),
                state:             RitualState::Active.as_str().to_string(),
                topic,
                participants_json: "{}".to_string(),
                created_at_ms:     ts,
                updated_at_ms:     ts,
            };
            if let Err(e) = store.upsert_ritual(&row) {
                log::error!("RitualRegistry: failed to persist ritual {}: {}", ritual_id, e);
            }
        }
        log::info!("RitualRegistry: created ritual {}", ritual_id);
        Ok(ritual_id)
    }

    /// Join an existing active Ritual and return a `RitualHandle` for the caller's Performance.
    ///
    /// When `participant_key` is `Some`, the join is **idempotent**: the same key always
    /// receives the same `performance_id`, allowing callers to re-join without orphaning
    /// prior handles. When `participant_key` is `None`, a fresh `performance_id` is generated
    /// on every call (used by `CycleController` where each deduction run is a distinct
    /// performance).
    pub fn join(
        &self,
        ritual_id:       Uuid,
        participant_key: Option<&str>,
    ) -> Result<RitualHandle, RitualError> {
        let mut guard  = self.rituals.write().unwrap();
        let ritual = guard.get_mut(&ritual_id).ok_or_else(|| {
            RitualError::TopicNotFound(ritual_id.to_string())
        })?;
        if ritual.state != RitualState::Active {
            return Err(RitualError::BrokerError(format!(
                "ritual {} is not active (state: {:?})",
                ritual_id, ritual.state
            )));
        }

        // Idempotent: reuse the existing performance_id for this participant key.
        let performance_id = if let Some(key) = participant_key {
            match ritual.participants.get(key) {
                Some(existing) => *existing,
                None => {
                    let fresh = Uuid::new_v4();
                    ritual.participants.insert(key.to_string(), fresh);
                    // Persist the grown map so the same key resumes the same
                    // performance_id after a restart.
                    if let Some(ref store) = self.store {
                        let json = serde_json::to_string(&ritual.participants)
                            .unwrap_or_else(|_| "{}".to_string());
                        if let Err(e) = store.set_ritual_participants(ritual_id, &json) {
                            log::error!(
                                "RitualRegistry: failed to persist participants for {}: {}",
                                ritual_id, e
                            );
                        }
                    }
                    fresh
                }
            }
        } else {
            Uuid::new_v4()
        };

        // Seed the consumer offset at the current latest so new handles do not
        // replay messages published before they joined.
        let initial_offset = self.broker.latest_offset(&ritual.topic).unwrap_or(0);

        let handle = RitualHandle::new(
            ritual_id,
            performance_id,
            self.dis_domain.clone(),
            self.broker.clone(),
            ritual.topic.clone(),
            initial_offset,
        );
        log::info!(
            "RitualRegistry: joined ritual {} (performance {}, participant={:?})",
            ritual_id, performance_id, participant_key
        );
        Ok(handle)
    }

    /// Mark a Ritual as terminated. Existing handles continue to work until
    /// the broker topic is deleted (Phase 5 admin API).
    pub fn terminate(&self, ritual_id: Uuid) -> Result<(), RitualError> {
        let mut guard  = self.rituals.write().unwrap();
        let ritual = guard.get_mut(&ritual_id).ok_or_else(|| {
            RitualError::TopicNotFound(ritual_id.to_string())
        })?;
        ritual.state = RitualState::Terminated;
        if let Some(ref store) = self.store {
            if let Err(e) = store.set_ritual_state(ritual_id, RitualState::Terminated.as_str()) {
                log::error!(
                    "RitualRegistry: failed to persist terminated state for {}: {}",
                    ritual_id, e
                );
            }
        }
        log::info!("RitualRegistry: terminated ritual {}", ritual_id);
        Ok(())
    }

    pub fn get_status(&self, ritual_id: Uuid) -> Option<RitualState> {
        self.rituals.read().unwrap().get(&ritual_id).map(|r| r.state.clone())
    }

    /// Return a snapshot of all currently active Rituals.
    ///
    /// Terminated Rituals are excluded; callers that need post-mortem
    /// analysis should consult logs or Coire session events instead.
    pub fn list_active(&self) -> Vec<RitualSummary> {
        self.rituals.read().unwrap()
            .values()
            .filter(|r| r.state == RitualState::Active)
            .map(|r| RitualSummary {
                ritual_id: r.ritual_id,
                name:      r.config.name.clone(),
                state:     "active".to_string(),
                topic:     r.topic.clone(),
            })
            .collect()
    }

    /// Ensure the Kafka topic for `ritual_id` exists, creating it if necessary.
    ///
    /// Delegates to the underlying `KafkaBridge::ensure_topic` implementation:
    /// - `InMemoryBroker`: no-op.
    /// - `RsKafkaClient`: calls `ControllerClient::create_topic()`.
    ///
    /// Note: `RitualRegistry::create()` already calls this internally, so
    /// callers typically do not need to invoke it directly.
    pub fn ensure_topic(
        &self,
        ritual_id:   Uuid,
        partitions:  i16,
        replication: i16,
    ) -> Result<(), RitualError> {
        let topic = {
            let guard = self.rituals.read().unwrap();
            let ritual = guard.get(&ritual_id).ok_or_else(|| {
                RitualError::TopicNotFound(ritual_id.to_string())
            })?;
            ritual.topic.clone()
        };
        self.broker.ensure_topic(&topic, partitions as i32, replication)
    }

    /// Reload persisted Rituals from the attached store into the in-memory map.
    ///
    /// Rows that fail to parse are skipped with an error log. For each
    /// restored **active** Ritual the Kafka topic is re-ensured (idempotent;
    /// heals a wiped broker volume) — an ensure failure is logged but the
    /// Ritual is still restored, matching the pre-restart in-memory state.
    /// Returns the number of Rituals restored; 0 when no store is attached.
    ///
    /// Must be called from a non-async thread: `ensure_topic` blocks on the
    /// broker's internal runtime.
    pub fn restore_from_store(&self) -> usize {
        let Some(ref store) = self.store else { return 0 };
        let rows = match store.load_rituals() {
            Ok(rows) => rows,
            Err(e) => {
                log::error!("RitualRegistry: failed to load persisted rituals: {}", e);
                return 0;
            }
        };
        let mut restored = 0;
        for row in rows {
            let Some(state) = RitualState::from_str(&row.state) else {
                log::error!(
                    "RitualRegistry: skipping ritual {} with unknown state '{}'",
                    row.ritual_id, row.state
                );
                continue;
            };
            let config: RitualConfig = match serde_json::from_str(&row.config_json) {
                Ok(c) => c,
                Err(e) => {
                    log::error!(
                        "RitualRegistry: skipping ritual {} with unparseable config: {}",
                        row.ritual_id, e
                    );
                    continue;
                }
            };
            let participants: HashMap<String, Uuid> =
                serde_json::from_str(&row.participants_json).unwrap_or_else(|e| {
                    log::error!(
                        "RitualRegistry: ritual {} participants map unparseable ({}); \
                         keyed joins will issue fresh performance ids",
                        row.ritual_id, e
                    );
                    HashMap::new()
                });
            if state == RitualState::Active {
                if let Err(e) = self.broker.ensure_topic(&row.topic, 1, 1) {
                    log::error!(
                        "RitualRegistry: ensure_topic failed for restored ritual {} ({}): {}",
                        row.ritual_id, row.topic, e
                    );
                }
            }
            self.rituals.write().unwrap().insert(
                row.ritual_id,
                Ritual {
                    ritual_id: row.ritual_id,
                    config,
                    state,
                    topic: row.topic,
                    participants,
                },
            );
            restored += 1;
        }
        if restored > 0 {
            log::info!("RitualRegistry: restored {} ritual(s) from store", restored);
        }
        restored
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::broker::InMemoryBroker;

    fn make_registry() -> RitualRegistry {
        RitualRegistry::new("dis.test", Arc::new(InMemoryBroker::new()))
    }

    fn make_config(name: &str) -> RitualConfig {
        RitualConfig { name: name.into(), participants: vec![] }
    }

    // ── create ────────────────────────────────────────────────────────────────

    #[test]
    fn create_returns_unique_ids() {
        let registry = make_registry();
        let a = registry.create(make_config("a")).unwrap();
        let b = registry.create(make_config("b")).unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn created_ritual_is_active() {
        let registry  = make_registry();
        let ritual_id = registry.create(make_config("r")).unwrap();
        assert_eq!(registry.get_status(ritual_id), Some(RitualState::Active));
    }

    // ── join ──────────────────────────────────────────────────────────────────

    #[test]
    fn join_active_ritual_succeeds() {
        let registry  = make_registry();
        let ritual_id = registry.create(make_config("r")).unwrap();
        assert!(registry.join(ritual_id, None).is_ok());
    }

    #[test]
    fn each_anonymous_join_produces_unique_performance_id() {
        let registry  = make_registry();
        let ritual_id = registry.create(make_config("r")).unwrap();
        let h1 = registry.join(ritual_id, None).unwrap();
        let h2 = registry.join(ritual_id, None).unwrap();
        assert_ne!(h1.performance_id, h2.performance_id);
    }

    #[test]
    fn keyed_join_is_idempotent() {
        let registry  = make_registry();
        let ritual_id = registry.create(make_config("r")).unwrap();
        let h1 = registry.join(ritual_id, Some("http://fiery-pit-1:8080")).unwrap();
        let h2 = registry.join(ritual_id, Some("http://fiery-pit-1:8080")).unwrap();
        assert_eq!(h1.performance_id, h2.performance_id, "same key must yield same performance_id");
    }

    #[test]
    fn different_keys_get_different_performance_ids() {
        let registry  = make_registry();
        let ritual_id = registry.create(make_config("r")).unwrap();
        let h1 = registry.join(ritual_id, Some("http://fiery-pit-1:8080")).unwrap();
        let h2 = registry.join(ritual_id, Some("http://fiery-pit-2:8080")).unwrap();
        assert_ne!(h1.performance_id, h2.performance_id);
    }

    #[test]
    fn join_nonexistent_ritual_errors() {
        let registry = make_registry();
        assert!(registry.join(Uuid::new_v4(), None).is_err());
    }

    #[test]
    fn join_terminated_ritual_errors() {
        let registry  = make_registry();
        let ritual_id = registry.create(make_config("r")).unwrap();
        registry.terminate(ritual_id).unwrap();
        assert!(registry.join(ritual_id, None).is_err());
    }

    // ── terminate ─────────────────────────────────────────────────────────────

    #[test]
    fn terminate_sets_state_to_terminated() {
        let registry  = make_registry();
        let ritual_id = registry.create(make_config("r")).unwrap();
        registry.terminate(ritual_id).unwrap();
        assert_eq!(registry.get_status(ritual_id), Some(RitualState::Terminated));
    }

    #[test]
    fn terminate_nonexistent_ritual_errors() {
        let registry = make_registry();
        assert!(registry.terminate(Uuid::new_v4()).is_err());
    }

    // ── get_status ────────────────────────────────────────────────────────────

    #[test]
    fn get_status_unknown_ritual_returns_none() {
        let registry = make_registry();
        assert_eq!(registry.get_status(Uuid::new_v4()), None);
    }

    // ── ensure_topic ──────────────────────────────────────────────────────────

    #[test]
    fn ensure_topic_on_existing_ritual_is_ok() {
        let registry  = make_registry();
        let ritual_id = registry.create(make_config("r")).unwrap();
        assert!(registry.ensure_topic(ritual_id, 1, 1).is_ok());
    }

    #[test]
    fn ensure_topic_on_nonexistent_ritual_errors() {
        let registry = make_registry();
        assert!(registry.ensure_topic(Uuid::new_v4(), 1, 1).is_err());
    }

    // ── list_active ───────────────────────────────────────────────────────────

    #[test]
    fn list_active_empty_registry() {
        let registry = make_registry();
        assert!(registry.list_active().is_empty());
    }

    #[test]
    fn list_active_includes_active_excludes_terminated() {
        let registry = make_registry();
        let id_a = registry.create(make_config("alpha")).unwrap();
        let id_b = registry.create(make_config("beta")).unwrap();
        registry.terminate(id_b).unwrap();

        let active = registry.list_active();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].ritual_id, id_a);
        assert_eq!(active[0].name, "alpha");
        assert_eq!(active[0].state, "active");
        assert!(active[0].topic.contains(&id_a.to_string()));
    }

    #[test]
    fn list_active_all_terminated_returns_empty() {
        let registry = make_registry();
        let id = registry.create(make_config("r")).unwrap();
        registry.terminate(id).unwrap();
        assert!(registry.list_active().is_empty());
    }

    // ── persistence / restore ─────────────────────────────────────────────────

    fn tmp_store() -> (CoireStore, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let store = CoireStore::open(dir.path().join("test.duckdb")).unwrap();
        (store, dir)
    }

    fn make_persistent_registry(store: &CoireStore) -> RitualRegistry {
        RitualRegistry::new("dis.test", Arc::new(InMemoryBroker::new()))
            .with_store(store.clone())
    }

    #[test]
    fn restore_without_store_returns_zero() {
        let registry = make_registry();
        assert_eq!(registry.restore_from_store(), 0);
    }

    #[test]
    fn created_ritual_survives_restart() {
        let (store, _dir) = tmp_store();
        let ritual_id = make_persistent_registry(&store)
            .create(make_config("phoenix"))
            .unwrap();

        // "Restart": a fresh registry over the same store.
        let reborn = make_persistent_registry(&store);
        assert_eq!(reborn.get_status(ritual_id), None, "empty before restore");
        assert_eq!(reborn.restore_from_store(), 1);

        assert_eq!(reborn.get_status(ritual_id), Some(RitualState::Active));
        assert!(reborn.join(ritual_id, None).is_ok());
        let active = reborn.list_active();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].name, "phoenix");
        assert!(active[0].topic.contains(&ritual_id.to_string()));
    }

    #[test]
    fn keyed_join_performance_id_survives_restart() {
        let (store, _dir) = tmp_store();
        let registry  = make_persistent_registry(&store);
        let ritual_id = registry.create(make_config("r")).unwrap();
        let before = registry.join(ritual_id, Some("http://fp1:6666")).unwrap();

        let reborn = make_persistent_registry(&store);
        reborn.restore_from_store();
        let after = reborn.join(ritual_id, Some("http://fp1:6666")).unwrap();
        assert_eq!(
            before.performance_id, after.performance_id,
            "keyed join must resume its pre-restart performance_id"
        );
    }

    #[test]
    fn terminated_state_survives_restart() {
        let (store, _dir) = tmp_store();
        let registry  = make_persistent_registry(&store);
        let ritual_id = registry.create(make_config("r")).unwrap();
        registry.terminate(ritual_id).unwrap();

        let reborn = make_persistent_registry(&store);
        assert_eq!(reborn.restore_from_store(), 1);
        assert_eq!(reborn.get_status(ritual_id), Some(RitualState::Terminated));
        assert!(reborn.join(ritual_id, None).is_err());
        assert!(reborn.list_active().is_empty());
    }

    #[test]
    fn restore_skips_corrupt_rows_and_keeps_good_ones() {
        let (store, _dir) = tmp_store();
        let registry  = make_persistent_registry(&store);
        let good_id   = registry.create(make_config("good")).unwrap();
        store
            .upsert_ritual(&clara_coire::RitualRow {
                ritual_id:         Uuid::new_v4(),
                name:              "corrupt".into(),
                config_json:       "not json".into(),
                state:             "active".into(),
                topic:             "dis.test.broken".into(),
                participants_json: "{}".into(),
                created_at_ms:     0,
                updated_at_ms:     0,
            })
            .unwrap();

        let reborn = make_persistent_registry(&store);
        assert_eq!(reborn.restore_from_store(), 1);
        assert_eq!(reborn.get_status(good_id), Some(RitualState::Active));
    }

    #[test]
    fn restore_does_not_duplicate_persisted_rows() {
        // Restoring must not write back: a second restore over the same
        // store sees the same single row.
        let (store, _dir) = tmp_store();
        make_persistent_registry(&store).create(make_config("r")).unwrap();

        let reborn = make_persistent_registry(&store);
        assert_eq!(reborn.restore_from_store(), 1);
        assert_eq!(store.load_rituals().unwrap().len(), 1);
        let again = make_persistent_registry(&store);
        assert_eq!(again.restore_from_store(), 1);
    }
}
