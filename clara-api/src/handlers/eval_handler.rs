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

    // Set session to Evaluating state
    log::debug!("Setting session status to Evaluating");
    let mut session = _session;
    session.start_evaluating();
    state
        .session_manager
        .update_session(session.clone())
        .map_err(|e| {
            log::error!("Failed to update session status {}: {:?}", session_id, e);
            ApiError::from(e)
        })?;

    // Execute the script using FFI
    log::debug!("Executing script via CLIPS FFI");
    let start = std::time::Instant::now();

    let result = state
        .session_manager
        .with_clips_env(&session_id_obj, |env| {
            env.eval(&req.script)
        })
        .map_err(|e| {
            log::error!("FFI execution failed for session {}: {:?}", session_id, e);
            // Return session to Active state on error
            session.status = clara_session::SessionStatus::Active;
            let _ = state.session_manager.update_session(session.clone());
            ApiError::from(e)
        })?;

    let elapsed_ms = start.elapsed().as_millis() as u64;

    // Complete evaluation and update session stats
    session.complete_evaluation(None); // TODO: extract rules_fired from result
    state
        .session_manager
        .update_session(session.clone())
        .map_err(|e| {
            log::error!("Failed to update session after evaluation {}: {:?}", session_id, e);
            ApiError::from(e)
        })?;

    let eval_result = clara_core::EvalResult::success(
        result,
        clara_core::EvalMetrics::with_elapsed(elapsed_ms)
    );

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
