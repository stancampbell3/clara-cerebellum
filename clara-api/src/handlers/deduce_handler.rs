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
                status:    CycleStatus::Running,
                result:    None,
                cycles:    0,
                interrupt: interrupt.clone(),
                created_at: std::time::Instant::now(),
            },
        );
    }

    let state_bg = state.clone();

    tokio::spawn(async move {
        let bg_result = tokio::task::spawn_blocking(move || {
            let mut session = DeductionSession::new()?;
            session.seed_prolog(&clauses)?;
            session.seed_clips(&constructs)?;
            let mut controller =
                CycleController::new(session, max_cycles, initial_goal, interrupt_bg);
            controller.run()
        })
        .await;

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
