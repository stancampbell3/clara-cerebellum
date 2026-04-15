use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use clara_dagda::{Binding, PredicateEntry, TruthValue};
use uuid::Uuid;

use crate::error::CycleError;
use crate::relay::{relay_clips_to_prolog, relay_prolog_to_clips, RelayRecorder};
use crate::result::{CoireSnapshot, CycleStatus, DeductionResult, InMemoryTraceEntry};
use crate::session::DeductionSession;

// ---------------------------------------------------------------------------
// GoalAgenda
// ---------------------------------------------------------------------------

/// Tracks the root goal and any derived goals introduced during forward
/// chaining, used to detect when the reasoning cycle has stopped making
/// progress.
///
/// "Progress" means at least one tableau entry changed truth value since the
/// previous cycle.  When no change occurs and all Coire mailboxes are empty,
/// the cycle has converged to a fixed point.
struct GoalAgenda {
    /// Functor name of the initial goal (e.g. `"launch_missiles"`).
    root_functor: Option<String>,
    /// Arity of the initial goal (0 for atoms, N for compound terms).
    root_arity: u32,
    /// Timestamp at the start of the last completed cycle; used to detect
    /// tableau changes via `tableau_changed_since`.
    last_cycle_ts: i64,
}

impl GoalAgenda {
    fn new(initial_goal: &Option<String>) -> Self {
        let root_functor = initial_goal.as_deref().map(extract_functor_from_goal);
        let root_arity   = initial_goal.as_deref().map(extract_arity_from_goal).unwrap_or(0);
        Self { root_functor, root_arity, last_cycle_ts: now_ms() }
    }

    /// Record the start of a new cycle (snapshot the current timestamp).
    fn begin_cycle(&mut self) {
        self.last_cycle_ts = now_ms();
    }

    /// Returns `true` if the tableau has changed since `last_cycle_ts`.
    fn tableau_progressed(&self, session: &DeductionSession) -> bool {
        session
            .tableau
            .tableau_changed_since(session.prolog_id, self.last_cycle_ts)
            .unwrap_or(true) // default to "changed" on error (conservative)
    }

    /// Returns `true` if the root goal's tableau entry has reached a resolved
    /// truth value (KnownTrue | KnownFalse | KnownUnresolved).
    fn root_goal_resolved(&self, session: &DeductionSession) -> bool {
        let functor = match &self.root_functor {
            Some(f) => f,
            None => return false,
        };
        // Look for any entry for this functor (at the correct arity) with a
        // resolved truth value.
        match session.tableau.list_by_functor(session.prolog_id, functor, self.root_arity) {
            Ok(entries) => entries.iter().any(|e| e.truth_value.is_resolved()),
            Err(_) => false,
        }
    }
}

// ---------------------------------------------------------------------------
// CycleController
// ---------------------------------------------------------------------------

/// Drives the Prolog → relay → CLIPS → relay → convergence loop for one
/// deduction request.
///
/// The controller is **blocking** — call it from `tokio::task::spawn_blocking`
/// inside async handlers so the async event loop is never stalled.
pub struct CycleController {
    /// Stable identifier for this deduction run, used to link tableau-change
    /// rows in the on-file DuckDB store.
    deduction_id:  Uuid,
    session:       DeductionSession,
    max_cycles:    u32,
    /// Optional Prolog goal to execute on the first cycle.
    initial_goal:  Option<String>,
    /// Set to `true` from outside to request early termination.
    interrupt:     Arc<AtomicBool>,
    /// Optional persistent store. When set, both mailboxes are saved on every
    /// exit from `run()` (converged, interrupted, or max-cycles exceeded).
    store:         Option<clara_coire::CoireStore>,
    /// Solutions captured by `re_evaluate_root_goal` when the root goal
    /// succeeds on a later cycle (after forward-chaining).  Overrides the
    /// cycle-0 `initial_solutions` in the final `DeductionResult`.
    final_solutions: Option<serde_json::Value>,
    /// When `true`, tableau snapshots are recorded at each cycle phase.
    /// If a store is configured they go to `tableau_changes`; otherwise they
    /// accumulate in `trace_log` and are returned in `DeductionResult.trace`.
    trace_mode: bool,
    /// In-memory trace accumulator used when `trace_mode = true` and no
    /// store is configured.
    trace_log: Vec<InMemoryTraceEntry>,
    /// Optional handle to an active Ritual. When set, `evaluator_pass` will
    /// poll incoming Tephras from peer evaluators and publish outbound
    /// evaluator-tagged Coire events to the Ritual topic.
    #[cfg(feature = "ritual")]
    ritual_handle: Option<clara_ritual::RitualHandle>,
    /// Count of Offerings published to the Ritual topic that have not yet
    /// been answered by a Hohi from a peer evaluator.
    ///
    /// Convergence is blocked while this is non-zero so the cycle does not
    /// terminate before peer responses arrive. The counter is decremented
    /// each time `ingest_tephra` processes a Hohi-labelled envelope.
    /// Max-cycles termination is NOT blocked — the cycle will still exhaust
    /// its budget and return `Err(MaxCyclesExceeded)` if responses never
    /// arrive, matching the existing no-solution convergence behaviour.
    #[cfg(feature = "ritual")]
    pending_evaluator_responses: usize,
}

impl CycleController {
    pub fn new(
        session:      DeductionSession,
        max_cycles:   u32,
        initial_goal: Option<String>,
        interrupt:    Arc<AtomicBool>,
    ) -> Self {
        Self {
            deduction_id: Uuid::new_v4(),
            session,
            max_cycles,
            initial_goal,
            interrupt,
            store: None,
            final_solutions: None,
            trace_mode: false,
            trace_log: Vec::new(),
            #[cfg(feature = "ritual")]
            ritual_handle: None,
            #[cfg(feature = "ritual")]
            pending_evaluator_responses: 0,
        }
    }

    /// Attach a [`RitualHandle`] so that `evaluator_pass` will publish
    /// outbound evaluator-tagged Coire events and ingest incoming Tephras
    /// from peer evaluators.
    #[cfg(feature = "ritual")]
    pub fn with_ritual(mut self, handle: clara_ritual::RitualHandle) -> Self {
        self.ritual_handle = Some(handle);
        self
    }

