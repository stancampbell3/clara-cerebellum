use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

/// Well-known label constants for `TephraEnvelope::label`.
pub mod label {
    pub const OFFERING: &str = "offering";
    pub const HOHI: &str = "hohi";
    pub const PROLOG_FACT: &str = "prolog_fact";
    pub const CLIPS_FIRE: &str = "clips_fire";
    pub const CLARA_FY_HIT: &str = "clara_fy_hit";
    pub const DEDUCTION_EVENT: &str = "deduction_event";
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
        }
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
