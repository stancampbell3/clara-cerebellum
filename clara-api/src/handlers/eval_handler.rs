use actix_web::{web, HttpResponse};
use crate::handlers::AppState;
use crate::models::{ApiError, EvalRequest, EvalResponse, EvalMetrics};

/// POST /sessions/{session_id}/eval - Evaluate CLIPS code in a session
pub async fn eval_session(
    state: web::Data<AppState>,
    path: web::Path<String>,
    req: web::Json<EvalRequest>,
) -> Result<HttpResponse, ApiError> {
    let session_id = path.into_inner();
    log::info!("Evaluating script in session: {}", session_id);
    log::debug!("Script content: {}", req.script);
    log::debug!("Timeout: {:?}ms", req.timeout_ms);

    // Verify session exists
    let session_id_obj = clara_session::SessionId(session_id.clone());
    log::debug!("Looking up session: {}", session_id);
    let _session = state
        .session_manager
        .get_session(&session_id_obj)
        .map_err(|e| {
            log::error!("Failed to get session {}: {:?}", session_id, e);
            ApiError::from(e)
        })?;
    log::debug!("Session found successfully");

    // Touch the session to update last activity
    log::debug!("Touching session to update last activity");
    state
        .session_manager
        .touch_session(&session_id_obj)
        .map_err(|e| {
            log::error!("Failed to touch session {}: {:?}", session_id, e);
            ApiError::from(e)
        })?;

    // Execute the command in the subprocess
    log::debug!("Executing script in subprocess pool");
    let eval_result = state
        .subprocess_pool
        .execute(&session_id, &req.script, req.timeout_ms)
        .map_err(|e| {
            log::error!("Subprocess execution failed for session {}: {:?}", session_id, e);
            ApiError::from(e)
        })?;
    
    log::info!(
        "Evaluation complete: exit_code={}, elapsed={}ms",
        eval_result.exit_code,
        eval_result.metrics.elapsed_ms
    );
    log::debug!("stdout length: {} bytes", eval_result.stdout.len());
    log::debug!("stderr length: {} bytes", eval_result.stderr.len());

    // Convert to API response
    let response = EvalResponse {
        stdout: eval_result.stdout,
        stderr: eval_result.stderr,
        exit_code: eval_result.exit_code,
        metrics: EvalMetrics {
            elapsed_ms: eval_result.metrics.elapsed_ms,
            facts_added: eval_result.metrics.facts_added,
            rules_fired: eval_result.metrics.rules_fired,
        },
        session: None,
    };

    log::debug!("Returning HTTP 200 response");
    Ok(HttpResponse::Ok().json(response))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_eval_response_structure() {
        let resp = EvalResponse {
            stdout: "test output".to_string(),
            stderr: String::new(),
            exit_code: 0,
            metrics: EvalMetrics {
                elapsed_ms: 100,
                facts_added: None,
                rules_fired: None,
            },
            session: None,
        };
        assert_eq!(resp.exit_code, 0);
    }
}
