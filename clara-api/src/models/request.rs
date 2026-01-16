use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
