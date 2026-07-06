use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

/// Well-known label constants for `TephraEnvelope::label`.
pub mod label {
    pub const OFFERING: &str = "offering";
    pub const HOHI: &str = "hohi";
    /// Error response from a peer evaluator.  A Tephra carrying a Tabu is
    /// always an error response; the peer responded but evaluation failed.
    pub const TABU: &str = "tabu";
    pub const PROLOG_FACT: &str = "prolog_fact";
    pub const CLIPS_FIRE: &str = "clips_fire";
    pub const CLARA_FY_HIT: &str = "clara_fy_hit";
    pub const DEDUCTION_EVENT: &str = "deduction_event";
}

/// Typed view of `TephraEnvelope::label`.
///
/// The wire format keeps `label` as a plain string for compatibility;
/// consumers that want exhaustive matching use [`TephraEnvelope::kind`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageKind {
    Offering,
    Hohi,
    Tabu,
    PrologFact,
    ClipsFire,
    ClaraFyHit,
    DeductionEvent,
}

impl MessageKind {
    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            label::OFFERING => Some(Self::Offering),
            label::HOHI => Some(Self::Hohi),
            label::TABU => Some(Self::Tabu),
            label::PROLOG_FACT => Some(Self::PrologFact),
            label::CLIPS_FIRE => Some(Self::ClipsFire),
            label::CLARA_FY_HIT => Some(Self::ClaraFyHit),
            label::DEDUCTION_EVENT => Some(Self::DeductionEvent),
            _ => None,
        }
    }

    pub fn as_label(&self) -> &'static str {
        match self {
            Self::Offering => label::OFFERING,
            Self::Hohi => label::HOHI,
            Self::Tabu => label::TABU,
            Self::PrologFact => label::PROLOG_FACT,
            Self::ClipsFire => label::CLIPS_FIRE,
            Self::ClaraFyHit => label::CLARA_FY_HIT,
            Self::DeductionEvent => label::DEDUCTION_EVENT,
        }
    }
}

/// Routing metadata for an addressed/correlated envelope.
///
/// All fields are optional; an envelope with no routing behaves exactly like
/// a pre-routing broadcast message. `source_node_id`/`target_node_id` are
/// *design-time* graph node ids (the Cobbler node `id`), deliberately
/// distinct from `producer_node` (deployment identity used for echo
/// suppression).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Routing {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_node_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_node_id: Option<String>,
    /// Stamped on an Offering; echoed unchanged on the Hohi/Tabu reply.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<Uuid>,
    /// Logical hierarchical channel, e.g.
    /// `dis.local/ritual/{performance}/psych-evals/{edge-id}`.
    /// Purely a routing/filter field — the physical Kafka topic stays
    /// one-per-Ritual.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topic_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

impl Routing {
    pub fn is_empty(&self) -> bool {
        self.source_node_id.is_none()
            && self.target_node_id.is_none()
            && self.correlation_id.is_none()
            && self.topic_path.is_none()
            && self.tags.is_none()
    }
}

/// The payload carried inside a `TephraEnvelope`.
///
/// `Plaintext` is used for all Phase 1–6 messages.
/// `Encrypted` is a stub reserved for Phase 7 (XChaCha20-Poly1305).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum TephraPayload {
    Plaintext { body: Value },
    Encrypted {
        cipher: String,
        nonce: String,
        ciphertext: String,
        aad: Value,
    },
}

