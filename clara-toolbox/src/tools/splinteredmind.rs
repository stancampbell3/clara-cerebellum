//! ClaraSplinteredMindTool - Bridge between CLIPS/Prolog and FieryPit API
//!
//! This tool enables CLIPS rules and Prolog predicates to call out to the
//! FieryPit REST API for LLM evaluation, session management, and cross-system
//! reasoning.

use crate::tool::{Tool, ToolError};
use fiery_pit_client::{CreateSessionRequest, FieryPitClient, SessionConfig};
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;

/// Operations supported by the SplinteredMind tool
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    // General FieryPit operations
    Health,
    Status,
    Info,
    Evaluate,
    ListEvaluators,
    GetEvaluator,
    SetEvaluator,
    ResetEvaluator,

    // CLIPS operations
    ClipsCreateSession,
    ClipsListSessions,
    ClipsGetSession,
    ClipsTerminateSession,
    ClipsEvaluate,
    ClipsLoadRules,
    ClipsLoadFacts,
    ClipsQueryFacts,
    ClipsRun,

    // Prolog operations
    PrologCreateSession,
    PrologListSessions,
    PrologGetSession,
    PrologTerminateSession,
    PrologQuery,
    PrologConsult,
}

/// Tool request arguments
#[derive(Debug, Deserialize)]
pub struct SplinteredMindArgs {
    pub operation: Operation,

    // Session management
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,

    // Session config
    #[serde(default)]
    pub max_facts: Option<i32>,
    #[serde(default)]
    pub max_rules: Option<i32>,
    #[serde(default)]
    pub max_memory_mb: Option<i32>,

    // CLIPS specific
    #[serde(default)]
    pub script: Option<String>,
    #[serde(default)]
    pub timeout_ms: Option<i32>,
    #[serde(default)]
    pub rules: Option<Vec<String>>,
    #[serde(default)]
    pub facts: Option<Vec<String>>,
    #[serde(default)]
    pub pattern: Option<String>,
    #[serde(default)]
    pub max_iterations: Option<i32>,

    // Prolog specific
    #[serde(default)]
    pub goal: Option<String>,
    #[serde(default)]
    pub all_solutions: Option<bool>,
    #[serde(default)]
    pub clauses: Option<Vec<String>>,

    // General evaluate
    #[serde(default)]
    pub data: Option<Value>,

    // Evaluator management
    #[serde(default)]
    pub evaluator: Option<String>,
}

/// ClaraSplinteredMindTool - Bridge to FieryPit API
pub struct ClaraSplinteredMindTool {
    client: Arc<FieryPitClient>,
}

impl ClaraSplinteredMindTool {
    /// Create a new ClaraSplinteredMindTool with the given FieryPitClient
    pub fn new(client: Arc<FieryPitClient>) -> Self {
        Self { client }
    }

    /// Create with a base URL
    pub fn with_url(base_url: impl Into<String>) -> Self {
        Self {
            client: Arc::new(FieryPitClient::new(base_url)),
        }
    }

    fn execute_operation(&self, args: SplinteredMindArgs) -> Result<Value, ToolError> {
        match args.operation {
            // =================================================================
            // General FieryPit operations
            // =================================================================
            Operation::Health => self
                .client
                .health()
                .map_err(|e| ToolError::ExecutionFailed(e.to_string())),

            Operation::Status => self
                .client
                .status()
                .map_err(|e| ToolError::ExecutionFailed(e.to_string())),

            Operation::Info => self
                .client
                .info()
                .map_err(|e| ToolError::ExecutionFailed(e.to_string())),

            Operation::Evaluate => {
                let data = args
                    .data
                    .ok_or_else(|| ToolError::InvalidArgs("'data' required for evaluate".into()))?;
                self.client
                    .evaluate(data)
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))
            }

            Operation::ListEvaluators => self
                .client
                .list_evaluators()
                .map_err(|e| ToolError::ExecutionFailed(e.to_string())),

