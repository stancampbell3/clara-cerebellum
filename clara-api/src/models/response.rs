use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Session response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionResponse {
    pub session_id: String,
    pub user_id: String,
    pub started: String,
    pub touched: String,
    pub status: String,
    pub resources: ResourceInfo,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limits: Option<ResourceInfo>,
}

/// Resource information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceInfo {
    pub facts: u32,
    pub rules: u32,
    pub objects: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_mb: Option<u32>,
}

/// Status response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusResponse {
    pub session_id: String,
    pub user_id: String,
    pub started: String,
    pub touched: String,
    pub status: String,
    pub resources: ResourceInfo,
    pub limits: ResourceInfo,
    pub health: String,
}

/// Eval response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalResponse {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub metrics: EvalMetrics,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session: Option<SessionResponse>,
}

/// Eval metrics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EvalMetrics {
    pub elapsed_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub facts_added: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rules_fired: Option<u32>,
}

/// Load response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadResponse {
    pub session_id: String,
    pub loaded: Vec<String>,
    pub resources: ResourceInfo,
}

/// Save response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveResponse {
    pub session_id: String,
    pub saved_as: String,
    pub timestamp: String,
}

/// Reload response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReloadResponse {
    pub session_id: String,
    pub status: String,
    pub resources: ResourceInfo,
}

/// Terminate response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminateResponse {
    pub session_id: String,
    pub status: String,
    pub saved: bool,
}

/// Health check response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub uptime_seconds: u64,
}

/// Run rules response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunResponse {
    pub rules_fired: u64,
    pub status: String,
    pub runtime_ms: u64,
}

/// Query facts response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryFactsResponse {
    pub matches: Vec<String>,
    pub count: usize,
}

/// Prolog query response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrologQueryResponse {
    /// The query result (bindings or output)
    pub result: String,
    /// Whether the query succeeded
    pub success: bool,
    /// Execution time in milliseconds
    pub runtime_ms: u64,
}

/// Response for POST /deduce — deduction accepted and running asynchronously.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeduceStartResponse {
    pub deduction_id: Uuid,
    pub status: String,
}

/// Response for GET /deduce/{id} — current state of a deduction run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeduceStatusResponse {
    pub deduction_id: Uuid,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    pub cycles: u32,
}

/// Response for DELETE /deduce/{id} — confirms interrupt was requested.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeduceInterruptResponse {
    pub deduction_id: Uuid,
    pub status: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_response() {
        let resp = SessionResponse {
            session_id: "sess-123".to_string(),
            user_id: "user-123".to_string(),
            started: "2025-10-23T17:03:00Z".to_string(),
            touched: "2025-10-23T17:03:00Z".to_string(),
            status: "active".to_string(),
            resources: ResourceInfo {
                facts: 0,
                rules: 0,
                objects: 0,
                memory_mb: None,
            },
            limits: None,
        };
        assert_eq!(resp.session_id, "sess-123");
    }
}
