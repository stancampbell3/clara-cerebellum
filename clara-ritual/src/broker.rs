use crate::envelope::TephraEnvelope;
use crate::error::RitualError;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Abstraction over the Kafka message broker.
///
/// Two implementations are provided:
/// - `InMemoryBroker`: in-process, no external dependency, for tests and
///   single-server development.
/// - `RsKafkaClient` (requires `rskafka` feature): wraps `rskafka 0.6` for
///   production use against a real Kafka cluster.
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

    /// Ensure the given topic exists in the broker, creating it if necessary.
    ///
    /// On `InMemoryBroker` this is a no-op.
    /// On `RsKafkaClient` this calls `ControllerClient::create_topic()`.
    /// Calling this when the topic already exists is not an error.
    fn ensure_topic(
        &self,
        topic:              &str,
        num_partitions:     i32,
        replication_factor: i16,
    ) -> Result<(), RitualError>;

    /// Return the offset of the next message that will be published to `topic`.
    ///
    /// For a topic with N messages the latest offset is N (one past the last
    /// message). A new `RitualHandle` should start polling from this offset so
    /// it does not replay messages published before it joined.
    ///
    /// On `InMemoryBroker` this returns the current length of the topic's Vec.
    /// On `RsKafkaClient` this calls `PartitionClient::get_offset(OffsetAt::Latest)`.
    /// Returns 0 if the topic does not exist yet.
    fn latest_offset(&self, topic: &str) -> Result<i64, RitualError>;
}

// ---------------------------------------------------------------------------
// InMemoryBroker
// ---------------------------------------------------------------------------

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

    fn ensure_topic(&self, _topic: &str, _num_partitions: i32, _replication_factor: i16)
        -> Result<(), RitualError>
    {
        Ok(()) // InMemoryBroker creates topics implicitly on first publish.
    }

    fn latest_offset(&self, topic: &str) -> Result<i64, RitualError> {
        let guard = self.topics.lock().unwrap();
        Ok(guard.get(topic).map(|v| v.len() as i64).unwrap_or(0))
    }
}

// ---------------------------------------------------------------------------
// RsKafkaClient (rskafka feature)
// ---------------------------------------------------------------------------

/// Production Kafka broker backend backed by `rskafka 0.6`.
///
/// Maintains a dedicated single-threaded tokio runtime so that `publish` and
/// `poll` can be called synchronously from a `spawn_blocking` context (the same
/// pattern as `FieryPitClient`). Caches `PartitionClient`s per topic to avoid
/// redundant broker round-trips.
///
/// # Construction
///
/// ```no_run
/// # #[cfg(feature = "rskafka")]
/// # {
/// use clara_ritual::RsKafkaClient;
/// let client = RsKafkaClient::new(vec!["localhost:9092".to_string()]).unwrap();
/// # }
/// ```
#[cfg(feature = "rskafka")]
pub struct RsKafkaClient {
    runtime:    tokio::runtime::Runtime,
    client:     Arc<rskafka::client::Client>,
    /// Cache of `PartitionClient`s keyed by topic name (partition 0 always).
    partitions: Mutex<HashMap<String, Arc<rskafka::client::partition::PartitionClient>>>,
}

#[cfg(feature = "rskafka")]
impl RsKafkaClient {
    /// Connect to the Kafka cluster and return a new `RsKafkaClient`.
    ///
    /// `bootstrap_brokers` is a list of `"host:port"` strings.  At least one
    /// must be reachable — the client uses it to discover the full cluster.
    pub fn new(bootstrap_brokers: Vec<String>) -> Result<Self, RitualError> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| RitualError::BrokerError(
                format!("failed to build tokio runtime for RsKafkaClient: {e}")
            ))?;

        let client = runtime.block_on(async {
            rskafka::client::ClientBuilder::new(bootstrap_brokers)
                .client_id("clara-dis")
                .build()
                .await
        }).map_err(|e| RitualError::BrokerError(
            format!("failed to connect to Kafka: {e}")
        ))?;

        Ok(Self {
            runtime,
            client: Arc::new(client),
            partitions: Mutex::new(HashMap::new()),
        })
    }

    /// Get or lazily create a cached `PartitionClient` for `topic` (partition 0).
    fn partition(
        &self,
        topic: &str,
    ) -> Result<Arc<rskafka::client::partition::PartitionClient>, RitualError> {
        let mut cache = self.partitions.lock().unwrap();
        if let Some(pc) = cache.get(topic) {
            return Ok(pc.clone());
        }
        let client     = self.client.clone();
        let topic_str  = topic.to_string();
        let pc = self.runtime.block_on(async move {
            client
                .partition_client(
                    topic_str,
                    0,
                    rskafka::client::partition::UnknownTopicHandling::Retry,
                )
                .await
        }).map_err(|e| RitualError::BrokerError(
            format!("partition_client failed for '{topic}': {e}")
        ))?;
        let pc = Arc::new(pc);
        cache.insert(topic.to_string(), pc.clone());
        Ok(pc)
    }
}

