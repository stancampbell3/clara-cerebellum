//! FieryPitClient - Synchronous client for FieryPit REST API
//!
//! Provides a blocking client to interact with the FieryPit REST API in lildaemon.
//! Supports health checks, evaluator management, evaluation monitoring,
//! hung-detector control, fish (input translator) management, CLIPS sessions,
//! and Prolog sessions.

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

// =========================================================================
// Session request types (CLIPS + Prolog)
// =========================================================================

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

// =========================================================================
// Evaluator request types
// =========================================================================

/// Optional authentication configuration for an evaluator.
/// `auth_token` is accepted but never logged by the server.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EvaluatorAuth {
    /// e.g. "bearer" or "basic"
    pub auth_type: String,
    /// Direct token value (avoid if possible; prefer `auth_token_env`)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_token: Option<String>,
    /// Name of an environment variable holding the token
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_token_env: Option<String>,
}

/// POST /evaluators/set
#[derive(Debug, Clone, Serialize)]
pub struct SetEvaluatorRequest {
    pub evaluator: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth: Option<EvaluatorAuth>,
}

/// POST /evaluators/{name}/load
#[derive(Debug, Clone, Serialize, Default)]
pub struct LoadEvaluatorRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth: Option<EvaluatorAuth>,
}

/// POST /evaluators/{name}/fish
#[derive(Debug, Clone, Serialize)]
pub struct SetFishRequest {
    pub fish: String,
}

/// POST /hung-detector/configure — all fields optional
#[derive(Debug, Clone, Serialize, Default)]
pub struct HungDetectorConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hung_threshold_seconds: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_cancel_hung: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub check_interval_seconds: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warn_threshold_seconds: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub critical_threshold_seconds: Option<f64>,
}

// =========================================================================
// Response types
// =========================================================================

/// Response from POST /evaluators/set, POST /evaluators/reset,
/// POST /evaluators/{name}/load
#[derive(Debug, Clone, Deserialize)]
pub struct EvaluatorActionResponse {
    pub status: String,
    pub evaluator: Option<String>,
    /// Present on /load responses
    #[serde(default)]
    pub auth_configured: Option<bool>,
}

/// Response from GET /evaluators/{name}/auth-status
#[derive(Debug, Clone, Deserialize)]
pub struct EvaluatorAuthStatus {
    pub auth_configured: bool,
    #[serde(default)]
    pub auth_type: Option<String>,
    #[serde(default)]
    pub auth_token_env_set: Option<bool>,
}

/// A single evaluation entry from the monitoring endpoints
#[derive(Debug, Clone, Deserialize)]
pub struct EvaluationEntry {
    pub task_id: String,
    pub status: String,
    #[serde(default)]
    pub evaluator: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub prompt_preview: Option<String>,
    #[serde(default)]
    pub started_at: Option<f64>,
    #[serde(default)]
    pub completed_at: Option<f64>,
    #[serde(default)]
    pub duration_ms: Option<i64>,
}

/// Response from GET /evaluations/stats
#[derive(Debug, Clone, Deserialize)]
pub struct EvaluationStats {
    #[serde(default)]
    pub running: u64,
    #[serde(default)]
    pub completed: u64,
    #[serde(default)]
    pub cancelled: u64,
    #[serde(default)]
    pub failed: u64,
    #[serde(default)]
    pub total_tracked: u64,
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

// =========================================================================
// Client
// =========================================================================

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
    /// * `base_url` - Base URL of the FieryPit API, e.g. "http://localhost:6666"
    pub fn new(base_url: impl Into<String>) -> Self {
        let base = base_url.into();
        FieryPitClient {
            base_url: Arc::new(base.trim_end_matches('/').to_string()),
            client: Client::new(),
        }
    }

    // =========================================================================
    // Internal helpers
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
    // Health & Status
    // =========================================================================

    /// Health check — GET /health
    pub fn health(&self) -> Result<Value, FieryPitError> {
        self.get("/health")
    }