    /// Attach a persistent [`CoireStore`]. Both mailboxes will be saved
    /// automatically on every exit from [`run()`].
    pub fn with_store(mut self, store: clara_coire::CoireStore) -> Self {
        self.store = Some(store);
        self
    }

    /// Override the deduction ID used to key `tableau_changes` rows.
    ///
    /// By default `CycleController::new` generates a fresh UUID.  Call this
    /// so the controller's internal ID matches the one already registered in
    /// `deduction_snapshots` — otherwise trace queries on the snapshot UUID
    /// will find no results.
    pub fn with_deduction_id(mut self, id: Uuid) -> Self {
        self.deduction_id = id;
        self
    }

    /// Enable per-phase tableau recording for trace visualization.
    ///
    /// When a store is attached, snapshots go to `tableau_changes`.
    /// When no store is configured, snapshots accumulate in memory and are
    /// returned in [`DeductionResult::trace`].
    pub fn with_trace(mut self, enabled: bool) -> Self {
        self.trace_mode = enabled;
        self
    }

    /// Reload a previous run's Coire state and tableau into the current session.
    ///
    /// Coire events stored under `prev_prolog_id` / `prev_clips_id` are written
    /// into the current session's IDs.  Tableau entries from `prev_tableau` are
    /// imported verbatim (their `session_id` is preserved as-is so the caller
    /// should supply entries originally exported for these sessions).
    /// Call this before [`run()`] when resuming a previous deduction.
    pub fn restore_from(
        &mut self,
        store: &clara_coire::CoireStore,
        prev_prolog_id: Uuid,
        prev_clips_id: Uuid,
        prev_tableau: &[PredicateEntry],
    ) -> Result<(), CycleError> {
        let coire = clara_coire::global();
        store.restore_session_as(prev_prolog_id, self.session.prolog_id, coire)?;
        store.restore_session_as(prev_clips_id, self.session.clips_id, coire)?;
        if !prev_tableau.is_empty() {
            self.session.tableau.import_session(prev_tableau).map_err(|e| {
                CycleError::SessionCreationFailed(format!("tableau restore failed: {e}"))
            })?;
        }
        Ok(())
    }

    /// Run the deduction loop until convergence, interrupt, or max cycles.
    ///
    /// Returns `Ok(DeductionResult)` for normal termination (converged or
    /// interrupted) and `Err(CycleError::MaxCyclesExceeded)` when the cycle
    /// budget is exhausted without convergence.
    pub fn run(&mut self) -> Result<DeductionResult, CycleError> {
        // Establish deduction context for evaluate-cache attribution.
        // The guard restores `None` on drop — including on unwind — so every
        // cache entry produced during this run is tagged with `deduction_id`.
        let _ctx = clara_toolbox::ffi::deduction_context(self.deduction_id);

        log::info!(
            "CycleController: starting (max_cycles={}, goal={:?})",
            self.max_cycles,
            self.initial_goal
        );

        let mut prev_snapshot = self.snapshot();
        let mut initial_solutions: Option<serde_json::Value> = None;
        let mut agenda = GoalAgenda::new(&self.initial_goal);

        // Record the initial tableau state before any cycles run.
        self.record_tableau("initial", 0);

        for cycle in 0..self.max_cycles {
            log::debug!("CycleController: cycle {}", cycle);
            agenda.begin_cycle();

            // 1. Prolog pass — consume Coire events + run goal
            log::debug!("Prolog pass");
            let solutions = self.prolog_pass(cycle)?;
            if cycle == 0 {
                initial_solutions = solutions;
                // Seed tableau truth values from cycle-0 Prolog solutions.
                self.update_tableau_from_solutions(&initial_solutions);
            }
            log::debug!("... prolog pass complete");

            // 2. Relay Prolog → CLIPS
            log::debug!("Relay Prolog → CLIPS");
            let rec = self.make_recorder(cycle);
            relay_prolog_to_clips(&mut self.session, rec.as_ref())?;
            log::debug!("... relay clips complete");

            // 3. Evaluator pass (structural stub — no LLM/FieryPit yet)
            log::debug!("Evaluator pass");
            self.evaluator_pass();
            log::debug!("... evaluator pass complete");

            // 4. CLIPS pass — consume Coire events + run inference engine
            log::debug!("CLIPS pass");
            self.clips_pass()?;
            log::debug!("... CLIPS pass complete");

            // 5. Relay CLIPS → Prolog
            log::debug!("Relay CLIPS → Prolog");
            let rec = self.make_recorder(cycle);
            relay_clips_to_prolog(&mut self.session, rec.as_ref())?;
            log::debug!("... relay prolog complete");

            // 6. Convergence check
            log::debug!("Convergence check");
            let curr_snapshot = self.snapshot();
            if self.has_converged(&prev_snapshot, &curr_snapshot, &agenda) {
                log::info!("CycleController: converged after {} cycle(s)", cycle + 1);
                let tableau = self.export_tableau();
                let goal_bindings = self.root_goal_bindings(&agenda);
                // Prefer solutions captured by re_evaluate_root_goal (post-forward-chain)
                // over the cycle-0 snapshot, which may have been empty.
                let solutions = self.final_solutions.take().or(initial_solutions);
                self.record_tableau("final_converged", cycle);
                self.save_to_store();
                self.evict_coire_sessions();
                return Ok(DeductionResult {
                    status:            CycleStatus::Converged,
                    cycles:            cycle + 1,
                    prolog_session_id: self.session.prolog_id,
                    clips_session_id:  self.session.clips_id,
                    prolog_solutions:  solutions,
                    goal_bindings,
                    tableau:           Some(tableau),
                    explanation:       None,
                    trace:             self.take_trace_log(),
                });
            } else {
                log::debug!("... not converged yet");
            }
            prev_snapshot = curr_snapshot;

            // 7. Interrupt check
            log::debug!("Interrupt check");
            if self.interrupt.load(Ordering::SeqCst) {
                log::info!("CycleController: interrupted after {} cycle(s)", cycle + 1);
                let tableau = self.export_tableau();
                let goal_bindings = self.root_goal_bindings(&agenda);
                let solutions = self.final_solutions.take().or(initial_solutions);
                self.record_tableau("final_interrupted", cycle);
                self.save_to_store();
                self.evict_coire_sessions();
                return Ok(DeductionResult {
                    status:            CycleStatus::Interrupted,
                    cycles:            cycle + 1,
                    prolog_session_id: self.session.prolog_id,
                    clips_session_id:  self.session.clips_id,
                    prolog_solutions:  solutions,
                    goal_bindings,
                    tableau:           Some(tableau),
                    explanation:       None,
                    trace:             self.take_trace_log(),
                });
            } else {
                log::debug!("... no interrupt signal");
            }
        }

        log::warn!(
            "CycleController: max cycles exceeded ({} cycles) without convergence",
            self.max_cycles
        );
        self.record_tableau("final_max_cycles", self.max_cycles.saturating_sub(1));
        self.save_to_store();
        self.evict_coire_sessions();
        Err(CycleError::MaxCyclesExceeded(self.max_cycles))
    }

