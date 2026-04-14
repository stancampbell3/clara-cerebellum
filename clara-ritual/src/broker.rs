use crate::envelope::TephraEnvelope;
use crate::error::RitualError;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Abstraction over the Kafka message broker.
///
/// The production implementation (`RsKafkaClient`, added in Phase 5) wraps
/// `rskafka`. The test implementation (`InMemoryBroker`) is a simple
/// in-memory store that allows integration tests to run without a broker.
pub trait KafkaBridge: Send + Sync {
    /// Serialize and publish `envelope` to `topic`.
    fn publish(&self, topic: &str, envelope: &TephraEnvelope) -> Result<(), RitualError>;

    /// Fetch envelopes from `topic` starting at `since_offset`.
    ///
    /// Returns `(envelopes, next_offset)`. Pass `next_offset` back as
    /// `since_offset` on the next call to avoid re-delivering messages.
    fn poll(
        &self,
        topic: &str,
        since_offset: i64,
    ) -> Result<(Vec<TephraEnvelope>, i64), RitualError>;
}

/// In-memory broker for integration tests.
///
/// Topics are append-only `Vec`s; offsets are indices into those vecs.
/// Cheap to clone — all clones share the same underlying state via `Arc`.
#[derive(Clone, Default)]
pub struct InMemoryBroker {
    topics: Arc<Mutex<HashMap<String, Vec<TephraEnvelope>>>>,
}

impl InMemoryBroker {
    pub fn new() -> Self {
        Self::default()
    }
}

impl KafkaBridge for InMemoryBroker {
    fn publish(&self, topic: &str, envelope: &TephraEnvelope) -> Result<(), RitualError> {
        self.topics
            .lock()
            .unwrap()
            .entry(topic.to_string())
            .or_default()
            .push(envelope.clone());
        Ok(())
    }

    fn poll(
        &self,
        topic: &str,
        since_offset: i64,
    ) -> Result<(Vec<TephraEnvelope>, i64), RitualError> {
        let guard = self.topics.lock().unwrap();
        let entries = guard.get(topic).map(Vec::as_slice).unwrap_or(&[]);
        let start = (since_offset.max(0) as usize).min(entries.len());
        let slice = &entries[start..];
        let next_offset = (start + slice.len()) as i64;
        Ok((slice.to_vec(), next_offset))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::envelope::{label, TephraPayload};
    use serde_json::json;
    use uuid::Uuid;

    fn make_envelope(ritual_id: Uuid, perf_id: Uuid, lbl: &str) -> TephraEnvelope {
        TephraEnvelope::new(
            ritual_id,
            perf_id,
            lbl,
            60_000,
            "dis.test",
            TephraPayload::Plaintext { body: json!({"test": true}) },
        )
    }

    #[test]
    fn publish_and_poll_round_trip() {
        let broker = InMemoryBroker::new();
        let ritual_id = Uuid::new_v4();
        let perf_id   = Uuid::new_v4();
        let topic     = "dis.local.ritual.test";

        let env = make_envelope(ritual_id, perf_id, label::OFFERING);
        broker.publish(topic, &env).unwrap();

        let (envelopes, next_offset) = broker.poll(topic, 0).unwrap();
        assert_eq!(envelopes.len(), 1);
        assert_eq!(envelopes[0].tephra_id, env.tephra_id);
        assert_eq!(next_offset, 1);
    }

    #[test]
    fn poll_advances_offset_correctly() {
        let broker = InMemoryBroker::new();
        let ritual_id = Uuid::new_v4();
        let perf_id   = Uuid::new_v4();
        let topic     = "dis.local.ritual.offset-test";

        for _ in 0..3 {
            broker.publish(topic, &make_envelope(ritual_id, perf_id, label::HOHI)).unwrap();
        }

        // First poll: all three
        let (batch1, offset) = broker.poll(topic, 0).unwrap();
        assert_eq!(batch1.len(), 3);
        assert_eq!(offset, 3);

        // Poll from saved offset — nothing new
        let (batch2, offset2) = broker.poll(topic, offset).unwrap();
        assert!(batch2.is_empty());
        assert_eq!(offset2, 3);

        // Publish one more, poll from saved offset
        broker.publish(topic, &make_envelope(ritual_id, perf_id, label::OFFERING)).unwrap();
        let (batch3, offset3) = broker.poll(topic, offset).unwrap();
        assert_eq!(batch3.len(), 1);
        assert_eq!(offset3, 4);
    }

    #[test]
    fn poll_nonexistent_topic_returns_empty() {
        let broker = InMemoryBroker::new();
        let (envelopes, offset) = broker.poll("no.such.topic", 0).unwrap();
        assert!(envelopes.is_empty());
        assert_eq!(offset, 0);
    }

    #[test]
    fn negative_offset_treated_as_zero() {
        let broker = InMemoryBroker::new();
        let ritual_id = Uuid::new_v4();
        let perf_id   = Uuid::new_v4();
        let topic     = "dis.local.ritual.neg-offset";

        broker.publish(topic, &make_envelope(ritual_id, perf_id, label::OFFERING)).unwrap();

        let (envelopes, _) = broker.poll(topic, -99).unwrap();
        assert_eq!(envelopes.len(), 1);
    }

    #[test]
    fn clones_share_state() {
        let broker  = InMemoryBroker::new();
        let broker2 = broker.clone();
        let ritual_id = Uuid::new_v4();
        let perf_id   = Uuid::new_v4();
        let topic     = "dis.local.ritual.shared";

        broker.publish(topic, &make_envelope(ritual_id, perf_id, label::OFFERING)).unwrap();

        let (envelopes, _) = broker2.poll(topic, 0).unwrap();
        assert_eq!(envelopes.len(), 1);
    }

    #[test]
    fn multiple_topics_isolated() {
        let broker = InMemoryBroker::new();
        let ritual_id = Uuid::new_v4();
        let perf_id   = Uuid::new_v4();

        broker.publish("topic.a", &make_envelope(ritual_id, perf_id, label::OFFERING)).unwrap();
        broker.publish("topic.a", &make_envelope(ritual_id, perf_id, label::OFFERING)).unwrap();
        broker.publish("topic.b", &make_envelope(ritual_id, perf_id, label::HOHI)).unwrap();

        let (a, _) = broker.poll("topic.a", 0).unwrap();
        let (b, _) = broker.poll("topic.b", 0).unwrap();
        assert_eq!(a.len(), 2);
        assert_eq!(b.len(), 1);
    }
}
