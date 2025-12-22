use actix_web::{web, HttpResponse};
use clara_session::SessionManager;
use crate::subprocess::SubprocessPool;

use crate::models::{
    ApiError, CreateSessionRequest, SaveSessionRequest, ResourceInfo, SessionResponse,
    TerminateResponse, LoadRulesRequest, LoadFactsRequest, RunRequest, RunResponse, QueryFactsResponse
};

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
        .create_session_with_name(req.user_id.clone(), req.name.clone(), limits)
        .map_err(ApiError::from)?;

    let response = session_to_response(&session);
    Ok(HttpResponse::Created().json(response))
}

/// POST /sessions/{session_id}/save - Save session state (facts and rules)
pub async fn save_session(
    state: web::Data<AppState>,
    req: web::Json<SaveSessionRequest>,
) -> Result<HttpResponse, ApiError> {
    let session_id = clara_session::SessionId(req.session_id.clone());
    log::info!("Saving session: {}", req.session_id);

    state
        .session_manager
        .save_session(&session_id)
        .map_err(ApiError::from)?;

    Ok(HttpResponse::Ok().json(serde_json::json!({"status": "saved"})))
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

/// POST /sessions/{session_id}/rules - Load rules into a session
pub async fn load_rules(
    state: web::Data<AppState>,
    path: web::Path<String>,
    req: web::Json<LoadRulesRequest>,
) -> Result<HttpResponse, ApiError> {
    let session_id = clara_session::SessionId(path.into_inner());
    log::info!("Loading {} rules into session: {}", req.rules.len(), session_id);

    // Verify session exists
    let _session = state
        .session_manager
        .get_session(&session_id)
        .map_err(ApiError::from)?;

    // Load each rule via CLIPS environment
    for rule in &req.rules {
        state
            .session_manager
            .with_clips_env(&session_id, |env| {
                env.eval(rule)
            })
            .map_err(ApiError::from)?;
    }

    // Touch session to update last activity
    state
        .session_manager
        .touch_session(&session_id)
        .map_err(ApiError::from)?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "rules_loaded",
        "count": req.rules.len()
    })))
}

/// POST /sessions/{session_id}/facts - Load facts into a session
pub async fn load_facts(
    state: web::Data<AppState>,
    path: web::Path<String>,
    req: web::Json<LoadFactsRequest>,
) -> Result<HttpResponse, ApiError> {
    let session_id = clara_session::SessionId(path.into_inner());
    log::info!("Loading {} facts into session: {}", req.facts.len(), session_id);

    // Verify session exists
    let _session = state
        .session_manager
        .get_session(&session_id)
        .map_err(ApiError::from)?;

    // Load each fact via CLIPS environment
    for fact in &req.facts {
        let assert_cmd = format!("(assert {})", fact);
        state
            .session_manager
            .with_clips_env(&session_id, |env| {
                env.eval(&assert_cmd)
            })
            .map_err(ApiError::from)?;
    }

    // Touch session to update last activity
    state
        .session_manager
        .touch_session(&session_id)
        .map_err(ApiError::from)?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "facts_loaded",
        "count": req.facts.len()
    })))
}

/// POST /sessions/{session_id}/run - Run rules in a session
pub async fn run_rules(
    state: web::Data<AppState>,
    path: web::Path<String>,
    req: web::Json<RunRequest>,
) -> Result<HttpResponse, ApiError> {
    let session_id = clara_session::SessionId(path.into_inner());
    log::info!("Running rules in session: {} with max_iterations: {}", session_id, req.max_iterations);

    // Verify session exists
    let _session = state
        .session_manager
        .get_session(&session_id)
        .map_err(ApiError::from)?;

    let start = std::time::Instant::now();

    // Run rules via CLIPS environment
    let run_cmd = if req.max_iterations < 0 {
        "(run)".to_string()
    } else {
        format!("(run {})", req.max_iterations)
    };

    let result = state
        .session_manager
        .with_clips_env(&session_id, |env| {
            env.eval(&run_cmd)
        })
        .map_err(ApiError::from)?;

    let elapsed_ms = start.elapsed().as_millis() as u64;

    // Parse result to get rules fired count
    let rules_fired = result.trim().parse::<u64>().unwrap_or(0);

    // Touch session to update last activity
    state
        .session_manager
        .touch_session(&session_id)
        .map_err(ApiError::from)?;

    let response = RunResponse {
        rules_fired,
        status: "completed".to_string(),
        runtime_ms: elapsed_ms,
    };

    Ok(HttpResponse::Ok().json(response))
}

/// GET /sessions/{session_id}/facts - Query facts in a session
pub async fn query_facts(
    state: web::Data<AppState>,
    path: web::Path<String>,
    query: web::Query<std::collections::HashMap<String, String>>,
) -> Result<HttpResponse, ApiError> {
    let session_id = clara_session::SessionId(path.into_inner());
    let pattern = query.get("pattern").cloned().unwrap_or_else(|| "?f".to_string());

    log::info!("Querying facts in session: {} with pattern: {}", session_id, pattern);

    // Verify session exists
    let _session = state
        .session_manager
        .get_session(&session_id)
        .map_err(ApiError::from)?;

    // Query facts via CLIPS environment
    let query_cmd = format!("(find-all-facts ((?f)) TRUE)");
    let result = state
        .session_manager
        .with_clips_env(&session_id, |env| {
            env.eval(&query_cmd)
        })
        .map_err(ApiError::from)?;

    // Parse result into list of facts
    // For now, just split by lines and filter empty
    let matches: Vec<String> = result
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|s| s.to_string())
        .collect();

    let count = matches.len();

    let response = QueryFactsResponse {
        matches,
        count,
    };

    Ok(HttpResponse::Ok().json(response))
}

/// GET /sessions - List all sessions
pub async fn list_all_sessions(
    state: web::Data<AppState>,
) -> Result<HttpResponse, ApiError> {
    log::info!("Listing all sessions");

    let sessions = state
        .session_manager
        .list_all_sessions()
        .map_err(ApiError::from)?;

    let responses: Vec<SessionResponse> = sessions
        .iter()
        .map(session_to_response)
        .collect();

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "sessions": responses,
        "total": responses.len()
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
