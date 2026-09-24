use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use actix_web::{web, HttpResponse};
use clara_coire::DeductionSnapshot;
use serde_json::json;
use uuid::Uuid;

use clara_cycle::{CycleController, CycleStatus, DeductionSession, PredicateEntry};

use crate::handlers::session_handler::{AppState, DeductionEntry};
use crate::module_sources::{self, ModuleError};
use crate::models::{
    DeduceDeleteSnapshotResponse, DeduceInterruptResponse, DeduceRequest, DeduceResumeRequest,
    DeduceStartResponse, DeduceStatusResponse,
};

/// Why a finished run did not converge, as a stable machine-readable string.
/// `None` for `Running` and `Converged`. Max-cycles and other errors arrive as
/// `Err` from the controller and are tagged at the call site.
fn reason_for_status(status: &CycleStatus) -> Option<String> {
    match status {
        CycleStatus::Interrupted => Some("interrupted".to_string()),
        CycleStatus::Expired     => Some("deadline".to_string()),
        CycleStatus::Error(_)    => Some("error".to_string()),
        CycleStatus::Running | CycleStatus::Converged => None,
    }
}

/// Stable machine-readable reason for a run that ended in a `CycleError`.
fn reason_for_error(e: &clara_cycle::CycleError) -> &'static str {
    match e {
        clara_cycle::CycleError::MaxCyclesExceeded(_)  => "max_cycles",
        clara_cycle::CycleError::ModuleDependency(_)   => "module_dependency",
        _                                              => "error",
    }
}

/// Load the run's Prolog program: module fragments first (in the order given), then the node clauses, in one
/// `seed_prolog` call. With modules present the sources are first inspected and the run is refused on any missing
/// module or predicate conflict; with none, this is exactly the old `seed_prolog`.
fn load_prolog_program(
    session:      &mut DeductionSession,
    module_ids:   &[Uuid],
    store:        Option<&clara_coire::CoireStore>,
    node_clauses: Vec<String>,
) -> Result<(), clara_cycle::CycleError> {
    let dep = |e: ModuleError| clara_cycle::CycleError::ModuleDependency(e.to_string());
    let modules = module_sources::resolve_module_sources(module_ids, store).map_err(dep)?;
    if !modules.is_empty() {
        let conflicts = module_sources::check_modules(
            &session.prolog,
            "node source",
            &node_clauses.join("\n"),
            &modules,
        )
        .map_err(dep)?;
        if !conflicts.is_empty() {
            return Err(dep(ModuleError::Conflicts(conflicts)));
        }
    }
    session.seed_prolog(&module_sources::with_modules(&modules, node_clauses))
}

