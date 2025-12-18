// Echo tool for testing - simply echoes back the input

use crate::tool::{Tool, ToolError};
use serde_json::{json, Value};

/// Simple echo tool that returns the input arguments
pub struct EchoTool;

impl Tool for EchoTool {
    fn name(&self) -> &str {
        "echo"
    }

    fn description(&self) -> &str {
        "Echoes back the provided arguments"
    }

    fn execute(&self, args: Value) -> Result<Value, ToolError> {
        log::debug!("EchoTool executing with args: {}", args);

        // Simply return the arguments wrapped in a message
        Ok(json!({
            "echoed": args,
            "message": "Echo tool received and returned your input"
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_echo_tool_basic() {
        let tool = EchoTool;
        assert_eq!(tool.name(), "echo");
        assert!(!tool.description().is_empty());
    }

    #[test]
    fn test_echo_tool_execute() {
        let tool = EchoTool;
        let args = json!({"message": "hello", "value": 42});
        let result = tool.execute(args.clone()).unwrap();

        assert_eq!(result["echoed"], args);
        assert!(result["message"].is_string());
    }

    #[test]
    fn test_echo_tool_empty_args() {
        let tool = EchoTool;
        let result = tool.execute(json!({})).unwrap();
        assert_eq!(result["echoed"], json!({}));
    }

    #[test]
    fn test_echo_tool_complex_args() {
        let tool = EchoTool;
        let args = json!({
            "nested": {
                "data": [1, 2, 3],
                "flag": true
            }
        });
        let result = tool.execute(args.clone()).unwrap();
        assert_eq!(result["echoed"], args);
    }
}