#[cfg(feature = "rskafka")]
impl KafkaBridge for RsKafkaClient {
    fn publish(&self, topic: &str, envelope: &TephraEnvelope) -> Result<(), RitualError> {
        let bytes = serde_json::to_vec(envelope)?;
        let record = rskafka::record::Record {
            key:       None,
            value:     Some(bytes),
            headers:   Default::default(),
            timestamp: chrono::Utc::now(),
        };
        let pc = self.partition(topic)?;
        self.runtime.block_on(async move {
            pc.produce(
                vec![record],
                rskafka::client::partition::Compression::NoCompression,
            )
            .await
        }).map_err(|e| RitualError::BrokerError(format!("produce failed on '{topic}': {e}")))?;
        Ok(())
    }

    fn poll(
        &self,
        topic: &str,
        since_offset: i64,
    ) -> Result<(Vec<TephraEnvelope>, i64), RitualError> {
        let offset = since_offset.max(0);
        let pc = self.partition(topic)?;
        let (records, _watermark) = self.runtime.block_on(async move {
            // bytes range: request at least 1 byte, at most 1 MiB.
            // max_wait_ms = 100 — short enough for a tight cycle loop.
            pc.fetch_records(offset, 1..1_048_576, 100).await
        }).map_err(|e| RitualError::BrokerError(format!("fetch_records failed on '{topic}': {e}")))?;

        let mut envelopes = Vec::with_capacity(records.len());
        let mut next_offset = offset;
        for rao in records {
            next_offset = rao.offset + 1;
            if let Some(bytes) = rao.record.value {
                match serde_json::from_slice::<TephraEnvelope>(&bytes) {
                    Ok(env) => envelopes.push(env),
                    Err(e) => log::warn!(
                        "RsKafkaClient: failed to deserialize envelope at offset {}: {}",
                        rao.offset, e
                    ),
                }
            }
        }
        Ok((envelopes, next_offset))
    }

    fn ensure_topic(
        &self,
        topic:              &str,
        num_partitions:     i32,
        replication_factor: i16,
    ) -> Result<(), RitualError> {
        let client     = self.client.clone();
        let topic_str  = topic.to_string();
        self.runtime.block_on(async move {
            let ctrl = client.controller_client()
                .map_err(|e| RitualError::BrokerError(
                    format!("controller_client failed: {e}")
                ))?;
            match ctrl.create_topic(topic_str, num_partitions, replication_factor, 5_000).await {
                Ok(()) => Ok(()),
                Err(rskafka::client::error::Error::ServerError {
                    protocol_error: rskafka::client::error::ProtocolError::TopicAlreadyExists,
                    ..
                }) => {
                    log::debug!("ensure_topic: topic '{topic}' already exists — skipping creation");
                    Ok(())
                }
                Err(e) => Err(RitualError::BrokerError(format!("create_topic failed: {e}"))),
            }
        })
    }

    fn latest_offset(&self, topic: &str) -> Result<i64, RitualError> {
        let pc = self.partition(topic)?;
        self.runtime.block_on(async move {
            pc.get_offset(rskafka::client::partition::OffsetAt::Latest).await
        }).map_err(|e| RitualError::BrokerError(format!("get_offset failed on '{topic}': {e}")))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

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

        let (batch1, offset) = broker.poll(topic, 0).unwrap();
        assert_eq!(batch1.len(), 3);
        assert_eq!(offset, 3);

        let (batch2, offset2) = broker.poll(topic, offset).unwrap();
        assert!(batch2.is_empty());
        assert_eq!(offset2, 3);

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

    #[test]
    fn ensure_topic_noop_on_in_memory_broker() {
        let broker = InMemoryBroker::new();
        assert!(broker.ensure_topic("any.topic", 1, 1).is_ok());
    }

    #[test]
    fn latest_offset_empty_topic_returns_zero() {
        let broker = InMemoryBroker::new();
        assert_eq!(broker.latest_offset("no.such.topic").unwrap(), 0);
    }

    #[test]
    fn latest_offset_reflects_published_count() {
        let broker = InMemoryBroker::new();
        let ritual_id = Uuid::new_v4();
        let perf_id   = Uuid::new_v4();
        let topic     = "dis.local.ritual.latest";

        broker.publish(topic, &make_envelope(ritual_id, perf_id, label::OFFERING)).unwrap();
        broker.publish(topic, &make_envelope(ritual_id, perf_id, label::HOHI)).unwrap();
        assert_eq!(broker.latest_offset(topic).unwrap(), 2);
    }
}
