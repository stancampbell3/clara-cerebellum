use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The result of a CLIPS evaluation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalResult {
    /// Standard output from CLIPS
    pub stdout: String,

    /// Standard error from CLIPS
    pub stderr: String,

    /// Exit code (0 = success)
    pub exit_code: i32,

    /// Metrics about the evaluation
    pub metrics: EvalMetrics,

    /// Any errors that occurred
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl EvalResult {
    /// Create a successful evaluation result
    pub fn success(stdout: String, metrics: EvalMetrics) -> Self {
        Self {
            stdout,
            stderr: String::new(),
            exit_code: 0,
            metrics,
            error: None,
        }
    }

    /// Create a failed evaluation result
    pub fn failure(error: String, metrics: EvalMetrics) -> Self {
        Self {
            stdout: String::new(),
            stderr: error.clone(),
            exit_code: 1,
            metrics,
            error: Some(error),
        }
    }

    /// Check if evaluation was successful
    pub fn is_success(&self) -> bool {
        self.exit_code == 0 && self.error.is_none()
    }
}

/// Metrics about an evaluation
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EvalMetrics {
    /// Time elapsed in milliseconds
    pub elapsed_ms: u64,

    /// Number of facts added
    #[serde(skip_serializing_if = "Option::is_none")]
    pub facts_added: Option<u32>,

    /// Number of rules fired
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rules_fired: Option<u32>,

    /// Additional custom metrics
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom: Option<HashMap<String, String>>,
}

impl EvalMetrics {
    /// Create metrics with elapsed time
    pub fn with_elapsed(elapsed_ms: u64) -> Self {
        Self {
            elapsed_ms,
            ..Default::default()
        }
    }
}

/// Evaluation mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EvalMode {
    /// Run all rules until quiescence
    Run,
    /// Execute a single CLIPS command/expression
    Command,
    /// Load rules from a file
    Load,
    /// Interactive REPL
    Interactive,
}

/// Evaluation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalRequest {
    /// CLIPS commands or script to evaluate
    pub script: String,

    /// Timeout in milliseconds
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,

    /// Mode of evaluation
    #[serde(default)]
    pub mode: EvalMode,
}

impl EvalRequest {
    /// Create a new eval request
    pub fn new(script: String) -> Self {
        Self {
            script,
            timeout_ms: default_timeout(),
            mode: EvalMode::Command,
        }
    }

    /// Set the timeout
    pub fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }

    /// Set the mode
    pub fn with_mode(mut self, mode: EvalMode) -> Self {
        self.mode = mode;
        self
    }
}

fn default_timeout() -> u64 {
    2000 // 2 seconds default
}

impl Default for EvalMode {
    fn default() -> Self {
        EvalMode::Command
    }
}

/// Evaluation response for API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalResponse {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub metrics: EvalMetrics,
}

impl From<EvalResult> for EvalResponse {
    fn from(result: EvalResult) -> Self {
        Self {
            stdout: result.stdout,
            stderr: result.stderr,
            exit_code: result.exit_code,
            metrics: result.metrics,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_eval_result_success() {
        let result = EvalResult::success(
            "output".to_string(),
            EvalMetrics::with_elapsed(100),
        );
        assert!(result.is_success());
        assert_eq!(result.exit_code, 0);
    }

    #[test]
    fn test_eval_result_failure() {
        let result = EvalResult::failure(
            "error message".to_string(),
            EvalMetrics::with_elapsed(100),
        );
        assert!(!result.is_success());
        assert_eq!(result.exit_code, 1);
    }

    #[test]
    fn test_eval_request() {
        let req = EvalRequest::new("(run)".to_string())
            .with_timeout(5000)
            .with_mode(EvalMode::Run);

        assert_eq!(req.script, "(run)");
        assert_eq!(req.timeout_ms, 5000);
        assert_eq!(req.mode, EvalMode::Run);
    }
}
