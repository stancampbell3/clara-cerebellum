use anyhow::Result;
use log::debug;
use serde_json::{json, Value};

pub struct ClipsClient {
    base_url: String,
    session_id: String,
    http_client: reqwest::Client,
}

#[derive(serde::Deserialize)]
pub struct EvalResponse {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub metrics: serde_json::Value,
}

impl ClipsClient {
    pub fn new(base_url: String, session_id: String) -> Self {
        Self {
            base_url,
            session_id,
            http_client: reqwest::Client::new(),
        }
    }

    /// Evaluate CLIPS expression
    pub async fn eval(&self, script: &str) -> Result<EvalResponse> {
        let url = format!(
            "{}/sessions/{}/eval",
            self.base_url, self.session_id
        );

        debug!("POST {} with script: {}", url, script);

        let response = self
            .http_client
            .post(&url)
            .json(&json!({
                "script": script
            }))
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await?;
            return Err(anyhow::anyhow!(
                "HTTP {} from {}: {}",
                status,
                url,
                body
            ));
        }

        let eval_response = response.json::<EvalResponse>().await?;
        Ok(eval_response)
    }

    /// Get session info (to check status)
    pub async fn get_session_info(&self) -> Result<Value> {
        let url = format!("{}/sessions/{}", self.base_url, self.session_id);

        debug!("GET {}", url);

        let response = self.http_client.get(&url).send().await?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await?;
            return Err(anyhow::anyhow!(
                "HTTP {} from {}: {}",
                status,
                url,
                body
            ));
        }

        let session_info = response.json::<Value>().await?;
        Ok(session_info)
    }

    /// Create a new session and return the session ID
    pub async fn ensure_session(&self, user_id: &str) -> Result<String> {
        let url = format!("{}/sessions", self.base_url);

        debug!("POST {} for user {}", url, user_id);

        let response = self
            .http_client
            .post(&url)
            .json(&json!({
                "user_id": user_id
            }))
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await?;
            return Err(anyhow::anyhow!(
                "HTTP {} from {}: {}",
                status,
                url,
                body
            ));
        }

        let session_data = response.json::<serde_json::Value>().await?;
        let session_id = session_data
            .get("session_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("No session_id in response"))?
            .to_string();

        Ok(session_id)
    }
}
