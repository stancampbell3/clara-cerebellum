// Evaluation Tool - Routes evaluation requests to a lil-daemon instance
//
// Accepts JSON input and passes it to a lil-daemon's /evaluate endpoint.
// Lil-daemons can provide LLM reasoning, rule-based evaluation, or other processing.
use crate::tool::{Tool, ToolError};
use demonic_voice::DemonicVoice;
use serde_json::Value;
use std::sync::Arc;

/// Tool for evaluating expressions via a lil-daemon's evaluation endpoint
///
/// Takes a JSON object as input and returns the evaluation result as JSON.
///
/// # Loop Prevention
/// During rule evaluation in CLIPS, an evaluation request may be made to a lil-daemon for LLM reasoning.
/// The lil-daemon may then call back to clara-cerebrum for further rule-based reasoning.
/// We MUST detect and prevent loops in such calls to avoid infinite recursion. When designing rule sets,
/// validation rules should ensure no loops are possible, with additional guards in the clara-cerebrum server.
pub struct EvaluateTool {
    daemon_voice: Arc<DemonicVoice>,
}

impl EvaluateTool {
    /// Create a new EvaluateTool with the given DemonicVoice client
    pub fn new(daemon_voice: Arc<DemonicVoice>) -> Self {
        Self { daemon_voice }
    }
}

impl Tool for EvaluateTool {
    fn name(&self) -> &str {
        "evaluate"
    }

    fn description(&self) -> &str {
        "Evaluates expressions via lil-daemon evaluation endpoint"
    }

    fn execute(&self, args: Value) -> Result<Value, ToolError> {
        log::debug!("EvaluateTool executing with args: {}", args);

        // Call lil-daemon's evaluation endpoint with the provided arguments
        match self.daemon_voice.evaluate(args) {
            Ok(response) => Ok(response),
            Err(e) => Err(ToolError::ExecutionFailed(format!(
                "Lil-daemon evaluation failed: {}",
                e
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use demonic_voice::DemonicVoice;
    use serde_json::json;
    use std::sync::Arc;

    #[test]
    fn test_evaluate_tool_basic() {
        let daemon_voice = Arc::new(DemonicVoice::new("http://localhost:8000"));
        let tool = EvaluateTool::new(daemon_voice);
        assert_eq!(tool.name(), "evaluate");
        assert!(!tool.description().is_empty());
    }

    // Note: Additional tests would require mocking DemonicVoice's evaluate method
}