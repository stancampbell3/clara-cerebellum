//! LilDevils (Prolog) session handlers
//!
//! REST API handlers for Prolog session management and query execution.

use actix_web::{web, HttpResponse};
use clara_core::ClaraError;
use clara_session::SessionType;

use crate::models::{
    ApiError, CreateSessionRequest, SessionResponse, ResourceInfo, TerminateResponse,
    PrologQueryRequest, PrologQueryResponse, PrologConsultRequest,
};

/// Application state (shared with session_handler)
pub use crate::handlers::session_handler::AppState;

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
    chrono::DateTime::from_timestamp(secs as i64, 0)
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string())
}

/// POST /devils/sessions - Create a new Prolog session
pub async fn create_prolog_session(
    state: web::Data<AppState>,
    req: web::Json<CreateSessionRequest>,
) -> Result<HttpResponse, ApiError> {
    log::info!("Creating Prolog session for user: {}", req.user_id);

    // Build resource limits from config if provided
    let limits = req.config.as_ref().map(|cfg| {
        clara_session::ResourceLimits {
            max_facts: cfg.max_facts.unwrap_or(1000),
            max_rules: cfg.max_rules.unwrap_or(500),
            max_memory_mb: cfg.max_memory_mb.unwrap_or(128),
        }
    });

    let session = state
        .session_manager
        .create_prolog_session(req.user_id.clone(), limits)
        .map_err(ApiError::from)?;

    let response = session_to_response(&session);
    Ok(HttpResponse::Created().json(response))
}

/// GET /devils/sessions - List all Prolog sessions
pub async fn list_prolog_sessions(
    state: web::Data<AppState>,
) -> Result<HttpResponse, ApiError> {
    log::info!("Listing all Prolog sessions");

    let sessions = state
        .session_manager
        .list_all_sessions()
        .map_err(ApiError::from)?;

    // Filter to only Prolog sessions
    let prolog_sessions: Vec<SessionResponse> = sessions
        .iter()
        .filter(|s| s.session_type == SessionType::Prolog)
        .map(session_to_response)
        .collect();

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "sessions": prolog_sessions,
        "total": prolog_sessions.len()
    })))
}

/// GET /devils/sessions/{session_id} - Get Prolog session details
pub async fn get_prolog_session(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let session_id_str = path.into_inner();
    log::info!("Getting Prolog session: {}", session_id_str);

    let session_id = clara_session::SessionId(session_id_str);
    let session = state
        .session_manager
        .get_session(&session_id)
        .map_err(ApiError::from)?;

    // Verify it's a Prolog session
    if session.session_type != SessionType::Prolog {
        return Err(ApiError::new(ClaraError::ValidationError(format!(
            "Session {} is not a Prolog session",
            session.session_id
        ))));
    }

    let response = session_to_response(&session);
    Ok(HttpResponse::Ok().json(response))
}

/// DELETE /devils/sessions/{session_id} - Terminate a Prolog session
pub async fn terminate_prolog_session(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let session_id_str = path.into_inner();
    log::info!("Terminating Prolog session: {}", session_id_str);

    let session_id = clara_session::SessionId(session_id_str.clone());
    let session = state
        .session_manager
        .terminate_prolog_session(&session_id)
        .map_err(ApiError::from)?;

    let response = TerminateResponse {
        session_id: session.session_id.to_string(),
        status: "terminated".to_string(),
        saved: false,
    };

    Ok(HttpResponse::Ok().json(response))
}

/// POST /devils/sessions/{session_id}/query - Execute a Prolog query
pub async fn query_prolog(
    state: web::Data<AppState>,
    path: web::Path<String>,
    req: web::Json<PrologQueryRequest>,
) -> Result<HttpResponse, ApiError> {
    let session_id_str = path.into_inner();
    log::info!("Executing Prolog query in session {}: {}", session_id_str, req.goal);

    let session_id = clara_session::SessionId(session_id_str);

    // Verify session exists and is a Prolog session
    let session = state
        .session_manager
        .get_session(&session_id)
        .map_err(ApiError::from)?;

    if session.session_type != SessionType::Prolog {
        return Err(ApiError::new(ClaraError::ValidationError(format!(
            "Session {} is not a Prolog session",
            session.session_id
        ))));
    }

    let start = std::time::Instant::now();

    // Execute query via Prolog environment
    let result = if req.all_solutions.unwrap_or(false) {
        state
            .session_manager
            .with_prolog_env(&session_id, |env| {
                env.query(&req.goal).map_err(|e| e.to_string())
            })
            .map_err(ApiError::from)?
    } else {
        state
            .session_manager
            .with_prolog_env(&session_id, |env| {
                env.query_once(&req.goal).map_err(|e| e.to_string())
            })
            .map_err(ApiError::from)?
    };

    let elapsed_ms = start.elapsed().as_millis() as u64;

    // Touch session to update last activity
    state
        .session_manager
        .touch_session(&session_id)
        .map_err(ApiError::from)?;

    let response = PrologQueryResponse {
        result,
        success: true,
        runtime_ms: elapsed_ms,
    };

    Ok(HttpResponse::Ok().json(response))
}

/// POST /devils/sessions/{session_id}/consult - Load Prolog clauses into session
pub async fn consult_prolog(
    state: web::Data<AppState>,
    path: web::Path<String>,
    req: web::Json<PrologConsultRequest>,
) -> Result<HttpResponse, ApiError> {
    let session_id_str = path.into_inner();
    log::info!("Loading {} clauses into Prolog session: {}", req.clauses.len(), session_id_str);

    let session_id = clara_session::SessionId(session_id_str);

    // Verify session exists and is a Prolog session
    let session = state
        .session_manager
        .get_session(&session_id)
        .map_err(ApiError::from)?;

    if session.session_type != SessionType::Prolog {
        return Err(ApiError::new(ClaraError::ValidationError(format!(
            "Session {} is not a Prolog session",
            session.session_id
        ))));
    }

    // Load each clause via assertz
    for clause in &req.clauses {
        state
            .session_manager
            .with_prolog_env(&session_id, |env| {
                // Special handling.  We use assertz/1 to load clauses,
                // but if the clause is a directive (starts with ':-' ), we use assertz/1
                // with the directive as-is.
                if clause.trim_start().starts_with(":-") {
                    log::debug!("Asserting directive into session {}: {}", session_id.0, clause);
                    env.assertz(clause).map_err(|e| e.to_string())
                } else {
                    log::debug!("Asserting clause into session {}: {}", session_id.0, clause);
                    env.assertz(&clause).map_err(|e| e.to_string())
                }
            })
            .map_err(ApiError::from)?;
    }

    // Touch session to update last activity
    state
        .session_manager
        .touch_session(&session_id)
        .map_err(ApiError::from)?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "clauses_loaded",
        "count": req.clauses.len()
    })))
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
