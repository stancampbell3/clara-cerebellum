use reqwest::blocking::Client;
use serde_json::{json, Value};
use std::collections::HashMap;
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

        log::debug!(
            "deduce id={} finished status={} after {} polls",
            deduction_id, status, poll_count
        );
        log::debug!("deduce result: {}", serde_json::to_string_pretty(&poll).unwrap_or_default());
        return Ok(poll);
    }
}

/// Extract all string values bound to `var_name` across all solutions.
/// Used for goals like `suggestion(visitor, S).` where each solution binds S once.
#[allow(dead_code)]
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

/// Extract the first solution as a map of variable name → JSON value.
/// Used for meta-goals like `daemonic_turn/5` that return multiple named variables
/// in a single solution.
pub fn extract_named_solutions(result: &Value) -> HashMap<String, Value> {
    result["result"]["prolog_solutions"]
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|sol| sol.as_object())
        .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default()
}

/// Extract a list-valued variable from a named solution.
///
/// The clara-api may serialize a Prolog list as a JSON array or as a Prolog list
/// atom string. This handles both.
pub fn extract_list_var(sol: &HashMap<String, Value>, var_name: &str) -> Vec<String> {
    match sol.get(var_name) {
        Some(Value::Array(arr)) => {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        }
        Some(Value::String(s)) => {
            // Prolog list atom: "['item one','item two']" — strip brackets, split on ','
            let trimmed = s.trim().trim_start_matches('[').trim_end_matches(']');
            if trimmed.is_empty() {
                return vec![];
            }
            trimmed
                .split("','")
                .map(|p| p.trim_matches('\'').trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        }
        _ => vec![],
    }
}

/// Extract a string-valued variable from a named solution, defaulting to "".
pub fn extract_str_var(sol: &HashMap<String, Value>, var_name: &str) -> String {
    sol.get(var_name)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}
