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
    /// Offerings published to the Ritual topic that have not yet been
    /// answered by a Hohi (or Tabu) from a peer evaluator, keyed by
    /// correlation id.
    ///
    /// Convergence is blocked while this is non-empty so the cycle does not
    /// terminate before peer responses arrive. Entries are removed when
    /// `ingest_tephra` matches a Hohi/Tabu to them, or when their per-offer
    /// patience expires. Max-cycles termination is NOT blocked — the cycle
    /// will still exhaust its budget and return `Err(MaxCyclesExceeded)` if
    /// responses never arrive.
    #[cfg(feature = "ritual")]
    pending_offers: std::collections::HashMap<Uuid, PendingOffer>,
    /// How many cycles a single outstanding Offering may wait before a
    /// synthetic timeout Tabu is injected for it and it is dropped.
    /// Default: 10.
    #[cfg(feature = "ritual")]
    evaluator_patience_cycles: u32,
    /// Synthetic incoming Offering injected into both engine mailboxes as a
    /// `ritual/offering` event before cycle 0 — makes the entry-node Run and
    /// the peer (Kafka-delivered) case look identical to generated auto-pipe
    /// rules. Taken (consumed) by the first `run()`.
    #[cfg(feature = "ritual")]
    initial_offering: Option<InitialOffering>,
    /// Design-time graph node id this deduction acts as. When set, non-reply
    /// Tephras addressed to a *different* node are never ingested into the
    /// mailboxes (mailbox hygiene); `None` keeps the legacy ingest-everything
    /// behavior.
    #[cfg(feature = "ritual")]
    self_node_id: Option<String>,
}

/// A synthetic incoming Offering handed to the controller by the caller
/// (lildaemon's Run endpoint, or a participant qualifying a peer Offering
/// into an inner deduction). Injected as a `ritual/offering` Coire event in
/// both mailboxes before cycle 0.
#[cfg(feature = "ritual")]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InitialOffering {
    /// The clean user payload (e.g. `{"prompt": ...}`).
    pub payload: serde_json::Value,
    /// Logical channel; defaults to "run" when absent.
    #[serde(default)]
    pub topic_path: Option<String>,
    /// Design-time node id of the offerer, when known.
    #[serde(default)]
    pub source_node_id: Option<String>,
    /// Dedup key for the pipe memo, echoed from the incoming envelope by the
    /// caller; a fresh UUID is minted when absent. Pipe rules only fire for
    /// correlated offerings.
    #[serde(default)]
    pub correlation_id: Option<Uuid>,
}

/// Parsed `_caws` transport block from an outbound caws event.
#[cfg(feature = "ritual")]
struct CawsDirective {
    routing: clara_ritual::Routing,
    correlation_id: Uuid,
    /// False for fire-and-forget squawks — no pending offer is registered
    /// and convergence is never blocked on a reply.
    expects_reply: bool,
}