/// What a resume must reload. A source-id run's snapshot keeps only the request's *inline* clauses (empty), so the
/// program is re-resolved from the registered source ids the snapshot recorded, falling back to the stored inline text.
fn resolve_resume_sources(
    snap:   &DeductionSnapshot,
    store:  &clara_coire::CoireStore,
    ttl_ms: i64,
) -> (Vec<String>, Option<String>, Vec<String>) {
    let (clauses, _) = resolve_prolog_source(snap.prolog_source_id, &snap.prolog_clauses, Some(store), ttl_ms);
    let (clips_file, constructs, _) = resolve_clips_source(
        snap.clips_source_id,
        snap.clips_file.clone(),
        &snap.clips_constructs,
        Some(store),
        ttl_ms,
    );
    (clauses, clips_file, constructs)
}

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
    let clauses           = req.prolog_clauses.clone();
    let constructs        = req.clips_constructs.clone();
    let clips_file        = req.clips_file.clone();
    let initial_goal      = req.initial_goal.clone();
    let max_cycles        = req.max_cycles.unwrap_or(100);
    let persist           = req.persist;
    let trace             = req.trace;
    let context           = req.context.clone();
    let req_prolog_src_id = req.prolog_source_id;
    let req_clips_src_id  = req.clips_source_id;
    let ritual_id_req     = req.ritual_id;
    let patience_req      = req.evaluator_patience_cycles;
    let module_ids        = req.prolog_module_source_ids.clone();
    let deadline = match state.deadline_policy.resolve(req.deadline_ms) {
        Ok(d)    => d,
        Err(msg) => return HttpResponse::BadRequest().json(json!({ "error": msg })),
    };
    let self_node_id_req  = req.self_node_id.clone();
    let initial_offering_req = req.initial_offering.clone();

    let deduction_id = Uuid::new_v4();
    let interrupt     = Arc::new(AtomicBool::new(false));
    let interrupt_bg  = interrupt.clone();

    // Register the entry immediately so polls can observe "running".
    {
        let mut deductions = state.deductions.write().unwrap();
        deductions.insert(
            deduction_id,
            DeductionEntry::new_running(
                interrupt.clone(),
                ritual_id_req,
                max_cycles,
                deadline.map(|d| d.as_millis() as u64),
                None,
            ),
        );
    }

    let state_bg           = state.clone();
    let coire_store        = state.coire_store.clone();
    let active_sessions    = state.active_coire_sessions.clone();
    let ritual_registry_bg = state.ritual_registry.clone();
    let state_perf         = state.clone();

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
        let snapshot_ttl_ms_bg = state_bg.snapshot_ttl_ms;
        let module_ids_bg   = module_ids.clone();

        let bg_handle = tokio::task::spawn_blocking(move || {
            // ── Source resolution ─────────────────────────────────────────────
            // Priority: prolog_source_id → inline clauses (auto-register if store).
            let (effective_clauses, resolved_prolog_src_id) =
                resolve_prolog_source(req_prolog_src_id, &clauses_bg, store_bg.as_ref(), snapshot_ttl_ms_bg);

            let (effective_clips_file, effective_constructs, resolved_clips_src_id) =
                resolve_clips_source(req_clips_src_id, clips_file_bg, &constructs_bg, store_bg.as_ref(), snapshot_ttl_ms_bg);

            let mut session = DeductionSession::new()?;
            load_prolog_program(&mut session, &module_ids_bg, store_bg.as_ref(), effective_clauses)?;
            if let Some(ref path) = effective_clips_file {
                session.seed_clips_file(path)?;
            }
            session.seed_clips(&effective_constructs)?;
            session.seed_context(&context_bg)?;
            // Notify the async context of the session IDs before blocking in run().
            let _ = ids_tx.send((session.prolog_id, session.clips_id));
            let mut controller = {
                let c = CycleController::new(session, max_cycles, initial_goal_bg, interrupt_bg)
                    .with_deduction_id(deduction_id);
                let c = if let Some(store) = store_bg { c.with_store(store) } else { c };
                let c = c.with_trace(trace).with_deadline(deadline);
                let c = if let Some(p) = patience_req { c.with_evaluator_patience(p) } else { c };
                let c = c.with_self_node_id(self_node_id_req)
                         .with_initial_offering(initial_offering_req);
                // Attach a RitualHandle when the caller supplied a ritual_id.
                // Each deduction run joins anonymously (fresh performance_id).
                if let Some(rid) = ritual_id_req {
                    match ritual_registry_bg.join(rid, None) {
                        Ok(handle) => {
                            log::info!(
                                "start_deduce: deduction {} joined ritual {} (performance {})",
                                deduction_id, rid, handle.performance_id
                            );
                            if let Some(entry) =
                                state_perf.deductions.write().unwrap().get_mut(&deduction_id)
                            {
                                entry.performance_id = Some(handle.performance_id);
                            }
                            c.with_ritual(handle)
                        }
                        Err(e) => {
                            log::warn!(
                                "start_deduce: failed to join ritual {} for deduction {}: {}",
                                rid, deduction_id, e
                            );
                            c
                        }
                    }
                } else { c }
            };
            let result = controller.run();
            // Pass resolved source IDs back alongside the result.
            result.map(|r| (r, resolved_prolog_src_id, resolved_clips_src_id))
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
        // bg_result is now Ok(Ok((DeductionResult, prolog_src_id, clips_src_id)))
        let (final_status, final_cycles, final_tableau, final_prolog_src_id, final_clips_src_id) = {
            let mut deductions = state_bg.deductions.write().unwrap();
            if let Some(entry) = deductions.get_mut(&deduction_id) {
                let (tableau, prolog_src, clips_src) = match bg_result {
                    Ok(Ok(ref triple)) => {
                        let (deduction_result, p_src, c_src) = triple;
                        entry.cycles = deduction_result.cycles;
                        entry.status = deduction_result.status.clone();
                        entry.reason = reason_for_status(&entry.status);
                        let t = deduction_result.tableau.clone();
                        entry.result = Some(deduction_result.clone());
                        (t, *p_src, *c_src)
                    }
                    Ok(Err(ref e)) => {
                        entry.reason = Some(reason_for_error(e).to_string());
                        if let clara_cycle::CycleError::MaxCyclesExceeded(n) = e {
                            entry.cycles = *n;
                        }
                        entry.status = CycleStatus::Error(e.to_string());
                        (None, None, None)
                    }
                    Err(ref join_err) => {
                        entry.status =
                            CycleStatus::Error(format!("spawn_blocking panicked: {}", join_err));
                        entry.reason = Some("error".to_string());
                        (None, None, None)
                    }
                };
                entry.mark_completed();
                (entry.status.to_string(), entry.cycles, tableau, prolog_src, clips_src)
            } else {
                (CycleStatus::Error("entry missing".into()).to_string(), 0, None, None, None)
            }
        };

        let final_performance_id = state_bg
            .deductions
            .read()
            .unwrap()
            .get(&deduction_id)
            .and_then(|e| e.performance_id);

        // Save snapshot if requested and store is configured.
        if persist {
            match (coire_store, tracked_ids) {
                (Some(store), Some((prolog_id, clips_id))) => {
                    let snapshot_ttl_ms = state_bg.snapshot_ttl_ms;
                    let created = now_ms();
                    let tableau_json = final_tableau
                        .as_deref()
                        .map(|t| serde_json::to_value(t).unwrap_or(serde_json::json!([])))
                        .unwrap_or(serde_json::json!([]));
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
                        tableau_entries:   tableau_json,
                        prolog_source_id:  final_prolog_src_id,
                        clips_source_id:   final_clips_src_id,
                        dot_artifact_id:   None,
                        ritual_id:         ritual_id_req,
                        performance_id:    final_performance_id,
                        deadline_ms:       deadline.map(|d| d.as_millis() as u64),
                        prolog_module_source_ids: module_ids.clone(),
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
    let trace        = req.trace;
    // Caller may override the context; otherwise reuse the snapshot's stored context.
    let context      = req.context.clone().unwrap_or_else(|| snap.context.clone());
    // Inherit source IDs from the snapshot being resumed.
    let snap_prolog_src_id = snap.prolog_source_id;
    let snap_clips_src_id  = snap.clips_source_id;
    // Request override, else the budget stored in the snapshot, else the server
    // default; always clamped to the ceiling. The budget starts fresh here.
    let resume_deadline = match state
        .deadline_policy
        .resolve_resume(req.deadline_ms, snap.deadline_ms)
    {
        Ok(d)    => d,
        Err(msg) => return HttpResponse::BadRequest().json(json!({ "error": msg })),
    };
    let deduction_id = Uuid::new_v4();
    let interrupt     = Arc::new(AtomicBool::new(false));
    let interrupt_bg  = interrupt.clone();

    {
        let mut deductions = state.deductions.write().unwrap();
        deductions.insert(
            deduction_id,
            DeductionEntry::new_running(
                interrupt.clone(),
                None,
                max_cycles,
                resume_deadline.map(|d| d.as_millis() as u64),
                Some(snap.deduction_id),
            ),
        );
    }

    let state_bg        = state.clone();
    let active_sessions = state.active_coire_sessions.clone();
    let prev_prolog_id  = snap.prolog_session_id;
    let prev_clips_id   = snap.clips_session_id;

    // Clone seed fields for use inside spawn_blocking and for snapshot save. The program is re-resolved from the
    // snapshot's registered source ids (see `resolve_resume_sources`), not just its stored inline text.
    let (clauses, clips_file, constructs) =
        resolve_resume_sources(&snap, &store, state.snapshot_ttl_ms);
    let initial_goal = snap.initial_goal.clone();
    let module_ids   = snap.prolog_module_source_ids.clone();
    let snap_inline_clauses    = snap.prolog_clauses.clone();
    let snap_inline_constructs = snap.clips_constructs.clone();
    let snap_inline_clips_file = snap.clips_file.clone();

    tokio::spawn(async move {
        let (ids_tx, ids_rx) = tokio::sync::oneshot::channel::<(Uuid, Uuid)>();

        let store_bg      = store.clone();
        let clauses_bg    = clauses.clone();
        let constructs_bg = constructs.clone();
        let clips_file_bg = clips_file.clone();
        let context_bg    = context.clone();
        let module_ids_bg = module_ids.clone();

        let snap_tableau = snap.tableau_entries.clone();
        let bg_handle = tokio::task::spawn_blocking(move || {
            // Deserialize stored tableau entries (best-effort; empty on failure).
            let prev_tableau: Vec<PredicateEntry> =
                serde_json::from_value(snap_tableau).unwrap_or_default();
            let mut session = DeductionSession::new()?;
            load_prolog_program(&mut session, &module_ids_bg, Some(&store_bg), clauses_bg)?;
            if let Some(ref path) = clips_file_bg {
                session.seed_clips_file(path)?;
            }
            session.seed_clips(&constructs_bg)?;
            session.seed_context(&context_bg)?;
            let _ = ids_tx.send((session.prolog_id, session.clips_id));
            let mut controller = CycleController::new(session, max_cycles, None, interrupt_bg)
                .with_deduction_id(deduction_id)
                .with_store(store_bg.clone())
                .with_trace(trace)
                .with_deadline(resume_deadline);
            controller.restore_from(&store_bg, prev_prolog_id, prev_clips_id, &prev_tableau)?;
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

        let (final_status, final_cycles, final_tableau) = {
            let mut deductions = state_bg.deductions.write().unwrap();
            if let Some(entry) = deductions.get_mut(&deduction_id) {
                let tableau = match bg_result {
                    Ok(Ok(ref r)) => {
                        entry.cycles = r.cycles;
                        entry.status = r.status.clone();
                        entry.reason = reason_for_status(&entry.status);
                        let t = r.tableau.clone();
                        entry.result = Some(r.clone());
                        t
                    }
                    Ok(Err(ref e)) => {
                        entry.reason = Some(reason_for_error(e).to_string());
                        if let clara_cycle::CycleError::MaxCyclesExceeded(n) = e {
                            entry.cycles = *n;
                        }
                        entry.status = CycleStatus::Error(e.to_string());
                        None
                    }
                    Err(ref join_err) => {
                        entry.status =
                            CycleStatus::Error(format!("spawn_blocking panicked: {}", join_err));
                        entry.reason = Some("error".to_string());
                        None
                    }
                };
                entry.mark_completed();
                (entry.status.to_string(), entry.cycles, tableau)
            } else {
                (CycleStatus::Error("entry missing".into()).to_string(), 0, None)
            }
        };

        if persist {
            if let Some((prolog_id, clips_id)) = tracked_ids {
                let snapshot_ttl_ms = state_bg.snapshot_ttl_ms;
                let created = now_ms();
                let tableau_json = final_tableau
                    .as_deref()
                    .map(|t| serde_json::to_value(t).unwrap_or(serde_json::json!([])))
                    .unwrap_or(serde_json::json!([]));
                // Keep the ORIGINAL inline text: the program itself is re-resolved from the source ids next time.
                let new_snap = DeductionSnapshot {
                    deduction_id,
                    prolog_clauses:    snap_inline_clauses,
                    clips_constructs:  snap_inline_constructs,
                    clips_file:        snap_inline_clips_file,
                    initial_goal,
                    max_cycles,
                    status:            final_status,
                    cycles_run:        final_cycles,
                    prolog_session_id: prolog_id,
                    clips_session_id:  clips_id,
                    created_at_ms:     created,
                    expires_at_ms:     created + snapshot_ttl_ms,
                    context,
                    tableau_entries:   tableau_json,
                    prolog_source_id:  snap_prolog_src_id,
                    clips_source_id:   snap_clips_src_id,
                    dot_artifact_id:   None,
                    ritual_id:         None,
                    performance_id:    None,
                    deadline_ms:       resume_deadline.map(|d| d.as_millis() as u64),
                    prolog_module_source_ids: module_ids,
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

/// GET /deduce/{id}/snapshot — inspect a persisted deduction snapshot.
///
/// Returns the [`DeductionSnapshot`] stored for the given `deduction_id`,
/// or `404` if no snapshot exists.
pub async fn get_snapshot(
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

    match store.load_snapshot(deduction_id) {
        Ok(Some(snap)) => HttpResponse::Ok().json(snap),
        Ok(None) => HttpResponse::NotFound()
            .json(json!({ "error": "snapshot not found" })),
        Err(e) => HttpResponse::InternalServerError()
            .json(json!({ "error": e.to_string() })),
    }
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

            // DELETE only sets the interrupt flag; report it immediately even
            // though the task has not finalized the entry yet.
            let status = entry.effective_status();
            let reason = if status == CycleStatus::Interrupted
                && matches!(entry.status, CycleStatus::Running)
            {
                Some("interrupted".to_string())
            } else {
                entry.reason.clone()
            };

            HttpResponse::Ok().json(DeduceStatusResponse {
                deduction_id:    id,
                status:          status.to_string(),
                result:          result_json,
                cycles:          entry.cycles,
                reason,
                ritual_id:       entry.ritual_id,
                performance_id:  entry.performance_id,
                max_cycles:      Some(entry.max_cycles),
                deadline_ms:     entry.deadline_ms,
                started_at_ms:   Some(entry.started_at_ms),
                completed_at_ms: entry.completed_at_ms,
                resumed_from:    entry.resumed_from,
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
            // Only signal. `status` stays `Running` until the task, the single
            // writer, finalizes it; polls report `interrupted` immediately via
            // `DeductionEntry::effective_status`.
            entry.interrupt.store(true, Ordering::SeqCst);
            HttpResponse::Ok().json(DeduceInterruptResponse {
                deduction_id: id,
                status:       "interrupted".to_string(),
            })
        }
    }
}

// ── Source resolution helpers ─────────────────────────────────────────────────

/// Resolve the Prolog source to use for a deduction.
///
/// Priority:
/// 1. `source_id` given → load content from registry (panics gracefully to inline)
/// 2. Inline `clauses` → auto-register in registry with snapshot TTL; return source_id
/// 3. No source → empty clauses
///
/// Returns `(effective_clauses, Option<source_id>)`.
fn resolve_prolog_source(
    source_id:    Option<Uuid>,
    clauses:      &[String],
    store:        Option<&clara_coire::CoireStore>,
    ttl_ms:       i64,
) -> (Vec<String>, Option<Uuid>) {
    // Case 1: caller supplied a pre-registered source_id.
    if let Some(sid) = source_id {
        if let Some(s) = store {
            match s.sources.get(sid) {
                Ok(Some(entry)) => {
                    // Split stored content back into individual clauses.
                    let loaded: Vec<String> = entry.content
                        .lines()
                        .map(str::trim)
                        .filter(|l| !l.is_empty())
                        .map(str::to_string)
                        .collect();
                    return (loaded, Some(sid));
                }
                Ok(None) => log::warn!("prolog_source_id {} not found; falling back to inline clauses", sid),
                Err(e)   => log::warn!("prolog_source_id {} lookup failed: {}; using inline clauses", sid, e),
            }
        }
    }

    // Case 2: auto-register inline clauses.
    if !clauses.is_empty() {
        if let Some(s) = store {
            let content = clauses.join("\n");
            let expires = ttl_ms.checked_add(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as i64,
            );
            match s.sources.register("prolog", None, &content, expires) {
                Ok((sid, _)) => return (clauses.to_vec(), Some(sid)),
                Err(e) => log::warn!("failed to auto-register prolog source: {}", e),
            }
        }
    }

    (clauses.to_vec(), None)
}

/// Resolve the CLIPS source to use for a deduction.
///
/// Priority:
/// 1. `source_id` given → load content from registry as a tempfile path
/// 2. Inline `clips_file` / `constructs` → auto-register content in registry
///
/// Returns `(effective_clips_file, effective_constructs, Option<source_id>)`.
fn resolve_clips_source(
    source_id:   Option<Uuid>,
    clips_file:  Option<String>,
    constructs:  &[String],
    store:       Option<&clara_coire::CoireStore>,
    ttl_ms:      i64,
) -> (Option<String>, Vec<String>, Option<Uuid>) {
    // Case 1: caller supplied a pre-registered clips source_id.
    // For CLIPS we register for identity/dedup only — no artifact generation.
    // The content is returned as inline constructs rather than a file path.
    // Constructs are balanced-paren s-expressions (a defrule spans many
    // lines and `;` comments must be dropped) — never split by line.
    if let Some(sid) = source_id {
        if let Some(s) = store {
            match s.sources.get(sid) {
                Ok(Some(entry)) => {
                    let loaded = clara_clips::ffi::split_clips_constructs(&entry.content);
                    return (None, loaded, Some(sid));
                }
                Ok(None) => log::warn!("clips_source_id {} not found; falling back", sid),
                Err(e)   => log::warn!("clips_source_id {} lookup failed: {}; falling back", sid, e),
            }
        }
    }

    // Case 2: auto-register clips_file content (read from disk) or inline constructs.
    let (content_opt, is_file) = if let Some(ref path) = clips_file {
        (std::fs::read_to_string(path).ok(), true)
    } else if !constructs.is_empty() {
        (Some(constructs.join("\n")), false)
    } else {
        (None, false)
    };

    if let (Some(content), Some(s)) = (&content_opt, store) {
        let expires = ttl_ms.checked_add(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64,
        );
        match s.sources.register("clips", None, content, expires) {
            Ok((sid, _)) => {
                if is_file {
                    return (clips_file, constructs.to_vec(), Some(sid));
                } else {
                    return (None, constructs.to_vec(), Some(sid));
                }
            }
            Err(e) => log::warn!("failed to auto-register clips source: {}", e),
        }
    }

    (clips_file, constructs.to_vec(), None)
}

// ── GET /deduce ───────────────────────────────────────────────────────────────

/// List persisted deductions, newest first.
///
/// Accepts an optional `?limit=N` query parameter (default 50, max 500).
/// Returns a JSON array of summary objects — each with the fields most useful
/// for picking a deduction to inspect further with `GET /deduce/{id}/trace`.
///
/// Requires persistence (Coire store) to be configured. Returns `503` if not.
pub async fn list_deductions(
    state: web::Data<AppState>,
    query: web::Query<ListDeductionsQuery>,
) -> HttpResponse {
    let store = match &state.coire_store {
        Some(s) => s.clone(),
        None => {
            return HttpResponse::ServiceUnavailable()
                .json(json!({ "error": "persistence not enabled" }));
        }
    };

    let limit = query.limit.unwrap_or(50).min(500);

    match store.list_snapshots(Some(limit)) {
        Ok(snaps) => {
            let items: Vec<_> = snaps
                .iter()
                .map(|s| json!({
                    "deduction_id":  s.deduction_id,
                    "status":        s.status,
                    "cycles_run":    s.cycles_run,
                    "initial_goal":  s.initial_goal,
                    "created_at_ms": s.created_at_ms,
                    "ritual_id":      s.ritual_id,
                    "performance_id": s.performance_id,
                    "deadline_ms":    s.deadline_ms,
                }))
                .collect();
            HttpResponse::Ok().json(json!({ "deductions": items }))
        }
        Err(e) => HttpResponse::InternalServerError()
            .json(json!({ "error": e.to_string() })),
    }
}

#[derive(serde::Deserialize)]
pub struct ListDeductionsQuery {
    pub limit: Option<u32>,
}

#[cfg(test)]
mod module_and_resume_tests {
    use super::*;

    fn temp_store() -> (clara_coire::CoireStore, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let store = clara_coire::CoireStore::open(dir.path().join("t.duckdb")).unwrap();
        (store, dir)
    }

    fn snapshot(prolog_source_id: Option<Uuid>, inline: &[&str]) -> DeductionSnapshot {
        DeductionSnapshot {
            deduction_id: Uuid::new_v4(),
            prolog_clauses: inline.iter().map(|s| s.to_string()).collect(),
            clips_constructs: vec![],
            clips_file: None,
            initial_goal: None,
            max_cycles: 10,
            status: "expired".into(),
            cycles_run: 1,
            prolog_session_id: Uuid::new_v4(),
            clips_session_id: Uuid::new_v4(),
            created_at_ms: 0,
            expires_at_ms: i64::MAX,
            context: vec![],
            tableau_entries: serde_json::json!([]),
            prolog_source_id,
            clips_source_id: None,
            dot_artifact_id: None,
            ritual_id: None,
            performance_id: None,
            deadline_ms: None,
            prolog_module_source_ids: vec![],
        }
    }

    /// The gap this fixes: a run started with a registered source id stores only its (empty) inline clauses, so a
    /// resume used to come back with no Prolog program at all.
    #[test]
    fn resume_reloads_the_program_from_the_registered_source_id() {
        let (store, _d) = temp_store();
        let (sid, _) = store.sources.register("prolog", None, "a(1).\nb(X) :- a(X).", None).unwrap();

        let (clauses, _, _) = resolve_resume_sources(&snapshot(Some(sid), &[]), &store, 60_000);
        assert_eq!(clauses, vec!["a(1).".to_string(), "b(X) :- a(X).".to_string()]);
    }

    #[test]
    fn resume_falls_back_to_stored_inline_clauses_without_a_source_id() {
        let (store, _d) = temp_store();
        let (clauses, _, _) = resolve_resume_sources(&snapshot(None, &["c(3)."]), &store, 60_000);
        assert_eq!(clauses, vec!["c(3).".to_string()]);
    }

    #[test]
    fn resume_falls_back_to_inline_when_the_source_was_swept() {
        let (store, _d) = temp_store();
        let ghost = Uuid::new_v4();
        let (clauses, _, _) = resolve_resume_sources(&snapshot(Some(ghost), &["kept(1)."]), &store, 60_000);
        assert_eq!(clauses, vec!["kept(1).".to_string()]);
    }

    #[test]
    fn reasons_for_errors_are_stable_machine_strings() {
        use clara_cycle::CycleError;
        assert_eq!(reason_for_error(&CycleError::MaxCyclesExceeded(5)), "max_cycles");
        assert_eq!(reason_for_error(&CycleError::ModuleDependency("x".into())), "module_dependency");
        assert_eq!(reason_for_error(&CycleError::Clips("x".into())), "error");
    }

    fn session() -> DeductionSession {
        DeductionSession::new().unwrap()
    }

    fn register_module(store: &clara_coire::CoireStore, label: &str, content: &str) -> Uuid {
        store.sources.register(module_sources::MODULE_SOURCE_TYPE, Some(label), content, None).unwrap().0
    }

    /// The loader itself, against a real engine: modules load ahead of the node source and are callable from it.
    #[test]
    fn modules_load_ahead_of_the_node_source_and_are_callable() {
        let (store, _d) = temp_store();
        let m = register_module(&store, "greet@1.0.0", "greeting(hello).");
        let mut s = session();
        load_prolog_program(&mut s, &[m], Some(&store), vec!["say(X) :- greeting(X).".into()]).unwrap();
        assert!(s.prolog.query_once("say(hello)").is_ok(), "the node rule reaches the module fact");
    }

    #[test]
    fn a_conflict_refuses_the_run_before_anything_is_loaded() {
        use clara_cycle::CycleError;
        let (store, _d) = temp_store();
        let m = register_module(&store, "dup@1.0.0", "shared(1).");
        let mut s = session();
        let err = load_prolog_program(&mut s, &[m], Some(&store), vec!["shared(2).".into()]).unwrap_err();
        match err {
            CycleError::ModuleDependency(msg) => assert!(msg.contains("shared/1"), "{msg}"),
            other => panic!("expected ModuleDependency, got {other:?}"),
        }
        assert!(s.prolog.query_once("shared(_)").is_err(), "nothing was loaded");
    }

    #[test]
    fn a_missing_module_refuses_the_run() {
        use clara_cycle::CycleError;
        let (store, _d) = temp_store();
        let mut s = session();
        let err = load_prolog_program(&mut s, &[Uuid::new_v4()], Some(&store), vec!["x(1).".into()]).unwrap_err();
        assert!(matches!(err, CycleError::ModuleDependency(m) if m.contains("not found")));
    }

    #[test]
    fn without_modules_the_node_source_loads_exactly_as_before() {
        let mut s = session();
        // Even a node source that redefines an overlay name is untouched when no modules are involved.
        load_prolog_program(&mut s, &[], None, vec!["strip_think(A, A).".into()]).unwrap();
        assert!(s.prolog.query_once("strip_think(x, x)").is_ok());
    }
}