/// The envelope wrapping every Ritual message on the Kafka topic.
///
/// Consumers must call `is_expired()` and discard stale envelopes before
/// processing. Filtering by `performance_id` and `label` is also the
/// consumer's responsibility (one topic per Ritual, not per Performance).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TephraEnvelope {
    pub tephra_id:      Uuid,
    pub ritual_id:      Uuid,
    pub performance_id: Uuid,
    pub label:          String,
    /// Unix timestamp (ms) at which this envelope was created.
    pub ts_ms:          i64,
    /// Time-to-live in milliseconds. Consumer drops if `now - ts_ms > ttl_ms`.
    pub ttl_ms:         u64,
    pub producer_node:  String,
    pub payload:        TephraPayload,
    /// Design-time graph node id of the producing node (Cobbler node `id`).
    /// Distinct from `producer_node`, which is deployment identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_node_id: Option<String>,
    /// Design-time graph node id this message is addressed to. Consumers
    /// whose node id differs must skip the message; `None` = broadcast
    /// (pre-routing behavior).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_node_id: Option<String>,
    /// Stamped on an Offering; echoed unchanged on the Hohi/Tabu reply so
    /// the originator can match responses to outstanding requests.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<Uuid>,
    /// Logical hierarchical channel path (the physical Kafka topic stays
    /// one-per-Ritual).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topic_path:     Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags:           Option<Vec<String>>,
}

impl TephraEnvelope {
    pub fn new(
        ritual_id:      Uuid,
        performance_id: Uuid,
        label:          impl Into<String>,
        ttl_ms:         u64,
        producer_node:  impl Into<String>,
        payload:        TephraPayload,
    ) -> Self {
        let ts_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        Self {
            tephra_id: Uuid::new_v4(),
            ritual_id,
            performance_id,
            label: label.into(),
            ts_ms,
            ttl_ms,
            producer_node: producer_node.into(),
            payload,
            source_node_id: None,
            target_node_id: None,
            correlation_id: None,
            topic_path: None,
            tags: None,
        }
    }

    /// Attach routing metadata (addressing, correlation, logical topic path).
    pub fn with_routing(mut self, routing: Routing) -> Self {
        self.source_node_id = routing.source_node_id;
        self.target_node_id = routing.target_node_id;
        self.correlation_id = routing.correlation_id;
        self.topic_path = routing.topic_path;
        self.tags = routing.tags;
        self
    }

    /// Typed view of `label`; `None` for unknown labels.
    pub fn kind(&self) -> Option<MessageKind> {
        MessageKind::from_label(&self.label)
    }

    /// Returns `true` if `now_ms - ts_ms > ttl_ms`.
    ///
    /// A future timestamp (`ts_ms > now`) is treated as not expired.
    pub fn is_expired(&self) -> bool {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let elapsed_ms = now_ms.saturating_sub(self.ts_ms);
        elapsed_ms > 0 && elapsed_ms as u64 > self.ttl_ms
    }
}