    /// Current status including active evaluator — GET /status
    pub fn status(&self) -> Result<Value, FieryPitError> {
        self.get("/status")
    }

    /// API metadata — GET /
    pub fn info(&self) -> Result<Value, FieryPitError> {
        self.get("/")
    }

    // =========================================================================
    // Evaluation
    // =========================================================================

    /// Evaluate using the current active evaluator — POST /evaluate
    pub fn evaluate(&self, data: Value) -> Result<Value, FieryPitError> {
        log::debug!("FieryPitClient evaluate with data: {}", data);
        self.post("/evaluate", &json!({ "data": data }))
    }

    /// Evaluate and return a typed Tephra envelope
    pub fn evaluate_tephra(&self, data: Value) -> Result<Tephra, FieryPitError> {
        let value = self.evaluate(data)?;
        Ok(serde_json::from_value(value)?)
    }

    // =========================================================================
    // Evaluation Monitoring — /evaluations/*
    // =========================================================================

    /// List all currently running evaluations — GET /evaluations/active
    pub fn evaluations_active(&self) -> Result<Vec<EvaluationEntry>, FieryPitError> {
        let v = self.get("/evaluations/active")?;
        Ok(serde_json::from_value(v["active_evaluations"].clone()).unwrap_or_default())
    }

    /// Evaluation statistics — GET /evaluations/stats
    pub fn evaluations_stats(&self) -> Result<EvaluationStats, FieryPitError> {
        let v = self.get("/evaluations/stats")?;
        Ok(serde_json::from_value(v)?)
    }

    /// Recent evaluation history — GET /evaluations/history?limit=N
    pub fn evaluations_history(&self, limit: Option<u32>) -> Result<Vec<EvaluationEntry>, FieryPitError> {
        let path = match limit {
            Some(n) => format!("/evaluations/history?limit={}", n),
            None => "/evaluations/history".to_string(),
        };
        let v = self.get(&path)?;
        Ok(serde_json::from_value(v["evaluations"].clone()).unwrap_or_default())
    }

    /// Evaluations exceeding the hung threshold — GET /evaluations/hung
    pub fn evaluations_hung(&self) -> Result<Vec<EvaluationEntry>, FieryPitError> {
        let v = self.get("/evaluations/hung")?;
        Ok(serde_json::from_value(v["hung_evaluations"].clone()).unwrap_or_default())
    }

    /// Evaluations running longer than a threshold — GET /evaluations/long-running
    ///
    /// `threshold_seconds`: uses server's `warn_threshold` if omitted.
    pub fn evaluations_long_running(
        &self,
        threshold_seconds: Option<f64>,
    ) -> Result<Vec<EvaluationEntry>, FieryPitError> {
        let path = match threshold_seconds {
            Some(t) => format!("/evaluations/long-running?threshold={}", t),
            None => "/evaluations/long-running".to_string(),
        };
        let v = self.get(&path)?;
        Ok(serde_json::from_value(v["long_running_evaluations"].clone()).unwrap_or_default())
    }

    /// Details of a specific evaluation — GET /evaluations/{task_id}
    pub fn evaluation_get(&self, task_id: &str) -> Result<EvaluationEntry, FieryPitError> {
        let v = self.get(&format!("/evaluations/{}", task_id))?;
        Ok(serde_json::from_value(v)?)
    }

    /// Cancel a specific active evaluation — DELETE /evaluations/{task_id}
    pub fn evaluation_cancel(&self, task_id: &str) -> Result<Value, FieryPitError> {
        self.delete(&format!("/evaluations/{}", task_id))
    }

    /// Cancel all hung evaluations — POST /evaluations/cancel-hung
    pub fn evaluations_cancel_hung(&self) -> Result<Value, FieryPitError> {
        self.post("/evaluations/cancel-hung", &json!({}))
    }

    // =========================================================================
    // Hung Detector — /hung-detector/*
    // =========================================================================

