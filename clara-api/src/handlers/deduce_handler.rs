use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use actix_web::{web, HttpResponse};
use clara_coire::DeductionSnapshot;
use serde_json::json;
use uuid::Uuid;

use clara_cycle::{CycleController, CycleStatus, DeductionSession};

use crate::handlers::session_handler::{AppState, DeductionEntry};
use crate::models::{
    DeduceDeleteSnapshotResponse, DeduceInterruptResponse, DeduceRequest, DeduceResumeRequest,
    DeduceStartResponse, DeduceStatusResponse,
};

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

/// POST /deduce — start an asynchronous deduction run.
///
/// Immediately returns `202 Accepted` with a `deduction_id` that can be
/// polled via `GET /deduce/{id}` or cancelled via `DELETE /deduce/{id}`.
///
/// When `persist: true` and a Coire store is configured, a
/// [`DeductionSnapshot`] is saved at cycle completion, enabling later
/// resumption via `POST /deduce/resume`.
pub async fn start_deduce(
    state: web::Data<AppState>,
    req:   web::Json<DeduceRequest>,
) -> HttpResponse {
    let clauses      = req.prolog_clauses.clone();
    let constructs   = req.clips_constructs.clone();
    let clips_file   = req.clips_file.clone();
    let initial_goal = req.initial_goal.clone();
    let max_cycles   = req.max_cycles.unwrap_or(100);
    let persist      = req.persist;
    let context      = req.context.clone();

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

        let store_bg = coire_store.clone();
        let clauses_bg      = clauses.clone();
        let constructs_bg   = constructs.clone();
        let clips_file_bg   = clips_file.clone();
        let initial_goal_bg = initial_goal.clone();
        let context_bg      = context.clone();

        let bg_handle = tokio::task::spawn_blocking(move || {
            let mut session = DeductionSession::new()?;
            session.seed_prolog(&clauses_bg)?;
            if let Some(ref path) = clips_file_bg {
                session.seed_clips_file(path)?;
            }
            session.seed_clips(&constructs_bg)?;
            session.seed_context(&context_bg)?;
            // Notify the async context of the session IDs before blocking in run().
            let _ = ids_tx.send((session.prolog_id, session.clips_id));
            let mut controller = {
                let c = CycleController::new(session, max_cycles, initial_goal_bg, interrupt_bg);
                if let Some(store) = store_bg { c.with_store(store) } else { c }
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
        let (final_status, final_cycles) = {
            let mut deductions = state_bg.deductions.write().unwrap();
            if let Some(entry) = deductions.get_mut(&deduction_id) {
                match bg_result {
                    Ok(Ok(ref deduction_result)) => {
                        entry.cycles = deduction_result.cycles;
                        entry.status = deduction_result.status.clone();
                        entry.result = Some(deduction_result.clone());
                    }
                    Ok(Err(ref e)) => {
                        if let clara_cycle::CycleError::MaxCyclesExceeded(n) = e {
                            entry.cycles = *n;
                        }
                        entry.status = CycleStatus::Error(e.to_string());
                    }
                    Err(ref join_err) => {
                        entry.status =
                            CycleStatus::Error(format!("spawn_blocking panicked: {}", join_err));
                    }
                }
                (entry.status.to_string(), entry.cycles)
            } else {
                (CycleStatus::Error("entry missing".into()).to_string(), 0)
            }
        };

        // Save snapshot if requested and store is configured.
        if persist {
            match (coire_store, tracked_ids) {
                (Some(store), Some((prolog_id, clips_id))) => {
                    let snapshot_ttl_ms = state_bg.snapshot_ttl_ms;
                    let created = now_ms();
                    let snap = DeductionSnapshot {
                        deduction_id,
                        prolog_clauses:    clauses,
                        clips_constructs:  constructs,
                        clips_file,
                        initial_goal,
                        max_cycles,
                        status:            final_status,
                        cycles_run:        final_cycles,
                        prolog_session_id: prolog_id,
                        clips_session_id:  clips_id,
                        created_at_ms:     created,
                        expires_at_ms:     created + snapshot_ttl_ms,
                        context,
                    };
                    if let Err(e) = store.save_snapshot(&snap) {
                        log::warn!("deduce {}: failed to save snapshot: {}", deduction_id, e);
                    }
                }
                (None, _) => {
                    log::warn!(
                        "deduce {}: persist=true but no Coire store configured",
                        deduction_id
                    );
                }
                _ => {}
            }
        }
    });

    HttpResponse::Accepted().json(DeduceStartResponse {
        deduction_id,
        status: "running".to_string(),
    })
}

/// POST /deduce/resume — resume a previously persisted deduction.
///
/// Looks up the [`DeductionSnapshot`] for `deduction_id`, re-seeds fresh
/// engine instances from the stored knowledge, restores any pending Coire
/// events, and runs the cycle. Returns `202 Accepted` with a new
/// `deduction_id` for the resumed run.
pub async fn resume_deduce(
    state: web::Data<AppState>,
    req:   web::Json<DeduceResumeRequest>,
) -> HttpResponse {
    let store = match &state.coire_store {
        Some(s) => s.clone(),
        None => {
            return HttpResponse::ServiceUnavailable()
                .json(json!({ "error": "persistence not enabled" }));
        }
    };

    // Load snapshot — fast blocking call; acceptable in async context.
    let snap = match store.load_snapshot(req.deduction_id) {
        Ok(Some(s)) => s,
        Ok(None) => {
            return HttpResponse::NotFound()
                .json(json!({ "error": "snapshot not found" }));
        }
        Err(e) => {
            return HttpResponse::InternalServerError()
                .json(json!({ "error": e.to_string() }));
        }
    };

    // Guard: reject if the previous session is still running.
    {
        let active = state.active_coire_sessions.read().unwrap();
        if active.contains(&snap.prolog_session_id) || active.contains(&snap.clips_session_id) {
            return HttpResponse::Conflict()
                .json(json!({ "error": "session still active" }));
        }
    }

    let max_cycles   = req.max_cycles.unwrap_or(snap.max_cycles);
    let persist      = req.persist;
    // Caller may override the context; otherwise reuse the snapshot's stored context.
    let context      = req.context.clone().unwrap_or_else(|| snap.context.clone());
    let deduction_id = Uuid::new_v4();
    let interrupt     = Arc::new(AtomicBool::new(false));
    let interrupt_bg  = interrupt.clone();

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
    let active_sessions = state.active_coire_sessions.clone();
    let prev_prolog_id  = snap.prolog_session_id;
    let prev_clips_id   = snap.clips_session_id;

    // Clone seed fields for use inside spawn_blocking and for snapshot save.
    let clauses      = snap.prolog_clauses.clone();
    let constructs   = snap.clips_constructs.clone();
    let clips_file   = snap.clips_file.clone();
    let initial_goal = snap.initial_goal.clone();

    tokio::spawn(async move {
        let (ids_tx, ids_rx) = tokio::sync::oneshot::channel::<(Uuid, Uuid)>();

        let store_bg      = store.clone();
        let clauses_bg    = clauses.clone();
        let constructs_bg = constructs.clone();
        let clips_file_bg = clips_file.clone();
        let context_bg    = context.clone();

        let bg_handle = tokio::task::spawn_blocking(move || {
            let mut session = DeductionSession::new()?;
            session.seed_prolog(&clauses_bg)?;
            if let Some(ref path) = clips_file_bg {
                session.seed_clips_file(path)?;
            }
            session.seed_clips(&constructs_bg)?;
            session.seed_context(&context_bg)?;
            let _ = ids_tx.send((session.prolog_id, session.clips_id));
            let mut controller = CycleController::new(session, max_cycles, None, interrupt_bg)
                .with_store(store_bg.clone());
            controller.restore_from(&store_bg, prev_prolog_id, prev_clips_id)?;
            controller.run()
        });

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

        if let Some((prolog_id, clips_id)) = tracked_ids {
            let mut active = active_sessions.write().unwrap();
            active.remove(&prolog_id);
            active.remove(&clips_id);
        }

        let (final_status, final_cycles) = {
            let mut deductions = state_bg.deductions.write().unwrap();
            if let Some(entry) = deductions.get_mut(&deduction_id) {
                match bg_result {
                    Ok(Ok(ref r)) => {
                        entry.cycles = r.cycles;
                        entry.status = r.status.clone();
                        entry.result = Some(r.clone());
                    }
                    Ok(Err(ref e)) => {
                        if let clara_cycle::CycleError::MaxCyclesExceeded(n) = e {
                            entry.cycles = *n;
                        }
                        entry.status = CycleStatus::Error(e.to_string());
                    }
                    Err(ref join_err) => {
                        entry.status =
                            CycleStatus::Error(format!("spawn_blocking panicked: {}", join_err));
                    }
                }
                (entry.status.to_string(), entry.cycles)
            } else {
                (CycleStatus::Error("entry missing".into()).to_string(), 0)
            }
        };

        if persist {
            if let Some((prolog_id, clips_id)) = tracked_ids {
                let snapshot_ttl_ms = state_bg.snapshot_ttl_ms;
                let created = now_ms();
                let new_snap = DeductionSnapshot {
                    deduction_id,
                    prolog_clauses:    clauses,
                    clips_constructs:  constructs,
                    clips_file,
                    initial_goal,
                    max_cycles,
                    status:            final_status,
                    cycles_run:        final_cycles,
                    prolog_session_id: prolog_id,
                    clips_session_id:  clips_id,
                    created_at_ms:     created,
                    expires_at_ms:     created + snapshot_ttl_ms,
                    context,
                };
                if let Err(e) = store.save_snapshot(&new_snap) {
                    log::warn!("resume {}: failed to save snapshot: {}", deduction_id, e);
                }
            }
        }
    });

    HttpResponse::Accepted().json(DeduceStartResponse {
        deduction_id,
        status: "running".to_string(),
    })
}

/// DELETE /deduce/{id}/snapshot — explicitly delete a persisted snapshot.
///
/// Removes the snapshot row and all associated Coire events from the store.
/// Returns `409 Conflict` if the session is still active.
pub async fn delete_snapshot(
    state: web::Data<AppState>,
    path:  web::Path<Uuid>,
) -> HttpResponse {
    let deduction_id = path.into_inner();

    let store = match &state.coire_store {
        Some(s) => s.clone(),
        None => {
            return HttpResponse::ServiceUnavailable()
                .json(json!({ "error": "persistence not enabled" }));
        }
    };

    // Verify the snapshot exists and check for active sessions.
    let snap = match store.load_snapshot(deduction_id) {
        Ok(Some(s)) => s,
        Ok(None) => {
            return HttpResponse::NotFound()
                .json(json!({ "error": "snapshot not found" }));
        }
        Err(e) => {
            return HttpResponse::InternalServerError()
                .json(json!({ "error": e.to_string() }));
        }
    };

    {
        let active = state.active_coire_sessions.read().unwrap();
        if active.contains(&snap.prolog_session_id) || active.contains(&snap.clips_session_id) {
            return HttpResponse::Conflict()
                .json(json!({ "error": "session still active — cannot delete snapshot of a running deduction" }));
        }
    }

    match store.delete_snapshot(deduction_id) {
        Ok(_) => HttpResponse::Ok().json(DeduceDeleteSnapshotResponse {
            deduction_id,
            status: "deleted".to_string(),
        }),
        Err(e) => HttpResponse::InternalServerError()
            .json(json!({ "error": e.to_string() })),
    }
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
