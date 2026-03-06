use thiserror::Error;

#[derive(Debug, Error)]
pub enum CycleError {
    #[error("Prolog error: {0}")]
    Prolog(#[from] clara_prolog::PrologError),

    #[error("CLIPS error: {0}")]
    Clips(String),

    #[error("Coire error: {0}")]
    Coire(#[from] clara_coire::CoireError),

    #[error("Max cycles ({0}) exceeded without convergence")]
    MaxCyclesExceeded(u32),

    #[error("Session creation failed: {0}")]
    SessionCreationFailed(String),

    #[error("Context seeding failed: {0}")]
    ContextSeedFailed(String),
}