            Operation::GetEvaluator => {
                let evaluator = args
                    .evaluator
                    .ok_or_else(|| ToolError::InvalidArgs("'evaluator' required".into()))?;
                self.client
                    .get_evaluator(&evaluator)
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))
            }

            Operation::SetEvaluator => {
                let evaluator = args
                    .evaluator
                    .ok_or_else(|| ToolError::InvalidArgs("'evaluator' required".into()))?;
                self.client
                    .set_evaluator(&evaluator)
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))
            }

            Operation::ResetEvaluator => self
                .client
                .reset_evaluator()
                .map_err(|e| ToolError::ExecutionFailed(e.to_string())),

            // =================================================================
            // CLIPS operations
            // =================================================================
            Operation::ClipsCreateSession => {
                let user_id = args.user_id.unwrap_or_else(|| "clara".into());
                let config = if args.max_facts.is_some()
                    || args.max_rules.is_some()
                    || args.max_memory_mb.is_some()
                {
                    Some(SessionConfig {
                        max_facts: args.max_facts,
                        max_rules: args.max_rules,
                        max_memory_mb: args.max_memory_mb,
                    })
                } else {
                    None
                };
                let req = CreateSessionRequest {
                    user_id,
                    name: args.name,
                    config,
                };
                self.client
                    .clips_create_session(req)
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))
            }

            Operation::ClipsListSessions => self
                .client
                .clips_list_sessions()
                .map_err(|e| ToolError::ExecutionFailed(e.to_string())),

            Operation::ClipsGetSession => {
                let session_id = args
                    .session_id
                    .ok_or_else(|| ToolError::InvalidArgs("'session_id' required".into()))?;
                self.client
                    .clips_get_session(&session_id)
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))
            }

            Operation::ClipsTerminateSession => {
                let session_id = args
                    .session_id
                    .ok_or_else(|| ToolError::InvalidArgs("'session_id' required".into()))?;
                self.client
                    .clips_terminate_session(&session_id)
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))
            }

            Operation::ClipsEvaluate => {
                let session_id = args
                    .session_id
                    .ok_or_else(|| ToolError::InvalidArgs("'session_id' required".into()))?;
                let script = args
                    .script
                    .ok_or_else(|| ToolError::InvalidArgs("'script' required".into()))?;
                self.client
                    .clips_evaluate(&session_id, &script, args.timeout_ms)
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))
            }

            Operation::ClipsLoadRules => {
                let session_id = args
                    .session_id
                    .ok_or_else(|| ToolError::InvalidArgs("'session_id' required".into()))?;
                let rules = args
                    .rules
                    .ok_or_else(|| ToolError::InvalidArgs("'rules' required".into()))?;
                self.client
                    .clips_load_rules(&session_id, rules)
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))
            }

            Operation::ClipsLoadFacts => {
                let session_id = args
                    .session_id
                    .ok_or_else(|| ToolError::InvalidArgs("'session_id' required".into()))?;
                let facts = args
                    .facts
                    .ok_or_else(|| ToolError::InvalidArgs("'facts' required".into()))?;
                self.client
                    .clips_load_facts(&session_id, facts)
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))
            }

            Operation::ClipsQueryFacts => {
                let session_id = args
                    .session_id
                    .ok_or_else(|| ToolError::InvalidArgs("'session_id' required".into()))?;
                self.client
                    .clips_query_facts(&session_id, args.pattern.as_deref())
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))
            }

            Operation::ClipsRun => {
                let session_id = args
                    .session_id
                    .ok_or_else(|| ToolError::InvalidArgs("'session_id' required".into()))?;
                self.client
                    .clips_run(&session_id, args.max_iterations)
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))
            }

            // =================================================================
            // Prolog operations
            // =================================================================
            Operation::PrologCreateSession => {
                let user_id = args.user_id.unwrap_or_else(|| "clara".into());
                let config = if args.max_facts.is_some()
                    || args.max_rules.is_some()
                    || args.max_memory_mb.is_some()
                {
                    Some(SessionConfig {
                        max_facts: args.max_facts,
                        max_rules: args.max_rules,
                        max_memory_mb: args.max_memory_mb,
                    })
                } else {
                    None
                };
                let req = CreateSessionRequest {
                    user_id,
                    name: args.name,
                    config,
                };
                self.client
                    .prolog_create_session(req)
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))
            }

            Operation::PrologListSessions => self
                .client
                .prolog_list_sessions()
                .map_err(|e| ToolError::ExecutionFailed(e.to_string())),

            Operation::PrologGetSession => {
                let session_id = args
                    .session_id
                    .ok_or_else(|| ToolError::InvalidArgs("'session_id' required".into()))?;
                self.client
                    .prolog_get_session(&session_id)
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))
            }

            Operation::PrologTerminateSession => {
                let session_id = args
                    .session_id
                    .ok_or_else(|| ToolError::InvalidArgs("'session_id' required".into()))?;
                self.client
                    .prolog_terminate_session(&session_id)
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))
            }

            Operation::PrologQuery => {
                let session_id = args
                    .session_id
                    .ok_or_else(|| ToolError::InvalidArgs("'session_id' required".into()))?;
                let goal = args
                    .goal
                    .ok_or_else(|| ToolError::InvalidArgs("'goal' required".into()))?;
                self.client
                    .prolog_query(&session_id, &goal, args.all_solutions.unwrap_or(false))
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))
            }

            Operation::PrologConsult => {
                let session_id = args
                    .session_id
                    .ok_or_else(|| ToolError::InvalidArgs("'session_id' required".into()))?;
                let clauses = args
                    .clauses
                    .ok_or_else(|| ToolError::InvalidArgs("'clauses' required".into()))?;
                self.client
                    .prolog_consult(&session_id, clauses)
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))
            }
        }
    }
}