    // -------------------------------------------------------------------------
    // Private helpers
    // -------------------------------------------------------------------------

    /// Build a [`RelayRecorder`] for the given `cycle`.
    /// Returns `None` when no store is attached or trace is disabled.
    /// The store is cloned cheaply via its internal `Arc`.
    fn make_recorder(&self, cycle: u32) -> Option<RelayRecorder> {
        if !self.trace_mode { return None; }
        self.store.as_ref().map(|store| RelayRecorder {
            store: store.clone(),
            deduction_id: self.deduction_id,
            cycle,
        })
    }

    /// Drain the in-memory trace log for inclusion in `DeductionResult`.
    ///
    /// Returns `Some(log)` only when `trace_mode` is set AND no store is
    /// configured (store-backed traces are queryable via the API separately).
    fn take_trace_log(&mut self) -> Option<Vec<InMemoryTraceEntry>> {
        if self.trace_mode && self.store.is_none() && !self.trace_log.is_empty() {
            Some(std::mem::take(&mut self.trace_log))
        } else {
            None
        }
    }

    /// Record a full tableau snapshot at a bookend phase.
    ///
    /// Does nothing when `trace_mode` is `false`.
    /// When a store is attached, writes to `tableau_changes`.
    /// Otherwise appends to `self.trace_log` for in-memory tracing.
    fn record_tableau(&mut self, phase: &str, cycle: u32) {
        if !self.trace_mode { return; }
        match self.session.tableau.export_session(self.session.prolog_id) {
            Ok(entries) => {
                if let Some(store) = &self.store {
                    if let Err(e) = store.record_tableau_change(
                        self.deduction_id,
                        cycle,
                        phase,
                        None,
                        None,
                        None,
                        &entries,
                    ) {
                        log::warn!("CycleController: failed to record tableau ({}): {}", phase, e);
                    }
                } else {
                    let now_ms = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as i64;
                    self.trace_log.push(InMemoryTraceEntry {
                        cycle_num: cycle,
                        phase: phase.to_string(),
                        recorded_at_ms: now_ms,
                        entries,
                    });
                }
            }
            Err(e) => log::warn!("CycleController: failed to export tableau ({}): {}", phase, e),
        }
    }

    /// Remove all events for both engine mailboxes from the global in-memory
    /// Coire. Always runs after `save_to_store()`.
    fn evict_coire_sessions(&self) {
        let coire = clara_coire::global();
        if let Err(e) = coire.clear_session(self.session.prolog_id) {
            log::warn!("CycleController: failed to evict prolog Coire mailbox: {}", e);
        }
        if let Err(e) = coire.clear_session(self.session.clips_id) {
            log::warn!("CycleController: failed to evict CLIPS Coire mailbox: {}", e);
        }
        log::debug!(
            "CycleController: evicted Coire mailboxes for prolog={} clips={}",
            self.session.prolog_id,
            self.session.clips_id
        );
    }

    /// Save both mailboxes to the store if one is configured.
    fn save_to_store(&self) {
        let Some(store) = &self.store else { return };
        let coire = clara_coire::global();
        if let Err(e) = store.save_session(self.session.prolog_id, coire) {
            log::warn!("CycleController: failed to save prolog session to store: {}", e);
        }
        if let Err(e) = store.save_session(self.session.clips_id, coire) {
            log::warn!("CycleController: failed to save clips session to store: {}", e);
        }
    }

    /// Run one Prolog pass.
    fn prolog_pass(&mut self, cycle: u32) -> Result<Option<serde_json::Value>, CycleError> {
        self.session.prolog.consume_coire_events()?;

        if cycle == 0 {
            let goal = self.initial_goal.clone().unwrap_or_else(|| "true".to_string());
            let solutions = match self.session.prolog.query_with_bindings(&goal) {
                Ok(json_str) => {
                    let v = serde_json::from_str::<serde_json::Value>(&json_str)
                        .unwrap_or(serde_json::json!([]));
                    let n = v.as_array().map(|a| a.len()).unwrap_or(0);
                    if n > 0 {
                        log::debug!("CycleController: prolog_pass goal proved ({} solution(s)): {}", n, goal);
                    } else {
                        log::debug!("CycleController: prolog_pass goal failed (no solutions): {}", goal);
                    }
                    v
                }
                Err(e) => {
                    log::warn!("CycleController: prolog_pass goal exception: {}: {}", goal, e);
                    serde_json::json!([])
                }
            };
            Ok(Some(solutions))
        } else {
            match self.session.prolog.query_once("true") {
                Ok(_)  => {}
                Err(e) => log::warn!("CycleController: prolog_pass tick failed: {}", e),
            }
            Ok(None)
        }
    }

    fn evaluator_pass(&mut self) {
        #[cfg(feature = "ritual")]
        self.evaluator_pass_ritual();

        #[cfg(not(feature = "ritual"))]
        log::debug!("CycleController: evaluator_pass (stub — ritual feature not enabled)");
    }

