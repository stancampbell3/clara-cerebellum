use clara_coire::ClaraEvent;
use serde_json::Value;
use uuid::Uuid;

use crate::error::CycleError;
use crate::session::DeductionSession;
use crate::transpile;

/// Context passed to relay functions to enable persistent tableau-change recording.
///
/// When `Some`, each relay event that updates the tableau is snapshotted into the
/// on-file DuckDB store immediately after the update.
///
/// `store` is a cheap `Arc`-backed clone — constructing one does not open a new file.
pub struct RelayRecorder {
    pub store:        clara_coire::CoireStore,
    pub deduction_id: Uuid,
    pub cycle:        u32,
}

/// Drain Prolog's Coire mailbox and re-emit every event into CLIPS's mailbox.
///
/// "assert"/"retract" event payloads are transpiled from Prolog term syntax to
/// CLIPS ordered-fact syntax en route:
///   - "assert"  data: `man_with_plan(stan)` → `(man_with_plan stan)`
///   - "retract" data: re-typed to "goal" with a `(do-for-all-facts ...)` expr
///
/// If `rec` is `Some`, a tableau snapshot is recorded to the on-file DuckDB store
/// after each event updates the tableau.
///
/// Returns the number of events forwarded.
pub fn relay_prolog_to_clips(
    session: &mut DeductionSession,
    rec: Option<&RelayRecorder>,
) -> Result<usize, CycleError> {
    let coire = clara_coire::global();
    // Only forward events that Prolog itself emitted (origin "prolog").
    // Events with origin "relay-clips:*" are inbound from CLIPS and are meant
    // to be consumed by Prolog's own coire_consume — leave them untouched.
    let events = coire.poll_pending_with_origin_prefix(session.prolog_id, "prolog")?;
    let count = events.len();
    for event in events {
        session.record_event_in_tableau(&event.payload);
        if let Some(r) = rec {
            record_tableau_snapshot(session, r, "prolog_to_clips", &event);
        }
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
/// If `rec` is `Some`, a tableau snapshot is recorded to the on-file DuckDB store
/// after each event updates the tableau.
///
/// Returns the number of events forwarded.
pub fn relay_clips_to_prolog(
    session: &mut DeductionSession,
    rec: Option<&RelayRecorder>,
) -> Result<usize, CycleError> {
    let coire = clara_coire::global();
    let events = coire.poll_pending(session.clips_id)?;
    let count = events.len();
    for event in events {
        let payload = translate_clips_to_prolog(event.payload);
        session.record_event_in_tableau(&payload);
        if let Some(r) = rec {
            // Build a synthetic event wrapper so we can pass origin/type to the recorder.
            let synthetic = ClaraEvent::new(session.prolog_id, event.origin.clone(), payload.clone());
            record_tableau_snapshot(session, r, "clips_to_prolog", &synthetic);
        }
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

// ── Recording helper ──────────────────────────────────────────────────────────

/// Export the current tableau and insert a `tableau_changes` row via `rec`.
/// Errors are logged but do not propagate — recording is best-effort.
fn record_tableau_snapshot(
    session: &DeductionSession,
    rec:     &RelayRecorder,
    phase:   &str,
    event:   &ClaraEvent,
) {
    match session.tableau.export_session(session.prolog_id) {
        Ok(entries) => {
            let ev_type = event.payload.get("type").and_then(|v| v.as_str());
            let ev_data = event.payload.to_string();
            if let Err(e) = rec.store.record_tableau_change(
                rec.deduction_id,
                rec.cycle,
                phase,
                Some(event.origin.as_str()),
                ev_type,
                Some(&ev_data),
                &entries,
            ) {
                log::warn!("relay: failed to record tableau change ({}): {}", phase, e);
            }
        }
        Err(e) => log::warn!("relay: failed to export tableau for recording: {}", e),
    }
}
