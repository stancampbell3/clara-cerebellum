// relay_filter_test.rs
// --------------------
// Regression tests for the CLIPS-mailbox consumer split
// (docs/typed_edges_followups.md #4).
//
// The CLIPS mailbox has two destructive consumers that used to poll it
// UNFILTERED, relying purely on phase order to not eat each other's events:
//   - consume_coire_events (clips_pass, phase 3) — wants ritual/*,
//     relay-prolog:*, and pushed events, dispatching them as (coire-event)
//     facts or assert/goal evals;
//   - relay_clips_to_prolog (phase 4) — wants only the "clips"-origin
//     events emitted by the coire-publish-* deffunctions.
// These tests pin the now-explicit origin contract: the relay consumes only
// "clips"-prefixed origins; consume_coire_events consumes the complement.

use clara_coire::ClaraEvent;
use clara_cycle::{relay::relay_clips_to_prolog, DeductionSession};
use std::sync::Once;

static INIT: Once = Once::new();

fn init_globals() {
    INIT.call_once(|| {
        let _ = env_logger::builder().is_test(true).try_init();
        clara_coire::init_global().expect("Failed to initialize Coire");
        clara_prolog::init_global();
    });
}

/// The relay forwards only the engine-emitted "clips"-origin event; the
/// ritual/* and relay-prolog:* events stay pending for consume_coire_events.
#[test]
fn relay_forwards_only_clips_origin() {
    init_globals();
    let mut session = DeductionSession::new().expect("DeductionSession::new failed");
    let (clips_id, prolog_id) = (session.clips_id, session.prolog_id);
    let coire = clara_coire::global();

    coire.write_event(&ClaraEvent::new(
        clips_id,
        "clips",
        serde_json::json!({"type": "goal", "data": "some_prolog_goal"}),
    )).unwrap();
    coire.write_event(&ClaraEvent::new(
        clips_id,
        "ritual/hohi",
        serde_json::json!({"response": "x", "_routing": {"correlation_id": "c1"}}),
    )).unwrap();
    coire.write_event(&ClaraEvent::new(
        clips_id,
        "relay-prolog:prolog",
        serde_json::json!({"type": "assert", "data": "(mood good)"}),
    )).unwrap();

    let forwarded = relay_clips_to_prolog(&mut session, None).expect("relay failed");
    assert_eq!(forwarded, 1, "only the clips-origin event may be relayed");

    let prolog_pending = coire.read_pending(prolog_id).unwrap();
    assert_eq!(prolog_pending.len(), 1);
    assert_eq!(prolog_pending[0].origin, "relay-clips:clips");

    let clips_pending = coire.read_pending(clips_id).unwrap();
    let mut origins: Vec<_> = clips_pending.iter().map(|e| e.origin.as_str()).collect();
    origins.sort();
    assert_eq!(
        origins,
        vec!["relay-prolog:prolog", "ritual/hohi"],
        "non-clips origins must be left pending for consume_coire_events"
    );
}

/// The hazard the filters close: a ritual event pending when the relay runs
/// FIRST (out of the usual phase order) must survive it and still reach the
/// CLIPS (coire-event ...) dispatch.
#[test]
fn ritual_event_survives_relay_running_first() {
    init_globals();
    let mut session = DeductionSession::new().expect("DeductionSession::new failed");
    let clips_id = session.clips_id;
    let coire = clara_coire::global();

    coire.write_event(&ClaraEvent::new(
        clips_id,
        "ritual/hohi",
        serde_json::json!({"response": "answer", "_routing": {"correlation_id": "cid-77", "topic_path": "consults/e1"}}),
    )).unwrap();

    // Relay first — previously this swallowed the event as relay-clips:ritual/hohi.
    let forwarded = relay_clips_to_prolog(&mut session, None).expect("relay failed");
    assert_eq!(forwarded, 0, "the relay must not touch ritual/* events");
    assert_eq!(coire.read_pending(clips_id).unwrap().len(), 1, "still pending after relay");

    // Now the CLIPS consume dispatches it as a (coire-event ...) fact.
    let dispatched = session.clips.consume_coire_events().expect("consume failed");
    assert_eq!(dispatched, 1);
    assert_eq!(coire.read_pending(clips_id).unwrap().len(), 0);

    let facts = session.clips.eval("(facts)").unwrap_or_default();
    assert!(
        facts.contains("coire-event") && facts.contains("ritual/hohi"),
        "ritual/hohi must land as a coire-event fact; facts:\n{facts}"
    );
    assert!(
        facts.contains("cid-77"),
        "correlation must be lifted into the coire-event slots; facts:\n{facts}"
    );
}

/// The mirror direction: consume_coire_events must not eval a pending
/// "clips"-origin event (its data is Prolog, not CLIPS) — it stays pending
/// until the relay drains it.
#[test]
fn consume_leaves_clips_origin_for_relay() {
    init_globals();
    let mut session = DeductionSession::new().expect("DeductionSession::new failed");
    let (clips_id, prolog_id) = (session.clips_id, session.prolog_id);
    let coire = clara_coire::global();

    coire.write_event(&ClaraEvent::new(
        clips_id,
        "clips",
        serde_json::json!({"type": "goal", "data": "caws_edge_reply('e1', hohi, 'cid')"}),
    )).unwrap();

    let dispatched = session.clips.consume_coire_events().expect("consume failed");
    assert_eq!(dispatched, 0, "consume must skip relay-bound clips-origin events");
    assert_eq!(coire.read_pending(clips_id).unwrap().len(), 1, "still pending for the relay");

    let forwarded = relay_clips_to_prolog(&mut session, None).expect("relay failed");
    assert_eq!(forwarded, 1);
    let prolog_pending = coire.read_pending(prolog_id).unwrap();
    assert_eq!(prolog_pending.len(), 1);
    assert_eq!(prolog_pending[0].origin, "relay-clips:clips");
}
