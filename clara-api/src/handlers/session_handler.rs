use actix_web::{web, HttpResponse};
use clara_session::SessionManager;
use crate::subprocess::SubprocessPool;

use crate::models::{ApiError, CreateSessionRequest, ResourceInfo, SessionResponse, TerminateResponse};

/// Application state
#[derive(Clone)]
pub struct AppState {
    pub session_manager: SessionManager,
    pub subprocess_pool: SubprocessPool,
}

/// Convert a clara-session::Session to API SessionResponse
fn session_to_response(session: &clara_session::Session) -> SessionResponse {
    SessionResponse {
        session_id: session.session_id.to_string(),
        user_id: session.user_id.clone(),
        started: format_timestamp(session.created_at),
        touched: format_timestamp(session.touched_at),
        status: session.status.to_string(),
        resources: ResourceInfo {
            facts: session.resources.facts,
            rules: session.resources.rules,
            objects: session.resources.objects,
            memory_mb: None,
        },
        limits: Some(ResourceInfo {
            facts: session.limits.max_facts,
            rules: session.limits.max_rules,
            objects: 0,
            memory_mb: Some(session.limits.max_memory_mb),
        }),
    }
}

fn format_timestamp(secs: u64) -> String {
    // Convert Unix timestamp to ISO8601 string
    chrono::DateTime::from_timestamp(secs as i64, 0)
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string())
}

/// POST /sessions - Create a new session
pub async fn create_session(
    state: web::Data<AppState>,
    req: web::Json<CreateSessionRequest>,
) -> Result<HttpResponse, ApiError> {
    log::info!("Creating session for user: {}", req.user_id);

    let session = state
        .session_manager
        .create_session(req.user_id.clone(), None)
        .map_err(ApiError::from)?;

    let response = session_to_response(&session);
    Ok(HttpResponse::Created().json(response))
}

/// GET /sessions/{session_id} - Get session details
pub async fn get_session(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let session_id = path.into_inner();
    log::info!("Getting session: {}", session_id);

    let session_id = clara_session::SessionId(session_id);
    let session = state
        .session_manager
        .get_session(&session_id)
        .map_err(ApiError::from)?;

    let response = session_to_response(&session);
    Ok(HttpResponse::Ok().json(response))
}

/// GET /sessions/user/{user_id} - List sessions for a user
pub async fn list_user_sessions(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let user_id = path.into_inner();
    log::info!("Listing sessions for user: {}", user_id);

    let sessions = state
        .session_manager
        .get_user_sessions(&user_id)
        .map_err(ApiError::from)?;

    let responses: Vec<SessionResponse> = sessions
        .iter()
        .map(session_to_response)
        .collect();

    Ok(HttpResponse::Ok().json(responses))
}

/// DELETE /sessions/{session_id} - Terminate a session
pub async fn terminate_session(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let session_id_str = path.into_inner();
    log::info!("Terminating session: {}", session_id_str);

    let session_id = clara_session::SessionId(session_id_str.clone());
    let session = state
        .session_manager
        .terminate_session(&session_id)
        .map_err(ApiError::from)?;

    // With transactional model, no persistent subprocess to terminate
    // Each eval spawns and cleans up its own process

    let response = TerminateResponse {
        session_id: session.session_id.to_string(),
        status: "terminated".to_string(),
        saved: false,
    };

    Ok(HttpResponse::Ok().json(response))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_timestamp() {
        let ts = 1729700580; // 2024-10-23 17:03:00 UTC
        let formatted = format_timestamp(ts);
        assert!(formatted.contains("2024-10-23"));
    }
}
