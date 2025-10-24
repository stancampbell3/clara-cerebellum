use crate::metadata::{ResourceLimits, Session, SessionId, SessionStatus};
use crate::store::{SessionStore, StoreError};
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
}

impl SessionManager {
    /// Create a new session manager
    pub fn new(config: ManagerConfig) -> Self {
        Self {
            store: SessionStore::new(),
            config,
        }
    }

    /// Create a new session for a user
    pub fn create_session(
        &self,
        user_id: String,
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

        let mut session = Session::new(user_id, limits);
        self.store.insert(session.clone())?;
        
        session.activate();
        self.store.update(session.clone())?;

        Ok(session)
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
        Ok(session)
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
}
