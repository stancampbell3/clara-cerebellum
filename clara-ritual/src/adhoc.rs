//! Ad hoc, non-Ritual topic operations.
//!
//! Ritual topics (`{dis_domain}.ritual.{uuid}`) are created, joined, and
//! torn down through [`crate::RitualRegistry`], which tracks participants
//! and survives restarts. Ad hoc topics (`{dis_domain}.coire.{subject_path}`,
//! see [`crate::coire_topic_name`]) are the opposite: freeform, caller-named
//! channels that any agent can create, publish to, or poll without
//! registering as a participant in anything — a research agent can spin one
//! up, converse on it, and let it evolve, and other agents can discover it
//! later via [`list_topics`] without having coordinated in advance.
//!
//! Every function here takes the broker and Dis domain explicitly so the
//! logic is unit-testable against a plain [`crate::InMemoryBroker`]. The FFI
//! layers (`clara-prolog`'s foreign predicates, this crate's `clips_bridge`)
//! are thin wrappers that supply `clara_ritual::global()` /
//! `clara_ritual::global_dis_domain()`.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use serde::Serialize;
use uuid::Uuid;

use crate::broker::KafkaBridge;
use crate::envelope::{label, Routing, TephraEnvelope, TephraPayload};
use crate::error::RitualError;
use crate::topic::coire_topic_name;

/// Ensure an ad hoc topic exists. Returns the physical Kafka topic name.
pub fn create_topic(
    bridge:             &dyn KafkaBridge,
    dis_domain:         &str,
    subject_path:       &str,
    num_partitions:     i32,
    replication_factor: i16,
) -> Result<String, RitualError> {
    let topic = coire_topic_name(dis_domain, subject_path)?;
    bridge.ensure_topic(&topic, num_partitions, replication_factor)?;
    Ok(topic)
}

/// List the subject paths of every ad hoc topic in `dis_domain` — i.e. every
/// broker topic matching `{dis_domain}.coire.*`, with that prefix stripped
/// back off so callers see the same subject path they created it with.
pub fn list_topics(bridge: &dyn KafkaBridge, dis_domain: &str) -> Result<Vec<String>, RitualError> {
    let prefix = format!("{}.coire.", dis_domain.replace('/', "."));
    let mut subjects: Vec<String> = bridge
        .list_topics()?
        .into_iter()
        .filter_map(|t| t.strip_prefix(prefix.as_str()).map(|s| s.to_string()))
        .collect();
    subjects.sort();
    Ok(subjects)
}

/// Delete an ad hoc topic. Deleting one that doesn't exist is not an error.
pub fn delete_topic(bridge: &dyn KafkaBridge, dis_domain: &str, subject_path: &str) -> Result<(), RitualError> {
    let topic = coire_topic_name(dis_domain, subject_path)?;
    bridge.delete_topic(&topic)
}

/// Publish a JSON body to an ad hoc topic.
///
/// `ritual_id`/`performance_id` are stamped as [`Uuid::nil`] on the wire
/// envelope — ad hoc traffic carries no Ritual identity. `label` defaults to
/// [`label::EVENT`], `ttl_ms` to 60 000 (1 minute). Returns the minted
/// `tephra_id` so the caller can correlate replies.
pub fn publish_topic(
    bridge:       &dyn KafkaBridge,
    dis_domain:   &str,
    subject_path: &str,
    body:         serde_json::Value,
    lbl:          Option<&str>,
    ttl_ms:       Option<u64>,
    routing:      Routing,
) -> Result<Uuid, RitualError> {
    let topic = coire_topic_name(dis_domain, subject_path)?;
    let payload = TephraPayload::Plaintext { body };
    let envelope = TephraEnvelope::new(
        Uuid::nil(),
        Uuid::nil(),
        lbl.unwrap_or(label::EVENT),
        ttl_ms.unwrap_or(60_000),
        dis_domain,
        payload,
    )
    .with_routing(routing);
    let tephra_id = envelope.tephra_id;
    bridge.publish(&topic, &envelope)?;
    Ok(tephra_id)
}

/// Result of an explicit-offset poll: the envelopes fetched plus the offset
/// to pass back in on the next call to avoid re-delivery.
#[derive(Debug, Clone, Serialize)]
pub struct PolledTopic {
    pub envelopes:   Vec<TephraEnvelope>,
    pub next_offset: i64,
}

/// Poll an ad hoc topic from an explicit offset. Expired envelopes are
/// dropped, same as [`crate::RitualHandle::poll_incoming`].
pub fn poll_topic_from(
    bridge:       &dyn KafkaBridge,
    dis_domain:   &str,
    subject_path: &str,
    since_offset: i64,
) -> Result<PolledTopic, RitualError> {
    let topic = coire_topic_name(dis_domain, subject_path)?;
    let (envelopes, next_offset) = bridge.poll(&topic, since_offset)?;
    Ok(PolledTopic {
        envelopes: envelopes.into_iter().filter(|e| !e.is_expired()).collect(),
        next_offset,
    })
}

// ── Ergonomic auto-advancing cursor ─────────────────────────────────────────
//
// Keyed by (consumer_id, topic) so independent consumers in the same process
// (e.g. a Prolog engine and a CLIPS engine both polling the same ad hoc
// topic) each get their own cursor rather than stealing each other's
// messages. `consumer_id` is expected to be the caller's own Coire session
// id (already available on both the Prolog and CLIPS side).

static CURSORS: OnceLock<Mutex<HashMap<(String, String), i64>>> = OnceLock::new();

fn cursors() -> &'static Mutex<HashMap<(String, String), i64>> {
    CURSORS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Poll an ad hoc topic using an auto-advancing cursor tracked per
