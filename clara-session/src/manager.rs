use crate::metadata::{ResourceLimits, Session, SessionId, SessionStatus, SessionType};
use crate::store::{SessionStore, StoreError};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ManagerError {
    #[error("Store error: {0}")]
    Store(#[from] StoreError),

    #[error("Too many sessions for user")]
    UserSessionLimitExceeded,

    #[error("Global session limit exceeded")]
    GlobalSessionLimitExceeded,

    #[error("Session already terminated")]
    SessionTerminated,

    #[error("Session not found")]
    SessionNotFound,

    #[error("Wrong session type: expected {expected}, got {actual}")]
    WrongSessionType { expected: String, actual: String },
}

/// Session manager configuration
#[derive(Debug, Clone)]
pub struct ManagerConfig {
    pub max_concurrent_sessions: usize,
    pub max_sessions_per_user: usize,
}

impl Default for ManagerConfig {
    fn default() -> Self {
        Self {
            max_concurrent_sessions: 100,
            max_sessions_per_user: 10,
        }
    }
}

/// High-level session manager
pub struct SessionManager {
    store: SessionStore,
    config: ManagerConfig,
    /// Separate storage for CLIPS environments (not cloneable/serializable)
    clips_envs: Arc<RwLock<HashMap<SessionId, clara_clips::ClipsEnvironment>>>,
    /// Separate storage for Prolog environments (LilDevils)
    prolog_envs: Arc<RwLock<HashMap<SessionId, clara_prolog::PrologEnvironment>>>,
}

impl SessionManager {
    /// Create a new session manager
    pub fn new(config: ManagerConfig) -> Self {
        Self {
            store: SessionStore::new(),
            config,
            clips_envs: Arc::new(RwLock::new(HashMap::new())),
            prolog_envs: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a new session for a user
    pub fn create_session(
        &self,
        user_id: String,
        limits: Option<ResourceLimits>,
    ) -> Result<Session, ManagerError> {
        self.create_session_with_name(user_id, None, limits)
    }

    /// Create a new session for a user with optional name
    pub fn create_session_with_name(
        &self,
        user_id: String,
        name: Option<String>,
        limits: Option<ResourceLimits>,
    ) -> Result<Session, ManagerError> {
        // Check global session limit
        let active_count = self.store.count_active()?;
        if active_count >= self.config.max_concurrent_sessions {
            return Err(ManagerError::GlobalSessionLimitExceeded);
        }

        // Check per-user session limit
        let user_count = self.store.count_user_sessions(&user_id)?;
        if user_count >= self.config.max_sessions_per_user {
            return Err(ManagerError::UserSessionLimitExceeded);
        }

        let mut session = Session::new_with_name(user_id, name, limits);

        // Create CLIPS FFI environment
        let clips_env = clara_clips::ClipsEnvironment::new()
            .map_err(|e| {
                log::error!("Failed to create CLIPS environment: {}", e);
                ManagerError::Store(StoreError::InvalidState)
            })?;

        // Insert session
        let session_id = session.session_id.clone();
        self.store.insert(session.clone())?;

        // Store CLIPS environment separately
        {
            let mut envs = self.clips_envs.write()
                .map_err(|_| ManagerError::Store(StoreError::LockPoisoned))?;
            envs.insert(session_id.clone(), clips_env);
        }

        // Activate and update
        session.activate();
        self.store.update(session.clone())?;

        // Return the session
        Ok(session)
    }
    
    /// Save a session's facts and rules
    pub fn save_session(&self, session_id: &SessionId) -> Result<(), ManagerError> {
        
        // Look up the session by the sessionId
        let session = self.store.get(session_id)?;
        if session.status == SessionStatus::Terminated {
            return Err(ManagerError::SessionTerminated);
        }
        
        // Log the save action (in a real implementation, this would persist to disk or database)
        log::info!("Saving session: {}", session_id.0);

        self.store.update(session)?;
        Ok(())
    }

    /// Get a session by ID
    pub fn get_session(&self, session_id: &SessionId) -> Result<Session, ManagerError> {
        let session = self.store.get(session_id)?;

        if session.status == SessionStatus::Terminated {
            return Err(ManagerError::SessionTerminated);
        }

        Ok(session)
    }

    /// Update a session's metadata
    pub fn update_session(&self, session: Session) -> Result<(), ManagerError> {
        if session.status == SessionStatus::Terminated {
            return Err(ManagerError::SessionTerminated);
        }

        self.store.update(session)?;
        Ok(())
    }

    /// Terminate a session
    pub fn terminate_session(&self, session_id: &SessionId) -> Result<Session, ManagerError> {
        let mut session = self.store.get(session_id)?;
        session.terminate();
        self.store.update(session.clone())?;

        // Remove CLIPS environment
        {
            let mut envs = self.clips_envs.write()
                .map_err(|_| ManagerError::Store(StoreError::LockPoisoned))?;
            envs.remove(session_id);
        }

        Ok(session)
    }

    /// Execute an operation on a session's CLIPS environment
    /// Returns an error if the session or environment doesn't exist
    pub fn with_clips_env<F, R>(&self, session_id: &SessionId, f: F) -> Result<R, ManagerError>
    where
        F: FnOnce(&mut clara_clips::ClipsEnvironment) -> Result<R, String>,
    {
        let mut envs = self.clips_envs.write()
            .map_err(|_| ManagerError::Store(StoreError::LockPoisoned))?;

        let env = envs.get_mut(session_id)
            .ok_or_else(|| ManagerError::SessionNotFound)?;

        f(env).map_err(|_e| ManagerError::Store(StoreError::InvalidState))
    }

    // =========================================================================
    // Prolog Session Methods (LilDevils)
    // =========================================================================

    /// Create a new Prolog session for a user
    pub fn create_prolog_session(
        &self,
        user_id: String,
        limits: Option<ResourceLimits>,
    ) -> Result<Session, ManagerError> {
        self.create_prolog_session_with_name(user_id, None, limits)
    }

    /// Create a new Prolog session for a user with optional name
    pub fn create_prolog_session_with_name(
        &self,
        user_id: String,
        name: Option<String>,
        limits: Option<ResourceLimits>,
    ) -> Result<Session, ManagerError> {
        // Check global session limit
        let active_count = self.store.count_active()?;
        if active_count >= self.config.max_concurrent_sessions {
            return Err(ManagerError::GlobalSessionLimitExceeded);
        }

        // Check per-user session limit
        let user_count = self.store.count_user_sessions(&user_id)?;
        if user_count >= self.config.max_sessions_per_user {
            return Err(ManagerError::UserSessionLimitExceeded);
        }

        let mut session = Session::new_typed_with_name(user_id, SessionType::Prolog, name, limits);

        // Create Prolog FFI environment
        let prolog_env = clara_prolog::PrologEnvironment::new()
            .map_err(|e| {
                log::error!("Failed to create Prolog environment: {}", e);
                ManagerError::Store(StoreError::InvalidState)
            })?;

        // Insert session
        let session_id = session.session_id.clone();
        self.store.insert(session.clone())?;

        // Store Prolog environment separately
        {
            let mut envs = self.prolog_envs.write()
                .map_err(|_| ManagerError::Store(StoreError::LockPoisoned))?;
            envs.insert(session_id.clone(), prolog_env);
        }

        // Activate and update
        session.activate();
        self.store.update(session.clone())?;

        log::info!("Created Prolog session: {}", session_id);

        Ok(session)
    }

    /// Terminate a Prolog session
    pub fn terminate_prolog_session(&self, session_id: &SessionId) -> Result<Session, ManagerError> {
        let mut session = self.store.get(session_id)?;

        // Verify it's a Prolog session
        if session.session_type != SessionType::Prolog {
            return Err(ManagerError::WrongSessionType {
                expected: "prolog".to_string(),
                actual: session.session_type.to_string(),
            });
        }

        session.terminate();
        self.store.update(session.clone())?;

        // Remove Prolog environment
        {
            let mut envs = self.prolog_envs.write()
                .map_err(|_| ManagerError::Store(StoreError::LockPoisoned))?;
            envs.remove(session_id);
        }

        log::info!("Terminated Prolog session: {}", session_id);

        Ok(session)
    }

    /// Execute an operation on a session's Prolog environment
    /// Returns an error if the session or environment doesn't exist
    pub fn with_prolog_env<F, R>(&self, session_id: &SessionId, f: F) -> Result<R, ManagerError>
    where
        F: FnOnce(&mut clara_prolog::PrologEnvironment) -> Result<R, String>,
    {
        let mut envs = self.prolog_envs.write()
            .map_err(|_| ManagerError::Store(StoreError::LockPoisoned))?;

        let env = envs.get_mut(session_id)
            .ok_or_else(|| ManagerError::SessionNotFound)?;

        f(env).map_err(|_e| ManagerError::Store(StoreError::InvalidState))
    }

    /// Get all sessions for a user
    pub fn get_user_sessions(&self, user_id: &str) -> Result<Vec<Session>, ManagerError> {
        let session_ids = self.store.get_user_sessions(user_id)?;

        let sessions = session_ids
            .iter()
            .filter_map(|id| self.store.get(id).ok())
            .filter(|s| s.status != SessionStatus::Terminated)
            .collect();

        Ok(sessions)
    }

    /// Get count of active sessions
    pub fn count_active_sessions(&self) -> Result<usize, ManagerError> {
        Ok(self.store.count_active()?)
    }

    /// Get count of active sessions for a user
    pub fn count_user_active_sessions(&self, user_id: &str) -> Result<usize, ManagerError> {
        let session_ids = self.store.get_user_sessions(user_id)?;

        let count = session_ids
            .iter()
            .filter_map(|id| self.store.get(id).ok())
            .filter(|s| s.status != SessionStatus::Terminated)
            .count();

        Ok(count)
    }

    /// List all sessions (across all users)
    pub fn list_all_sessions(&self) -> Result<Vec<Session>, ManagerError> {
        self.store.list_all()
            .map_err(|e| e.into())
    }

    /// Touch a session (update its last access time)
    pub fn touch_session(&self, session_id: &SessionId) -> Result<(), ManagerError> {
        let mut session = self.store.get(session_id)?;

        if session.status == SessionStatus::Terminated {
            return Err(ManagerError::SessionTerminated);
        }

        session.touch();
        self.store.update(session)?;
        Ok(())
    }
}

impl Clone for SessionManager {
    fn clone(&self) -> Self {
        Self {
            store: self.store.clone(),
            config: self.config.clone(),
            clips_envs: Arc::clone(&self.clips_envs),
            prolog_envs: Arc::clone(&self.prolog_envs),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_session() {
        let manager = SessionManager::new(ManagerConfig::default());
        let session = manager.create_session("user-1".to_string(), None).unwrap();

        assert_eq!(session.user_id, "user-1");
        assert_eq!(session.status, SessionStatus::Active);
    }

    #[test]
    fn test_get_session() {
        let manager = SessionManager::new(ManagerConfig::default());
        let created = manager.create_session("user-1".to_string(), None).unwrap();

        let retrieved = manager.get_session(&created.session_id).unwrap();
        assert_eq!(retrieved.session_id, created.session_id);
    }

    #[test]
    fn test_user_session_limit() {
        let config = ManagerConfig {
            max_concurrent_sessions: 100,
            max_sessions_per_user: 2,
        };
        let manager = SessionManager::new(config);

        manager.create_session("user-1".to_string(), None).unwrap();
        manager.create_session("user-1".to_string(), None).unwrap();

        let result = manager.create_session("user-1".to_string(), None);
        assert!(matches!(result, Err(ManagerError::UserSessionLimitExceeded)));
    }

    #[test]
    fn test_terminate_session() {
        let manager = SessionManager::new(ManagerConfig::default());
        let session = manager.create_session("user-1".to_string(), None).unwrap();

        manager.terminate_session(&session.session_id).unwrap();

        let result = manager.get_session(&session.session_id);
        assert!(matches!(result, Err(ManagerError::SessionTerminated)));
    }

    #[test]
    fn test_get_user_sessions() {
        let manager = SessionManager::new(ManagerConfig::default());

        manager.create_session("user-1".to_string(), None).unwrap();
        manager.create_session("user-1".to_string(), None).unwrap();
        manager.create_session("user-2".to_string(), None).unwrap();

        let user1_sessions = manager.get_user_sessions("user-1").unwrap();
        assert_eq!(user1_sessions.len(), 2);

        let user2_sessions = manager.get_user_sessions("user-2").unwrap();
        assert_eq!(user2_sessions.len(), 1);
    }

    // =========================================================================
    // Prolog Session Tests (LilDevils)
    // =========================================================================

    #[test]
    fn test_create_prolog_session() {
        let manager = SessionManager::new(ManagerConfig::default());
        let session = manager.create_prolog_session("user-1".to_string(), None).unwrap();

        assert_eq!(session.user_id, "user-1");
        assert_eq!(session.status, SessionStatus::Active);
        assert_eq!(session.session_type, SessionType::Prolog);
    }

    #[test]
    fn test_terminate_prolog_session() {
        let manager = SessionManager::new(ManagerConfig::default());
        let session = manager.create_prolog_session("user-1".to_string(), None).unwrap();

        manager.terminate_prolog_session(&session.session_id).unwrap();

        let result = manager.get_session(&session.session_id);
        assert!(matches!(result, Err(ManagerError::SessionTerminated)));
    }

    #[test]
    fn test_prolog_session_wrong_type() {
        let manager = SessionManager::new(ManagerConfig::default());
        // Create a CLIPS session
        let clips_session = manager.create_session("user-1".to_string(), None).unwrap();

        // Try to terminate it as Prolog - should fail
        let result = manager.terminate_prolog_session(&clips_session.session_id);
        assert!(matches!(result, Err(ManagerError::WrongSessionType { .. })));
    }

    #[test]
    fn test_with_prolog_env() {
        let manager = SessionManager::new(ManagerConfig::default());
        let session = manager.create_prolog_session("user-1".to_string(), None).unwrap();

        // Execute a simple query
        let result = manager.with_prolog_env(&session.session_id, |env| {
            env.query_once("X = 42").map_err(|e| e.to_string())
        });

        assert!(result.is_ok(), "Query should succeed: {:?}", result);
    }

    #[test]
    fn test_mixed_clips_prolog_sessions() {
        let manager = SessionManager::new(ManagerConfig::default());

        // Create one of each type
        let clips_session = manager.create_session("user-1".to_string(), None).unwrap();
        let prolog_session = manager.create_prolog_session("user-1".to_string(), None).unwrap();

        // Verify types
        assert_eq!(clips_session.session_type, SessionType::Clips);
        assert_eq!(prolog_session.session_type, SessionType::Prolog);

        // Both should be in user sessions
        let user_sessions = manager.get_user_sessions("user-1").unwrap();
        assert_eq!(user_sessions.len(), 2);

        // Terminate both
        manager.terminate_session(&clips_session.session_id).unwrap();
        manager.terminate_prolog_session(&prolog_session.session_id).unwrap();
    }
}
