// Tool trait and request/response types

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

/// Error types for tool execution
#[derive(Error, Debug)]
pub enum ToolError {
    #[error("Tool not found: {0}")]
    NotFound(String),

    #[error("Invalid arguments: {0}")]
    InvalidArgs(String),

    #[error("Execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Timeout")]
    Timeout,

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),
}

/// Tool trait that all tools must implement
pub trait Tool: Send + Sync {
    /// Get the tool's name
    fn name(&self) -> &str;

    /// Get the tool's description
    fn description(&self) -> &str;

    /// Execute the tool with the given arguments
    fn execute(&self, args: Value) -> Result<Value, ToolError>;
}

/// Tool request structure (what CLIPS sends)
#[derive(Debug, Deserialize)]
pub struct ToolRequest {
    pub tool: String,
    #[serde(default)]
    pub arguments: Value,
}

/// Tool response structure (what we send back to CLIPS)
#[derive(Debug, Serialize)]
pub struct ToolResponse {
    pub status: String,
    #[serde(flatten)]
    pub result: Value,
}

impl ToolResponse {
    /// Create a success response
    pub fn success(result: Value) -> Self {
        Self {
            status: "success".to_string(),
            result,
        }
    }

    /// Create an error response
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            status: "error".to_string(),
            result: serde_json::json!({
                "message": message.into()
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_tool_request_deserialize() {
        let json_str = r#"{"tool":"echo","arguments":{"message":"hello"}}"#;
        let req: ToolRequest = serde_json::from_str(json_str).unwrap();
        assert_eq!(req.tool, "echo");
        assert_eq!(req.arguments["message"], "hello");
    }

    #[test]
    fn test_tool_request_no_arguments() {
        let json_str = r#"{"tool":"test"}"#;
        let req: ToolRequest = serde_json::from_str(json_str).unwrap();
        assert_eq!(req.tool, "test");
        assert!(req.arguments.is_null());
    }

    #[test]
    fn test_tool_response_success() {
        let resp = ToolResponse::success(json!({"data": "test"}));
        assert_eq!(resp.status, "success");
        assert_eq!(resp.result["data"], "test");
    }

    #[test]
    fn test_tool_response_error() {
        let resp = ToolResponse::error("Something went wrong");
        assert_eq!(resp.status, "error");
        assert_eq!(resp.result["message"], "Something went wrong");
    }

    #[test]
    fn test_tool_response_serialize() {
        let resp = ToolResponse::success(json!({"value": 42}));
        let json_str = serde_json::to_string(&resp).unwrap();
        assert!(json_str.contains("success"));
        assert!(json_str.contains("\"value\":42"));
    }
}