    /// Full ritual evaluator pass: poll peer Tephras, ingest them, then drain
    /// outbound evaluator-tagged Coire events to the Ritual topic.
    ///
    /// Only compiled when the `ritual` feature is enabled. Called from
    /// `evaluator_pass`; can also be called directly from tests.
    #[cfg(feature = "ritual")]
    fn evaluator_pass_ritual(&mut self) {
        let handle = match self.ritual_handle.as_ref().map(|h| h.clone()) {
            Some(h) => h,
            None => {
                log::debug!("CycleController: evaluator_pass (no ritual handle)");
                return;
            }
        };

        // 1. Drain incoming Tephras from peer evaluators → Prolog mailbox
        match handle.poll_incoming() {
            Ok(tephras) => {
                for tephra in &tephras {
                    self.ingest_tephra(tephra);
                }
                if !tephras.is_empty() {
                    log::debug!(
                        "CycleController: evaluator_pass ingested {} tephra(s)",
                        tephras.len()
                    );
                }
            }
            Err(e) => log::warn!("CycleController: evaluator_pass poll failed: {}", e),
        }

        // 2. Drain outbound evaluator-tagged Coire events → Ritual topic
        self.publish_evaluator_events(&handle);
    }

    /// Unpack a [`TephraEnvelope`] and write its payload as a new [`ClaraEvent`]
    /// into the Prolog Coire mailbox with origin `"ritual/{label}"`.
    ///
    /// A `Hohi`-labelled envelope decrements `pending_evaluator_responses`,
    /// allowing convergence to proceed once all outstanding Offerings have
    /// been answered.  Encrypted payloads are logged and skipped until Phase 7.
    #[cfg(feature = "ritual")]
    fn ingest_tephra(&mut self, tephra: &clara_ritual::TephraEnvelope) {
        let body = match &tephra.payload {
            clara_ritual::TephraPayload::Plaintext { body } => body.clone(),
            clara_ritual::TephraPayload::Encrypted { .. } => {
                log::warn!(
                    "CycleController: ingest_tephra — encrypted payload not yet \
                     supported (tephra {}) — skipping",
                    tephra.tephra_id
                );
                return;
            }
        };

        // A Hohi means a peer evaluator has answered one of our Offerings.
        if tephra.label == clara_ritual::label::HOHI {
            self.pending_evaluator_responses =
                self.pending_evaluator_responses.saturating_sub(1);
            log::debug!(
                "CycleController: ingest_tephra Hohi received — pending now {}",
                self.pending_evaluator_responses
            );
        }

        let event = clara_coire::ClaraEvent::new(
            self.session.prolog_id,
            format!("ritual/{}", tephra.label),
            body,
        );

        match clara_coire::global().write_event(&event) {
            Ok(()) => log::debug!(
                "CycleController: ingest_tephra wrote tephra {} (label={}) \
                 as Coire event {}",
                tephra.tephra_id, tephra.label, event.event_id
            ),
            Err(e) => log::warn!(
                "CycleController: ingest_tephra failed to write tephra {}: {}",
                tephra.tephra_id, e
            ),
        }
    }

    /// Drain all pending Coire events whose origin starts with `"evaluator/"`
    /// and publish each as a Tephra to the Ritual topic.
    ///
    /// Each successfully published Offering increments
    /// `pending_evaluator_responses` so that convergence is blocked until a
    /// matching Hohi arrives via `ingest_tephra`.  Events with other origins
    /// are left untouched so the Prolog and CLIPS passes can consume them
    /// normally.
    #[cfg(feature = "ritual")]
    fn publish_evaluator_events(&mut self, handle: &clara_ritual::RitualHandle) {
        let coire = clara_coire::global();
        let events = match coire.poll_pending_with_origin_prefix(
            self.session.prolog_id,
            "evaluator/",
        ) {
            Ok(ev) => ev,
            Err(e) => {
                log::warn!("CycleController: publish_evaluator_events — Coire poll failed: {}", e);
                return;
            }
        };

        let mut published = 0usize;
        for event in &events {
            match handle.publish_event(event, clara_ritual::label::OFFERING, None) {
                Ok(()) => {
                    published += 1;
                    log::debug!(
                        "CycleController: publish_evaluator_events published event {} \
                         (origin={}) as Tephra",
                        event.event_id, event.origin
                    );
                }
                Err(e) => log::warn!(
                    "CycleController: publish_evaluator_events failed to publish \
                     event {}: {}",
                    event.event_id, e
                ),
            }
        }

        if published > 0 {
            self.pending_evaluator_responses += published;
            log::info!(
                "CycleController: publish_evaluator_events published {} Offering(s) — \
                 pending_evaluator_responses now {}",
                published, self.pending_evaluator_responses
            );
        }
    }

    fn clips_pass(&mut self) -> Result<(), CycleError> {
        self.session
            .clips
            .consume_coire_events()
            .map_err(CycleError::Clips)?;
        self.session
            .clips
            .eval("(run)")
            .map_err(CycleError::Clips)?;
        Ok(())
    }

    /// Capture the current pending-event counts for both sessions.
    fn snapshot(&self) -> CoireSnapshot {
        let coire = clara_coire::global();
        CoireSnapshot {
            // Count only `relay-*` events for prolog_pending.  Those are the
            // events that `coire_consume` (called from prolog_pass) drains, and
            // therefore the only ones that drive new inference.  Informational
            // events such as `ritual/...` events written by `ingest_tephra` are
            // consumed independently (via explicit `coire_poll/2` in Prolog rules)
            // and must not block convergence when no rules process them.
            prolog_pending: coire
                .count_pending_with_origin_prefix(self.session.prolog_id, "relay-")
                .unwrap_or(1),
            clips_pending: coire.count_pending(self.session.clips_id).unwrap_or(1),
        }
    }

