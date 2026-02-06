//! FieryPitClient - Synchronous client for FieryPit REST API
//!
//! Provides a blocking client to interact with the FieryPit REST API in lildaemon.
//! Supports health checks, evaluator management, CLIPS sessions, and Prolog sessions.

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum FieryPitError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Non-success status {0}: {1}")]
    Status(reqwest::StatusCode, Value),
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Session configuration for CLIPS/Prolog sessions
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_facts: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_rules: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_memory_mb: Option<i32>,
}

/// Request to create a new session
#[derive(Debug, Clone, Serialize)]
pub struct CreateSessionRequest {
    pub user_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<SessionConfig>,
}

/// CLIPS evaluation request
#[derive(Debug, Clone, Serialize)]
pub struct ClipsEvalRequest {
    pub script: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<i32>,
}

/// CLIPS load rules request
#[derive(Debug, Clone, Serialize)]
pub struct ClipsLoadRulesRequest {
    pub rules: Vec<String>,
}

/// CLIPS load facts request
#[derive(Debug, Clone, Serialize)]
pub struct ClipsLoadFactsRequest {
    pub facts: Vec<String>,
}

/// CLIPS run request
#[derive(Debug, Clone, Serialize)]
pub struct ClipsRunRequest {
    pub max_iterations: i32,
}

/// Prolog query request
#[derive(Debug, Clone, Serialize)]
pub struct PrologQueryRequest {
    pub goal: String,
    #[serde(default)]
    pub all_solutions: bool,
}

/// Prolog consult request
#[derive(Debug, Clone, Serialize)]
pub struct PrologConsultRequest {
    pub clauses: Vec<String>,
}

/// Set evaluator request
#[derive(Debug, Clone, Serialize)]
pub struct SetEvaluatorRequest {
    pub evaluator: String,
}

// =========================================================================
// Response types
// =========================================================================

/// Response from POST /evaluators/set or POST /evaluators/reset
#[derive(Debug, Clone, Deserialize)]
pub struct EvaluatorActionResponse {
    pub status: String,
    pub evaluator: Option<String>,
}

/// Success payload inside a Tephra response
#[derive(Debug, Clone, Deserialize)]
pub struct Hohi {
    pub response: Value,
    #[serde(default)]
    pub code: Option<i32>,
}

/// Error payload inside a Tephra response
#[derive(Debug, Clone, Deserialize)]
pub struct Tabu {
    pub message: String,
    #[serde(default)]
    pub code: Option<i32>,
    #[serde(default)]
    pub details: Option<Value>,
}

/// Tephra response envelope from POST /evaluate
#[derive(Debug, Clone, Deserialize)]
pub struct Tephra {
    #[serde(default)]
    pub timestamp: Option<i64>,
    #[serde(default)]
    pub hohi: Option<Hohi>,
    #[serde(default)]
    pub tabu: Option<Tabu>,
    #[serde(default)]
    pub task_id: Option<String>,
}

impl Tephra {
    /// Returns true if the response contains a success payload
    pub fn is_success(&self) -> bool {
        self.hohi.is_some()
    }

    /// Extract the inner response value from a successful evaluation
    pub fn response(&self) -> Option<&Value> {
        self.hohi.as_ref().map(|h| &h.response)
    }

    /// Extract the error message if this is an error response
    pub fn error_message(&self) -> Option<&str> {
        self.tabu.as_ref().map(|t| t.message.as_str())
    }

    /// Consume self and return the inner response or an error
    pub fn into_response(self) -> Result<Value, FieryPitError> {
        if let Some(hohi) = self.hohi {
            Ok(hohi.response)
        } else if let Some(tabu) = self.tabu {
            Err(FieryPitError::Status(
                reqwest::StatusCode::from_u16(tabu.code.unwrap_or(400) as u16)
                    .unwrap_or(reqwest::StatusCode::BAD_REQUEST),
                serde_json::json!({ "message": tabu.message, "details": tabu.details }),
            ))
        } else {
            Err(FieryPitError::Status(
                reqwest::StatusCode::INTERNAL_SERVER_ERROR,
                serde_json::json!({ "message": "Empty Tephra response" }),
            ))
        }
    }
}

/// FieryPit REST API Client
#[derive(Clone)]
pub struct FieryPitClient {
    base_url: Arc<String>,
    client: Client,
}

impl FieryPitClient {
    /// Create a new FieryPitClient
    ///
    /// # Arguments
    /// * `base_url` - Base URL of the FieryPit API, e.g. "http://localhost:8000"
    pub fn new(base_url: impl Into<String>) -> Self {
        let base = base_url.into();
        FieryPitClient {
            base_url: Arc::new(base.trim_end_matches('/').to_string()),
            client: Client::new(),
        }
    }

    // =========================================================================
    // Helper methods
    // =========================================================================

    fn get(&self, path: &str) -> Result<Value, FieryPitError> {
        let url = format!("{}{}", self.base_url, path);
        log::debug!("FieryPitClient GET {}", url);
        let resp = self.client.get(&url).send()?;
        self.handle_response(resp)
    }

