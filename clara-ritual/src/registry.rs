use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use uuid::Uuid;

use crate::broker::KafkaBridge;
use crate::envelope::RitualConfig;
use crate::error::RitualError;
use crate::handle::RitualHandle;
use crate::ritual::{Ritual, RitualState};
use crate::topic::topic_name;

/// Server-level registry of all active and terminated Rituals.
///
/// One `RitualRegistry` lives in `AppState`. It owns the broker connection
/// (shared across all Rituals) and hands out `RitualHandle`s to callers.
pub struct RitualRegistry {
    dis_domain: String,
    broker:     Arc<dyn KafkaBridge>,
    rituals:    Arc<RwLock<HashMap<Uuid, Ritual>>>,
}

impl RitualRegistry {
    pub fn new(dis_domain: impl Into<String>, broker: Arc<dyn KafkaBridge>) -> Self {
        Self {
            dis_domain: dis_domain.into(),
            broker,
            rituals: Arc::new(RwLock::new(HashMap::new())),
        }
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
                config,
                state: RitualState::Active,
                topic,
                participants: std::collections::HashMap::new(),
            },
        );
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
            *ritual.participants
                .entry(key.to_string())
                .or_insert_with(Uuid::new_v4)
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
        log::info!("RitualRegistry: terminated ritual {}", ritual_id);
        Ok(())
    }

    pub fn get_status(&self, ritual_id: Uuid) -> Option<RitualState> {
        self.rituals.read().unwrap().get(&ritual_id).map(|r| r.state.clone())
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
}
