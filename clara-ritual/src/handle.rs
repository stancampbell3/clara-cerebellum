use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

use clara_coire::ClaraEvent;
use uuid::Uuid;

use crate::broker::KafkaBridge;
use crate::envelope::{TephraEnvelope, TephraPayload};
use crate::error::RitualError;

/// A lightweight handle to an active Ritual Performance.
///
/// Created by `RitualRegistry::join`. Cheap to clone — all clones share
/// the same consumer offset and broker connection.
pub struct RitualHandle {
    pub ritual_id:      Uuid,
    pub performance_id: Uuid,
    pub dis_domain:     String,
    broker:             Arc<dyn KafkaBridge>,
    topic:              String,
    /// Offset of the next unread message on the Ritual topic.
    /// Shared across clones so offset advances are visible everywhere.
    consumer_offset:    Arc<AtomicI64>,
}

impl RitualHandle {
    pub(crate) fn new(
        ritual_id:      Uuid,
        performance_id: Uuid,
        dis_domain:     String,
        broker:         Arc<dyn KafkaBridge>,
        topic:          String,
        initial_offset: i64,
    ) -> Self {
        Self {
            ritual_id,
            performance_id,
            dis_domain,
            broker,
            topic,
            consumer_offset: Arc::new(AtomicI64::new(initial_offset)),
        }
    }

    /// The Kafka topic this handle publishes to and consumes from.
    pub fn topic(&self) -> &str {
        &self.topic
    }

    /// Wrap `event` in a `TephraEnvelope` and publish it to the Ritual topic.
    ///
    /// `ttl_ms` defaults to 60 000 ms (1 minute) if `None`.
    pub fn publish_event(
        &self,
        event:  &ClaraEvent,
        label:  &str,
        ttl_ms: Option<u64>,
    ) -> Result<(), RitualError> {
        let body    = serde_json::to_value(event)?;
        let payload = TephraPayload::Plaintext { body };
        let envelope = TephraEnvelope::new(
            self.ritual_id,
            self.performance_id,
            label,
            ttl_ms.unwrap_or(60_000),
            &self.dis_domain,
            payload,
        );
        self.broker.publish(&self.topic, &envelope)
    }

    /// Poll for incoming envelopes since the last consumed offset.
    ///
    /// Expired envelopes are silently dropped. The offset is advanced past
    /// all fetched messages (including expired ones) so they are never
    /// re-delivered on the next call.
    pub fn poll_incoming(&self) -> Result<Vec<TephraEnvelope>, RitualError> {
        let offset = self.consumer_offset.load(Ordering::SeqCst);
        let (envelopes, next_offset) = self.broker.poll(&self.topic, offset)?;
        self.consumer_offset.store(next_offset, Ordering::SeqCst);
        Ok(envelopes.into_iter().filter(|e| !e.is_expired()).collect())
    }
}