/// A published Offering awaiting its correlated Hohi/Tabu.
#[cfg(feature = "ritual")]
#[derive(Debug)]
struct PendingOffer {
    /// Cycles spent waiting; at `evaluator_patience_cycles` a synthetic
    /// `ritual/tabu-timeout` event carrying this offer's correlation id is
    /// injected and the offer is dropped (timeout-to-false).
    cycles_waiting: u32,
    /// True when the Offering was published with a `_caws` correlation id
    /// the reply is expected to echo. Legacy offers (plain `coire_publish`
    /// with an `evaluator/` origin) are matched by arrival order instead.
    expects_correlation: bool,
    /// Logical channel the Offering was published on, if any.
    topic_path: Option<String>,
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
            pending_offers: std::collections::HashMap::new(),
            #[cfg(feature = "ritual")]
            evaluator_patience_cycles: 10,
            #[cfg(feature = "ritual")]
            initial_offering: None,
            #[cfg(feature = "ritual")]
            self_node_id: None,
        }
    }

    /// Inject a synthetic incoming Offering (as a `ritual/offering` Coire
    /// event in both mailboxes) before cycle 0. See [`InitialOffering`].
    #[cfg(feature = "ritual")]
    pub fn with_initial_offering(mut self, offering: Option<InitialOffering>) -> Self {
        self.initial_offering = offering;
        self
    }

    /// Set the design-time graph node id this deduction acts as; non-reply
    /// Tephras addressed to another node are then skipped by `ingest_tephra`.
    #[cfg(feature = "ritual")]
    pub fn with_self_node_id(mut self, node_id: Option<String>) -> Self {
        self.self_node_id = node_id;
        self
    }

    /// Override the number of consecutive cycles without a peer response
    /// before a synthetic Tabu is asserted.  Default: 10.
    #[cfg(feature = "ritual")]
    pub fn with_evaluator_patience(mut self, patience: u32) -> Self {
        self.evaluator_patience_cycles = patience;
        self
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

        // Surface the caller-supplied incoming Offering (Run query or peer
        // Offering) as a ritual/offering event so generated auto-pipe rules
        // see it exactly like a Kafka-delivered one.
        #[cfg(feature = "ritual")]
        self.inject_initial_offering();

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

            // 3. CLIPS pass — consume Coire events + run inference engine.
            //    Runs before the evaluator pass so that CLIPS rules can emit
            //    evaluator/ events (e.g. peer-evaluation requests) that
            //    publish_evaluator_events then picks up in the same cycle.
            log::debug!("CLIPS pass");
            self.clips_pass()?;
            log::debug!("... CLIPS pass complete");

            // 4. Relay CLIPS → Prolog
            log::debug!("Relay CLIPS → Prolog");
            let rec = self.make_recorder(cycle);
            relay_clips_to_prolog(&mut self.session, rec.as_ref())?;
            log::debug!("... relay prolog complete");

            // 5. Evaluator pass — poll peer Tephras + publish evaluator/ events.
            //    Positioned after CLIPS so CLIPS-emitted evaluator/ events are
            //    visible in the same cycle they are produced.
            log::debug!("Evaluator pass");
            self.evaluator_pass();
            log::debug!("... evaluator pass complete");

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
        let mut ingested_any = false;
        match handle.poll_incoming() {
            Ok(tephras) => {
                ingested_any = !tephras.is_empty();
                for tephra in &tephras {
                    self.ingest_tephra(tephra);
                }
                if ingested_any {
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

        // 3. Pace the wait for outstanding replies. An otherwise-idle cycle
        //    completes in tens of milliseconds, so unpaced patience cycles
        //    would burn through in ~1s — far less than typical Kafka
        //    consumer delivery latency. Sleeping only while offers are
        //    pending (and nothing just arrived) makes patience roughly a
        //    wall-clock budget of ~EVALUATOR_WAIT_MS per cycle without
        //    slowing active flows. Runs on the dedicated blocking thread.
        if !self.pending_offers.is_empty() && !ingested_any {
            const EVALUATOR_WAIT_MS: u64 = 250;
            std::thread::sleep(std::time::Duration::from_millis(EVALUATOR_WAIT_MS));
        }
    }

    /// Unpack a [`TephraEnvelope`] and write its payload as a new [`ClaraEvent`]
    /// into the Prolog Coire mailbox with origin `"ritual/{label}"`.
    ///
    /// A Hohi/Tabu envelope resolves the matching entry in `pending_offers`:
    /// by echoed `correlation_id` when present, otherwise (legacy peers) the
    /// oldest order-matched offer. Responses for a different performance, or
    /// with an unknown correlation id, are logged and dropped — they answer
    /// someone else's Offering. Encrypted payloads are logged and skipped
    /// until Phase 7.
    #[cfg(feature = "ritual")]
    fn ingest_tephra(&mut self, tephra: &clara_ritual::TephraEnvelope) {
        let mut body = match &tephra.payload {
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

        // A Hohi or Tabu means a peer evaluator has responded to an Offering.
        // Both count as "responded" — Tabu is an error response, not silence.
        if tephra.label == clara_ritual::label::HOHI
            || tephra.label == clara_ritual::label::TABU
        {
            if !self.resolve_pending_offer(tephra) {
                return; // not an answer to any of our outstanding Offerings
            }
        } else {
            // Mailbox hygiene for non-reply tephra (Offerings, events):
            // our own published Offerings come back on the shared topic —
            // dropping by performance id prevents auto-pipe rules from
            // triggering on their own output. (Replies are unaffected: peers
            // echo the *offerer's* performance id, which resolve_pending_offer
            // already checks.)
            if let Some(h) = &self.ritual_handle {
                if tephra.performance_id == h.performance_id {
                    log::debug!(
                        "CycleController: ingest_tephra skipping own echo {} (label={})",
                        tephra.tephra_id, tephra.label
                    );
                    return;
                }
            }
            // Tephra addressed to a different graph node is not ours.
            // Unaddressed tephra, or a controller with no self_node_id
            // (legacy callers), keeps the ingest-everything behavior.
            if let (Some(me), Some(target)) = (&self.self_node_id, &tephra.target_node_id) {
                if me != target {
                    log::debug!(
                        "CycleController: ingest_tephra skipping tephra {} addressed \
                         to node {} (we are {})",
                        tephra.tephra_id, target, me
                    );
                    return;
                }
            }
        }

        // Surface routing metadata to rules (e.g. caws_await matching on the
        // echoed correlation id) by merging it into the event payload.
        if let Some(obj) = body.as_object_mut() {
            let mut routing = serde_json::Map::new();
            if let Some(cid) = tephra.correlation_id {
                routing.insert("correlation_id".into(), serde_json::json!(cid.to_string()));
            }
            if let Some(src) = &tephra.source_node_id {
                routing.insert("source_node_id".into(), serde_json::json!(src));
            }
            if let Some(tp) = &tephra.topic_path {
                routing.insert("topic_path".into(), serde_json::json!(tp));
            }
            if !routing.is_empty() {
                obj.insert("_routing".into(), serde_json::Value::Object(routing));
            }
        }

        let coire = clara_coire::global();
        let origin = format!("ritual/{}", tephra.label);

        // Write to Prolog mailbox — consumed via explicit coire_poll/2 in rules.
        let prolog_event = clara_coire::ClaraEvent::new(
            self.session.prolog_id,
            origin.clone(),
            body.clone(),
        );
        match coire.write_event(&prolog_event) {
            Ok(()) => log::debug!(
                "CycleController: ingest_tephra wrote tephra {} (label={}) \
                 to Prolog mailbox (session {}) as event {}",
                tephra.tephra_id, tephra.label, self.session.prolog_id, prolog_event.event_id
            ),
            Err(e) => log::warn!(
                "CycleController: ingest_tephra failed to write tephra {} \
                 to Prolog mailbox: {}",
                tephra.tephra_id, e
            ),
        }

        // Also write to CLIPS mailbox so rules can react to Hohi/Tabu responses.
        // consume_coire_events() will dispatch it as a (coire-event ...) template
        // fact, allowing (defrule receive-hohi-answer ...) to fire.
        let clips_event = clara_coire::ClaraEvent::new(
            self.session.clips_id,
            origin,
            body,
        );
        match coire.write_event(&clips_event) {
            Ok(()) => log::debug!(
                "CycleController: ingest_tephra wrote tephra {} (label={}) \
                 to CLIPS mailbox as event {}",
                tephra.tephra_id, tephra.label, clips_event.event_id
            ),
            Err(e) => log::warn!(
                "CycleController: ingest_tephra failed to write tephra {} \
                 to CLIPS mailbox: {}",
                tephra.tephra_id, e
            ),
        }
    }

    /// Match an incoming Hohi/Tabu to an outstanding Offering in
    /// `pending_offers` and remove that entry.
    ///
    /// Returns `true` when the response resolved one of this controller's
    /// offers (the caller should ingest it into the mailboxes), `false` when
    /// it belongs to a different performance or an unknown correlation id —
    /// i.e. it answers someone else's Offering — and must be dropped.
    #[cfg(feature = "ritual")]
    fn resolve_pending_offer(&mut self, tephra: &clara_ritual::TephraEnvelope) -> bool {
        let own_performance = self
            .ritual_handle
            .as_ref()
            .map(|h| h.performance_id == tephra.performance_id)
            .unwrap_or(true);
        if !own_performance {
            log::debug!(
                "CycleController: ingest_tephra {} for performance {} is not \
                 ours — dropping",
                tephra.label, tephra.performance_id
            );
            return false;
        }

        match tephra.correlation_id {
            Some(cid) => {
                if self.pending_offers.remove(&cid).is_some() {
                    log::debug!(
                        "CycleController: ingest_tephra {} resolved offer {} — \
                         {} still pending",
                        tephra.label, cid, self.pending_offers.len()
                    );
                    true
                } else {
                    log::warn!(
                        "CycleController: ingest_tephra {} with unknown \
                         correlation id {} (late or foreign reply) — dropping",
                        tephra.label, cid
                    );
                    false
                }
            }
            None => {
                // Legacy peer with no correlation echo: resolve the oldest
                // outstanding offer that was published without a correlation
                // id (order-matched, single-outstanding-request semantics).
                let key = self
                    .pending_offers
                    .iter()
                    .filter(|(_, offer)| !offer.expects_correlation)
                    .max_by_key(|(_, offer)| offer.cycles_waiting)
                    .map(|(k, _)| *k);
                match key {
                    Some(k) => {
                        self.pending_offers.remove(&k);
                        log::debug!(
                            "CycleController: ingest_tephra {} (uncorrelated) \
                             resolved legacy offer — {} still pending",
                            tephra.label, self.pending_offers.len()
                        );
                        true
                    }
                    None => {
                        log::warn!(
                            "CycleController: ingest_tephra {} with no \
                             correlation id and no legacy offer outstanding — \
                             dropping",
                            tephra.label
                        );
                        false
                    }
                }
            }
        }
    }

    /// Write a synthetic `ritual/tabu` Coire event indicating that a peer
    /// evaluator timed out (was silent for `evaluator_patience_cycles` cycles
    /// on one Offering).
    ///
    /// The event carries the timed-out offer's correlation id (when it had
    /// one) so rules — e.g. `caws_await/2` — can fail exactly the operation
    /// that timed out (timeout-to-false) while other outstanding consults
    /// keep waiting.
    #[cfg(feature = "ritual")]
    fn assert_evaluator_timeout_tabu(&self, correlation_id: Option<Uuid>) {
        let mut body = serde_json::json!({ "error": "evaluator_timeout" });
        if let Some(cid) = correlation_id {
            body["_routing"] = serde_json::json!({ "correlation_id": cid.to_string() });
        }
        // Both mailboxes: Prolog so caws_await/2 fails the exact consult, and
        // CLIPS so generated edge-*-on-timeout-result dispatch rules can fire.
        for session_id in [self.session.prolog_id, self.session.clips_id] {
            let event = clara_coire::ClaraEvent::new(
                session_id,
                "ritual/tabu-timeout",
                body.clone(),
            );
            match clara_coire::global().write_event(&event) {
                Ok(()) => log::debug!(
                    "CycleController: asserted timeout Tabu as Coire event {} \
                     (session {})",
                    event.event_id, session_id
                ),
                Err(e) => log::warn!(
                    "CycleController: failed to assert timeout Tabu: {}",
                    e
                ),
            }
        }
    }

    /// Write the caller-supplied [`InitialOffering`] into both engine
    /// mailboxes as a `ritual/offering` event (with a `_routing` block), the
    /// same shape `ingest_tephra` produces for a Kafka-delivered Offering.
    /// Consumes `self.initial_offering`; no-op when none was supplied.
    #[cfg(feature = "ritual")]
    fn inject_initial_offering(&mut self) {
        let Some(off) = self.initial_offering.take() else { return };
        let mut body = off.payload;
        let cid = off.correlation_id.unwrap_or_else(Uuid::new_v4);
        if let Some(obj) = body.as_object_mut() {
            let mut routing = serde_json::Map::new();
            routing.insert("correlation_id".into(), serde_json::json!(cid.to_string()));
            routing.insert(
                "topic_path".into(),
                serde_json::json!(off.topic_path.unwrap_or_else(|| "run".to_string())),
            );
            if let Some(src) = off.source_node_id {
                routing.insert("source_node_id".into(), serde_json::json!(src));
            }
            obj.insert("_routing".into(), serde_json::Value::Object(routing));
        }
        for session_id in [self.session.prolog_id, self.session.clips_id] {
            let event = clara_coire::ClaraEvent::new(
                session_id,
                "ritual/offering",
                body.clone(),
            );
            match clara_coire::global().write_event(&event) {
                Ok(()) => log::debug!(
                    "CycleController: injected initial Offering (cid {}) as \
                     Coire event {} (session {})",
                    cid, event.event_id, session_id
                ),
                Err(e) => log::warn!(
                    "CycleController: failed to inject initial Offering: {}",
                    e
                ),
            }
        }
    }

    /// Drain all pending Coire events whose origin starts with `"evaluator/"`
    /// and publish each as a Tephra to the Ritual topic.
    ///
    /// Routing metadata embedded in the event payload's reserved `_caws`
    /// object (`{correlation_id, target_node_id, topic_path, tags}` — written
    /// by `caws_offer/4` and friends) is lifted onto the envelope. Every
    /// published Offering registers a `PendingOffer` so that convergence is
    /// blocked until the matching Hohi/Tabu arrives via `ingest_tephra` (or
    /// its patience expires). Events with other origins are left untouched so
    /// the Prolog and CLIPS passes can consume them normally.
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
            let caws = Self::caws_directive(event, handle);
            let publish_result = match &caws {
                // caws event: publish the clean user payload (minus the
                // `_caws` transport block) so the target evaluator's
                // `_validate_input` sees exactly the authored payload.
                Some(directive) => {
                    let mut body = event.payload.clone();
                    if let Some(obj) = body.as_object_mut() {
                        obj.remove("_caws");
                    }
                    handle.publish_body_routed(
                        body,
                        clara_ritual::label::OFFERING,
                        None,
                        directive.routing.clone(),
                    )
                }
                // Legacy event: whole-ClaraEvent body, broadcast (unchanged
                // pre-caws behavior).
                None => handle.publish_event(event, clara_ritual::label::OFFERING, None),
            };
            match publish_result {
                Ok(()) => {
                    published += 1;
                    let (correlation_id, expects_correlation, expects_reply, topic_path) =
                        match caws {
                            Some(d) => (
                                d.correlation_id,
                                true,
                                d.expects_reply,
                                d.routing.topic_path,
                            ),
                            None => (Uuid::new_v4(), false, true, None),
                        };
                    if expects_reply {
                        self.pending_offers.insert(
                            correlation_id,
                            PendingOffer {
                                cycles_waiting: 0,
                                expects_correlation,
                                topic_path,
                            },
                        );
                    }
                    log::debug!(
                        "CycleController: publish_evaluator_events published event {} \
                         (origin={}, correlation={}) as Tephra",
                        event.event_id, event.origin, correlation_id
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
            log::info!(
                "CycleController: publish_evaluator_events published {} Offering(s) — \
                 {} offer(s) now pending",
                published, self.pending_offers.len()
            );
        }
    }

    /// Extract the routing directive from an outbound event's reserved
    /// `_caws` payload object (written by `caws_offer/4` / `caws_squawk/3`).
    /// `None` for legacy events (plain `coire_publish` with an `evaluator/`
    /// origin) — those keep the pre-caws broadcast behavior.
    #[cfg(feature = "ritual")]
    fn caws_directive(
        event: &clara_coire::ClaraEvent,
        handle: &clara_ritual::RitualHandle,
    ) -> Option<CawsDirective> {
        let caws = event.payload.get("_caws").and_then(|v| v.as_object())?;

        let str_field = |key: &str| {
            caws.get(key)
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        };
        let correlation_id = str_field("correlation_id")
            .and_then(|s| Uuid::parse_str(&s).ok())
            .unwrap_or_else(Uuid::new_v4);
        let topic_path = str_field("topic_path").or_else(|| {
            // Default logical channel: {dis_domain}/ritual/{performance_id}
            Some(format!(
                "{}/ritual/{}",
                handle.dis_domain, handle.performance_id
            ))
        });
        let tags = caws.get("tags").and_then(|v| v.as_array()).map(|arr| {
            arr.iter()
                .filter_map(|t| t.as_str().map(|s| s.to_string()))
                .collect::<Vec<_>>()
        });
        let expects_reply = caws
            .get("expects_reply")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        Some(CawsDirective {
            routing: clara_ritual::Routing {
                source_node_id: str_field("source_node_id"),
                target_node_id: str_field("target_node_id"),
                correlation_id: Some(correlation_id),
                topic_path,
                tags,
            },
            correlation_id,
            expects_reply,
        })
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

        // Block convergence while Offerings are awaiting Hohi/Tabu responses from
        // peer evaluators. This prevents the cycle from declaring a fixed point
        // before peer responses arrive.  Max-cycles termination is unaffected.
        //
        // Patience timeout, per offer: each outstanding Offering ages every
        // cycle; one that reaches `evaluator_patience_cycles` gets a synthetic
        // timeout Tabu carrying its correlation id and is dropped, so exactly
        // that operation fails (timeout-to-false) while others keep waiting.
        #[cfg(feature = "ritual")]
        let any_timed_out = {
            let mut timed_out: Vec<Uuid> = Vec::new();
            for (cid, offer) in self.pending_offers.iter_mut() {
                offer.cycles_waiting += 1;
                if offer.cycles_waiting >= self.evaluator_patience_cycles {
                    timed_out.push(*cid);
                }
            }
            let any = !timed_out.is_empty();
            for cid in timed_out {
                let offer = self.pending_offers.remove(&cid);
                let (expects_correlation, topic_path) = offer
                    .map(|o| (o.expects_correlation, o.topic_path))
                    .unwrap_or((false, None));
                log::warn!(
                    "CycleController: evaluator patience exhausted after {} cycles \
                     for offer {} (topic={}) — asserting timeout Tabu ({} still pending)",
                    self.evaluator_patience_cycles,
                    cid,
                    topic_path.as_deref().unwrap_or("-"),
                    self.pending_offers.len(),
                );
                self.assert_evaluator_timeout_tabu(
                    expects_correlation.then_some(cid),
                );
            }
            any
        };
        #[cfg(not(feature = "ritual"))]
        let any_timed_out = false;
        #[cfg(feature = "ritual")]
        let pending_responses_zero = self.pending_offers.is_empty();
        #[cfg(not(feature = "ritual"))]
        let pending_responses_zero = true;

        // A cycle that just injected a timeout Tabu never converges: the
        // event still has to flow CLIPS→Prolog so dispatch rules (e.g.
        // edge_result(_, tabu, _)) see it before the run ends.
        let converged = mailboxes_empty
            && clips_agenda_empty
            && pending_responses_zero
            && !any_timed_out
            && (tableau_stable || root_resolved);

        #[cfg(feature = "ritual")]
        let pending_count = self.pending_offers.len();
        #[cfg(not(feature = "ritual"))]
        let pending_count: usize = 0;

        log::debug!(
            "CycleController: convergence — prolog_pending={}, clips_pending={}, \
             agenda_empty={}, snapshot_stable={}, tableau_stable={}, root_resolved={}, \
             pending_offers={} → {}",
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

    /// Extract the final bindings for the root goal.
    ///
    /// Prefers bindings captured by `re_evaluate_root_goal` (accurate for any
    /// functor arity) over the wildcard tableau lookup, which only works for
    /// 1-arity predicates due to the fixed `["*"]` key.
    fn root_goal_bindings(&self, agenda: &GoalAgenda) -> Option<Vec<Binding>> {
        // Primary path: bindings from the most recent re-evaluation of the root
        // goal.  `final_solutions` holds the raw Prolog query result as JSON
        // like `[{"Prompt": "hello", "Answer": "chanter_responded"}]`.
        if let Some(solutions) = &self.final_solutions {
            if let Some(arr) = solutions.as_array() {
                if let Some(first) = arr.first() {
                    if let Some(obj) = first.as_object() {
                        let bindings: Vec<Binding> = obj.iter()
                            .filter(|(k, _)| is_prolog_variable(k))
                            .map(|(k, v)| Binding {
                                var: k.clone(),
                                val: v.as_str().unwrap_or(&v.to_string()).to_string(),
                            })
                            .collect();
                        if !bindings.is_empty() {
                            return Some(bindings);
                        }
                    }
                }
            }
        }

        // Fallback: wildcard tableau lookup (works for 0- or 1-arity goals).
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

    /// A legacy pending offer (no correlation) for `ctrl`, keyed internally.
    fn seed_legacy_offer(ctrl: &mut CycleController) -> Uuid {
        let key = Uuid::new_v4();
        ctrl.pending_offers.insert(key, PendingOffer {
            cycles_waiting: 0,
            expects_correlation: false,
            topic_path: None,
        });
        key
    }

    #[test]
    fn ingest_tephra_writes_to_prolog_mailbox() {
        setup_coire();
        let session   = DeductionSession::new().unwrap();
        let prolog_id = session.prolog_id;

        let (registry, _broker) = make_registry();
        let ritual_id = registry
            .create(RitualConfig { name: "t".into(), participants: vec![] })
            .unwrap();
        let handle   = registry.join(ritual_id, None).unwrap();
        let perf_id  = handle.performance_id;
        let mut ctrl = make_ctrl(session, handle);
        seed_legacy_offer(&mut ctrl);

        // A well-behaved peer echoes our performance_id on the response.
        let tephra = clara_ritual::TephraEnvelope::new(
            ritual_id,
            perf_id,
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
            source_node_id: None,
            target_node_id: None,
            correlation_id: None,
            topic_path:     None,
            tags:           None,
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

    // ── pending_offers correlation ────────────────────────────────────────────

    #[test]
    fn publish_offering_registers_pending_offer() {
        setup_coire();
        let (registry, _broker) = make_registry();
        let ritual_id = registry
            .create(RitualConfig { name: "counter-test".into(), participants: vec![] })
            .unwrap();
        let session   = DeductionSession::new().unwrap();
        let prolog_id = session.prolog_id;
        let handle    = registry.join(ritual_id, None).unwrap();
        let mut ctrl  = make_ctrl(session, handle.clone());

        assert!(ctrl.pending_offers.is_empty());

        clara_coire::global().write_event(&clara_coire::ClaraEvent::new(
            prolog_id,
            "evaluator/offering",
            serde_json::json!({"goal": "ask_peer"}),
        )).unwrap();

        ctrl.publish_evaluator_events(&handle);
        assert_eq!(ctrl.pending_offers.len(), 1, "one Offering → one pending offer");
        let offer = ctrl.pending_offers.values().next().unwrap();
        assert!(
            !offer.expects_correlation,
            "a plain (no _caws) event is a legacy, order-matched offer"
        );
    }

    #[test]
    fn publish_offering_with_caws_stamps_envelope_routing() {
        setup_coire();
        let (registry, broker) = make_registry();
        let ritual_id = registry
            .create(RitualConfig { name: "caws-test".into(), participants: vec![] })
            .unwrap();
        let session   = DeductionSession::new().unwrap();
        let prolog_id = session.prolog_id;
        let handle    = registry.join(ritual_id, None).unwrap();
        let mut ctrl  = make_ctrl(session, handle.clone());

        let cid = Uuid::new_v4();
        clara_coire::global().write_event(&clara_coire::ClaraEvent::new(
            prolog_id,
            "evaluator/offering",
            serde_json::json!({
                "prompt": "hello",
                "_caws": {
                    "correlation_id": cid.to_string(),
                    "target_node_id": "n2",
                    "topic_path": "dis.test/ritual/p/consults/e1",
                    "tags": ["urgent"],
                },
            }),
        )).unwrap();

        ctrl.publish_evaluator_events(&handle);

        assert_eq!(ctrl.pending_offers.len(), 1);
        assert!(ctrl.pending_offers.contains_key(&cid), "offer keyed by _caws correlation id");
        assert!(ctrl.pending_offers[&cid].expects_correlation);

        let topic = clara_ritual::topic_name("dis.test", ritual_id).unwrap();
        let (envelopes, _) = broker.poll(&topic, 0).unwrap();
        assert_eq!(envelopes.len(), 1);
        let env = &envelopes[0];
        assert_eq!(env.correlation_id, Some(cid));
        assert_eq!(env.target_node_id.as_deref(), Some("n2"));
        assert_eq!(env.topic_path.as_deref(), Some("dis.test/ritual/p/consults/e1"));
        assert_eq!(env.tags, Some(vec!["urgent".to_string()]));
    }

    #[test]
    fn correlated_hohi_resolves_exactly_its_offer() {
        setup_coire();
        let (registry, _broker) = make_registry();
        let ritual_id = registry
            .create(RitualConfig { name: "hohi-test".into(), participants: vec![] })
            .unwrap();
        let session  = DeductionSession::new().unwrap();
        let handle   = registry.join(ritual_id, None).unwrap();
        let perf_id  = handle.performance_id;
        let mut ctrl = make_ctrl(session, handle);

        let cid_a = Uuid::new_v4();
        let cid_b = Uuid::new_v4();
        for cid in [cid_a, cid_b] {
            ctrl.pending_offers.insert(cid, PendingOffer {
                cycles_waiting: 0,
                expects_correlation: true,
                topic_path: None,
            });
        }

        let hohi = clara_ritual::TephraEnvelope::new(
            ritual_id,
            perf_id,
            clara_ritual::label::HOHI,
            60_000,
            "dis.peer",
            clara_ritual::TephraPayload::Plaintext {
                body: serde_json::json!({"answer": "yes"}),
            },
        )
        .with_routing(clara_ritual::Routing {
            correlation_id: Some(cid_a),
            ..Default::default()
        });

        ctrl.ingest_tephra(&hohi);
        assert!(!ctrl.pending_offers.contains_key(&cid_a), "answered offer removed");
        assert!(ctrl.pending_offers.contains_key(&cid_b), "other offer still pending");
    }

    #[test]
    fn hohi_with_unknown_correlation_is_dropped() {
        setup_coire();
        let (registry, _broker) = make_registry();
        let ritual_id = registry
            .create(RitualConfig { name: "unknown-cid".into(), participants: vec![] })
            .unwrap();
        let session   = DeductionSession::new().unwrap();
        let prolog_id = session.prolog_id;
        let handle    = registry.join(ritual_id, None).unwrap();
        let perf_id   = handle.performance_id;
        let mut ctrl  = make_ctrl(session, handle);
        let kept = seed_legacy_offer(&mut ctrl);

        let hohi = clara_ritual::TephraEnvelope::new(
            ritual_id,
            perf_id,
            clara_ritual::label::HOHI,
            60_000,
            "dis.peer",
            clara_ritual::TephraPayload::Plaintext { body: serde_json::json!(null) },
        )
        .with_routing(clara_ritual::Routing {
            correlation_id: Some(Uuid::new_v4()), // nobody's offer
            ..Default::default()
        });

        ctrl.ingest_tephra(&hohi);
        assert!(ctrl.pending_offers.contains_key(&kept), "unrelated offer untouched");
        let pending = clara_coire::global().read_pending(prolog_id).unwrap();
        assert!(pending.is_empty(), "dropped response must not reach the mailbox");
    }

    #[test]
    fn hohi_for_other_performance_is_dropped() {
        setup_coire();
        let (registry, _broker) = make_registry();
        let ritual_id = registry
            .create(RitualConfig { name: "other-perf".into(), participants: vec![] })
            .unwrap();
        let session   = DeductionSession::new().unwrap();
        let prolog_id = session.prolog_id;
        let mut ctrl  = make_ctrl(session, registry.join(ritual_id, None).unwrap());
        let kept = seed_legacy_offer(&mut ctrl);

        let hohi = clara_ritual::TephraEnvelope::new(
            ritual_id,
            Uuid::new_v4(), // a different performance's response
            clara_ritual::label::HOHI,
            60_000,
            "dis.peer",
            clara_ritual::TephraPayload::Plaintext { body: serde_json::json!(null) },
        );

        ctrl.ingest_tephra(&hohi);
        assert!(ctrl.pending_offers.contains_key(&kept), "our offer stays pending");
        let pending = clara_coire::global().read_pending(prolog_id).unwrap();
        assert!(pending.is_empty(), "foreign response must not reach the mailbox");
    }

    #[test]
    fn unexpected_hohi_with_no_offers_is_dropped() {
        setup_coire();
        let (registry, _broker) = make_registry();
        let ritual_id = registry
            .create(RitualConfig { name: "sat-test".into(), participants: vec![] })
            .unwrap();
        let session   = DeductionSession::new().unwrap();
        let prolog_id = session.prolog_id;
        let handle    = registry.join(ritual_id, None).unwrap();
        let perf_id   = handle.performance_id;
        let mut ctrl  = make_ctrl(session, handle);

        let hohi = clara_ritual::TephraEnvelope::new(
            ritual_id,
            perf_id,
            clara_ritual::label::HOHI,
            60_000,
            "dis.peer",
            clara_ritual::TephraPayload::Plaintext { body: serde_json::json!(null) },
        );
        ctrl.ingest_tephra(&hohi);
        assert!(ctrl.pending_offers.is_empty());
        let pending = clara_coire::global().read_pending(prolog_id).unwrap();
        assert!(pending.is_empty(), "no offer outstanding → response dropped");
    }

    #[test]
    fn per_offer_patience_times_out_with_correlation_id() {
        setup_coire();
        let (registry, _broker) = make_registry();
        let ritual_id = registry
            .create(RitualConfig { name: "patience".into(), participants: vec![] })
            .unwrap();
        let session   = DeductionSession::new().unwrap();
        let prolog_id = session.prolog_id;
        let mut ctrl  = make_ctrl(session, registry.join(ritual_id, None).unwrap())
            .with_evaluator_patience(2);

        let cid = Uuid::new_v4();
        ctrl.pending_offers.insert(cid, PendingOffer {
            cycles_waiting: 0,
            expects_correlation: true,
            topic_path: None,
        });

        let agenda = GoalAgenda::new(&None);
        let snap   = ctrl.snapshot();
        // Cycle 1: not yet timed out; convergence blocked by the pending offer.
        assert!(!ctrl.has_converged(&snap.clone(), &snap.clone(), &agenda));
        assert_eq!(ctrl.pending_offers[&cid].cycles_waiting, 1);
        // Cycle 2: patience (2) reached — offer dropped, timeout Tabu injected.
        ctrl.has_converged(&snap.clone(), &snap.clone(), &agenda);
        assert!(ctrl.pending_offers.is_empty(), "timed-out offer removed");

        let pending = clara_coire::global().read_pending(prolog_id).unwrap();
        let timeout_events: Vec<_> = pending
            .iter()
            .filter(|e| e.origin == "ritual/tabu-timeout")
            .collect();
        assert_eq!(timeout_events.len(), 1);
        assert_eq!(
            timeout_events[0].payload["_routing"]["correlation_id"],
            serde_json::json!(cid.to_string()),
            "timeout Tabu must carry the timed-out offer's correlation id"
        );
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
            let mut offset = 0i64;

            // Generous budget (~10s): under full-suite parallelism, engine
            // startup can delay cycle 0's publish well past the poll start.
            for _ in 0..2000 {
                let (envelopes, next_offset) = mock_broker
                    .poll(&mock_topic, offset)
                    .expect("mock poll failed");
                offset = next_offset;

                for env in &envelopes {
                    if env.label == clara_ritual::label::OFFERING {
                        // Echo the Offering's performance_id, as a real
                        // RitualParticipant does — required by the
                        // correlation filter in ingest_tephra.
                        let hohi = TephraEnvelope::new(
                            ritual_id,
                            env.performance_id,
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

                std::thread::sleep(std::time::Duration::from_millis(5));
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

    // ── Phase 6 e2e: CLIPS rule → evaluator/ → Kafka → Hohi → CLIPS → Prolog ──

    /// End-to-end test for the full ritual peer-evaluation round-trip driven by
    /// CLIPS forward-chaining rules.
    ///
    /// Cycle trace (CLIPS before evaluator pass):
    ///
    ///   Cycle 0
    ///     Prolog: peer_answered(hello, A) fails — no answered/2 yet
    ///     CLIPS:  (need-peer-eval "hello") → ask-peer-chanter fires
    ///             → coire-emit to Prolog mailbox: "evaluator/ask-chanter"
    ///     Evaluator: publish_evaluator_events drains "evaluator/ask-chanter"
    ///                → Offering on topic, pending = 1
    ///     Convergence: pending > 0 → NOT CONVERGED
    ///
    ///   Cycle 1  (mock evaluator responds with Hohi)
    ///     Evaluator: ingest_tephra → pending = 0
    ///                dual-writes ritual/hohi to Prolog + CLIPS mailboxes
    ///     Convergence: clips_pending = 1 → NOT CONVERGED
    ///
    ///   Cycle 2
    ///     CLIPS: consume_coire_events dispatches (coire-event origin=ritual/hohi)
    ///            receive-hohi-answer fires → coire-publish-assert answered(hello,chanter_responded)
    ///     Relay C→P: answered/2 asserted in Prolog mailbox
    ///     Convergence: prolog_pending = 1 → NOT CONVERGED
    ///
    ///   Cycle 3
    ///     Prolog: consume relay event → assertz(answered(hello, chanter_responded))
    ///             peer_answered(hello, A) succeeds! A = chanter_responded
    ///     Convergence: mailboxes empty, pending = 0, root resolved → CONVERGED
    ///
    /// Uses InMemoryBroker — no Kafka or lildaemon required.
    #[test]
    fn run_loop_ritual_chanter_e2e() {
        use clara_ritual::{TephraEnvelope, TephraPayload, topic_name};
        use std::sync::atomic::{AtomicBool, Ordering};

        setup_coire();

        // ── ritual setup ──────────────────────────────────────────────────────
        let broker    = Arc::new(InMemoryBroker::new());
        let registry  = RitualRegistry::new("dis.test", broker.clone());
        let ritual_id = registry
            .create(RitualConfig { name: "chanter-e2e".into(), participants: vec![] })
            .unwrap();
        let topic     = topic_name("dis.test", ritual_id).unwrap();
        let cc_handle = registry.join(ritual_id, Some("cc")).unwrap();

        // ── session setup — seed Prolog + CLIPS KBs ───────────────────────────
        let mut session = DeductionSession::new().unwrap();

        // Prolog: peer_answered/2 depends on answered/2 (asserted later by CLIPS)
        session.seed_prolog(&[
            ":- use_module(library(the_coire)).".into(),
            ":- dynamic answered/2.".into(),
            "peer_answered(Prompt, Answer) :- answered(Prompt, Answer).".into(),
        ]).expect("seed_prolog failed");

        // CLIPS: load the two ritual rules from the test resource file
        session.seed_clips_file(
            "tests/resources/ritual_chanter_test_clara.clp"
        ).expect("seed_clips_file failed");

        // Seed initial CLIPS working-memory fact that triggers ask-peer-chanter
        session.clips.eval("(assert (need-peer-eval \"hello\"))").expect("WM seed failed");

        // ── mock evaluator thread ─────────────────────────────────────────────
        // Polls the broker for an Offering and responds with a Hohi.
        let mock_broker    = broker.clone();
        let mock_topic     = topic.clone();
        let mock_responded = Arc::new(AtomicBool::new(false));
        let mock_flag      = mock_responded.clone();

        let mock_thread = std::thread::spawn(move || {
            let mut offset = 0i64;

            // Generous budget (~10s) — see run_loop_converges_with_mock_evaluator_hohi.
            for _ in 0..2000 {
                let (envelopes, next_offset) = mock_broker
                    .poll(&mock_topic, offset)
                    .expect("mock poll failed");
                offset = next_offset;

                for env in &envelopes {
                    if env.label == clara_ritual::label::OFFERING {
                        // Echo the Offering's performance_id, as a real
                        // RitualParticipant does — required by the
                        // correlation filter in ingest_tephra.
                        let hohi = TephraEnvelope::new(
                            ritual_id,
                            env.performance_id,
                            clara_ritual::label::HOHI,
                            60_000,
                            "chanter.test",
                            TephraPayload::Plaintext {
                                body: serde_json::json!({
                                    "responder": "chanter",
                                    "data": { "prompt": "hello" }
                                }),
                            },
                        );
                        mock_broker
                            .publish(&mock_topic, &hohi)
                            .expect("mock publish failed");
                        mock_flag.store(true, Ordering::Relaxed);
                        return;
                    }
                }

                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        });

        // ── run CycleController ───────────────────────────────────────────────
        let mut ctrl = CycleController::new(
            session,
            50,
            Some("peer_answered(hello, Answer)".into()),
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
            result.cycles >= 3,
            "expected at least 3 cycles (CLIPS→evaluator→Hohi→relay→Prolog path), \
             got {} cycle(s)", result.cycles
        );

        // The mock evaluator must have seen the Offering.
        mock_thread.join().expect("mock evaluator thread panicked");
        assert!(
            mock_responded.load(Ordering::Relaxed),
            "mock evaluator never received an Offering"
        );

        // The Kafka topic must have exactly one Offering and one Hohi.
        let (all_msgs, _) = broker.poll(&topic, 0).expect("final broker poll failed");
        let offerings: Vec<_> = all_msgs.iter()
            .filter(|e| e.label == clara_ritual::label::OFFERING)
            .collect();
        let hohis: Vec<_> = all_msgs.iter()
            .filter(|e| e.label == clara_ritual::label::HOHI)
            .collect();
        assert_eq!(offerings.len(), 1, "expected exactly 1 Offering on topic");
        assert_eq!(hohis.len(),    1, "expected exactly 1 Hohi on topic");

        // The root goal must be KnownTrue with Answer bound to chanter_responded.
        let goal_bindings = result.goal_bindings
            .expect("goal_bindings should be present after convergence");
        let answer = goal_bindings.iter()
            .find(|b| b.var == "Answer")
            .map(|b| b.val.as_str())
            .expect("Answer binding missing from goal_bindings");
        assert_eq!(answer, "chanter_responded",
            "expected Answer = chanter_responded, got {:?}", answer);
    }

    // ── caws typed-edge round trip (docs/deduction_redux.md) ──────────────────

    /// Full caws_consult/4 round trip: Prolog publishes an addressed,
    /// correlated Offering; a mock peer replies with a Hohi echoing the
    /// correlation id; caws_await resolves it and the goal converges with
    /// the peer's answer bound.
    #[test]
    fn run_loop_caws_consult_round_trip() {
        use clara_ritual::{Routing, TephraEnvelope, TephraPayload, topic_name};
        use std::sync::atomic::{AtomicBool, Ordering};

        setup_coire();

        let broker    = Arc::new(InMemoryBroker::new());
        let registry  = RitualRegistry::new("dis.test", broker.clone());
        let ritual_id = registry
            .create(RitualConfig { name: "caws-round-trip".into(), participants: vec![] })
            .unwrap();
        let topic     = topic_name("dis.test", ritual_id).unwrap();
        let cc_handle = registry.join(ritual_id, Some("cc")).unwrap();

        let mut session = DeductionSession::new().unwrap();
        session.seed_prolog(&[
            ":- use_module(library(the_coire)).".into(),
            "peer_answer(Q, A) :- \
                caws_consult(n2, 'dis.test/consults/e1', _{prompt: Q}, R), \
                get_dict(response, R, A).".into(),
        ]).expect("seed_prolog failed");

        // Mock peer: consumes the addressed Offering, echoes correlation id.
        let mock_broker    = broker.clone();
        let mock_topic     = topic.clone();
        let mock_responded = Arc::new(AtomicBool::new(false));
        let mock_flag      = mock_responded.clone();

        let mock_thread = std::thread::spawn(move || {
            let mut offset = 0i64;
            for _ in 0..2000 {
                let (envelopes, next_offset) =
                    mock_broker.poll(&mock_topic, offset).expect("mock poll failed");
                offset = next_offset;
                for env in &envelopes {
                    if env.label == clara_ritual::label::OFFERING {
                        // The published body must be the clean user payload —
                        // {"prompt": ...} — with routing on the envelope.
                        let body = match &env.payload {
                            TephraPayload::Plaintext { body } => body.clone(),
                            _ => panic!("unexpected payload"),
                        };
                        assert_eq!(
                            body.get("prompt").and_then(|v| v.as_str()),
                            Some("hello"),
                            "caws Offering body must be the raw payload dict, got {body}"
                        );
                        assert!(body.get("_caws").is_none(), "_caws must be stripped");
                        assert_eq!(env.target_node_id.as_deref(), Some("n2"));
                        assert_eq!(env.topic_path.as_deref(), Some("dis.test/consults/e1"));
                        let cid = env.correlation_id.expect("Offering must carry correlation id");

                        let hohi = TephraEnvelope::new(
                            ritual_id,
                            env.performance_id,
                            clara_ritual::label::HOHI,
                            60_000,
                            "mock-groq.test",
                            TephraPayload::Plaintext {
                                body: serde_json::json!({"response": "forty_two"}),
                            },
                        )
                        .with_routing(Routing {
                            correlation_id: Some(cid),
                            source_node_id: Some("n2".into()),
                            ..Default::default()
                        });
                        mock_broker.publish(&mock_topic, &hohi).expect("mock publish failed");
                        mock_flag.store(true, Ordering::Relaxed);
                        return;
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        });

        let mut ctrl = CycleController::new(
            session,
            50,
            Some("peer_answer(hello, Answer)".into()),
            Arc::new(AtomicBool::new(false)),
        ).with_ritual(cc_handle);

        let result = ctrl.run().expect("run() should converge");
        mock_thread.join().expect("mock peer thread panicked");
        assert!(mock_responded.load(Ordering::Relaxed), "mock peer never saw the Offering");

        assert_eq!(result.status, crate::result::CycleStatus::Converged);
        assert!(ctrl.pending_offers.is_empty(), "resolved offer must be cleared");

        let bindings = serde_json::to_string(&result.goal_bindings).unwrap_or_default();
        let solutions = serde_json::to_string(&result.prolog_solutions).unwrap_or_default();
        assert!(
            bindings.contains("forty_two") || solutions.contains("forty_two"),
            "peer answer must reach the goal bindings; bindings={bindings} solutions={solutions}"
        );

        // Exactly one Offering was published — the memoized re-query must
        // not have re-offered.
        let (all_msgs, _) = broker.poll(&topic, 0).expect("final poll failed");
        let offerings = all_msgs.iter()
            .filter(|e| e.label == clara_ritual::label::OFFERING)
            .count();
        assert_eq!(offerings, 1, "caws_offer must be idempotent across goal re-runs");
    }

    /// caws_await with a silent peer: the per-offer patience timeout injects
    /// a correlated timeout Tabu and the awaiting goal fails —
    /// timeout-to-false — letting the cycle converge instead of hanging.
    #[test]
    fn caws_await_times_out_to_false() {
        use std::sync::atomic::AtomicBool;

        setup_coire();

        let broker    = Arc::new(InMemoryBroker::new());
        let registry  = RitualRegistry::new("dis.test", broker.clone());
        let ritual_id = registry
            .create(RitualConfig { name: "caws-timeout".into(), participants: vec![] })
            .unwrap();
        let cc_handle = registry.join(ritual_id, Some("cc")).unwrap();

        let mut session = DeductionSession::new().unwrap();
        session.seed_prolog(&[
            ":- use_module(library(the_coire)).".into(),
            "peer_answer(Q, A) :- \
                caws_consult(n2, 'dis.test/consults/e1', _{prompt: Q}, R), \
                get_dict(response, R, A).".into(),
        ]).expect("seed_prolog failed");

        // Nobody answers.
        let mut ctrl = CycleController::new(
            session,
            30,
            Some("peer_answer(hello, Answer)".into()),
            Arc::new(AtomicBool::new(false)),
        )
        .with_ritual(cc_handle)
        .with_evaluator_patience(2);

        let result = ctrl.run().expect("run() should converge via timeout, not hang");
        assert_eq!(result.status, crate::result::CycleStatus::Converged);
        assert!(ctrl.pending_offers.is_empty(), "timed-out offer must be cleared");

        // The goal must have failed — no solutions carrying an answer.
        let solutions = serde_json::to_string(&result.prolog_solutions).unwrap_or_default();
        assert!(
            !solutions.contains("forty_two"),
            "silent peer must not produce an answer; solutions={solutions}"
        );
    }

    // ── initial offering & mailbox hygiene (typed-edge auto-pipe) ─────────────

    #[test]
    fn initial_offering_lands_in_both_mailboxes_with_routing() {
        setup_coire();
        let session   = DeductionSession::new().unwrap();
        let prolog_id = session.prolog_id;
        let clips_id  = session.clips_id;

        let (registry, _broker) = make_registry();
        let ritual_id = registry
            .create(RitualConfig { name: "t".into(), participants: vec![] })
            .unwrap();
        let cid = Uuid::new_v4();
        let mut ctrl = make_ctrl(session, registry.join(ritual_id, None).unwrap())
            .with_self_node_id(Some("n1".into()))
            .with_initial_offering(Some(InitialOffering {
                payload:        serde_json::json!({"prompt": "hello"}),
                topic_path:     None, // must default to "run"
                source_node_id: Some("user".into()),
                correlation_id: Some(cid),
            }));

        ctrl.inject_initial_offering();

        let coire = clara_coire::global();
        for (name, id) in [("prolog", prolog_id), ("clips", clips_id)] {
            let pending = coire.read_pending(id).unwrap();
            assert_eq!(pending.len(), 1, "{name} mailbox should hold the offering");
            assert_eq!(pending[0].origin, "ritual/offering");
            let routing = pending[0].payload.get("_routing")
                .unwrap_or_else(|| panic!("{name} event missing _routing"));
            assert_eq!(
                routing.get("correlation_id").and_then(|v| v.as_str()),
                Some(cid.to_string().as_str()),
            );
            assert_eq!(routing.get("topic_path").and_then(|v| v.as_str()), Some("run"));
            assert_eq!(routing.get("source_node_id").and_then(|v| v.as_str()), Some("user"));
            assert_eq!(
                pending[0].payload.get("prompt").and_then(|v| v.as_str()),
                Some("hello"),
            );
        }

        // Consumed on injection — a second call must be a no-op.
        ctrl.inject_initial_offering();
        assert_eq!(coire.read_pending(prolog_id).unwrap().len(), 1);
    }

    #[test]
    fn ingest_tephra_skips_offering_addressed_elsewhere() {
        use clara_ritual::{Routing, TephraEnvelope, TephraPayload};

        setup_coire();
        let session   = DeductionSession::new().unwrap();
        let prolog_id = session.prolog_id;

        let (registry, _broker) = make_registry();
        let ritual_id = registry
            .create(RitualConfig { name: "t".into(), participants: vec![] })
            .unwrap();
        let mut ctrl = make_ctrl(session, registry.join(ritual_id, None).unwrap())
            .with_self_node_id(Some("n1".into()));

        let offering = |target: Option<&str>| {
            let env = TephraEnvelope::new(
                ritual_id,
                Uuid::new_v4(), // a peer's performance, not ours
                clara_ritual::label::OFFERING,
                60_000,
                "dis.peer",
                TephraPayload::Plaintext { body: serde_json::json!({"prompt": "q"}) },
            );
            env.with_routing(Routing {
                target_node_id: target.map(str::to_string),
                correlation_id: Some(Uuid::new_v4()),
                ..Default::default()
            })
        };

        let coire = clara_coire::global();

        // Addressed to another node — dropped.
        ctrl.ingest_tephra(&offering(Some("n2")));
        assert!(coire.read_pending(prolog_id).unwrap().is_empty(),
            "offering addressed to n2 must not be ingested by n1");

        // Addressed to us — ingested.
        ctrl.ingest_tephra(&offering(Some("n1")));
        assert_eq!(coire.read_pending(prolog_id).unwrap().len(), 1);

        // Unaddressed — always ingested.
        ctrl.ingest_tephra(&offering(None));
        assert_eq!(coire.read_pending(prolog_id).unwrap().len(), 2);
    }

    #[test]
    fn ingest_tephra_suppresses_own_performance_offering() {
        use clara_ritual::{Routing, TephraEnvelope, TephraPayload};

        setup_coire();
        let session   = DeductionSession::new().unwrap();
        let prolog_id = session.prolog_id;

        let (registry, _broker) = make_registry();
        let ritual_id = registry
            .create(RitualConfig { name: "t".into(), participants: vec![] })
            .unwrap();
        let handle  = registry.join(ritual_id, None).unwrap();
        let perf_id = handle.performance_id;
        // No self_node_id — echo suppression must not depend on it.
        let mut ctrl = make_ctrl(session, handle);

        // Our own published Offering coming back off the shared topic.
        let echo = TephraEnvelope::new(
            ritual_id,
            perf_id,
            clara_ritual::label::OFFERING,
            60_000,
            "dis.test",
            TephraPayload::Plaintext { body: serde_json::json!({"prompt": "q"}) },
        )
        .with_routing(Routing {
            correlation_id: Some(Uuid::new_v4()),
            target_node_id: Some("n2".into()),
            ..Default::default()
        });

        ctrl.ingest_tephra(&echo);
        assert!(
            clara_coire::global().read_pending(prolog_id).unwrap().is_empty(),
            "own-performance offering echo must be dropped (auto-pipe loop guard)"
        );
    }

    #[test]
    fn timeout_tabu_written_to_both_mailboxes() {
        setup_coire();
        let session   = DeductionSession::new().unwrap();
        let prolog_id = session.prolog_id;
        let clips_id  = session.clips_id;

        let (registry, _broker) = make_registry();
        let ritual_id = registry
            .create(RitualConfig { name: "t".into(), participants: vec![] })
            .unwrap();
        let ctrl = make_ctrl(session, registry.join(ritual_id, None).unwrap());

        let cid = Uuid::new_v4();
        ctrl.assert_evaluator_timeout_tabu(Some(cid));

        let coire = clara_coire::global();
        for (name, id) in [("prolog", prolog_id), ("clips", clips_id)] {
            let pending = coire.read_pending(id).unwrap();
            assert_eq!(pending.len(), 1, "{name} mailbox should hold the timeout Tabu");
            assert_eq!(pending[0].origin, "ritual/tabu-timeout");
            assert_eq!(
                pending[0].payload.pointer("/_routing/correlation_id").and_then(|v| v.as_str()),
                Some(cid.to_string().as_str()),
                "{name} timeout event must carry the timed-out correlation id"
            );
        }
    }

    // ── auto-pipe round trips (generated caws_auto_pipe_* / caws_edge_reply) ──

    /// Seed the exact Prolog/CLIPS snippets `transduce_graph` generates for an
    /// auto offering edge e1 (n1 → n2), plus an authored root goal that
    /// consumes `edge_result/3`.
    fn seed_auto_pipe_edge(session: &mut DeductionSession, root_clause: &str) {
        session.seed_prolog(&[
            ":- use_module(library(the_coire)).".into(),
            // generated: auto-pipe wrapper
            "caws_auto_pipe_e1(Cid) :- caws_pipe('e1', 'n2', 'consults/e1', Cid).".into(),
            "caws_auto_pipe_e1(_).".into(),
            // authored: root goal over the dispatched edge result
            root_clause.into(),
        ]).expect("seed_prolog failed");

        // generated: pipe + typed reply dispatch rules
        session.seed_clips(&[
            "(defrule edge-e1-auto-pipe\n    \
                (coire-event (origin \"ritual/offering\") (correlation ?cid&~\"\"))\n    \
                =>\n    \
                (coire-publish-goal (str-cat \"caws_auto_pipe_e1('\" ?cid \"')\")))".into(),
            "(defrule edge-e1-on-hohi-result\n    \
                (coire-event (origin \"ritual/hohi\") (topic \"consults/e1\") (correlation ?cid&~\"\"))\n    \
                =>\n    \
                (coire-publish-goal (str-cat \"caws_edge_reply('e1', hohi, '\" ?cid \"')\")))".into(),
            "(defrule edge-e1-on-tabu-result\n    \
                (coire-event (origin \"ritual/tabu\") (topic \"consults/e1\") (correlation ?cid&~\"\"))\n    \
                =>\n    \
                (coire-publish-goal (str-cat \"caws_edge_reply('e1', tabu, '\" ?cid \"')\")))".into(),
            "(defrule edge-e1-on-timeout-result\n    \
                (coire-event (origin \"ritual/tabu-timeout\") (correlation ?cid&~\"\"))\n    \
                =>\n    \
                (coire-publish-goal (str-cat \"caws_edge_reply('e1', tabu_timeout, '\" ?cid \"')\")))".into(),
        ]).expect("seed_clips failed");
    }

    /// Keystone: an InitialOffering triggers the generated auto-pipe rule,
    /// the Offering is published addressed to n2, a mock peer replies with a
    /// correlated Hohi, the dispatch rule asserts `edge_result(e1, hohi, R)`,
    /// and the re-proved root goal converges with the peer's answer bound.
    #[test]
    fn run_loop_auto_pipe_round_trip() {
        use clara_ritual::{Routing, TephraEnvelope, TephraPayload, topic_name};
        use std::sync::atomic::{AtomicBool, Ordering};

        setup_coire();

        let broker    = Arc::new(InMemoryBroker::new());
        let registry  = RitualRegistry::new("dis.test", broker.clone());
        let ritual_id = registry
            .create(RitualConfig { name: "auto-pipe-round-trip".into(), participants: vec![] })
            .unwrap();
        let topic     = topic_name("dis.test", ritual_id).unwrap();
        let cc_handle = registry.join(ritual_id, Some("cc")).unwrap();

        let mut session = DeductionSession::new().unwrap();
        seed_auto_pipe_edge(
            &mut session,
            "reasoned(A) :- edge_result(e1, hohi, R), get_dict(response, R, A).",
        );

        // Mock peer: consumes the piped Offering, replies with a correlated
        // Hohi echoing topic_path (as RitualParticipant does).
        let mock_broker    = broker.clone();
        let mock_topic     = topic.clone();
        let mock_responded = Arc::new(AtomicBool::new(false));
        let mock_flag      = mock_responded.clone();

        let mock_thread = std::thread::spawn(move || {
            let mut offset = 0i64;
            for _ in 0..2000 {
                let (envelopes, next_offset) =
                    mock_broker.poll(&mock_topic, offset).expect("mock poll failed");
                offset = next_offset;
                for env in &envelopes {
                    if env.label == clara_ritual::label::OFFERING {
                        // The piped body must be the clean user payload.
                        let body = match &env.payload {
                            TephraPayload::Plaintext { body } => body.clone(),
                            _ => panic!("unexpected payload"),
                        };
                        assert_eq!(
                            body.get("prompt").and_then(|v| v.as_str()),
                            Some("hello"),
                            "piped Offering body must be the raw payload, got {body}"
                        );
                        assert!(body.get("_routing").is_none(), "_routing must be stripped");
                        assert!(body.get("_caws").is_none(), "_caws must be stripped");
                        assert_eq!(env.target_node_id.as_deref(), Some("n2"));
                        assert_eq!(env.topic_path.as_deref(), Some("consults/e1"));
                        let cid = env.correlation_id.expect("piped Offering must carry a cid");

                        let hohi = TephraEnvelope::new(
                            ritual_id,
                            env.performance_id,
                            clara_ritual::label::HOHI,
                            60_000,
                            "mock-groq.test",
                            TephraPayload::Plaintext {
                                body: serde_json::json!({"response": "forty_two"}),
                            },
                        )
                        .with_routing(Routing {
                            correlation_id: Some(cid),
                            source_node_id: Some("n2".into()),
                            topic_path:     env.topic_path.clone(),
                            ..Default::default()
                        });
                        mock_broker.publish(&mock_topic, &hohi).expect("mock publish failed");
                        mock_flag.store(true, Ordering::Relaxed);
                        return;
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        });

        let mut ctrl = CycleController::new(
            session,
            50,
            Some("reasoned(Answer)".into()),
            Arc::new(AtomicBool::new(false)),
        )
        .with_ritual(cc_handle)
        .with_self_node_id(Some("n1".into()))
        .with_initial_offering(Some(InitialOffering {
            payload:        serde_json::json!({"prompt": "hello"}),
            topic_path:     Some("run".into()),
            source_node_id: None,
            correlation_id: Some(Uuid::new_v4()),
        }));

        let result = ctrl.run().expect("run() should converge");
        mock_thread.join().expect("mock peer thread panicked");
        assert!(mock_responded.load(Ordering::Relaxed), "mock peer never saw the Offering");

        assert_eq!(result.status, crate::result::CycleStatus::Converged);
        assert!(ctrl.pending_offers.is_empty(), "resolved offer must be cleared");

        let bindings  = serde_json::to_string(&result.goal_bindings).unwrap_or_default();
        let solutions = serde_json::to_string(&result.prolog_solutions).unwrap_or_default();
        assert!(
            bindings.contains("forty_two") || solutions.contains("forty_two"),
            "peer answer must reach the goal via edge_result/3; \
             bindings={bindings} solutions={solutions}"
        );

        // Exactly one Offering: the pipe memo + echo suppression must prevent
        // both re-piping on goal re-runs and piping our own echoed Offering.
        let (all_msgs, _) = broker.poll(&topic, 0).expect("final poll failed");
        let offerings = all_msgs.iter()
            .filter(|e| e.label == clara_ritual::label::OFFERING)
            .count();
        assert_eq!(offerings, 1, "auto-pipe must publish exactly one Offering");
    }

    /// Auto-pipe with a silent peer: patience expires, the timeout Tabu is
    /// dispatched through the generated edge-e1-on-timeout-result rule, and
    /// `edge_result(e1, tabu, _)` is asserted so authored fallback clauses
    /// can fire (and the run converges instead of hanging).
    #[test]
    fn auto_pipe_timeout_asserts_tabu_edge_result() {
        use std::sync::atomic::AtomicBool;

        setup_coire();

        let broker    = Arc::new(InMemoryBroker::new());
        let registry  = RitualRegistry::new("dis.test", broker.clone());
        let ritual_id = registry
            .create(RitualConfig { name: "auto-pipe-timeout".into(), participants: vec![] })
            .unwrap();
        let cc_handle = registry.join(ritual_id, Some("cc")).unwrap();

        let mut session = DeductionSession::new().unwrap();
        seed_auto_pipe_edge(
            &mut session,
            "reasoned(A) :- edge_result(e1, tabu, R), get_dict(error, R, A).",
        );

        // Nobody answers the piped Offering.
        let mut ctrl = CycleController::new(
            session,
            30,
            Some("reasoned(Answer)".into()),
            Arc::new(AtomicBool::new(false)),
        )
        .with_ritual(cc_handle)
        .with_self_node_id(Some("n1".into()))
        .with_evaluator_patience(3)
        .with_initial_offering(Some(InitialOffering {
            payload:        serde_json::json!({"prompt": "hello"}),
            topic_path:     Some("run".into()),
            source_node_id: None,
            correlation_id: Some(Uuid::new_v4()),
        }));

        let result = ctrl.run().expect("run() should converge via the timeout path");
        assert_eq!(result.status, crate::result::CycleStatus::Converged);
        assert!(ctrl.pending_offers.is_empty(), "timed-out offer must be cleared");

        let bindings  = serde_json::to_string(&result.goal_bindings).unwrap_or_default();
        let solutions = serde_json::to_string(&result.prolog_solutions).unwrap_or_default();
        assert!(
            bindings.contains("evaluator_timeout") || solutions.contains("evaluator_timeout"),
            "timeout must surface as edge_result(e1, tabu, ...); \
             bindings={bindings} solutions={solutions}"
        );
    }
}
