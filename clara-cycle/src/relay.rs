use clara_coire::ClaraEvent;

use crate::error::CycleError;
use crate::session::DeductionSession;

/// Drain Prolog's Coire mailbox and re-emit every event into CLIPS's mailbox.
///
/// The source events are atomically marked `Processed` by `poll_pending`.
/// Each forwarded event gets a fresh `event_id`, `Pending` status, and the
/// CLIPS session UUID as its `session_id`, while payload is preserved verbatim.
///
/// Returns the number of events forwarded.
pub fn relay_prolog_to_clips(session: &mut DeductionSession) -> Result<usize, CycleError> {
    let coire  = clara_coire::global();
    let events = coire.poll_pending(session.prolog_id)?;
    let count  = events.len();
    for event in events {
        let forwarded = ClaraEvent::new(
            session.clips_id,
            format!("relay-prolog:{}", event.origin),
            event.payload,
        );
        coire.write_event(&forwarded)?;
    }
    log::debug!("relay_prolog_to_clips: forwarded {} events", count);
    Ok(count)
}

/// Drain CLIPS's Coire mailbox and re-emit every event into Prolog's mailbox.
///
/// Mirror of [`relay_prolog_to_clips`] in the opposite direction.
///
/// Returns the number of events forwarded.
pub fn relay_clips_to_prolog(session: &mut DeductionSession) -> Result<usize, CycleError> {
    let coire  = clara_coire::global();
    let events = coire.poll_pending(session.clips_id)?;
    let count  = events.len();
    for event in events {
        let forwarded = ClaraEvent::new(
            session.prolog_id,
            format!("relay-clips:{}", event.origin),
            event.payload,
        );
        coire.write_event(&forwarded)?;
    }
    log::debug!("relay_clips_to_prolog: forwarded {} events", count);
    Ok(count)
}
