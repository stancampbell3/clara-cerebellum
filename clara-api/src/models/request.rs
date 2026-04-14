use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Session configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    #[serde(default)]
    pub max_facts: Option<u32>,
    #[serde(default)]
    pub max_rules: Option<u32>,
    #[serde(default)]
    pub max_memory_mb: Option<u32>,
}

/// Create session request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSessionRequest {
    pub user_id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub config: Option<SessionConfig>,
    #[serde(default)]
    pub preload: Vec<String>,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

// Save session request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveSessionRequest {
    pub user_id: String,
    pub session_id: String,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

/// Eval request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalRequest {
    pub script: String,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
}

/// Load request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadRequest {
    pub files: Vec<String>,
}

/// Reload request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReloadRequest {
    pub label: String,
}

/// Load rules request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadRulesRequest {
    pub rules: Vec<String>,
}

/// Load facts request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadFactsRequest {
    pub facts: Vec<String>,
}

/// Run rules request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunRequest {
    #[serde(default = "default_max_iterations")]
    pub max_iterations: i64,
}

fn default_timeout() -> u64 {
    2000
}

fn default_max_iterations() -> i64 {
    -1 // -1 means run until completion
}

/// Prolog query request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrologQueryRequest {
    /// The Prolog goal to execute
    pub goal: String,
    /// If true, return all solutions; if false, return first solution only
    #[serde(default)]
    pub all_solutions: Option<bool>,
}

/// Prolog consult request - load clauses into the knowledge base
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrologConsultRequest {
    /// Prolog clauses to assert (facts and rules)
    pub clauses: Vec<String>,
}

/// Request to start a new deduction cycle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeduceRequest {
    /// Prolog clauses (facts and rules) to seed the Prolog engine.
    /// Ignored when `prolog_source_id` is present.
    #[serde(default)]
    pub prolog_clauses: Vec<String>,
    /// CLIPS constructs (`defrule`, `deftemplate`, etc.) to seed the CLIPS engine.
    /// Ignored when `clips_source_id` is present.
    #[serde(default)]
    pub clips_constructs: Vec<String>,
    /// Optional server-side path to a `.clp` file loaded before `clips_constructs`.
    /// Ignored when `clips_source_id` is present.
    #[serde(default)]
    pub clips_file: Option<String>,
    /// Pre-registered Prolog source ID from `POST /source`.
    /// When present, `prolog_clauses` is ignored and the stored source content
    /// is used. The source's artifacts (DOT, parsed rules) are also available.
    #[serde(default)]
    pub prolog_source_id: Option<Uuid>,
    /// Pre-registered CLIPS source ID from `POST /source`.
    /// When present, `clips_file` and `clips_constructs` are ignored.
    #[serde(default)]
    pub clips_source_id: Option<Uuid>,
    /// Optional Prolog goal to execute on the first cycle.
    #[serde(default)]
    pub initial_goal: Option<String>,
    /// Maximum number of Prolog↔CLIPS cycles before aborting (default: 100).
    #[serde(default)]
    pub max_cycles: Option<u32>,
    /// When `true` and persistence is configured, save a full
    /// [`DeductionSnapshot`] (seed knowledge + pending Coire events) to the
    /// store at cycle completion. The snapshot can later be resumed via
    /// `POST /deduce/resume`. Silently ignored if no store is configured.
    #[serde(default)]
    pub persist: bool,
    /// Enable per-phase tableau recording for trace visualization.
    ///
    /// When `true` and a store is configured, snapshots are written to
    /// `tableau_changes` and queryable via `GET /deduce/{id}/trace`.
    /// When `true` and no store is configured, the trace is returned inline
    /// in `DeductionResult.trace`.  When `false` (default), no per-phase
    /// tableau recording occurs.
    #[serde(default)]
    pub trace: bool,
    /// Optional conversational context (external message history) to inject
    /// into the deduction session. Each element is a JSON object — typically
    /// `{"role": "...", "content": "..."}` — and is made available to Prolog
    /// rules via `deduce_context_json/1` / `current_context/1` and forwarded
    /// to LLM evaluate calls that accept a `context` field.
    #[serde(default)]
    pub context: Vec<serde_json::Value>,
    /// Optional ID of an active Ritual to join for this deduction run.
    ///
    /// When set and the `ritual` feature is enabled, the `CycleController`
    /// will receive a `RitualHandle` via `with_ritual()`, enabling
    /// `evaluator_pass` to exchange Tephras with peer FieryPit evaluators.
    /// Each deduction run joins anonymously (fresh `performance_id`) so
    /// independent runs participating in the same Ritual are distinct
    /// performances.
    #[serde(default)]
    pub ritual_id: Option<Uuid>,
}

/// Request to resume a previously persisted deduction.
///
/// Looks up the [`DeductionSnapshot`] saved for `deduction_id`, re-seeds
/// fresh engine instances from the stored knowledge, restores any pending
/// Coire events, and runs the cycle again.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeduceResumeRequest {
    /// The `deduction_id` returned by the original `POST /deduce` response.
    pub deduction_id: Uuid,
    /// Override the cycle budget for this run. Defaults to the value stored
    /// in the snapshot.
    #[serde(default)]
    pub max_cycles: Option<u32>,
    /// When `true`, save a new snapshot of this resumed run at completion,
    /// enabling further chained resumes.
    #[serde(default)]
    pub persist: bool,
    /// Enable per-phase tableau recording. Same semantics as in [`DeduceRequest`].
    #[serde(default)]
    pub trace: bool,
    /// Conversational context to inject into the resumed session. If omitted,
    /// the context stored in the original snapshot is used.
    #[serde(default)]
    pub context: Option<Vec<serde_json::Value>>,
}

/// Request body for the Coire push endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoirePushRequest {
    pub session_id:  uuid::Uuid,
    pub origin:      String,
    pub event_type:  String,
    pub data:        String,
}

/// Request body for `POST /source` — register a Prolog or CLIPS source file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterSourceRequest {
    /// `"prolog"` or `"clips"`.
    pub source_type: String,
    /// Optional human-readable label.
    #[serde(default)]
    pub label: Option<String>,
    /// Raw source text (Prolog clauses or CLIPS constructs).
    pub content: String,
    /// Optional TTL in milliseconds from now. `None` = no expiry.
    #[serde(default)]
    pub ttl_ms: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_session_request() {
        let req = CreateSessionRequest {
            user_id: "user-123".to_string(),
            name: Some("Test Session".to_string()),
            config: None,
            preload: vec![],
            metadata: HashMap::new(),
        };
        assert_eq!(req.user_id, "user-123");
    }
    
    #[test]
    fn test_save_session_request() {
        let req = SaveSessionRequest {
            user_id: "user-123".to_string(),
            session_id: "session-456".to_string(),
            metadata: HashMap::new(),
        };
        assert_eq!(req.user_id, "user-123");
        assert_eq!(req.session_id, "session-456");
    }
}