    /// Return `true` when the cycle has converged.
    ///
    /// Convergence requires **all** of:
    ///
    /// 1. Both Coire mailboxes are empty.
    /// 2. The CLIPS agenda is empty.
    /// 3. The tableau has not changed since the previous cycle (fixed point), OR
    ///    the root goal has reached a resolved truth value.
    fn has_converged(
        &mut self,
        prev: &CoireSnapshot,
        curr: &CoireSnapshot,
        agenda: &GoalAgenda,
    ) -> bool {
        let clips_agenda_empty = self
            .session
            .clips
            .eval("(= (length$ (get-agenda)) 0)")
            .map(|s| s.trim() == "TRUE")
            .unwrap_or(true);

        let mailboxes_empty = curr.prolog_pending == 0 && curr.clips_pending == 0;

        // When all queues have drained, re-query the root goal so the tableau
        // reflects the latest truth value before we test convergence.  This
        // catches the case where forward-chaining added the missing fact that
        // makes the original goal succeed on a later cycle.
        if mailboxes_empty && clips_agenda_empty {
            self.re_evaluate_root_goal();
        }

        let snapshot_stable = prev == curr;
        let tableau_stable  = !agenda.tableau_progressed(&self.session);
        let root_resolved   = agenda.root_goal_resolved(&self.session);

        // Block convergence while Offerings are awaiting Hohi responses from
        // peer evaluators. This prevents the cycle from declaring a fixed point
        // before peer responses arrive.  Max-cycles termination is unaffected.
        #[cfg(feature = "ritual")]
        let pending_responses_zero = self.pending_evaluator_responses == 0;
        #[cfg(not(feature = "ritual"))]
        let pending_responses_zero = true;

        let converged = mailboxes_empty
            && clips_agenda_empty
            && pending_responses_zero
            && (tableau_stable || root_resolved);

        #[cfg(feature = "ritual")]
        let pending_count = self.pending_evaluator_responses;
        #[cfg(not(feature = "ritual"))]
        let pending_count: usize = 0;

        log::debug!(
            "CycleController: convergence — prolog_pending={}, clips_pending={}, \
             agenda_empty={}, snapshot_stable={}, tableau_stable={}, root_resolved={}, \
             pending_evaluator_responses={} → {}",
            curr.prolog_pending,
            curr.clips_pending,
            clips_agenda_empty,
            snapshot_stable,
            tableau_stable,
            root_resolved,
            pending_count,
            converged
        );

        converged
    }

    /// Seed tableau truth values from cycle-0 Prolog solutions.
    ///
    /// If the initial goal resolved with solutions, mark the root goal's
    /// tableau entry as `KnownTrue` with its bindings.  If the solutions list
    /// is empty, mark it `KnownFalse`.
    fn update_tableau_from_solutions(&mut self, solutions: &Option<serde_json::Value>) {
        let Some(ref goal_str) = self.initial_goal else { return };
        let functor = extract_functor_from_goal(goal_str);
        let arity   = extract_arity_from_goal(goal_str) as usize;
        let wildcards: Vec<&str> = vec!["*"; arity];

        let Some(ref sols) = solutions else { return };
        let arr = match sols.as_array() {
            Some(a) => a,
            None    => return,
        };

        let (truth, bindings): (TruthValue, Vec<Binding>) = if arr.is_empty() {
            (TruthValue::KnownFalse, vec![])
        } else {
            // Flatten first solution's bindings into our Binding type.
            let first = &arr[0];
            let binds: Vec<Binding> = first
                .as_object()
                .map(|m| {
                    m.iter()
                        .map(|(k, v)| Binding {
                            var: k.clone(),
                            val: v.as_str().unwrap_or(&v.to_string()).to_string(),
                        })
                        .collect()
                })
                .unwrap_or_default();
            (TruthValue::KnownTrue, binds)
        };

        if let Err(e) = self.session.tableau.update_truth(
            self.session.prolog_id,
            &functor,
            &wildcards,
            truth,
            &bindings,
        ) {
            log::warn!("CycleController: failed to update tableau truth for {}: {}", functor, e);
        }
    }

    /// Export the full tableau for the Prolog session (snapshot for persistence).
    fn export_tableau(&self) -> Vec<PredicateEntry> {
        self.session
            .tableau
            .export_session(self.session.prolog_id)
            .unwrap_or_default()
    }

    /// Re-query the initial goal against the current Prolog database and
    /// update the tableau with the result.
    ///
    /// Called from `has_converged` once all Coire mailboxes and the CLIPS
    /// agenda are empty, so we capture any truth value changes that resulted
    /// from forward-chaining in the completed cycle.
    fn re_evaluate_root_goal(&mut self) {
        use crate::transpile::{parse_prolog_term, Term};

        let Some(goal_str) = self.initial_goal.clone() else { return };

        let term = match parse_prolog_term(&goal_str) {
            Ok(t)  => t,
            Err(e) => {
                log::warn!("re_evaluate_root_goal: parse failed for '{}': {}", goal_str, e);
                return;
            }
        };

        // Extract functor + per-argument template strings (atoms kept as-is,
        // variables kept by name so we can substitute from solution bindings).
        let (functor, arg_templates) = match term {
            Term::Atom(f) => (f, vec![]),
            Term::Compound { functor, args } => {
                let templates: Vec<String> = args.iter().map(term_to_template_str).collect();
                (functor, templates)
            }
            _ => return,
        };

        // Re-query Prolog for the current truth of the root goal.
        let json_str = match self.session.prolog.query_with_bindings(&goal_str) {
            Ok(s)  => s,
            Err(e) => {
                log::debug!("re_evaluate_root_goal: '{}' still fails: {}", goal_str, e);
                return; // Goal still failing — leave existing tableau entry alone.
            }
        };

        let solutions: serde_json::Value = match serde_json::from_str(&json_str) {
            Ok(v)  => v,
            Err(_) => return,
        };

        let arr = match solutions.as_array() {
            Some(a) if !a.is_empty() => a,
            _ => return, // No solutions — do not downgrade to KnownFalse here.
        };

        // Build ground args for the first solution by substituting variable
        // bindings into the template.
        let sol = &arr[0];
        let bindings_map = sol.as_object().cloned().unwrap_or_default();

        let ground_args: Vec<String> = arg_templates.iter().map(|t| {
            if is_prolog_variable(t) {
                bindings_map.get(t)
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| t.clone())
            } else {
                t.clone()
            }
        }).collect();

        let bindings: Vec<Binding> = bindings_map.iter().map(|(k, v)| Binding {
            var: k.clone(),
            val: v.as_str().unwrap_or(&v.to_string()).to_string(),
        }).collect();

        let ground_refs: Vec<&str> = ground_args.iter().map(String::as_str).collect();

