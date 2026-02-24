use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

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
}

impl CycleController {
    pub fn new(
        session:      DeductionSession,
        max_cycles:   u32,
        initial_goal: Option<String>,
        interrupt:    Arc<AtomicBool>,
    ) -> Self {
        Self { session, max_cycles, initial_goal, interrupt }
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

        for cycle in 0..self.max_cycles {
            log::debug!("CycleController: cycle {}", cycle);

            // 1. Prolog pass — consume Coire events + run goal
            log::debug!("Prolog pass");
            self.prolog_pass(cycle)?;
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
                return Ok(DeductionResult {
                    status:            CycleStatus::Converged,
                    cycles:            cycle + 1,
                    prolog_session_id: self.session.prolog_id,
                    clips_session_id:  self.session.clips_id,
                });
            } else {
                log::debug!("... not converged yet");
            }
            prev_snapshot = curr_snapshot;

            // 7. Interrupt check
            log::debug!("Interrupt check");
            if self.interrupt.load(Ordering::SeqCst) {
                log::info!("CycleController: interrupted after {} cycle(s)", cycle + 1);
                return Ok(DeductionResult {
                    status:            CycleStatus::Interrupted,
                    cycles:            cycle + 1,
                    prolog_session_id: self.session.prolog_id,
                    clips_session_id:  self.session.clips_id,
                });
            } else {
                log::debug!("... no interrupt signal");
            }
        }
        log::warn!(
            "CycleController: max cycles exceeded ({} cycles) without convergence",
            self.max_cycles
        );
        Err(CycleError::MaxCyclesExceeded(self.max_cycles))
    }

    // -------------------------------------------------------------------------
    // Private helpers
    // -------------------------------------------------------------------------

    fn prolog_pass(&mut self, cycle: u32) -> Result<(), CycleError> {
        // Dispatch any events waiting in Prolog's Coire mailbox.
        self.session.prolog.consume_coire_events()?;

        // On the first cycle, execute the caller-supplied goal (or "true").
        // On subsequent cycles, just tick with "true" so the engine is active.
        let goal = if cycle == 0 {
            self.initial_goal.clone().unwrap_or_else(|| "true".to_string())
        } else {
            "true".to_string()
        };

        match self.session.prolog.query_once(&goal) {
            Ok(_)  => log::debug!("CycleController: prolog_pass goal succeeded: {}", goal),
            Err(e) => log::warn!("CycleController: prolog_pass goal failed: {}: {}", goal, e),
        }

        Ok(())
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
