use crate::error::ClaraResult;
use crate::types::*;
use std::collections::HashMap;

/// Session service trait - manages session lifecycle
pub trait SessionService: Send + Sync + Clone {
    /// Create a new session for a user
    fn create_session(&self, req: CreateSessionRequest) -> ClaraResult<SessionResponse>;

    /// Get session by ID
    fn get_session(&self, session_id: &str) -> ClaraResult<SessionResponse>;

    /// Get status of a session
    fn get_status(&self, session_id: &str) -> ClaraResult<StatusResponse>;

    /// Update session metadata
    fn update_session_metadata(
        &self,
        session_id: &str,
        metadata: HashMap<String, String>,
    ) -> ClaraResult<()>;

    /// Terminate a session
    fn terminate_session(&self, session_id: &str) -> ClaraResult<TerminateResponse>;

    /// List all sessions for a user
    fn list_user_sessions(&self, user_id: &str) -> ClaraResult<Vec<SessionResponse>>;
}

/// Evaluation service trait - executes CLIPS code
pub trait EvalService: Send + Sync + Clone {
    /// Evaluate a scripts-dev in the context of a session
    fn eval_session(&self, session_id: &str, req: EvalRequest) -> ClaraResult<EvalResponse>;

    /// Execute an ephemeral evaluation (no session state)
    fn eval_ephemeral(&self, req: EvalRequest) -> ClaraResult<EvalResponse>;
}

/// Resource loading service trait
pub trait LoadService: Send + Sync + Clone {
    /// Load files/rules into a session
    fn load_session(&self, session_id: &str, req: LoadRequest) -> ClaraResult<LoadResponse>;
}

/// Persistence service trait (future)
pub trait PersistenceService: Send + Sync + Clone {
    /// Save session state
    fn save_session(&self, session_id: &str, req: SaveRequest) -> ClaraResult<SaveResponse>;

    /// Reload session from saved state
    fn reload_session(&self, session_id: &str, req: ReloadRequest) -> ClaraResult<ReloadResponse>;
}

/// REPL protocol handler - manages communication with CLIPS subprocess
pub trait ReplProtocol: Send + Sync {
    /// Initialize connection to CLIPS
    fn initialize(&mut self) -> ClaraResult<()>;

    /// Send a command and get output
    fn execute(&mut self, command: &str, timeout_ms: u64) -> ClaraResult<EvalResult>;

    /// Check if subprocess is alive
    fn is_alive(&self) -> bool;

    /// Terminate the subprocess
    fn terminate(&mut self) -> ClaraResult<()>;
}

/// Security filter for command validation
pub trait SecurityFilter: Send + Sync {
    /// Check if a command is allowed to execute
    fn is_allowed(&self, command: &str) -> Result<(), String>;

    /// Validate file path for access
    fn validate_file_path(&self, path: &str) -> Result<(), String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mock session service for testing
    #[derive(Clone)]
    struct MockSessionService;

    impl SessionService for MockSessionService {
        fn create_session(&self, req: CreateSessionRequest) -> ClaraResult<SessionResponse> {
            Ok(SessionResponse {
                session_id: "sess-test-123".to_string(),
                user_id: req.user_id,
                started: "2025-10-23T17:03:00Z".to_string(),
                touched: "2025-10-23T17:03:00Z".to_string(),
                status: "active".to_string(),
                resources: ResourceInfo::default(),
                limits: None,
            })
        }

        fn get_session(&self, session_id: &str) -> ClaraResult<SessionResponse> {
            Ok(SessionResponse {
                session_id: session_id.to_string(),
                user_id: "user-123".to_string(),
                started: "2025-10-23T17:03:00Z".to_string(),
                touched: "2025-10-23T17:03:00Z".to_string(),
                status: "active".to_string(),
                resources: ResourceInfo::default(),
                limits: None,
            })
        }

        fn get_status(&self, session_id: &str) -> ClaraResult<StatusResponse> {
            Ok(StatusResponse {
                session_id: session_id.to_string(),
                user_id: "user-123".to_string(),
                started: "2025-10-23T17:03:00Z".to_string(),
                touched: "2025-10-23T17:03:00Z".to_string(),
                status: "active".to_string(),
                resources: ResourceInfo::default(),
                limits: ResourceInfo {
                    facts: 1000,
                    rules: 500,
                    objects: 0,
                    memory_mb: Some(128),
                },
                health: "ok".to_string(),
            })
        }

        fn update_session_metadata(
            &self,
            _session_id: &str,
            _metadata: HashMap<String, String>,
        ) -> ClaraResult<()> {
            Ok(())
        }

        fn terminate_session(&self, session_id: &str) -> ClaraResult<TerminateResponse> {
            Ok(TerminateResponse {
                session_id: session_id.to_string(),
                status: "terminated".to_string(),
                saved: false,
            })
        }

        fn list_user_sessions(&self, user_id: &str) -> ClaraResult<Vec<SessionResponse>> {
            Ok(vec![SessionResponse {
                session_id: "sess-test-123".to_string(),
                user_id: user_id.to_string(),
                started: "2025-10-23T17:03:00Z".to_string(),
                touched: "2025-10-23T17:03:00Z".to_string(),
                status: "active".to_string(),
                resources: ResourceInfo::default(),
                limits: None,
            }])
        }
    }

    #[test]
    fn test_mock_session_service() {
        let service = MockSessionService;
        let req = CreateSessionRequest::new("user-123".to_string());
        let result = service.create_session(req);
        assert!(result.is_ok());
    }
}
