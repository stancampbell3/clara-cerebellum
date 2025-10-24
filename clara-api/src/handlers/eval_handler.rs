use actix_web::{web, HttpResponse};
use crate::handlers::AppState;
use crate::models::{ApiError, EvalRequest, EvalResponse, EvalMetrics};

/// POST /sessions/{session_id}/eval - Evaluate CLIPS code in a session
pub async fn eval_session(
    state: web::Data<AppState>,
    path: web::Path<String>,
    _req: web::Json<EvalRequest>,
) -> Result<HttpResponse, ApiError> {
    let session_id = path.into_inner();
    log::info!("Evaluating script in session: {}", session_id);

    // Verify session exists
    let session_id_obj = clara_session::SessionId(session_id.clone());
    let _session = state
        .session_manager
        .get_session(&session_id_obj)
        .map_err(ApiError::from)?;

    // Touch the session to update last activity
    state
        .session_manager
        .touch_session(&session_id_obj)
        .map_err(ApiError::from)?;

    // For MVP, return a stub response since we don't have CLIPS subprocess yet
    // In a real implementation, this would execute the script via CLIPS subprocess
    let response = EvalResponse {
        stdout: format!("(executed in session {})", session_id),
        stderr: String::new(),
        exit_code: 0,
        metrics: EvalMetrics {
            elapsed_ms: 10,
            facts_added: None,
            rules_fired: None,
        },
        session: None,
    };

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