        if let Err(e) = self.session.tableau.update_truth(
            self.session.prolog_id,
            &functor,
            &ground_refs,
            TruthValue::KnownTrue,
            &bindings,
        ) {
            log::warn!(
                "re_evaluate_root_goal: tableau update failed for {}: {}",
                functor, e
            );
        } else {
            log::debug!(
                "re_evaluate_root_goal: marked {}({}) KnownTrue — capturing {} solution(s)",
                functor,
                ground_args.join(", "),
                arr.len(),
            );
            // Save re-evaluated solutions so the final DeductionResult reflects
            // the goal state AFTER forward-chaining, not just cycle-0.
            self.final_solutions = Some(solutions.clone());
        }
    }

    /// Extract the final bindings for the root goal from the tableau.
    fn root_goal_bindings(&self, agenda: &GoalAgenda) -> Option<Vec<Binding>> {
        let functor = agenda.root_functor.as_deref()?;
        let entry = self
            .session
            .tableau
            .get_entry(self.session.prolog_id, functor, &["*"])
            .ok()??;
        if entry.bindings.is_empty() {
            None
        } else {
            Some(entry.bindings)
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Flatten a parsed `Term` into a template string.
///
/// Atoms and numbers are rendered literally; variables keep their name (so
/// callers can substitute from solution binding maps).
fn term_to_template_str(term: &crate::transpile::Term) -> String {
    use crate::transpile::Term;
    match term {
        Term::Atom(a)     => a.clone(),
        Term::Variable(v) => v.clone(),
        Term::Integer(i)  => i.to_string(),
        Term::Float(f)    => f.to_string(),
        Term::Str(s)      => s.clone(),
        Term::Compound { functor, args } => {
            let inner = args.iter().map(term_to_template_str).collect::<Vec<_>>().join(",");
            format!("{}({})", functor, inner)
        }
    }
}

/// Returns `true` for Prolog variable names (uppercase first char or `_`).
fn is_prolog_variable(s: &str) -> bool {
    s.starts_with(|c: char| c.is_uppercase() || c == '_')
}

/// Extract the functor name from a goal string like `"launch_missiles"` or
/// `"commie(mary)"`.
fn extract_functor_from_goal(goal: &str) -> String {
    let trimmed = goal.trim();
    if let Some(paren) = trimmed.find('(') {
        trimmed[..paren].to_string()
    } else {
        trimmed.to_string()
    }
}

/// Extract the arity from a goal string.
///
/// Returns 0 for atoms with no argument list, and counts top-level commas + 1
/// inside the first `(…)` for compound terms.
fn extract_arity_from_goal(goal: &str) -> u32 {
    let trimmed = goal.trim();
    let paren = match trimmed.find('(') {
        Some(p) => p,
        None    => return 0,
    };
    // Slice the argument list (strip outer parens).
    let inner = &trimmed[paren + 1..];
    let inner = inner.trim_end_matches(')');
    if inner.trim().is_empty() {
        return 0;
    }
    // Count top-level commas (depth-aware).
    let mut depth: u32 = 0;
    let mut commas: u32 = 0;
    for ch in inner.chars() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => commas += 1,
            _ => {}
        }
    }
    commas + 1
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before epoch")
        .as_millis() as i64
}

// ---------------------------------------------------------------------------
// Phase 3 integration tests — only compiled with the `ritual` feature
// ---------------------------------------------------------------------------

#[cfg(all(test, feature = "ritual"))]
mod ritual_tests {
    use super::*;
    use clara_ritual::{InMemoryBroker, KafkaBridge, RitualConfig, RitualRegistry};

    /// Initialize the global Coire. Silently ignores AlreadyInitialized so
    /// tests sharing the same process can each call this safely.
    fn setup_coire() {
        let _ = clara_coire::init_global();
    }

    fn make_registry() -> (RitualRegistry, std::sync::Arc<InMemoryBroker>) {
        let broker   = std::sync::Arc::new(InMemoryBroker::new());
        let registry = RitualRegistry::new("dis.test", broker.clone());
        (registry, broker)
    }

    fn make_ctrl(
        session: DeductionSession,
        handle:  clara_ritual::RitualHandle,
    ) -> CycleController {
        CycleController::new(session, 10, None, Arc::new(AtomicBool::new(false)))
            .with_ritual(handle)
    }

    // ── ingest_tephra ─────────────────────────────────────────────────────────

