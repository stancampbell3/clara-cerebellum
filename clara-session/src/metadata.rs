use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::hash::BuildHasher;
use std::time::{SystemTime, UNIX_EPOCH};

/// Unique session identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub String);

impl SessionId {
    /// Generate a new unique session ID
    pub fn new() -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();

        // Simple unique ID: prefix + timestamp + random component
        let random: u32 = (std::collections::hash_map::RandomState::new().hash_one(
            &SystemTime::now()
        ) as u32) % 100000;

        let mut id = format!("sess-{:x}-{}", timestamp, random);
        id.truncate(32); // Keep it reasonable length
        SessionId(id)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Resource usage tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUsage {
    pub facts: u32,
    pub rules: u32,
    pub objects: u32,
    pub memory_bytes: u64,
}

impl Default for ResourceUsage {
    fn default() -> Self {
        Self {
            facts: 0,
            rules: 0,
            objects: 0,
            memory_bytes: 0,
        }
    }
}

impl ResourceUsage {
    pub fn is_within_limits(&self, limits: &ResourceLimits) -> bool {
        self.facts <= limits.max_facts as u32
            && self.rules <= limits.max_rules as u32
            && self.memory_bytes <= (limits.max_memory_mb as u64 * 1024 * 1024)
    }
}

/// Resource limits for a session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    pub max_facts: u32,
    pub max_rules: u32,
    pub max_memory_mb: u32,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_facts: 1000,
            max_rules: 500,
            max_memory_mb: 128,
        }
    }
}

/// Type of reasoning engine for the session
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionType {
    /// CLIPS expert system (LilDaemon)
    Clips,
    /// SWI-Prolog logic engine (LilDevils)
    Prolog,
}

impl Default for SessionType {
    fn default() -> Self {
        Self::Clips
    }
}

impl std::fmt::Display for SessionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Clips => write!(f, "clips"),
            Self::Prolog => write!(f, "prolog"),
        }
    }
}

/// Session status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionStatus {
    /// Session is active and ready for evals
    Active,
    /// Session is being initialized
    Initializing,
    /// Session is idle (not being used but still active)
    Idle,
    /// Session is currently evaluating/running rules
    Evaluating,
    /// Session is paused (execution suspended, state preserved)
    Paused,
    /// Session is suspended (e.g., for maintenance)
    Suspended,
    /// Session has been terminated
    Terminated,
}

impl std::fmt::Display for SessionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Active => write!(f, "active"),
            Self::Initializing => write!(f, "initializing"),
            Self::Idle => write!(f, "idle"),
            Self::Evaluating => write!(f, "evaluating"),
            Self::Paused => write!(f, "paused"),
            Self::Suspended => write!(f, "suspended"),
            Self::Terminated => write!(f, "terminated"),
        }
    }
}

/// Session statistics for tracking activity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStats {
    /// Total number of rules fired in this session
    pub rules_fired_total: u64,
    /// Total number of evaluations performed
    pub evaluations_total: u64,
    /// Timestamp of last activity
    pub last_activity: u64,
}

impl Default for SessionStats {
    fn default() -> Self {
        Self {
            rules_fired_total: 0,
            evaluations_total: 0,
            last_activity: current_timestamp(),
        }
    }
}

/// Session metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Unique session identifier
    pub session_id: SessionId,

    /// User who owns this session
    pub user_id: String,

    /// Optional human-readable name for this session
    pub name: Option<String>,

    /// Type of reasoning engine (CLIPS or Prolog)
    #[serde(default)]
    pub session_type: SessionType,

    /// When the session was created (Unix timestamp in seconds)
    pub created_at: u64,

    /// When the session was last accessed (Unix timestamp in seconds)
    pub touched_at: u64,

    /// Current status of the session
    pub status: SessionStatus,

    /// Current resource usage
    pub resources: ResourceUsage,

    /// Resource limits for this session
    pub limits: ResourceLimits,

    /// Session statistics
    pub stats: SessionStats,

    /// Custom metadata (user-provided)
    pub metadata: HashMap<String, String>,

    /// List of preloaded files/rules
    pub loaded_files: Vec<String>,
}

impl Session {
    /// Create a new CLIPS session for a given user (default)
    pub fn new(user_id: String, limits: Option<ResourceLimits>) -> Self {
        Self::new_typed(user_id, SessionType::Clips, limits)
    }

    /// Create a new session with a specific type
    pub fn new_typed(user_id: String, session_type: SessionType, limits: Option<ResourceLimits>) -> Self {
        let now = current_timestamp();
        Self {
            session_id: SessionId::new(),
            user_id,
            name: None,
            session_type,
            created_at: now,
            touched_at: now,
            status: SessionStatus::Initializing,
            resources: ResourceUsage::default(),
            limits: limits.unwrap_or_default(),
            stats: SessionStats::default(),
            metadata: HashMap::new(),
            loaded_files: Vec::new(),
        }
    }

    /// Create a new session with an optional name
    pub fn new_with_name(
        user_id: String,
        name: Option<String>,
        limits: Option<ResourceLimits>,
    ) -> Self {
        let mut session = Self::new(user_id, limits);
        session.name = name;
        session
    }

    /// Create a new session with a specific type and optional name
    pub fn new_typed_with_name(
        user_id: String,
        session_type: SessionType,
        name: Option<String>,
        limits: Option<ResourceLimits>,
    ) -> Self {
        let mut session = Self::new_typed(user_id, session_type, limits);
        session.name = name;
        session
    }

    /// Mark the session as active
    pub fn activate(&mut self) {
        self.status = SessionStatus::Active;
        self.touch();
    }

    /// Update the touched timestamp
    pub fn touch(&mut self) {
        let now = current_timestamp();
        self.touched_at = now;
        self.stats.last_activity = now;
    }

    /// Set session status to Evaluating
    pub fn start_evaluating(&mut self) {
        self.status = SessionStatus::Evaluating;
        self.touch();
    }

    /// Record completion of an evaluation
    pub fn complete_evaluation(&mut self, rules_fired: Option<u64>) {
        self.stats.evaluations_total += 1;
        if let Some(count) = rules_fired {
            self.stats.rules_fired_total += count;
        }
        self.status = SessionStatus::Active;
        self.touch();
    }

    /// Terminate the session
    pub fn terminate(&mut self) {
        self.status = SessionStatus::Terminated;
        self.touch();
    }

    /// Get session uptime in seconds
    pub fn uptime_seconds(&self) -> u64 {
        current_timestamp() - self.created_at
    }

    /// Check if session has exceeded its resource limits
    pub fn is_resource_limited(&self) -> bool {
        !self.resources.is_within_limits(&self.limits)
    }

    /// Add a file to the loaded files list
    pub fn add_loaded_file(&mut self, file: String) {
        self.loaded_files.push(file);
    }
}

/// Get current Unix timestamp in seconds
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_creation() {
        let session = Session::new("user-123".to_string(), None);
        assert_eq!(session.user_id, "user-123");
        assert_eq!(session.status, SessionStatus::Initializing);
        assert_eq!(session.resources.facts, 0);
    }

    #[test]
    fn test_session_id_unique() {
        let id1 = SessionId::new();
        let id2 = SessionId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_resource_limits() {
        let usage = ResourceUsage {
            facts: 100,
            ..Default::default()
        };
        let limits = ResourceLimits {
            max_facts: 1000,
            ..Default::default()
        };
        assert!(usage.is_within_limits(&limits));
    }
}
