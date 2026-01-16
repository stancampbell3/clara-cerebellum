use anyhow::Result;
use log::debug;
use serde_json::{json, Value};

pub struct PrologClient {
    base_url: String,
    session_id: String,
    http_client: reqwest::Client,
}

#[derive(serde::Deserialize)]
pub struct QueryResponse {
    pub result: String,
    pub success: bool,
    pub runtime_ms: u64,
}

#[derive(serde::Deserialize)]
pub struct ConsultResponse {
    pub status: String,
    pub count: usize,
}

impl PrologClient {
    pub fn new(base_url: String, session_id: String) -> Self {
        Self {
            base_url,
            session_id,
            http_client: reqwest::Client::new(),
        }
    }

    /// Execute a Prolog query
    pub async fn query(&self, goal: &str, all_solutions: bool) -> Result<QueryResponse> {
        let url = format!(
            "{}/devils/sessions/{}/query",
            self.base_url, self.session_id
        );

        debug!("POST {} with goal: {}", url, goal);

        let response = self
            .http_client
            .post(&url)
            .json(&json!({
                "goal": goal,
                "all_solutions": all_solutions
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

        let query_response = response.json::<QueryResponse>().await?;
        Ok(query_response)
    }

    /// Load clauses into the knowledge base
    pub async fn consult(&self, clauses: &[String]) -> Result<ConsultResponse> {
        let url = format!(
            "{}/devils/sessions/{}/consult",
            self.base_url, self.session_id
        );

        debug!("POST {} with {} clauses", url, clauses.len());

        let response = self
            .http_client
            .post(&url)
            .json(&json!({
                "clauses": clauses
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

        let consult_response = response.json::<ConsultResponse>().await?;
        Ok(consult_response)
    }

    /// Get session info (to check status)
    pub async fn get_session_info(&self) -> Result<Value> {
        let url = format!("{}/devils/sessions/{}", self.base_url, self.session_id);

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

    /// Create a new Prolog session and return the session ID
    pub async fn ensure_session(&self, user_id: &str) -> Result<String> {
        let url = format!("{}/devils/sessions", self.base_url);

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
