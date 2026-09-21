use crate::metadata::{Session, SessionId, SessionStatus};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum StoreError {
    #[error("Session not found: {0}")]
    NotFound(String),

    #[error("Session already exists: {0}")]
    AlreadyExists(String),

    #[error("Invalid session state")]
    InvalidState,

    #[error("Lock poisoned")]
    LockPoisoned,
}

/// In-memory session store using a HashMap
pub struct SessionStore {
    sessions: Arc<RwLock<HashMap<SessionId, Session>>>,
}

impl SessionStore {
    /// Create a new empty session store
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Insert a session into the store
    pub fn insert(&self, session: Session) -> Result<(), StoreError> {
        let mut sessions = self
            .sessions
            .write()
            .map_err(|_| StoreError::LockPoisoned)?;

        if sessions.contains_key(&session.session_id) {
            return Err(StoreError::AlreadyExists(session.session_id.to_string()));
        }

        sessions.insert(session.session_id.clone(), session);
        Ok(())
    }

    /// Get a session by ID
    pub fn get(&self, session_id: &SessionId) -> Result<Session, StoreError> {
        let sessions = self
            .sessions
            .read()
            .map_err(|_| StoreError::LockPoisoned)?;

        sessions
            .get(session_id)
            .cloned()
            .ok_or_else(|| StoreError::NotFound(session_id.to_string()))
    }

    /// Update a session in the store
    pub fn update(&self, session: Session) -> Result<(), StoreError> {
        let mut sessions = self
            .sessions
            .write()
            .map_err(|_| StoreError::LockPoisoned)?;

        if !sessions.contains_key(&session.session_id) {
            return Err(StoreError::NotFound(session.session_id.to_string()));
        }

        sessions.insert(session.session_id.clone(), session);
        Ok(())
    }

    /// Remove a session from the store
    pub fn remove(&self, session_id: &SessionId) -> Result<Session, StoreError> {
        let mut sessions = self
            .sessions
            .write()
            .map_err(|_| StoreError::LockPoisoned)?;

        sessions
            .remove(session_id)
            .ok_or_else(|| StoreError::NotFound(session_id.to_string()))
    }

    /// Get all session IDs for a specific user
    pub fn get_user_sessions(&self, user_id: &str) -> Result<Vec<SessionId>, StoreError> {
        let sessions = self
            .sessions
            .read()
            .map_err(|_| StoreError::LockPoisoned)?;

        let ids = sessions
            .iter()
            .filter(|(_, session)| session.user_id == user_id)
            .map(|(id, _)| id.clone())
            .collect();

        Ok(ids)
    }

    /// List all sessions (across all users)
    pub fn list_all(&self) -> Result<Vec<Session>, StoreError> {
        let sessions = self
            .sessions
            .read()
            .map_err(|_| StoreError::LockPoisoned)?;

        Ok(sessions.values().cloned().collect())
    }

    /// Get count of live sessions: everything except terminated ones.
    ///
    /// Terminated sessions stay in the store (so a client can still read their final state) but must not
    /// count against the concurrency limits. Counting them made every ritual participant that ever joined
    /// permanently consume a slot, until the process restarted: after `max_concurrent` lifetime sessions
    /// no new session could be created (found 2026-09-21: Dis refused all new joins with 429 while only
    /// 6 of its 50 sessions were live).
    pub fn count_active(&self) -> Result<usize, StoreError> {
        let sessions = self
            .sessions
            .read()
            .map_err(|_| StoreError::LockPoisoned)?;

        Ok(sessions
            .values()
            .filter(|s| s.status != SessionStatus::Terminated)
            .count())
    }

    /// Get count of live (non-terminated) sessions for a specific user
    pub fn count_user_sessions(&self, user_id: &str) -> Result<usize, StoreError> {
        let sessions = self
            .sessions
            .read()
            .map_err(|_| StoreError::LockPoisoned)?;

        Ok(sessions
            .values()
            .filter(|s| s.user_id == user_id && s.status != SessionStatus::Terminated)
            .count())
    }