impl Clone for RitualHandle {
    fn clone(&self) -> Self {
        Self {
            ritual_id:       self.ritual_id,
            performance_id:  self.performance_id,
            dis_domain:      self.dis_domain.clone(),
            broker:          self.broker.clone(),
            topic:           self.topic.clone(),
            consumer_offset: self.consumer_offset.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::broker::InMemoryBroker;
    use crate::envelope::{label, RitualConfig};
    use crate::registry::RitualRegistry;
    use serde_json::json;

    fn make_registry() -> RitualRegistry {
        RitualRegistry::new("dis.test", Arc::new(InMemoryBroker::new()))
    }

    fn make_ritual(registry: &RitualRegistry) -> Uuid {
        registry
            .create(RitualConfig {
                name:         "handle-test".into(),
                participants: vec![],
            })
            .unwrap()
    }

    fn make_event() -> ClaraEvent {
        ClaraEvent::new(Uuid::new_v4(), "evaluator/offering", json!({"goal": "test"}))
    }

    // ── Round-trip ────────────────────────────────────────────────────────────

    #[test]
    fn publish_and_poll_round_trip() {
        let registry   = make_registry();
        let ritual_id  = make_ritual(&registry);
        let handle_a   = registry.join(ritual_id, None).unwrap();
        let handle_b   = registry.join(ritual_id, None).unwrap();

        handle_a.publish_event(&make_event(), label::OFFERING, Some(60_000)).unwrap();

        let tephras = handle_b.poll_incoming().unwrap();
        assert_eq!(tephras.len(), 1);
        assert_eq!(tephras[0].label, label::OFFERING);
        assert_eq!(tephras[0].ritual_id, ritual_id);
    }

    #[test]
    fn performance_id_stamped_on_envelope() {
        let registry   = make_registry();
        let ritual_id  = make_ritual(&registry);
        let handle_a   = registry.join(ritual_id, None).unwrap();
        let handle_b   = registry.join(ritual_id, None).unwrap();

        handle_a.publish_event(&make_event(), label::OFFERING, None).unwrap();

        let tephras = handle_b.poll_incoming().unwrap();
        assert_eq!(tephras[0].performance_id, handle_a.performance_id);
    }

    // ── TTL filtering ─────────────────────────────────────────────────────────

    #[test]
    fn expired_envelopes_dropped_by_poll_incoming() {
        let broker     = Arc::new(InMemoryBroker::new());
        let registry   = RitualRegistry::new("dis.test", broker.clone());
        let ritual_id  = make_ritual(&registry);
        let handle     = registry.join(ritual_id, None).unwrap();

        let topic = crate::topic::topic_name("dis.test", ritual_id).unwrap();

        // Publish one stale envelope directly to bypass TephraEnvelope::new()
        let stale = TephraEnvelope {
            tephra_id:      Uuid::new_v4(),
            ritual_id,
            performance_id: handle.performance_id,
            label:          label::OFFERING.into(),
            ts_ms:          1_000_000, // epoch + 1000 s — definitely expired
            ttl_ms:         1_000,
            producer_node:  "dis.test".into(),
            payload:        TephraPayload::Plaintext { body: json!({"stale": true}) },
        };
        broker.publish(&topic, &stale).unwrap();

        // Publish one live envelope
        handle.publish_event(&make_event(), label::HOHI, Some(60_000)).unwrap();

        let tephras = handle.poll_incoming().unwrap();
        assert_eq!(tephras.len(), 1, "expired envelope should be dropped");
        assert_eq!(tephras[0].label, label::HOHI);
    }

    #[test]
    fn all_expired_returns_empty() {
        let broker    = Arc::new(InMemoryBroker::new());
        let registry  = RitualRegistry::new("dis.test", broker.clone());
        let ritual_id = make_ritual(&registry);
        let handle    = registry.join(ritual_id, None).unwrap();
        let topic     = crate::topic::topic_name("dis.test", ritual_id).unwrap();

        for _ in 0..3 {
            let stale = TephraEnvelope {
                tephra_id:      Uuid::new_v4(),
                ritual_id,
                performance_id: handle.performance_id,
                label:          label::OFFERING.into(),
                ts_ms:          1_000_000,
                ttl_ms:         1_000,
                producer_node:  "dis.test".into(),
                payload:        TephraPayload::Plaintext { body: json!(null) },
            };
            broker.publish(&topic, &stale).unwrap();
        }

        assert!(handle.poll_incoming().unwrap().is_empty());
        // Offset should still advance (don't re-deliver expired)
        assert!(handle.poll_incoming().unwrap().is_empty());
    }

    // ── Offset advancement ────────────────────────────────────────────────────

    #[test]
    fn offset_advances_so_messages_not_redelivered() {
        let registry  = make_registry();
        let ritual_id = make_ritual(&registry);
        let sender    = registry.join(ritual_id, None).unwrap();
        let receiver  = registry.join(ritual_id, None).unwrap();

        sender.publish_event(&make_event(), label::OFFERING, None).unwrap();
        let first = receiver.poll_incoming().unwrap();
        assert_eq!(first.len(), 1);

        // Second poll should find nothing
        let second = receiver.poll_incoming().unwrap();
        assert!(second.is_empty());

        // Publish again — receiver picks up only the new one
        sender.publish_event(&make_event(), label::HOHI, None).unwrap();
        let third = receiver.poll_incoming().unwrap();
        assert_eq!(third.len(), 1);
        assert_eq!(third[0].label, label::HOHI);
    }

    // ── Clone shares offset ───────────────────────────────────────────────────

    #[test]
    fn cloned_handle_shares_consumer_offset() {
        let registry  = make_registry();
        let ritual_id = make_ritual(&registry);
        let sender    = registry.join(ritual_id, None).unwrap();
        let receiver  = registry.join(ritual_id, None).unwrap();
        let receiver2 = receiver.clone();

        sender.publish_event(&make_event(), label::OFFERING, None).unwrap();

        // receiver reads it
        let got = receiver.poll_incoming().unwrap();
        assert_eq!(got.len(), 1);

        // receiver2 (clone) shares the offset — nothing left
        let got2 = receiver2.poll_incoming().unwrap();
        assert!(got2.is_empty());
    }
}
