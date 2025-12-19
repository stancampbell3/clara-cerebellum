//! DemonicVoice - Synchronous client for lil-daemon REST instances
//!
//! Provides a small blocking client to call a lil-daemon's /evaluate endpoint.
//! Lil-daemons are REST services offering arbitrary JSON evaluation via LLMs,
//! rule engines, or other evaluators.

use reqwest::blocking::Client;
use serde_json::Value;
use thiserror::Error;
use std::sync::Arc;

#[derive(Error, Debug)]
pub enum DemonicVoiceError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("non-success status {0}: {1}")]
    Status(reqwest::StatusCode, Value),
    #[error("invalid base url: {0}")]
    InvalidBaseUrl(String),
}

#[derive(Clone)]
pub struct DemonicVoice {
    base_url: Arc<String>,
    client: Client,
}

impl DemonicVoice {
    /// Create a new client for a lil-daemon instance
    ///
    /// # Arguments
    /// * `base_url` - Base URL of the lil-daemon, e.g. "http://localhost:8000"
    pub fn new(base_url: impl Into<String>) -> Self {
        let base = base_url.into();
        DemonicVoice {
            base_url: Arc::new(base),
            client: Client::new(),
        }
    }

    /// Evaluate a JSON payload via the lil-daemon's /evaluate endpoint.
    /// Returns the JSON response on success.
    pub fn evaluate(&self, payload: Value) -> Result<Value, DemonicVoiceError> {
        let url = format!("{}/evaluate", self.base_url.as_ref().trim_end_matches('/'));
        log::debug!("DemonicVoice::evaluate -> POST {} with payload: {}", url, payload);
        let resp = self.client.post(&url).json(&payload).send()?;
        let status = resp.status();
        // Read response body as text first so we can return it in the Status error if needed.
        let text = resp.text()?;
        let json: Value = serde_json::from_str(&text).unwrap_or(Value::String(text.clone()));
        if status.is_success() {
            Ok(json)
        } else {
            Err(DemonicVoiceError::Status(status, json))
        }
    }
}
