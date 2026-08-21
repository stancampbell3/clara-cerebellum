//! Blocking HTTP client for lildaemon's goat/app/assistant/ REST API.
//!
//! Replaces the old deduce.rs (direct clara-api /deduce polling) and the
//! FieryPitClient-based /evaluate call — the assistant backend now owns the
//! whole per-turn deduction/LLM orchestration, so this frontend only needs
//! auth + three simple REST calls.

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AssistantError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("assistant API error: {0}")]
    Api(String),
}

/// POST /auth/login-demo — no-password, per-visitor login. Upserts a
/// service-role account by username and returns a bearer JWT scoped to
/// that one identity (replaces the old shared-service-account
/// register_and_login, which every browser session used to reuse).
pub fn login_demo(http: &Client, base_url: &str, username: &str) -> Result<String, AssistantError> {
    let resp = http
        .post(format!("{base_url}/auth/login-demo"))
        .json(&json!({"username": username}))
        .send()?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().unwrap_or_default();
        return Err(AssistantError::Api(format!(
            "login-demo failed ({status}): {body}"
        )));
    }
    let body: Value = resp.json()?;
    body["access_token"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| AssistantError::Api(format!("no access_token in response: {body}")))
}

/// POST /assistant/sessions — returns the new session_id.
pub fn create_session(http: &Client, base_url: &str, token: &str) -> Result<String, AssistantError> {
    let resp = http
        .post(format!("{base_url}/assistant/sessions"))
        .bearer_auth(token)
        .send()?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().unwrap_or_default();
        return Err(AssistantError::Api(format!(
            "create_session failed ({status}): {body}"
        )));
    }
    let body: Value = resp.json()?;
    body["session_id"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| AssistantError::Api(format!("no session_id in response: {body}")))
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RulesetInfo {
    pub ruleset_key: String,
    pub label: String,
    pub description: String,
}

/// GET /assistant/rulesets — available rulesets for the ruleset dropdown.
pub fn list_rulesets(http: &Client, base_url: &str, token: &str) -> Result<Vec<RulesetInfo>, AssistantError> {
    let resp = http
        .get(format!("{base_url}/assistant/rulesets"))
        .bearer_auth(token)
        .send()?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().unwrap_or_default();
        return Err(AssistantError::Api(format!(
            "list_rulesets failed ({status}): {body}"
        )));
    }
    Ok(resp.json()?)
}

/// PUT /assistant/sessions/{id}/ruleset — set this session's active ruleset.
pub fn set_session_ruleset(
    http: &Client,
    base_url: &str,
    token: &str,
    session_id: &str,
    ruleset_key: &str,
) -> Result<(), AssistantError> {
    let resp = http
        .put(format!("{base_url}/assistant/sessions/{session_id}/ruleset"))
        .bearer_auth(token)
        .json(&json!({"ruleset_key": ruleset_key}))
        .send()?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().unwrap_or_default();
        return Err(AssistantError::Api(format!(
            "set_session_ruleset failed ({status}): {body}"
        )));
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct SendResponse {
    pub reply: String,
    pub action_taken: String,
    pub workspace_slug: Option<String>,
    #[serde(default)]
    pub citation_count: u32,
}

/// POST /assistant/sessions/{id}/send — the core per-turn call.
pub fn send(
    http: &Client,
    base_url: &str,
    token: &str,
    session_id: &str,
    text: &str,
) -> Result<SendResponse, AssistantError> {
    let resp = http
        .post(format!("{base_url}/assistant/sessions/{session_id}/send"))
        .bearer_auth(token)
        .json(&json!({"text": text}))
        .send()?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().unwrap_or_default();
        return Err(AssistantError::Api(format!(
            "send failed ({status}): {body}"
        )));
    }
    Ok(resp.json()?)
}
