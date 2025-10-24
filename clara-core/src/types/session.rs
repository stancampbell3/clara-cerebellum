use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Session create request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSessionRequest {
    /// User ID for the session
    pub user_id: String,

    /// Optional files to preload
    #[serde(default)]
    pub preload: Vec<String>,

    /// Optional metadata
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

impl CreateSessionRequest {
    pub fn new(user_id: String) -> Self {
        Self {
            user_id,
            preload: Vec::new(),
            metadata: HashMap::new(),
        }
    }
}

/// Session response with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionResponse {
    /// Unique session ID
    pub session_id: String,

    /// User ID
    pub user_id: String,

    /// Creation timestamp (ISO8601)
    pub started: String,

    /// Last touched timestamp (ISO8601)
    pub touched: String,

    /// Current session status
    pub status: String,

    /// Resource usage
    pub resources: ResourceInfo,

    /// Resource limits
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limits: Option<ResourceInfo>,
}

/// Resource information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceInfo {
    /// Number of facts
    pub facts: u32,

    /// Number of rules
    pub rules: u32,

    /// Number of objects
    pub objects: u32,

    /// Memory usage in MB
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_mb: Option<u32>,
}

impl Default for ResourceInfo {
    fn default() -> Self {
        Self {
            facts: 0,
            rules: 0,
            objects: 0,
            memory_mb: None,
        }
    }
}

/// Load request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadRequest {
    /// Files to load
    pub files: Vec<String>,
}

/// Load response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadResponse {
    pub session_id: String,

    pub loaded: Vec<String>,

    pub resources: ResourceInfo,
}

/// Status response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusResponse {
    pub session_id: String,

    pub user_id: String,

    pub started: String,

    pub touched: String,

    pub status: String,

    pub resources: ResourceInfo,

    pub limits: ResourceInfo,

    pub health: String,
}

/// Save request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveRequest {
    /// Label for the saved session
    pub label: String,
}

/// Save response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveResponse {
    pub session_id: String,

    pub saved_as: String,

    pub timestamp: String,
}

/// Reload request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReloadRequest {
    /// Label of the saved session to reload
    pub label: String,
}

/// Reload response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReloadResponse {
    pub session_id: String,

    pub status: String,

    pub resources: ResourceInfo,
}

/// Terminate response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminateResponse {
    pub session_id: String,

    pub status: String,

    pub saved: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_session_request() {
        let req = CreateSessionRequest::new("user-123".to_string());
        assert_eq!(req.user_id, "user-123");
        assert!(req.preload.is_empty());
    }

    #[test]
    fn test_session_response() {
        let resp = SessionResponse {
            session_id: "sess-abc".to_string(),
            user_id: "user-123".to_string(),
            started: "2025-10-23T17:03:00Z".to_string(),
            touched: "2025-10-23T17:03:00Z".to_string(),
            status: "active".to_string(),
            resources: ResourceInfo::default(),
            limits: None,
        };

        assert_eq!(resp.session_id, "sess-abc");
        assert_eq!(resp.status, "active");
    }
}