    fn post(&self, path: &str, body: &impl Serialize) -> Result<Value, FieryPitError> {
        let url = format!("{}{}", self.base_url, path);
        log::debug!("FieryPitClient POST {}", url);
        let resp = self.client.post(&url).json(body).send()?;
        self.handle_response(resp)
    }

    fn delete(&self, path: &str) -> Result<Value, FieryPitError> {
        let url = format!("{}{}", self.base_url, path);
        log::debug!("FieryPitClient DELETE {}", url);
        let resp = self.client.delete(&url).send()?;
        self.handle_response(resp)
    }

    fn handle_response(&self, resp: reqwest::blocking::Response) -> Result<Value, FieryPitError> {
        let status = resp.status();
        let text = resp.text()?;
        let json: Value = serde_json::from_str(&text).unwrap_or(Value::String(text.clone()));
        if status.is_success() {
            Ok(json)
        } else {
            Err(FieryPitError::Status(status, json))
        }
    }

    // =========================================================================
    // Health & Status Endpoints
    // =========================================================================

    /// Health check - GET /health
    pub fn health(&self) -> Result<Value, FieryPitError> {
        self.get("/health")
    }

    /// Get status - GET /status
    pub fn status(&self) -> Result<Value, FieryPitError> {
        self.get("/status")
    }

    /// Get API info - GET /
    pub fn info(&self) -> Result<Value, FieryPitError> {
        self.get("/")
    }

    // =========================================================================
    // Evaluation Endpoints
    // =========================================================================

    /// Evaluate using current evaluator - POST /evaluate (raw JSON)
    pub fn evaluate(&self, data: Value) -> Result<Value, FieryPitError> {
        log::debug!("FieryPitClient evaluate with data: {}", data);
        self.post("/evaluate", &json!({ "data": data }))
    }

    /// Evaluate and return a typed Tephra response
    pub fn evaluate_tephra(&self, data: Value) -> Result<Tephra, FieryPitError> {
        let value = self.evaluate(data)?;
        Ok(serde_json::from_value(value)?)
    }

    // =========================================================================
    // Evaluator Management Endpoints
    // =========================================================================

    /// List all evaluators - GET /evaluators
    pub fn list_evaluators(&self) -> Result<Value, FieryPitError> {
        log::debug!("FieryPitClient list_evaluators");
        self.get("/evaluators")
    }

    /// Get evaluator details - GET /evaluators/{name}
    pub fn get_evaluator(&self, name: &str) -> Result<Value, FieryPitError> {
        log::debug!("FieryPitClient get_evaluator {}", name);
        self.get(&format!("/evaluators/{}", name))
    }

    /// Set current evaluator - POST /evaluators/set (raw JSON)
    pub fn set_evaluator(&self, evaluator: &str) -> Result<Value, FieryPitError> {
        log::debug!("FieryPitClient set_evaluator to {}", evaluator);
        self.post("/evaluators/set", &SetEvaluatorRequest {
            evaluator: evaluator.to_string(),
        })
    }

    /// Set current evaluator and return typed response
    pub fn set_evaluator_typed(&self, evaluator: &str) -> Result<EvaluatorActionResponse, FieryPitError> {
        let value = self.set_evaluator(evaluator)?;
        Ok(serde_json::from_value(value)?)
    }

    /// Reset to default evaluator - POST /evaluators/reset
    pub fn reset_evaluator(&self) -> Result<Value, FieryPitError> {
        log::debug!("FieryPitClient reset_evaluator");
        self.post("/evaluators/reset", &json!({}))
    }

    // =========================================================================
    // CLIPS Session Endpoints
    // =========================================================================

    /// Create CLIPS session - POST /clips/sessions
    pub fn clips_create_session(&self, req: CreateSessionRequest) -> Result<Value, FieryPitError> {
        self.post("/clips/sessions", &req)
    }

    /// List CLIPS sessions - GET /clips/sessions
    pub fn clips_list_sessions(&self) -> Result<Value, FieryPitError> {
        self.get("/clips/sessions")
    }

    /// Get CLIPS session - GET /clips/sessions/{id}
    pub fn clips_get_session(&self, session_id: &str) -> Result<Value, FieryPitError> {
        self.get(&format!("/clips/sessions/{}", session_id))
    }

    /// Terminate CLIPS session - DELETE /clips/sessions/{id}
    pub fn clips_terminate_session(&self, session_id: &str) -> Result<Value, FieryPitError> {
        self.delete(&format!("/clips/sessions/{}", session_id))
    }

    /// Evaluate CLIPS code - POST /clips/sessions/{id}/evaluate
    pub fn clips_evaluate(
        &self,
        session_id: &str,
        script: &str,
        timeout_ms: Option<i32>,
    ) -> Result<Value, FieryPitError> {
        self.post(
            &format!("/clips/sessions/{}/evaluate", session_id),
            &ClipsEvalRequest {
                script: script.to_string(),
                timeout_ms,
            },
        )
    }

