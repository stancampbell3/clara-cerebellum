use clara_coire::ClaraEvent;
use serde_json::Value;

use crate::error::CycleError;
use crate::session::DeductionSession;
use crate::transpile;

/// Drain Prolog's Coire mailbox and re-emit every event into CLIPS's mailbox.
///
/// "assert"/"retract" event payloads are transpiled from Prolog term syntax to
/// CLIPS ordered-fact syntax en route:
///   - "assert"  data: `man_with_plan(stan)` → `(man_with_plan stan)`
///   - "retract" data: re-typed to "goal" with a `(do-for-all-facts ...)` expr
///
/// Returns the number of events forwarded.
pub fn relay_prolog_to_clips(session: &mut DeductionSession) -> Result<usize, CycleError> {
    let coire = clara_coire::global();
    let events = coire.poll_pending(session.prolog_id)?;
    let count = events.len();
    for event in events {
        session.record_event_in_tableau(&event.payload);
        let payload = translate_prolog_to_clips(event.payload);
        let forwarded = ClaraEvent::new(
            session.clips_id,
            format!("relay-prolog:{}", event.origin),
            payload,
        );
        coire.write_event(&forwarded)?;
    }
    log::debug!("relay_prolog_to_clips: forwarded {} events", count);
    Ok(count)
}

/// Drain CLIPS's Coire mailbox and re-emit every event into Prolog's mailbox.
///
/// "assert"/"retract" event payloads are transpiled from CLIPS ordered-fact
/// syntax to Prolog term syntax en route, provided the data starts with `(`.
/// If the data does not start with `(` it is assumed to already be in Prolog
/// syntax (the legacy convention from `the_coire.clp`) and is passed through.
///
/// Returns the number of events forwarded.
pub fn relay_clips_to_prolog(session: &mut DeductionSession) -> Result<usize, CycleError> {
    let coire = clara_coire::global();
    let events = coire.poll_pending(session.clips_id)?;
    let count = events.len();
    for event in events {
        let payload = translate_clips_to_prolog(event.payload);
        session.record_event_in_tableau(&payload);
        let forwarded = ClaraEvent::new(
            session.prolog_id,
            format!("relay-clips:{}", event.origin),
            payload,
        );
        coire.write_event(&forwarded)?;
    }
    log::debug!("relay_clips_to_prolog: forwarded {} events", count);
    Ok(count)
}

// ── Payload translators ───────────────────────────────────────────────────────

fn translate_prolog_to_clips(mut payload: Value) -> Value {
    let ev_type = payload.get("type").and_then(|v| v.as_str()).map(String::from);
    let data = payload.get("data").and_then(|v| v.as_str()).map(String::from);

    let (Some(ev_type), Some(data)) = (ev_type, data) else {
        return payload;
    };

    match ev_type.as_str() {
        "assert" => match transpile::prolog_to_clips_fact(&data) {
            Ok(clips_fact) => {
                payload["data"] = Value::String(clips_fact);
            }
            Err(e) => {
                log::warn!("relay P→C: assert transpile failed for {:?}: {}", data, e);
            }
        },
        "retract" => match transpile::prolog_to_clips_retract(&data) {
            Ok(clips_expr) => {
                payload["type"] = Value::String("goal".into());
                payload["data"] = Value::String(clips_expr);
            }
            Err(e) => {
                log::warn!("relay P→C: retract transpile failed for {:?}: {}", data, e);
            }
        },
        _ => {}
    }

    payload
}

fn translate_clips_to_prolog(mut payload: Value) -> Value {
    let ev_type = payload.get("type").and_then(|v| v.as_str()).map(String::from);
    let data = payload.get("data").and_then(|v| v.as_str()).map(String::from);

    let (Some(ev_type), Some(data)) = (ev_type, data) else {
        return payload;
    };

    // Only attempt CLIPS→Prolog conversion when the data looks like a CLIPS
    // fact (starts with '(').  Data that doesn't start with '(' is assumed to
    // already be in Prolog syntax (legacy convention).
    if !data.trim_start().starts_with('(') {
        return payload;
    }

    match ev_type.as_str() {
        "assert" | "retract" => match transpile::clips_fact_to_prolog(&data) {
            Ok(prolog_term) => {
                payload["data"] = Value::String(prolog_term);
            }
            Err(e) => {
                log::warn!("relay C→P: transpile failed for {:?}: {}", data, e);
            }
        },
        _ => {}
    }

    payload
}
