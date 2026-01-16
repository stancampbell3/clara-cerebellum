use crate::client::PrologClient;
use log::{debug, error};
use serde_json::{json, Value};

/// Execute a Prolog query
pub async fn query(client: &PrologClient, args: &Value) -> Value {
    let goal = match args.get("goal").and_then(|v| v.as_str()) {
        Some(g) => g,
        None => {
            return json!({
                "error": "Missing required parameter: goal"
            });
        }
    };

    let all_solutions = args
        .get("all_solutions")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    debug!("Querying: {} (all_solutions: {})", goal, all_solutions);

    match client.query(goal, all_solutions).await {
        Ok(response) => {
            json!({
                "success": response.success,
                "result": response.result,
                "runtime_ms": response.runtime_ms
            })
        }
        Err(e) => {
            error!("Query failed: {}", e);
            json!({
                "error": e.to_string()
            })
        }
    }
}

/// Load clauses into the Prolog knowledge base
pub async fn consult(client: &PrologClient, args: &Value) -> Value {
    let clauses = match args.get("clauses").and_then(|v| v.as_array()) {
        Some(c) => c,
        None => {
            return json!({
                "error": "Missing required parameter: clauses (must be array)"
            });
        }
    };

    let clause_strings: Vec<String> = clauses
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect();

    if clause_strings.is_empty() {
        return json!({
            "error": "No valid clauses provided"
        });
    }

    debug!("Consulting {} clauses", clause_strings.len());

    match client.consult(&clause_strings).await {
        Ok(response) => {
            json!({
                "success": true,
                "status": response.status,
                "count": response.count
            })
        }
        Err(e) => {
            error!("Consult failed: {}", e);
            json!({
                "error": e.to_string()
            })
        }
    }
}

/// Retract clauses from the Prolog knowledge base
pub async fn retract(client: &PrologClient, args: &Value) -> Value {
    let clause = match args.get("clause").and_then(|v| v.as_str()) {
        Some(c) => c,
        None => {
            return json!({
                "error": "Missing required parameter: clause"
            });
        }
    };

    let all = args
        .get("all")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    debug!("Retracting: {} (all: {})", clause, all);

    // Use retractall if all=true, otherwise retract
    let goal = if all {
        format!("retractall({})", clause)
    } else {
        format!("retract({})", clause)
    };

    match client.query(&goal, false).await {
        Ok(response) => {
            json!({
                "success": response.success,
                "result": response.result,
                "runtime_ms": response.runtime_ms
            })
        }
        Err(e) => {
            error!("Retract failed: {}", e);
            json!({
                "error": e.to_string()
            })
        }
    }
}

/// Get Prolog engine status
pub async fn status(client: &PrologClient, _args: &Value) -> Value {
    debug!("Getting Prolog status");

    // Try to get session info
    match client.get_session_info().await {
        Ok(session_info) => {
            json!({
                "success": true,
                "session": session_info
            })
        }
        Err(e) => {
            error!("Status failed: {}", e);
            json!({
                "error": e.to_string()
            })
        }
    }
}