/// `(consumer_id, subject_path)` pair — repeated calls act like a stream,
/// with no offset bookkeeping required of the caller.
pub fn poll_topic_cursor(
    bridge:       &dyn KafkaBridge,
    dis_domain:   &str,
    consumer_id:  &str,
    subject_path: &str,
) -> Result<Vec<TephraEnvelope>, RitualError> {
    let topic = coire_topic_name(dis_domain, subject_path)?;
    let key = (consumer_id.to_string(), topic.clone());
    let since = *cursors().lock().unwrap().get(&key).unwrap_or(&0);
    let (envelopes, next_offset) = bridge.poll(&topic, since)?;
    cursors().lock().unwrap().insert(key, next_offset);
    Ok(envelopes.into_iter().filter(|e| !e.is_expired()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::broker::InMemoryBroker;
    use serde_json::json;

    #[test]
    fn create_then_list_shows_subject_path_not_full_topic() {
        let broker = InMemoryBroker::new();
        create_topic(&broker, "dis.test", "research.edge-detection", 1, 1).unwrap();
        let topics = list_topics(&broker, "dis.test").unwrap();
        assert_eq!(topics, vec!["research.edge-detection".to_string()]);
    }

    #[test]
    fn list_topics_excludes_ritual_and_other_domain_topics() {
        let broker = InMemoryBroker::new();
        create_topic(&broker, "dis.test", "scratch", 1, 1).unwrap();
        // A Ritual topic and a topic in a different domain must not appear.
        broker.publish(
            "dis.test.ritual.11111111-1111-1111-1111-111111111111",
            &TephraEnvelope::new(Uuid::nil(), Uuid::nil(), label::OFFERING, 1000, "dis.test",
                TephraPayload::Plaintext { body: json!(null) }),
        ).unwrap();
        broker.publish(
            "dis.other.coire.scratch",
            &TephraEnvelope::new(Uuid::nil(), Uuid::nil(), label::EVENT, 1000, "dis.other",
                TephraPayload::Plaintext { body: json!(null) }),
        ).unwrap();

        assert_eq!(list_topics(&broker, "dis.test").unwrap(), vec!["scratch".to_string()]);
    }

    #[test]
    fn delete_topic_removes_it_from_list() {
        let broker = InMemoryBroker::new();
        create_topic(&broker, "dis.test", "scratch", 1, 1).unwrap();
        delete_topic(&broker, "dis.test", "scratch").unwrap();
        assert!(list_topics(&broker, "dis.test").unwrap().is_empty());
    }

    #[test]
    fn delete_nonexistent_ad_hoc_topic_is_ok() {
        let broker = InMemoryBroker::new();
        assert!(delete_topic(&broker, "dis.test", "never-created").is_ok());
    }

    #[test]
    fn publish_then_poll_from_round_trips() {
        let broker = InMemoryBroker::new();
        publish_topic(&broker, "dis.test", "chatter", json!({"hello": "world"}), None, None, Routing::default()).unwrap();

        let polled = poll_topic_from(&broker, "dis.test", "chatter", 0).unwrap();
        assert_eq!(polled.envelopes.len(), 1);
        assert_eq!(polled.next_offset, 1);
        assert_eq!(polled.envelopes[0].ritual_id, Uuid::nil());
        assert_eq!(polled.envelopes[0].performance_id, Uuid::nil());
        assert_eq!(polled.envelopes[0].label, label::EVENT);
    }

    #[test]
    fn publish_respects_custom_label_ttl_and_routing() {
        let broker = InMemoryBroker::new();
        let routing = Routing { tags: Some(vec!["urgent".into()]), ..Default::default() };
        publish_topic(&broker, "dis.test", "chatter", json!(null), Some("prolog_fact"), Some(5_000), routing).unwrap();

        let polled = poll_topic_from(&broker, "dis.test", "chatter", 0).unwrap();
        assert_eq!(polled.envelopes[0].label, "prolog_fact");
        assert_eq!(polled.envelopes[0].ttl_ms, 5_000);
        assert_eq!(polled.envelopes[0].tags, Some(vec!["urgent".to_string()]));
    }

    #[test]
    fn poll_topic_cursor_advances_independently_per_consumer() {
        let broker = InMemoryBroker::new();
        publish_topic(&broker, "dis.test", "shared-cursor-topic", json!(1), None, None, Routing::default()).unwrap();

        let alice_first = poll_topic_cursor(&broker, "dis.test", "alice", "shared-cursor-topic").unwrap();
        assert_eq!(alice_first.len(), 1);
        // Alice's second poll sees nothing new.
        assert!(poll_topic_cursor(&broker, "dis.test", "alice", "shared-cursor-topic").unwrap().is_empty());

        // Bob, a different consumer, still sees the original message — his
        // cursor is independent of Alice's.
        let bob_first = poll_topic_cursor(&broker, "dis.test", "bob", "shared-cursor-topic").unwrap();
        assert_eq!(bob_first.len(), 1);
    }

    #[test]
    fn poll_topic_cursor_advances_across_new_publishes() {
        let broker = InMemoryBroker::new();
        publish_topic(&broker, "dis.test", "growing-topic", json!(1), None, None, Routing::default()).unwrap();
        assert_eq!(poll_topic_cursor(&broker, "dis.test", "carol", "growing-topic").unwrap().len(), 1);

        publish_topic(&broker, "dis.test", "growing-topic", json!(2), None, None, Routing::default()).unwrap();
        let second = poll_topic_cursor(&broker, "dis.test", "carol", "growing-topic").unwrap();
        assert_eq!(second.len(), 1);
        match &second[0].payload {
            TephraPayload::Plaintext { body } => assert_eq!(*body, json!(2)),
            other => panic!("expected Plaintext payload, got {other:?}"),
        }
    }
}
