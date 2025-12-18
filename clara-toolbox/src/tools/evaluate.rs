// Clara Evaluation Tool
// -- Accepts JSON input and passes it on as a request to Clara's evaluation endpoint
use crate::tool::{Tool, ToolError};
use clara_client::ClaraClient;
use serde_json::Value;
use std::sync::Arc;
/// Tool for evaluating expressions using Clara's evaluation endpoint
pub struct EvaluateTool {
    clara_client: Arc<ClaraClient>,
}

impl EvaluateTool {
    /// Create a new EvaluateTool with the given ClaraClient
    pub fn new(clara_client: Arc<ClaraClient>) -> Self {
        Self { clara_client }
    }
}
impl Tool for EvaluateTool {
    fn name(&self) -> &str {
        "evaluate"
    }

    fn description(&self) -> &str {
        "Evaluates expressions using Clara's evaluation endpoint"
    }

    fn execute(&self, args: Value) -> Result<Value, ToolError> {
        log::debug!("EvaluateTool executing with args: {}", args);

        // Call Clara's evaluation endpoint with the provided arguments
        match self.clara_client.evaluate(args) {
            Ok(response) => Ok(response),
            Err(e) => Err(ToolError::ExecutionFailed(format!(
                "Clara evaluation failed: {}",
                e
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clara_client::ClaraClient;
    use serde_json::json;
    use std::sync::Arc;

    #[test]
    fn test_evaluate_tool_basic() {
        let clara_client = Arc::new(ClaraClient::new("http://localhost:8000"));
        let tool = EvaluateTool::new(clara_client);
        assert_eq!(tool.name(), "evaluate");
        assert!(!tool.description().is_empty());
    }

    // Note: Additional tests would require mocking ClaraClient's evaluate method
}