    /// Remove terminated sessions last touched more than `older_than_secs` ago, returning how many.
    /// Bounds the memory a long-running process spends on sessions nobody will read again.
    pub fn purge_terminated(&self, now: u64, older_than_secs: u64) -> Result<usize, StoreError> {
        let mut sessions = self
            .sessions
            .write()
            .map_err(|_| StoreError::LockPoisoned)?;

        let before = sessions.len();
        sessions.retain(|_, s| {
            !(s.status == SessionStatus::Terminated
                && now.saturating_sub(s.touched_at) > older_than_secs)
        });
        Ok(before - sessions.len())
    }

    /// Check if a session exists
    pub fn exists(&self, session_id: &SessionId) -> Result<bool, StoreError> {
        let sessions = self
            .sessions
            .read()
            .map_err(|_| StoreError::LockPoisoned)?;

        Ok(sessions.contains_key(session_id))
    }

    /// Clear all sessions (for testing)
    pub fn clear(&self) -> Result<(), StoreError> {
        let mut sessions = self
            .sessions
            .write()
            .map_err(|_| StoreError::LockPoisoned)?;

        sessions.clear();
        Ok(())
    }
}

impl Clone for SessionStore {
    fn clone(&self) -> Self {
        Self {
            sessions: Arc::clone(&self.sessions),
        }
    }
}

impl Default for SessionStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_and_get() {
        let store = SessionStore::new();
        let session = Session::new("user-1".to_string(), None);
        let session_id = session.session_id.clone();

        store.insert(session.clone()).unwrap();
        let retrieved = store.get(&session_id).unwrap();

        assert_eq!(retrieved.user_id, "user-1");
    }

    #[test]
    fn test_duplicate_insert() {
        let store = SessionStore::new();
        let session = Session::new("user-1".to_string(), None);

        store.insert(session.clone()).unwrap();
        assert!(store.insert(session).is_err());
    }

    #[test]
    fn test_user_sessions() {
        let store = SessionStore::new();
        let s1 = Session::new("user-1".to_string(), None);
        let s2 = Session::new("user-1".to_string(), None);
        let s3 = Session::new("user-2".to_string(), None);

        store.insert(s1).unwrap();
        store.insert(s2).unwrap();
        store.insert(s3).unwrap();

        let user1_sessions = store.get_user_sessions("user-1").unwrap();
        assert_eq!(user1_sessions.len(), 2);

        let user2_sessions = store.get_user_sessions("user-2").unwrap();
        assert_eq!(user2_sessions.len(), 1);
    }

    #[test]
    fn test_terminated_sessions_do_not_count_against_the_limits() {
        let store = SessionStore::new();
        let mut dead = Session::new("user-1".to_string(), None);
        dead.terminate();
        let live = Session::new("user-1".to_string(), None);
        store.insert(dead).unwrap();
        store.insert(live).unwrap();

        assert_eq!(store.count_active().unwrap(), 1);
        assert_eq!(store.count_user_sessions("user-1").unwrap(), 1);
        // the terminated one is still readable
        assert_eq!(store.list_all().unwrap().len(), 2);
    }

    #[test]
    fn test_purge_removes_only_old_terminated_sessions() {
        let store = SessionStore::new();
        let mut old_dead = Session::new("u".to_string(), None);
        old_dead.terminate();
        old_dead.touched_at = 100;
        let mut fresh_dead = Session::new("u".to_string(), None);
        fresh_dead.terminate();
        fresh_dead.touched_at = 990;
        let mut old_live = Session::new("u".to_string(), None);
        old_live.touched_at = 100;
        let old_live_id = old_live.session_id.clone();
        store.insert(old_dead).unwrap();
        store.insert(fresh_dead).unwrap();
        store.insert(old_live).unwrap();

        assert_eq!(store.purge_terminated(1000, 300).unwrap(), 1);
        assert_eq!(store.list_all().unwrap().len(), 2);
        assert!(store.exists(&old_live_id).unwrap(), "a live session is never purged, however idle");
        assert_eq!(store.purge_terminated(1000, 300).unwrap(), 0);
    }
}
