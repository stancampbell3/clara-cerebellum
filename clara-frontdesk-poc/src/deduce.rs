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
    log::debug!("deduce POST {} goal={:?}", start_url, initial_goal);
    log::debug!("deduce request body: {}", serde_json::to_string_pretty(&body).unwrap_or_default());

    let start_resp: Value = client
        .post(&start_url)
        .json(&body)
        .send()?
        .json()?;

    log::debug!("deduce POST response: {}", start_resp);

    let deduction_id = start_resp["deduction_id"]
        .as_str()
        .ok_or_else(|| DeduceError::Logic(format!("No deduction_id in response: {}", start_resp)))?
        .to_string();

    log::debug!("deduce id={} polling…", deduction_id);

    let poll_url = format!("{}/deduce/{}", clara_api_url, deduction_id);
    let mut poll_count = 0u32;

    loop {
        let poll: Value = client.get(&poll_url).send()?.json()?;
        let status = poll["status"].as_str().unwrap_or("unknown");
        poll_count += 1;

        if status == "running" {
            log::debug!("deduce id={} still running (poll #{})", deduction_id, poll_count);
            thread::sleep(Duration::from_millis(150));
            continue;
        }

        if status.starts_with("error") {
            log::warn!("deduce id={} error after {} polls: {}", deduction_id, poll_count, poll);
            return Err(DeduceError::Logic(format!("Deduction failed: {}", status)));
        }

        // converged or interrupted
        log::debug!(
            "deduce id={} finished status={} after {} polls",
            deduction_id, status, poll_count
        );
        log::debug!("deduce result: {}", serde_json::to_string_pretty(&poll).unwrap_or_default());
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
