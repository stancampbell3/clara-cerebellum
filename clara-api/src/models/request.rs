use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Create session request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSessionRequest {
    pub user_id: String,
    #[serde(default)]
    pub preload: Vec<String>,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

/// Eval request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalRequest {
    pub script: String,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
}

/// Load request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadRequest {
    pub files: Vec<String>,
}

/// Save request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveRequest {
    pub label: String,
}

/// Reload request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReloadRequest {
    pub label: String,
}

fn default_timeout() -> u64 {
    2000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_session_request() {
        let req = CreateSessionRequest {
            user_id: "user-123".to_string(),
            preload: vec![],
            metadata: HashMap::new(),
        };
        assert_eq!(req.user_id, "user-123");
    }
}