    /// Hung detector status and configuration — GET /hung-detector/status
    pub fn hung_detector_status(&self) -> Result<Value, FieryPitError> {
        self.get("/hung-detector/status")
    }

    /// Update hung detector configuration — POST /hung-detector/configure
    ///
    /// All fields optional; only specified values are updated.
    pub fn hung_detector_configure(&self, config: HungDetectorConfig) -> Result<Value, FieryPitError> {
        self.post("/hung-detector/configure", &config)
    }

    // =========================================================================
    // Evaluator Management — /evaluators/*
    // =========================================================================

    /// List all available evaluators — GET /evaluators
    pub fn list_evaluators(&self) -> Result<Value, FieryPitError> {
        self.get("/evaluators")
    }

    /// Details for a specific evaluator — GET /evaluators/{name}
    pub fn get_evaluator(&self, name: &str) -> Result<Value, FieryPitError> {
        self.get(&format!("/evaluators/{}", name))
    }

    /// Authentication configuration status for an evaluator — GET /evaluators/{name}/auth-status
    pub fn get_evaluator_auth_status(&self, name: &str) -> Result<EvaluatorAuthStatus, FieryPitError> {
        let v = self.get(&format!("/evaluators/{}/auth-status", name))?;
        Ok(serde_json::from_value(v)?)
    }

    /// Set the current active evaluator — POST /evaluators/set
    ///
    /// Simple form: name only, no params or auth. Existing callers unchanged.
    pub fn set_evaluator(&self, evaluator: &str) -> Result<Value, FieryPitError> {
        log::debug!("FieryPitClient set_evaluator to {}", evaluator);
        self.post(
            "/evaluators/set",
            &SetEvaluatorRequest {
                evaluator: evaluator.to_string(),
                params: None,
                auth: None,
            },
        )
    }

    /// Set the current active evaluator with optional params and auth config.
    pub fn set_evaluator_with_config(
        &self,
        evaluator: &str,
        params: Option<Value>,
        auth: Option<EvaluatorAuth>,
    ) -> Result<Value, FieryPitError> {
        log::debug!("FieryPitClient set_evaluator_with_config to {}", evaluator);
        self.post(
            "/evaluators/set",
            &SetEvaluatorRequest {
                evaluator: evaluator.to_string(),
                params,
                auth,
            },
        )
    }

    /// Set the current active evaluator and return a typed response
    pub fn set_evaluator_typed(&self, evaluator: &str) -> Result<EvaluatorActionResponse, FieryPitError> {
        let value = self.set_evaluator(evaluator)?;
        Ok(serde_json::from_value(value)?)
    }

    /// Load/verify an evaluator with optional parameter overrides — POST /evaluators/{name}/load
    pub fn load_evaluator(
        &self,
        name: &str,
        req: LoadEvaluatorRequest,
    ) -> Result<EvaluatorActionResponse, FieryPitError> {
        let v = self.post(&format!("/evaluators/{}/load", name), &req)?;
        Ok(serde_json::from_value(v)?)
    }

    /// Load an evaluator with no overrides — convenience wrapper
    pub fn load_evaluator_simple(&self, name: &str) -> Result<EvaluatorActionResponse, FieryPitError> {
        self.load_evaluator(name, LoadEvaluatorRequest::default())
    }

    /// Reset to the default echo evaluator — POST /evaluators/reset
    pub fn reset_evaluator(&self) -> Result<Value, FieryPitError> {
        log::debug!("FieryPitClient reset_evaluator");
        self.post("/evaluators/reset", &json!({}))
    }

    /// Unload/unregister an evaluator — DELETE /evaluators/{name}
    pub fn delete_evaluator(&self, name: &str) -> Result<Value, FieryPitError> {
        self.delete(&format!("/evaluators/{}", name))
    }

    // =========================================================================
    // Fish (Input Translators) — /fish, /evaluators/{name}/fish
    // =========================================================================

