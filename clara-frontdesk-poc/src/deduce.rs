use reqwest::blocking::Client;
use serde_json::{json, Value};
use std::thread;
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DeduceError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Deduction error: {0}")]
    Logic(String),
}

/// POST /deduce, then poll GET /deduce/{id} until terminal, return result JSON.
pub fn run_deduce(
    client: &Client,
    clara_api_url: &str,
    prolog_clauses: Vec<String>,
    clips_file: &str,
    initial_goal: &str,
    context: Vec<Value>,
    max_cycles: u32,
) -> Result<Value, DeduceError> {
    let body = json!({
        "prolog_clauses":   prolog_clauses,
        "clips_constructs": [],
        "clips_file":       clips_file,
        "initial_goal":     initial_goal,
        "context":          context,
        "max_cycles":       max_cycles
    });

    let start_url = format!("{}/deduce", clara_api_url);
    let start_resp: Value = client
        .post(&start_url)
        .json(&body)
        .send()?
        .json()?;

    let deduction_id = start_resp["deduction_id"]
        .as_str()
        .ok_or_else(|| DeduceError::Logic(format!("No deduction_id in response: {}", start_resp)))?
        .to_string();

    let poll_url = format!("{}/deduce/{}", clara_api_url, deduction_id);

    loop {
        let poll: Value = client.get(&poll_url).send()?.json()?;
        let status = poll["status"].as_str().unwrap_or("unknown");

        if status == "running" {
            thread::sleep(Duration::from_millis(150));
            continue;
        }

        if status.starts_with("error") {
            return Err(DeduceError::Logic(format!("Deduction failed: {}", status)));
        }

        // converged or interrupted — either way return what we have
        return Ok(poll);
    }
}

/// Extract all string values bound to `var_name` from prolog_solutions.
pub fn extract_solutions(result: &Value, var_name: &str) -> Vec<String> {
    result["result"]["prolog_solutions"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|sol| sol[var_name].as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}
