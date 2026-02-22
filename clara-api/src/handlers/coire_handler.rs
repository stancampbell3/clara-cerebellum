use actix_web::{web, HttpResponse};
use clara_coire::ClaraEvent;
use serde_json::json;

use crate::handlers::session_handler::AppState;
use crate::models::CoirePushRequest;

/// GET /cycle/coire/snapshot
///
/// Returns pending Coire event counts for all sessions that belong to
/// completed (or still-running-but-finished) deduction results.
pub async fn snapshot(state: web::Data<AppState>) -> HttpResponse {
    let coire      = clara_coire::global();
    let deductions = state.deductions.read().unwrap();

    let mut sessions = Vec::new();

    for entry in deductions.values() {
        if let Some(ref result) = entry.result {
            for &session_id in &[result.prolog_session_id, result.clips_session_id] {
                let pending = coire.count_pending(session_id).unwrap_or(0);
                sessions.push(json!({
                    "session_id":    session_id,
                    "pending_count": pending,
                }));
            }
        }
    }

    HttpResponse::Ok().json(json!({ "sessions": sessions }))
}

/// POST /cycle/coire/push
///
/// Inject a synthetic event into a known Coire session.  Useful for
/// testing the relay pipeline or triggering engine behaviour from outside.
pub async fn push(
    _state: web::Data<AppState>,
    req:    web::Json<CoirePushRequest>,
) -> HttpResponse {
    let coire = clara_coire::global();

    let event = ClaraEvent::new(
        req.session_id,
        req.origin.clone(),
        json!({
            "type": req.event_type,
            "data": req.data,
        }),
    );

    match coire.write_event(&event) {
        Ok(()) => HttpResponse::Ok().json(json!({ "event_id": event.event_id })),
        Err(e) => HttpResponse::InternalServerError()
            .json(json!({ "error": e.to_string() })),
    }
}
