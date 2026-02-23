use clara_clips::ClipsEnvironment;
use clara_prolog::PrologEnvironment;
use uuid::Uuid;

use crate::error::CycleError;

/// A paired Prolog + CLIPS environment for a single deduction run.
///
/// Each deduction gets its own fresh engine pair so sessions are fully isolated.
/// Both engines are automatically assigned Coire session UUIDs by their
/// respective `::new()` constructors.
pub struct DeductionSession {
    pub prolog:    PrologEnvironment,
    pub clips:     ClipsEnvironment,
    pub prolog_id: Uuid,
    pub clips_id:  Uuid,
}

impl DeductionSession {
    /// Create a fresh Prolog + CLIPS engine pair.
    pub fn new() -> Result<Self, CycleError> {
        let prolog    = PrologEnvironment::new()?;
        let clips     = ClipsEnvironment::new().map_err(CycleError::SessionCreationFailed)?;
        let prolog_id = prolog.session_id();
        let clips_id  = clips.session_id();
        Ok(Self { prolog, clips, prolog_id, clips_id })
    }

    /// Load Prolog clauses into the Prolog engine.
    ///
    /// All clauses are joined and loaded via `consult_string`, which handles
    /// standard Prolog clause syntax including trailing periods.
    pub fn seed_prolog(&mut self, clauses: &[String]) -> Result<(), CycleError> {
        if clauses.is_empty() {
            return Ok(());
        }
        let code = clauses.join("\n");
        self.prolog.consult_string(&code)?;
        Ok(())
    }

    /// Load CLIPS constructs (`defrule`, `deftemplate`, etc.) into the CLIPS engine.
    ///
    /// Each construct string is passed to `ClipsEnvironment::build`.
    pub fn seed_clips(&mut self, constructs: &[String]) -> Result<(), CycleError> {
        for construct in constructs {
            self.clips
                .build(construct)
                .map_err(CycleError::Clips)?;
        }
        Ok(())
    }
}