    /// List all available fish (input translators) — GET /fish
    pub fn list_fish(&self) -> Result<Value, FieryPitError> {
        self.get("/fish")
    }

    /// Set the fish (input translator) for a specific evaluator — POST /evaluators/{name}/fish
    pub fn set_evaluator_fish(&self, evaluator_name: &str, fish: &str) -> Result<Value, FieryPitError> {
        self.post(
            &format!("/evaluators/{}/fish", evaluator_name),
            &SetFishRequest { fish: fish.to_string() },
        )
    }

    // =========================================================================
    // CLIPS Sessions — /clips/sessions/*
    // =========================================================================

    /// Create CLIPS session — POST /clips/sessions
    pub fn clips_create_session(&self, req: CreateSessionRequest) -> Result<Value, FieryPitError> {
        self.post("/clips/sessions", &req)
    }

    /// List CLIPS sessions — GET /clips/sessions
    pub fn clips_list_sessions(&self) -> Result<Value, FieryPitError> {
        self.get("/clips/sessions")
    }

    /// Get CLIPS session — GET /clips/sessions/{id}
    pub fn clips_get_session(&self, session_id: &str) -> Result<Value, FieryPitError> {
        self.get(&format!("/clips/sessions/{}", session_id))
    }

    /// Terminate CLIPS session — DELETE /clips/sessions/{id}
    pub fn clips_terminate_session(&self, session_id: &str) -> Result<Value, FieryPitError> {
        self.delete(&format!("/clips/sessions/{}", session_id))
    }

    /// Execute raw CLIPS code — POST /clips/sessions/{id}/evaluate
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

    /// Load CLIPS rules — POST /clips/sessions/{id}/rules
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

    /// Assert CLIPS facts — POST /clips/sessions/{id}/facts
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

    /// Query CLIPS facts — GET /clips/sessions/{id}/facts
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

    /// Run the CLIPS rule engine — POST /clips/sessions/{id}/run
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
    // Prolog Sessions — /prolog/sessions/*
    // =========================================================================

    /// Create Prolog session — POST /prolog/sessions
    pub fn prolog_create_session(&self, req: CreateSessionRequest) -> Result<Value, FieryPitError> {
        self.post("/prolog/sessions", &req)
    }

    /// Create a Prolog session and return just the session_id
    pub fn prolog_create_session_id(&self, req: CreateSessionRequest) -> Result<String, FieryPitError> {
        let value = self.prolog_create_session(req)?;
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

    /// List Prolog sessions — GET /prolog/sessions
    pub fn prolog_list_sessions(&self) -> Result<Value, FieryPitError> {
        self.get("/prolog/sessions")
    }

    /// Get Prolog session — GET /prolog/sessions/{id}
    pub fn prolog_get_session(&self, session_id: &str) -> Result<Value, FieryPitError> {
        self.get(&format!("/prolog/sessions/{}", session_id))
    }

    /// Terminate Prolog session — DELETE /prolog/sessions/{id}
    pub fn prolog_terminate_session(&self, session_id: &str) -> Result<Value, FieryPitError> {
        self.delete(&format!("/prolog/sessions/{}", session_id))
    }

    /// Execute a Prolog goal — POST /prolog/sessions/{id}/query
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

    /// Load Prolog clauses into a session — POST /prolog/sessions/{id}/consult
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

// =========================================================================
// Tests
// =========================================================================

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

    #[test]
    fn test_set_evaluator_request_no_optional_fields() {
        // Verify that simple set_evaluator omits params and auth from JSON
        let req = SetEvaluatorRequest {
            evaluator: "kindling".to_string(),
            params: None,
            auth: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("kindling"));
        assert!(!json.contains("params"));
        assert!(!json.contains("auth"));
    }

    #[test]
    fn test_hung_detector_config_empty() {
        let cfg = HungDetectorConfig::default();
        let json = serde_json::to_string(&cfg).unwrap();
        // All fields None → empty object
        assert_eq!(json, "{}");
    }
}