    /// Load CLIPS rules - POST /clips/sessions/{id}/rules
    pub fn clips_load_rules(
        &self,
        session_id: &str,
        rules: Vec<String>,
    ) -> Result<Value, FieryPitError> {
        self.post(
            &format!("/clips/sessions/{}/rules", session_id),
            &ClipsLoadRulesRequest { rules },
        )
    }

    /// Load CLIPS facts - POST /clips/sessions/{id}/facts
    pub fn clips_load_facts(
        &self,
        session_id: &str,
        facts: Vec<String>,
    ) -> Result<Value, FieryPitError> {
        self.post(
            &format!("/clips/sessions/{}/facts", session_id),
            &ClipsLoadFactsRequest { facts },
        )
    }

    /// Query CLIPS facts - GET /clips/sessions/{id}/facts
    pub fn clips_query_facts(
        &self,
        session_id: &str,
        pattern: Option<&str>,
    ) -> Result<Value, FieryPitError> {
        let path = match pattern {
            Some(p) => format!(
                "/clips/sessions/{}/facts?pattern={}",
                session_id,
                urlencoding::encode(p)
            ),
            None => format!("/clips/sessions/{}/facts", session_id),
        };
        self.get(&path)
    }

    /// Run CLIPS rule engine - POST /clips/sessions/{id}/run
    pub fn clips_run(
        &self,
        session_id: &str,
        max_iterations: Option<i32>,
    ) -> Result<Value, FieryPitError> {
        self.post(
            &format!("/clips/sessions/{}/run", session_id),
            &ClipsRunRequest {
                max_iterations: max_iterations.unwrap_or(-1),
            },
        )
    }

    // =========================================================================
    // Prolog Session Endpoints
    // =========================================================================

    /// Create Prolog session - POST /prolog/sessions (raw JSON)
    pub fn prolog_create_session(&self, req: CreateSessionRequest) -> Result<Value, FieryPitError> {
        self.post("/prolog/sessions", &req)
    }

    /// Create a Prolog session and return just the session_id
    pub fn prolog_create_session_id(&self, req: CreateSessionRequest) -> Result<String, FieryPitError> {
        let value = self.prolog_create_session(req)?;
        // Try several common shapes: direct string, .session_id, .id
        if let Some(s) = value.as_str() {
            return Ok(s.to_string());
        }
        if let Some(obj) = value.as_object() {
            if let Some(id) = obj.get("session_id").and_then(|v| v.as_str()) {
                return Ok(id.to_string());
            }
            if let Some(id) = obj.get("id").and_then(|v| v.as_str()) {
                return Ok(id.to_string());
            }
        }
        Err(FieryPitError::Status(
            reqwest::StatusCode::INTERNAL_SERVER_ERROR,
            json!({ "message": format!("No session_id in response: {}", value) }),
        ))
    }

    /// List Prolog sessions - GET /prolog/sessions
    pub fn prolog_list_sessions(&self) -> Result<Value, FieryPitError> {
        self.get("/prolog/sessions")
    }

    /// Get Prolog session - GET /prolog/sessions/{id}
    pub fn prolog_get_session(&self, session_id: &str) -> Result<Value, FieryPitError> {
        self.get(&format!("/prolog/sessions/{}", session_id))
    }

    /// Terminate Prolog session - DELETE /prolog/sessions/{id}
    pub fn prolog_terminate_session(&self, session_id: &str) -> Result<Value, FieryPitError> {
        self.delete(&format!("/prolog/sessions/{}", session_id))
    }

    /// Execute Prolog query - POST /prolog/sessions/{id}/query
    pub fn prolog_query(
        &self,
        session_id: &str,
        goal: &str,
        all_solutions: bool,
    ) -> Result<Value, FieryPitError> {
        self.post(
            &format!("/prolog/sessions/{}/query", session_id),
            &PrologQueryRequest {
                goal: goal.to_string(),
                all_solutions,
            },
        )
    }

    /// Consult Prolog clauses - POST /prolog/sessions/{id}/consult
    pub fn prolog_consult(
        &self,
        session_id: &str,
        clauses: Vec<String>,
    ) -> Result<Value, FieryPitError> {
        self.post(
            &format!("/prolog/sessions/{}/consult", session_id),
            &PrologConsultRequest { clauses },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let client = FieryPitClient::new("http://localhost:8000");
        assert_eq!(client.base_url.as_ref(), "http://localhost:8000");
    }

    #[test]
    fn test_client_creation_trims_slash() {
        let client = FieryPitClient::new("http://localhost:8000/");
        assert_eq!(client.base_url.as_ref(), "http://localhost:8000");
    }

    #[test]
    fn test_session_config_serialization() {
        let config = SessionConfig {
            max_facts: Some(1000),
            max_rules: None,
            max_memory_mb: Some(128),
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("max_facts"));
        assert!(json.contains("1000"));
        assert!(!json.contains("max_rules")); // None should be skipped
    }

    #[test]
    fn test_create_session_request_serialization() {
        let req = CreateSessionRequest {
            user_id: "test_user".to_string(),
            name: Some("my-session".to_string()),
            config: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("test_user"));
        assert!(json.contains("my-session"));
    }
}