    #[test]
    fn ingest_tephra_writes_to_prolog_mailbox() {
        setup_coire();
        let session   = DeductionSession::new().unwrap();
        let prolog_id = session.prolog_id;

        let (registry, _broker) = make_registry();
        let ritual_id = registry
            .create(RitualConfig { name: "t".into(), participants: vec![] })
            .unwrap();
        let mut ctrl = make_ctrl(session, registry.join(ritual_id, None).unwrap());

        let tephra = clara_ritual::TephraEnvelope::new(
            ritual_id,
            Uuid::new_v4(),
            clara_ritual::label::HOHI,
            60_000,
            "dis.peer",
            clara_ritual::TephraPayload::Plaintext {
                body: serde_json::json!({"result": "done"}),
            },
        );

        ctrl.ingest_tephra(&tephra);

        let pending = clara_coire::global().read_pending(prolog_id).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].origin, "ritual/hohi");
        assert_eq!(pending[0].payload, serde_json::json!({"result": "done"}));
    }

    #[test]
    fn encrypted_tephra_skipped_without_panic() {
        setup_coire();
        let session   = DeductionSession::new().unwrap();
        let prolog_id = session.prolog_id;

        let (registry, _) = make_registry();
        let ritual_id = registry
            .create(RitualConfig { name: "t".into(), participants: vec![] })
            .unwrap();
        let mut ctrl = make_ctrl(session, registry.join(ritual_id, None).unwrap());

        let tephra = clara_ritual::TephraEnvelope {
            tephra_id:      Uuid::new_v4(),
            ritual_id,
            performance_id: Uuid::new_v4(),
            label:          clara_ritual::label::OFFERING.to_string(),
            ts_ms:          1_000_000_000,
            ttl_ms:         60_000,
            producer_node:  "dis.peer".to_string(),
            payload: clara_ritual::TephraPayload::Encrypted {
                cipher:     "XChaCha20-Poly1305".into(),
                nonce:      "abc".into(),
                ciphertext: "def".into(),
                aad:        serde_json::json!({}),
            },
        };

        ctrl.ingest_tephra(&tephra);

        let pending = clara_coire::global().read_pending(prolog_id).unwrap();
        assert!(pending.is_empty(), "encrypted tephra should be skipped");
    }

    // ── publish_evaluator_events ──────────────────────────────────────────────

    #[test]
    fn publish_evaluator_events_drains_prefix_and_publishes_tephra() {
        setup_coire();
        let session   = DeductionSession::new().unwrap();
        let prolog_id = session.prolog_id;

        let (registry, broker) = make_registry();
        let ritual_id = registry
            .create(RitualConfig { name: "t".into(), participants: vec![] })
            .unwrap();
        let handle = registry.join(ritual_id, None).unwrap();
        let mut ctrl = make_ctrl(session, handle.clone());

        let coire = clara_coire::global();

        // Evaluator-tagged event — should be drained and published
        coire.write_event(&clara_coire::ClaraEvent::new(
            prolog_id,
            "evaluator/offering",
            serde_json::json!({"goal": "test_goal"}),
        )).unwrap();

        // Non-evaluator event — should be left pending
        coire.write_event(&clara_coire::ClaraEvent::new(
            prolog_id,
            "prolog/assert",
            serde_json::json!({"fact": "mortal(stan)"}),
        )).unwrap();

        ctrl.publish_evaluator_events(&handle);

        // One Tephra should appear on the broker topic
        let topic = clara_ritual::topic_name("dis.test", ritual_id).unwrap();
        let (tephras, _): (Vec<clara_ritual::TephraEnvelope>, _) =
            broker.poll(&topic, 0).unwrap();
        assert_eq!(tephras.len(), 1);
        assert_eq!(tephras[0].label, clara_ritual::label::OFFERING);

        // Only the non-evaluator event should still be pending
        let pending = coire.read_pending(prolog_id).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].origin, "prolog/assert");
    }

    // ── evaluator_pass full round-trip ────────────────────────────────────────

    #[test]
    fn evaluator_pass_full_round_trip() {
        setup_coire();

        let (registry, _broker) = make_registry();
        let ritual_id = registry
            .create(RitualConfig { name: "t".into(), participants: vec![] })
            .unwrap();

        // Controller A — publishes an evaluator event to the Ritual topic
        let session_a   = DeductionSession::new().unwrap();
        let prolog_id_a = session_a.prolog_id;
        let mut ctrl_a  = make_ctrl(session_a, registry.join(ritual_id, None).unwrap());

        // Controller B — receives the Tephra and ingests it into its mailbox
        let session_b   = DeductionSession::new().unwrap();
        let prolog_id_b = session_b.prolog_id;
        let mut ctrl_b  = make_ctrl(session_b, registry.join(ritual_id, None).unwrap());

        let coire = clara_coire::global();

        // Write an evaluator-tagged event to ctrl_a's Prolog mailbox
        coire.write_event(&clara_coire::ClaraEvent::new(
            prolog_id_a,
            "evaluator/offering",
            serde_json::json!({"goal": "peer_eval"}),
        )).unwrap();

        // ctrl_a: publish the event to the broker
        ctrl_a.evaluator_pass_ritual();

        // ctrl_b: poll the broker and ingest into its own Prolog mailbox
        ctrl_b.evaluator_pass_ritual();

        // ctrl_b's mailbox should have one event with a ritual-namespaced origin
        let pending_b = coire.read_pending(prolog_id_b).unwrap();
        assert_eq!(pending_b.len(), 1, "ctrl_b should have received one ingested tephra");
        assert!(
            pending_b[0].origin.starts_with("ritual/"),
            "expected origin 'ritual/...', got '{}'",
            pending_b[0].origin
        );

        // ctrl_a's mailbox should be empty — the event was drained
        let pending_a = coire.read_pending(prolog_id_a).unwrap();
        assert!(pending_a.is_empty(), "ctrl_a's evaluator event should be drained");
    }

    // ── pending_evaluator_responses counter ───────────────────────────────────

    #[test]
    fn publish_offering_increments_pending_counter() {
        setup_coire();
        let (registry, _broker) = make_registry();
        let ritual_id = registry
            .create(RitualConfig { name: "counter-test".into(), participants: vec![] })
            .unwrap();
        let session   = DeductionSession::new().unwrap();
        let prolog_id = session.prolog_id;
        let handle    = registry.join(ritual_id, None).unwrap();
        let mut ctrl  = make_ctrl(session, handle.clone());

        assert_eq!(ctrl.pending_evaluator_responses, 0);

        clara_coire::global().write_event(&clara_coire::ClaraEvent::new(
            prolog_id,
            "evaluator/offering",
            serde_json::json!({"goal": "ask_peer"}),
        )).unwrap();

        ctrl.publish_evaluator_events(&handle);
        assert_eq!(ctrl.pending_evaluator_responses, 1, "one Offering → pending should be 1");
    }

    #[test]
    fn hohi_decrements_pending_counter() {
        setup_coire();
        let (registry, _broker) = make_registry();
        let ritual_id = registry
            .create(RitualConfig { name: "hohi-test".into(), participants: vec![] })
            .unwrap();
        let session  = DeductionSession::new().unwrap();
        let handle   = registry.join(ritual_id, None).unwrap();
        let mut ctrl = make_ctrl(session, handle);

        // Simulate one outstanding Offering.
        ctrl.pending_evaluator_responses = 1;

        let hohi = clara_ritual::TephraEnvelope::new(
            ritual_id,
            Uuid::new_v4(),
            clara_ritual::label::HOHI,
            60_000,
            "dis.peer",
            clara_ritual::TephraPayload::Plaintext {
                body: serde_json::json!({"answer": "yes"}),
            },
        );

        ctrl.ingest_tephra(&hohi);
        assert_eq!(ctrl.pending_evaluator_responses, 0, "Hohi should decrement counter to 0");
    }

    #[test]
    fn pending_counter_saturates_at_zero() {
        setup_coire();
        let (registry, _broker) = make_registry();
        let ritual_id = registry
            .create(RitualConfig { name: "sat-test".into(), participants: vec![] })
            .unwrap();
        let session  = DeductionSession::new().unwrap();
        let handle   = registry.join(ritual_id, None).unwrap();
        let mut ctrl = make_ctrl(session, handle);

        // Counter starts at 0; an unexpected Hohi should not underflow.
        let hohi = clara_ritual::TephraEnvelope::new(
            ritual_id,
            Uuid::new_v4(),
            clara_ritual::label::HOHI,
            60_000,
            "dis.peer",
            clara_ritual::TephraPayload::Plaintext {
                body: serde_json::json!(null),
            },
        );
        ctrl.ingest_tephra(&hohi);
        assert_eq!(ctrl.pending_evaluator_responses, 0, "saturating_sub should not underflow");
    }

    // ── run() integration: full round-trip with mock evaluator ───────────────

    /// End-to-end test for the `evaluator_pass` path inside `CycleController::run()`.
    ///
    /// Exercises the full cycle-loop path that the unit tests leave untested:
    ///
    ///   evaluator/ Coire event
    ///     → `publish_evaluator_events` publishes Offering (pending=1)
    ///     → `has_converged` blocked while pending>0
    ///     → mock evaluator thread sees Offering, responds with Hohi
    ///     → `poll_incoming` receives Hohi
    ///     → `ingest_tephra` decrements pending to 0
    ///     → `has_converged` passes → `CycleStatus::Converged`
    ///
    /// Uses `InMemoryBroker` — no Kafka, no FieryPit required.
    #[test]
    fn run_loop_converges_with_mock_evaluator_hohi() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use clara_ritual::{TephraEnvelope, TephraPayload, topic_name};

        setup_coire();

        // ── ritual setup ──────────────────────────────────────────────────────
        let broker    = Arc::new(InMemoryBroker::new());
        let registry  = RitualRegistry::new("dis.test", broker.clone());
        let ritual_id = registry
            .create(RitualConfig { name: "mock-eval-test".into(), participants: vec![] })
            .unwrap();

        let topic = topic_name("dis.test", ritual_id).unwrap();

        // CycleController handle — consumer offset seeded at latest (0, empty topic).
        let cc_handle = registry.join(ritual_id, Some("cc")).unwrap();

        // ── pre-seed evaluator/ Coire event ──────────────────────────────────
        // Simulates what a CLIPS rule would emit. `publish_evaluator_events`
        // will drain this on cycle 0 and publish it as an Offering.
        let session   = DeductionSession::new().unwrap();
        let prolog_id = session.prolog_id;

        clara_coire::global().write_event(&clara_coire::ClaraEvent::new(
            prolog_id,
            "evaluator/ask-peer",
            serde_json::json!({"query": "is_valid(X)"}),
        )).unwrap();

        // ── mock evaluator thread ─────────────────────────────────────────────
        // Polls the broker directly for Offerings, responds with a Hohi.
        let mock_broker    = broker.clone();
        let mock_topic     = topic.clone();
        let mock_responded = Arc::new(AtomicBool::new(false));
        let mock_flag      = mock_responded.clone();

        let mock_thread = std::thread::spawn(move || {
            let mock_perf_id = Uuid::new_v4();
            let mut offset   = 0i64;

            for _ in 0..200 {
                let (envelopes, next_offset) = mock_broker
                    .poll(&mock_topic, offset)
                    .expect("mock poll failed");
                offset = next_offset;

                for env in &envelopes {
                    if env.label == clara_ritual::label::OFFERING {
                        let hohi = TephraEnvelope::new(
                            ritual_id,
                            mock_perf_id,
                            clara_ritual::label::HOHI,
                            60_000,
                            "mock-evaluator.test",
                            TephraPayload::Plaintext {
                                body: serde_json::json!({"answer": "valid", "echo": &env.payload}),
                            },
                        );
                        mock_broker
                            .publish(&mock_topic, &hohi)
                            .expect("mock publish failed");
                        mock_flag.store(true, Ordering::Relaxed);
                        return;
                    }
                }

                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            // If we get here without finding an Offering the test will fail at
            // the mock_responded assertion below.
        });

        // ── run CycleController ───────────────────────────────────────────────
        let mut ctrl = CycleController::new(
            session,
            50,   // generous max_cycles — expect convergence by cycle 3
            None, // goal defaults to "true" (empty session, no rules needed)
            Arc::new(AtomicBool::new(false)),
        ).with_ritual(cc_handle);

        let result = ctrl.run().expect("run() should converge, not hit max_cycles");

        // ── assertions ────────────────────────────────────────────────────────
        assert_eq!(
            result.status,
            crate::result::CycleStatus::Converged,
            "expected Converged, got {:?}", result.status
        );
        assert!(
            result.cycles >= 2,
            "expected at least 2 cycles (pending counter must have blocked cycle 0 convergence), \
             got {} cycle(s)", result.cycles
        );

        // Confirm the mock evaluator actually fired — the Offering reached it.
        mock_thread.join().expect("mock evaluator thread panicked");
        assert!(
            mock_responded.load(Ordering::Relaxed),
            "mock evaluator never saw an Offering — publish_evaluator_events may not have fired"
        );
    }

    // ── no-ritual guard ───────────────────────────────────────────────────────

    #[test]
    fn evaluator_pass_noop_when_no_handle() {
        setup_coire();
        let session   = DeductionSession::new().unwrap();
        let prolog_id = session.prolog_id;

        // Controller with NO ritual handle attached
        let mut ctrl = CycleController::new(
            session, 10, None, Arc::new(AtomicBool::new(false)),
        );

        let coire = clara_coire::global();
        coire.write_event(&clara_coire::ClaraEvent::new(
            prolog_id,
            "evaluator/offering",
            serde_json::json!({"goal": "test"}),
        )).unwrap();

        ctrl.evaluator_pass();

        // Event should still be pending — evaluator_pass was a no-op
        let pending = coire.read_pending(prolog_id).unwrap();
        assert_eq!(pending.len(), 1);
    }
}
