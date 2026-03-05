use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use actix_web::{web, HttpResponse};
use serde_json::json;
use uuid::Uuid;

use clara_cycle::{CycleController, CycleStatus, DeductionSession};

use crate::handlers::session_handler::{AppState, DeductionEntry};
use crate::models::{DeduceInterruptResponse, DeduceRequest, DeduceStartResponse, DeduceStatusResponse};

/// POST /deduce — start an asynchronous deduction run.
///
/// Immediately returns `202 Accepted` with a `deduction_id` that can be
/// polled via `GET /deduce/{id}` or cancelled via `DELETE /deduce/{id}`.
pub async fn start_deduce(
    state: web::Data<AppState>,
    req:   web::Json<DeduceRequest>,
) -> HttpResponse {
    let clauses      = req.prolog_clauses.clone();
    let constructs   = req.clips_constructs.clone();
    let clips_file   = req.clips_file.clone();
    let initial_goal = req.initial_goal.clone();
    let max_cycles   = req.max_cycles.unwrap_or(100);

    let deduction_id = Uuid::new_v4();
    let interrupt     = Arc::new(AtomicBool::new(false));
    let interrupt_bg  = interrupt.clone();

    // Register the entry immediately so polls can observe "running".
    {
        let mut deductions = state.deductions.write().unwrap();
        deductions.insert(
            deduction_id,
            DeductionEntry {
                status:            CycleStatus::Running,
                result:            None,
                cycles:            0,
                interrupt:         interrupt.clone(),
                created_at:        std::time::Instant::now(),
                prolog_session_id: None,
                clips_session_id:  None,
            },
        );
    }

    let state_bg        = state.clone();
    let coire_store     = state.coire_store.clone();
    let active_sessions = state.active_coire_sessions.clone();

    tokio::spawn(async move {
        // Channel: blocking thread sends session UUIDs as soon as they exist,
        // before run() starts, so we can register them as active immediately.
        let (ids_tx, ids_rx) = tokio::sync::oneshot::channel::<(Uuid, Uuid)>();

        let bg_handle = tokio::task::spawn_blocking(move || {
            let mut session = DeductionSession::new()?;
            session.seed_prolog(&clauses)?;
            if let Some(ref path) = clips_file {
                session.seed_clips_file(path)?;
            }
            session.seed_clips(&constructs)?;
            // Notify the async context of the session IDs before blocking in run().
            let _ = ids_tx.send((session.prolog_id, session.clips_id));
            let mut controller = {
                let c = CycleController::new(session, max_cycles, initial_goal, interrupt_bg);
                if let Some(store) = coire_store { c.with_store(store) } else { c }
            };
            controller.run()
        });

        // Track session IDs so the carrion-picker won't delete live mailboxes.
        let mut tracked_ids: Option<(Uuid, Uuid)> = None;
        if let Ok((prolog_id, clips_id)) = ids_rx.await {
            tracked_ids = Some((prolog_id, clips_id));
            {
                let mut active = active_sessions.write().unwrap();
                active.insert(prolog_id);
                active.insert(clips_id);
            }
            {
                let mut deductions = state_bg.deductions.write().unwrap();
                if let Some(entry) = deductions.get_mut(&deduction_id) {
                    entry.prolog_session_id = Some(prolog_id);
                    entry.clips_session_id  = Some(clips_id);
                }
            }
        }

        let bg_result = bg_handle.await;

        // Remove session IDs from the active set — run() has finished.
        if let Some((prolog_id, clips_id)) = tracked_ids {
            let mut active = active_sessions.write().unwrap();
            active.remove(&prolog_id);
            active.remove(&clips_id);
        }

        // Update the deduction entry with the final result.
        let mut deductions = state_bg.deductions.write().unwrap();
        if let Some(entry) = deductions.get_mut(&deduction_id) {
            match bg_result {
                Ok(Ok(deduction_result)) => {
                    entry.cycles = deduction_result.cycles;
                    entry.status = deduction_result.status.clone();
                    entry.result = Some(deduction_result);
                }
                Ok(Err(e)) => {
                    if let clara_cycle::CycleError::MaxCyclesExceeded(n) = &e {
                        entry.cycles = *n;
                    }
                    entry.status = CycleStatus::Error(e.to_string());
                }
                Err(join_err) => {
                    entry.status =
                        CycleStatus::Error(format!("spawn_blocking panicked: {}", join_err));
                }
            }
        }
    });

    HttpResponse::Accepted().json(DeduceStartResponse {
        deduction_id,
        status: "running".to_string(),
    })
}

/// GET /deduce/{id} — poll the status of a deduction run.
pub async fn poll_deduce(
    state: web::Data<AppState>,
    path:  web::Path<Uuid>,
) -> HttpResponse {
    let id         = path.into_inner();
    let deductions = state.deductions.read().unwrap();

    match deductions.get(&id) {
        None => HttpResponse::NotFound().json(json!({ "error": "deduction not found" })),
        Some(entry) => {
            let result_json = entry
                .result
                .as_ref()
                .map(|r| serde_json::to_value(r).unwrap_or(json!(null)));

            HttpResponse::Ok().json(DeduceStatusResponse {
                deduction_id: id,
                status:       entry.status.to_string(),
                result:       result_json,
                cycles:       entry.cycles,
            })
        }
    }
}

/// DELETE /deduce/{id} — request interrupt of a running deduction.
///
/// Sets the interrupt flag; the background task will observe it at the end of
/// its next cycle and return early with `Interrupted` status.
pub async fn interrupt_deduce(
    state: web::Data<AppState>,
    path:  web::Path<Uuid>,
) -> HttpResponse {
    let id          = path.into_inner();
    let mut deductions = state.deductions.write().unwrap();

    match deductions.get_mut(&id) {
        None => HttpResponse::NotFound().json(json!({ "error": "deduction not found" })),
        Some(entry) => {
            entry.interrupt.store(true, Ordering::SeqCst);
            // Optimistically mark interrupted; the background task will confirm.
            if matches!(entry.status, CycleStatus::Running) {
                entry.status = CycleStatus::Interrupted;
            }
            HttpResponse::Ok().json(DeduceInterruptResponse {
                deduction_id: id,
                status:       "interrupted".to_string(),
            })
        }
    }
}
