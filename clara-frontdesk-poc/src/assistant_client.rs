//! Blocking HTTP client for lildaemon's goat/app/assistant/ REST API.
//!
//! Replaces the old deduce.rs (direct clara-api /deduce polling) and the
//! FieryPitClient-based /evaluate call — the assistant backend now owns the
//! whole per-turn deduction/LLM orchestration, so this frontend only needs
//! auth + three simple REST calls.

use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::{json, Value};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AssistantError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("assistant API error: {0}")]
    Api(String),
}

/// POST /auth/register (409 on an already-registered username is fine),
/// then POST /auth/token — returns a bearer JWT. Mirrors
/// examples_ritual_rumination_ingest.py's `_fierypit_bearer_token`.
pub fn register_and_login(
    http: &Client,
    base_url: &str,
    username: &str,
    password: &str,
) -> Result<String, AssistantError> {
    let register_resp = http
        .post(format!("{base_url}/auth/register"))
        .json(&json!({"username": username, "password": password}))
        .send()?;
    let status = register_resp.status();
    if !status.is_success() && status.as_u16() != 409 {
        let body = register_resp.text().unwrap_or_default();
        return Err(AssistantError::Api(format!(
            "register failed ({status}): {body}"
        )));
    }

    let token_resp = http
        .post(format!("{base_url}/auth/token"))
        .form(&[
            ("username", username),
            ("password", password),
            ("grant_type", "password"),
        ])
        .send()?;
    let status = token_resp.status();
    if !status.is_success() {
        let body = token_resp.text().unwrap_or_default();
        return Err(AssistantError::Api(format!(
            "login failed ({status}): {body}"
        )));
    }
    let body: Value = token_resp.json()?;
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
