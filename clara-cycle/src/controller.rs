use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use clara_dagda::{Binding, PredicateEntry, TruthValue};
use uuid::Uuid;

use crate::error::CycleError;
use crate::relay::{relay_clips_to_prolog, relay_prolog_to_clips};
use crate::result::{CoireSnapshot, CycleStatus, DeductionResult};
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
    /// Timestamp at the start of the last completed cycle; used to detect
    /// tableau changes via `tableau_changed_since`.
    last_cycle_ts: i64,
}

impl GoalAgenda {
    fn new(initial_goal: &Option<String>) -> Self {
        let root_functor = initial_goal.as_deref().map(extract_functor_from_goal);
        Self { root_functor, last_cycle_ts: now_ms() }
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
        // Look for any entry for this functor with a resolved truth value.
        match session.tableau.list_by_functor(session.prolog_id, functor, 0) {
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
    session:       DeductionSession,
    max_cycles:    u32,
    /// Optional Prolog goal to execute on the first cycle.
    initial_goal:  Option<String>,
    /// Set to `true` from outside to request early termination.
    interrupt:     Arc<AtomicBool>,
    /// Optional persistent store. When set, both mailboxes are saved on every
    /// exit from `run()` (converged, interrupted, or max-cycles exceeded).
    store:         Option<clara_coire::CoireStore>,
}

impl CycleController {
    pub fn new(
        session:      DeductionSession,
        max_cycles:   u32,
        initial_goal: Option<String>,
        interrupt:    Arc<AtomicBool>,
    ) -> Self {
        Self { session, max_cycles, initial_goal, interrupt, store: None }
    }

    /// Attach a persistent [`CoireStore`]. Both mailboxes will be saved
    /// automatically on every exit from [`run()`].
    pub fn with_store(mut self, store: clara_coire::CoireStore) -> Self {
        self.store = Some(store);
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
        log::info!(
            "CycleController: starting (max_cycles={}, goal={:?})",
            self.max_cycles,
            self.initial_goal
        );

        let mut prev_snapshot = self.snapshot();
        let mut initial_solutions: Option<serde_json::Value> = None;
        let mut agenda = GoalAgenda::new(&self.initial_goal);

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
            relay_prolog_to_clips(&mut self.session)?;
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
            relay_clips_to_prolog(&mut self.session)?;
            log::debug!("... relay prolog complete");

            // 6. Convergence check
            log::debug!("Convergence check");
            let curr_snapshot = self.snapshot();
            if self.has_converged(&prev_snapshot, &curr_snapshot, &agenda) {
                log::info!("CycleController: converged after {} cycle(s)", cycle + 1);
                let tableau = self.export_tableau();
                let goal_bindings = self.root_goal_bindings(&agenda);
                self.save_to_store();
                self.evict_coire_sessions();
                return Ok(DeductionResult {
                    status:            CycleStatus::Converged,
                    cycles:            cycle + 1,
                    prolog_session_id: self.session.prolog_id,
                    clips_session_id:  self.session.clips_id,
                    prolog_solutions:  initial_solutions,
                    goal_bindings,
                    tableau:           Some(tableau),
                    explanation:       None,
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
                self.save_to_store();
                self.evict_coire_sessions();
                return Ok(DeductionResult {
                    status:            CycleStatus::Interrupted,
                    cycles:            cycle + 1,
                    prolog_session_id: self.session.prolog_id,
                    clips_session_id:  self.session.clips_id,
                    prolog_solutions:  initial_solutions,
                    goal_bindings,
                    tableau:           Some(tableau),
                    explanation:       None,
                });
            } else {
                log::debug!("... no interrupt signal");
            }
        }

        log::warn!(
            "CycleController: max cycles exceeded ({} cycles) without convergence",
            self.max_cycles
        );
        self.save_to_store();
        self.evict_coire_sessions();
        Err(CycleError::MaxCyclesExceeded(self.max_cycles))
    }

    // -------------------------------------------------------------------------
    // Private helpers
    // -------------------------------------------------------------------------

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
                    log::debug!("CycleController: prolog_pass goal succeeded: {}", goal);
                    serde_json::from_str::<serde_json::Value>(&json_str)
                        .unwrap_or(serde_json::json!([]))
                }
                Err(e) => {
                    log::warn!("CycleController: prolog_pass goal failed: {}: {}", goal, e);
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

    fn evaluator_pass(&self) {
        log::debug!("CycleController: evaluator_pass (stub)");
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
            prolog_pending: coire.count_pending(self.session.prolog_id).unwrap_or(1),
            clips_pending:  coire.count_pending(self.session.clips_id).unwrap_or(1),
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
        let snapshot_stable = prev == curr;
        let tableau_stable  = !agenda.tableau_progressed(&self.session);
        let root_resolved   = agenda.root_goal_resolved(&self.session);

        let converged = mailboxes_empty
            && clips_agenda_empty
            && (tableau_stable || root_resolved);

        log::debug!(
            "CycleController: convergence — prolog_pending={}, clips_pending={}, \
             agenda_empty={}, snapshot_stable={}, tableau_stable={}, root_resolved={} → {}",
            curr.prolog_pending,
            curr.clips_pending,
            clips_agenda_empty,
            snapshot_stable,
            tableau_stable,
            root_resolved,
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
            &["*"],
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

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before epoch")
        .as_millis() as i64
}
