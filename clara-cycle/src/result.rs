use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Status of a deduction cycle run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CycleStatus {
    Running,
    Converged,
    Interrupted,
    Error(String),
}

impl std::fmt::Display for CycleStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CycleStatus::Running     => write!(f, "running"),
            CycleStatus::Converged   => write!(f, "converged"),
            CycleStatus::Interrupted => write!(f, "interrupted"),
            CycleStatus::Error(e)   => write!(f, "error: {}", e),
        }
    }
}

/// Final result produced when a cycle run completes (successfully or not).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeductionResult {
    pub status:           CycleStatus,
    pub cycles:           u32,
    pub prolog_session_id: Uuid,
    pub clips_session_id:  Uuid,
}

/// Point-in-time snapshot of pending Coire event counts used for convergence
/// detection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoireSnapshot {
    pub prolog_pending: usize,
    pub clips_pending:  usize,
}