/// Configuration supplied when creating a new Ritual via `RitualRegistry::create`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RitualConfig {
    /// Human-readable name for this Ritual.
    pub name: String,
    /// Addresses or identifiers of FieryPit peer evaluators participating in this Ritual.
    pub participants: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fresh(ttl_ms: u64) -> TephraEnvelope {
        TephraEnvelope::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            label::OFFERING,
            ttl_ms,
            "dis.test",
            TephraPayload::Plaintext { body: json!(null) },
        )
    }

    #[test]
    fn not_expired_with_large_ttl() {
        assert!(!fresh(3_600_000).is_expired());
    }

    #[test]
    fn expired_when_ts_far_in_past() {
        let env = TephraEnvelope {
            tephra_id:      Uuid::new_v4(),
            ritual_id:      Uuid::new_v4(),
            performance_id: Uuid::new_v4(),
            label:          label::HOHI.to_string(),
            ts_ms:          1_000_000, // epoch + 1000 s — definitely in the past
            ttl_ms:         1_000,     // 1 s TTL
            producer_node:  "dis.test".to_string(),
            payload:        TephraPayload::Plaintext { body: json!(null) },
            ..fresh(1_000)
        };
        assert!(env.is_expired());
    }

    #[test]
    fn future_timestamp_not_expired() {
        let env = TephraEnvelope {
            tephra_id:      Uuid::new_v4(),
            ritual_id:      Uuid::new_v4(),
            performance_id: Uuid::new_v4(),
            label:          label::OFFERING.to_string(),
            ts_ms:          i64::MAX, // far future
            ttl_ms:         1_000,
            producer_node:  "dis.test".to_string(),
            payload:        TephraPayload::Plaintext { body: json!(null) },
            ..fresh(1_000)
        };
        assert!(!env.is_expired());
    }

    #[test]
    fn round_trip_serialization() {
        let env = fresh(60_000);
        let json = serde_json::to_string(&env).unwrap();
        let back: TephraEnvelope = serde_json::from_str(&json).unwrap();
        assert_eq!(back.tephra_id, env.tephra_id);
        assert_eq!(back.label, env.label);
        assert_eq!(back.ttl_ms, env.ttl_ms);
    }

    #[test]
    fn legacy_json_without_routing_fields_deserializes() {
        // A pre-routing envelope as produced by older peers — no
        // source_node_id/target_node_id/correlation_id/topic_path/tags keys.
        let json = format!(
            r#"{{"tephra_id":"{}","ritual_id":"{}","performance_id":"{}",
                "label":"offering","ts_ms":1,"ttl_ms":60000,
                "producer_node":"dis.local",
                "payload":{{"type":"plaintext","body":{{"goal":"g"}}}}}}"#,
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
        );
        let env: TephraEnvelope = serde_json::from_str(&json).unwrap();
        assert!(env.target_node_id.is_none());
        assert!(env.correlation_id.is_none());
        assert_eq!(env.kind(), Some(MessageKind::Offering));
    }

    #[test]
    fn routing_fields_omitted_from_json_when_none() {
        let json = serde_json::to_string(&fresh(60_000)).unwrap();
        assert!(!json.contains("target_node_id"));
        assert!(!json.contains("correlation_id"));
        assert!(!json.contains("topic_path"));
        assert!(!json.contains("tags"));
    }

    #[test]
    fn with_routing_round_trips() {
        let cid = Uuid::new_v4();
        let env = fresh(60_000).with_routing(Routing {
            source_node_id: Some("n1".into()),
            target_node_id: Some("n2".into()),
            correlation_id: Some(cid),
            topic_path:     Some("dis.local/ritual/p1/psych-evals/e1".into()),
            tags:           Some(vec!["urgent".into()]),
        });
        let back: TephraEnvelope =
            serde_json::from_str(&serde_json::to_string(&env).unwrap()).unwrap();
        assert_eq!(back.source_node_id.as_deref(), Some("n1"));
        assert_eq!(back.target_node_id.as_deref(), Some("n2"));
        assert_eq!(back.correlation_id, Some(cid));
        assert_eq!(
            back.topic_path.as_deref(),
            Some("dis.local/ritual/p1/psych-evals/e1")
        );
        assert_eq!(back.tags, Some(vec!["urgent".to_string()]));
    }

    #[test]
    fn kind_maps_all_labels_and_rejects_unknown() {
        for (lbl, kind) in [
            (label::OFFERING, MessageKind::Offering),
            (label::HOHI, MessageKind::Hohi),
            (label::TABU, MessageKind::Tabu),
            (label::PROLOG_FACT, MessageKind::PrologFact),
            (label::CLIPS_FIRE, MessageKind::ClipsFire),
            (label::CLARA_FY_HIT, MessageKind::ClaraFyHit),
            (label::DEDUCTION_EVENT, MessageKind::DeductionEvent),
        ] {
            assert_eq!(MessageKind::from_label(lbl), Some(kind));
            assert_eq!(kind.as_label(), lbl);
        }
        assert_eq!(MessageKind::from_label("mystery"), None);
    }

    #[test]
    fn ritual_config_round_trip() {
        let cfg = RitualConfig {
            name: "test-ritual".to_string(),
            participants: vec!["http://localhost:6666".to_string()],
        };
        let back: RitualConfig = serde_json::from_str(&serde_json::to_string(&cfg).unwrap()).unwrap();
        assert_eq!(back.name, cfg.name);
        assert_eq!(back.participants, cfg.participants);
    }
}
