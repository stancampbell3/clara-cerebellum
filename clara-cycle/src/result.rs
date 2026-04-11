use clara_dagda::PredicateEntry;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A single tableau snapshot captured during an in-memory trace run.
///
/// Populated when `trace_mode = true` and no persistent store is configured.
/// When a store is configured, snapshots are written to `tableau_changes` in
/// DuckDB instead and this type is not used.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InMemoryTraceEntry {
    pub cycle_num:      u32,
    /// Lifecycle phase: `"initial"`, `"prolog_to_clips"`, `"clips_to_prolog"`,
    /// `"final_converged"`, `"final_interrupted"`, or `"final_max_cycles"`.
    pub phase:          String,
    pub recorded_at_ms: i64,
    pub entries:        Vec<PredicateEntry>,
}

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
    pub status:            CycleStatus,
    pub cycles:            u32,
    pub prolog_session_id: Uuid,
    pub clips_session_id:  Uuid,
    /// All solutions produced by the `initial_goal` on cycle 0.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prolog_solutions:  Option<serde_json::Value>,
    /// Final variable bindings for the root goal once it resolves.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goal_bindings:     Option<Vec<clara_dagda::Binding>>,
    /// Final tableau state on completion.  Absent for in-memory-only sessions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tableau:           Option<Vec<PredicateEntry>>,
    /// Reserved for future proof-tree / explanation output.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explanation:       Option<serde_json::Value>,
    /// In-memory trace log, populated when `trace_mode = true` and no
    /// persistent store is configured.  When a store is present the trace is
    /// written to `tableau_changes` and this field is `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace:             Option<Vec<InMemoryTraceEntry>>,
}

/// Point-in-time snapshot of pending Coire event counts used for convergence
/// detection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoireSnapshot {
    pub prolog_pending: usize,
    pub clips_pending:  usize,
}
