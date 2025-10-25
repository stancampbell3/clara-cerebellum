use crate::client::ClipsClient;
use log::{debug, error};
use serde_json::{json, Value};

/// Evaluate CLIPS expressions
pub async fn eval(client: &ClipsClient, args: &Value) -> Value {
    let expression = match args.get("expression").and_then(|v| v.as_str()) {
        Some(expr) => expr,
        None => {
            return json!({
                "error": "Missing required parameter: expression"
            });
        }
    };

    debug!("Evaluating: {}", expression);

    match client.eval(expression).await {
        Ok(response) => {
            json!({
                "success": true,
                "stdout": response.stdout,
                "stderr": response.stderr,
                "exit_code": response.exit_code,
                "metrics": response.metrics
            })
        }
        Err(e) => {
            error!("Eval failed: {}", e);
            json!({
                "error": e.to_string()
            })
        }
    }
}

/// Query facts from CLIPS
pub async fn query(client: &ClipsClient, args: &Value) -> Value {
    let template = match args.get("template").and_then(|v| v.as_str()) {
        Some(t) => t,
        None => {
            return json!({
                "error": "Missing required parameter: template"
            });
        }
    };

    let limit = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(100)
        .to_string();

    debug!("Querying template: {} (limit: {})", template, limit);

    // Build a (find-all-facts) expression to query
    let expression = format!(
        "(find-all-facts ((?f)) (eq ?f {}))",
        template
    );

    match client.eval(&expression).await {
        Ok(response) => {
            json!({
                "success": true,
                "results": response.stdout,
                "metrics": response.metrics
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

/// Assert facts into CLIPS
pub async fn assert_facts(client: &ClipsClient, args: &Value) -> Value {
    let facts = match args.get("facts").and_then(|v| v.as_array()) {
        Some(f) => f,
        None => {
            return json!({
                "error": "Missing required parameter: facts (must be array)"
            });
        }
    };

    debug!("Asserting {} facts", facts.len());

    // Build an assert expression for each fact
    let mut assert_expr = String::from("(progn");
    for fact in facts {
        if let Some(fact_str) = fact.as_str() {
            assert_expr.push(' ');
            assert_expr.push_str(&format!("(assert {})", fact_str));
        }
    }
    assert_expr.push(')');

    debug!("Assert expression: {}", assert_expr);

    match client.eval(&assert_expr).await {
        Ok(response) => {
            json!({
                "success": true,
                "count": facts.len(),
                "stdout": response.stdout,
                "metrics": response.metrics
            })
        }
        Err(e) => {
            error!("Assert failed: {}", e);
            json!({
                "error": e.to_string()
            })
        }
    }
}

/// Reset the CLIPS engine
pub async fn reset(client: &ClipsClient, args: &Value) -> Value {
    let _preserve_globals = args
        .get("preserve_globals")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    debug!("Resetting CLIPS engine");

    match client.eval("(reset)").await {
        Ok(response) => {
            json!({
                "success": true,
                "stdout": response.stdout,
                "metrics": response.metrics
            })
        }
        Err(e) => {
            error!("Reset failed: {}", e);
            json!({
                "error": e.to_string()
            })
        }
    }
}

/// Get engine status
pub async fn status(client: &ClipsClient, _args: &Value) -> Value {
    debug!("Getting CLIPS status");

    // Evaluate (facts) to see what facts are in the engine
    match client.eval("(facts)").await {
        Ok(response) => {
            // Try to get session info for more details
            let session_info = client
                .get_session_info()
                .await
                .unwrap_or(json!({}));

            json!({
                "success": true,
                "facts": response.stdout,
                "session": session_info,
                "metrics": response.metrics
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