impl Tool for ClaraSplinteredMindTool {
    fn name(&self) -> &str {
        "splinteredmind"
    }

    fn description(&self) -> &str {
        "Bridge to FieryPit REST API for CLIPS/Prolog sessions and LLM evaluation"
    }

    fn execute(&self, args: Value) -> Result<Value, ToolError> {
        log::debug!("SplinteredMindTool executing with args: {}", args);

        let parsed_args: SplinteredMindArgs = serde_json::from_value(args)
            .map_err(|e| ToolError::InvalidArgs(format!("Failed to parse arguments: {}", e)))?;

        self.execute_operation(parsed_args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_name() {
        let tool = ClaraSplinteredMindTool::with_url("http://localhost:8000");
        assert_eq!(tool.name(), "splinteredmind");
    }

    #[test]
    fn test_tool_description() {
        let tool = ClaraSplinteredMindTool::with_url("http://localhost:8000");
        assert!(!tool.description().is_empty());
    }

    #[test]
    fn test_operation_deserialize() {
        let json = r#"{"operation": "status"}"#;
        let args: SplinteredMindArgs = serde_json::from_str(json).unwrap();
        assert!(matches!(args.operation, Operation::Status));
    }

    #[test]
    fn test_clips_evaluate_args() {
        let json = r#"{
            "operation": "clips_evaluate",
            "session_id": "abc123",
            "script": "(printout t \"hello\" crlf)",
            "timeout_ms": 5000
        }"#;
        let args: SplinteredMindArgs = serde_json::from_str(json).unwrap();
        assert!(matches!(args.operation, Operation::ClipsEvaluate));
        assert_eq!(args.session_id, Some("abc123".to_string()));
        assert_eq!(
            args.script,
            Some("(printout t \"hello\" crlf)".to_string())
        );
        assert_eq!(args.timeout_ms, Some(5000));
    }

    #[test]
    fn test_prolog_query_args() {
        let json = r#"{
            "operation": "prolog_query",
            "session_id": "xyz789",
            "goal": "member(X, [1,2,3])",
            "all_solutions": true
        }"#;
        let args: SplinteredMindArgs = serde_json::from_str(json).unwrap();
        assert!(matches!(args.operation, Operation::PrologQuery));
        assert_eq!(args.session_id, Some("xyz789".to_string()));
        assert_eq!(args.goal, Some("member(X, [1,2,3])".to_string()));
        assert_eq!(args.all_solutions, Some(true));
    }

    #[test]
    fn test_missing_operation_fails() {
        let json = r#"{"session_id": "abc"}"#;
        let result: Result<SplinteredMindArgs, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }
}
