use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use uuid::Uuid;

use crate::error::CycleError;
use crate::relay::{relay_clips_to_prolog, relay_prolog_to_clips};
use crate::result::{CoireSnapshot, CycleStatus, DeductionResult};
use crate::session::DeductionSession;

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

    /// Reload a previous run's Coire state into the current session's mailboxes.
    ///
    /// Events stored under `prev_prolog_id` / `prev_clips_id` are written into
    /// the current session's IDs with their `session_id` fields rewritten.
    /// Call this before [`run()`] when resuming a previous deduction.
    pub fn restore_from(
        &mut self,
        store: &clara_coire::CoireStore,
        prev_prolog_id: Uuid,
        prev_clips_id: Uuid,
    ) -> Result<(), CycleError> {
        let coire = clara_coire::global();
        store.restore_session_as(prev_prolog_id, self.session.prolog_id, coire)?;
        store.restore_session_as(prev_clips_id, self.session.clips_id, coire)?;
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

        for cycle in 0..self.max_cycles {
            log::debug!("CycleController: cycle {}", cycle);

            // 1. Prolog pass — consume Coire events + run goal
            log::debug!("Prolog pass");
            let solutions = self.prolog_pass(cycle)?;
            if cycle == 0 {
                initial_solutions = solutions;
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
            if self.has_converged(&prev_snapshot, &curr_snapshot) {
                log::info!("CycleController: converged after {} cycle(s)", cycle + 1);
                self.save_to_store();
                self.evict_coire_sessions();
                return Ok(DeductionResult {
                    status:            CycleStatus::Converged,
                    cycles:            cycle + 1,
                    prolog_session_id: self.session.prolog_id,
                    clips_session_id:  self.session.clips_id,
                    prolog_solutions:  initial_solutions,
                });
            } else {
                log::debug!("... not converged yet");
            }
            prev_snapshot = curr_snapshot;

            // 7. Interrupt check
            log::debug!("Interrupt check");
            if self.interrupt.load(Ordering::SeqCst) {
                log::info!("CycleController: interrupted after {} cycle(s)", cycle + 1);
                self.save_to_store();
                self.evict_coire_sessions();
                return Ok(DeductionResult {
                    status:            CycleStatus::Interrupted,
                    cycles:            cycle + 1,
                    prolog_session_id: self.session.prolog_id,
                    clips_session_id:  self.session.clips_id,
                    prolog_solutions:  initial_solutions,
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
    /// Coire. Called at every `run()` exit so processed events do not linger
    /// after the session's engines are dropped. Always runs after
    /// `save_to_store()` so the persistent snapshot is written first.
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
    /// Failures are logged as warnings and do not affect the cycle result.
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
    ///
    /// On cycle 0 the `initial_goal` is executed and **all** solutions are
    /// collected with their variable bindings (e.g. `[{"Man": "stan"}]`).
    /// On subsequent cycles only `true` is run to keep the engine ticking.
    ///
    /// Returns `Some(solutions)` on cycle 0, `None` on all later cycles.
    fn prolog_pass(&mut self, cycle: u32) -> Result<Option<serde_json::Value>, CycleError> {
        // Dispatch any events waiting in Prolog's Coire mailbox.
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
        // Structural stub — CycleMember evaluator integration (FieryPit/LilDaemon)
        // will be wired here in a future milestone.
        log::debug!("CycleController: evaluator_pass (stub)");
    }

    fn clips_pass(&mut self) -> Result<(), CycleError> {
        // Dispatch any events waiting in CLIPS's Coire mailbox.
        self.session
            .clips
            .consume_coire_events()
            .map_err(CycleError::Clips)?;

        // Run the CLIPS inference engine to saturation.
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

    /// Return `true` when both mailboxes are empty, the CLIPS agenda is empty,
    /// and the snapshot has not changed since the previous cycle.
    fn has_converged(&mut self, prev: &CoireSnapshot, curr: &CoireSnapshot) -> bool {
        // If the CLIPS agenda check fails for any reason, assume empty (liberal).
        let clips_agenda_empty = self
            .session
            .clips
            .eval("(= (length$ (get-agenda)) 0)")
            .map(|s| s.trim() == "TRUE")
            .unwrap_or(true);

        let converged = curr.prolog_pending == 0
            && curr.clips_pending == 0
            && clips_agenda_empty
            && prev == curr;

        log::debug!(
            "CycleController: convergence check — prolog_pending={}, clips_pending={}, \
             agenda_empty={}, snapshot_stable={} → {}",
            curr.prolog_pending,
            curr.clips_pending,
            clips_agenda_empty,
            prev == curr,
            converged
        );

        converged
    }
}
