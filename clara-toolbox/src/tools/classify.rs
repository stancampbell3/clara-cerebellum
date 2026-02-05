// ClassifyTool - Text classification using fastText models

use crate::tool::{Tool, ToolError};
use fasttext::FastText;
use serde_json::{json, Value};

/// Pre-process text before classification
fn preprocess(text: &str) -> String {
    text.to_lowercase()
}

/// Text classification tool using fastText models.
///
/// Classifies input text and returns predicted labels with probabilities.
/// The model is loaded once at construction time for performance.
pub struct ClassifyTool {
    model: FastText,
}

impl ClassifyTool {
    /// Create a new ClassifyTool by loading a fastText model from the given path.
    pub fn new(model_path: &str) -> Result<Self, ToolError> {
        let mut model = FastText::new();
        model
            .load_model(model_path)
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to load fastText model: {}", e)))?;
        log::info!("ClassifyTool loaded model from: {}", model_path);
        Ok(Self { model })
    }
}

impl Tool for ClassifyTool {
    fn name(&self) -> &str {
        "classify"
    }

    fn description(&self) -> &str {
        "Classifies text using a fastText model"
    }

    fn execute(&self, args: Value) -> Result<Value, ToolError> {
        log::debug!("ClassifyTool executing with args: {}", args);

        let text = args
            .get("text")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgs("Missing required 'text' argument".to_string()))?;

        let k = args
            .get("k")
            .and_then(|v| v.as_i64())
            .unwrap_or(1) as i32;

        let preprocessed = preprocess(text);

        let predictions = self
            .model
            .predict(&preprocessed, k, 0.0)
            .map_err(|e| ToolError::ExecutionFailed(format!("Classification failed: {}", e)))?;

        if predictions.is_empty() {
            return Err(ToolError::ExecutionFailed(
                "No predictions returned".to_string(),
            ));
        }

        let pred_json: Vec<Value> = predictions
            .iter()
            .map(|p| {
                json!({
                    "label": p.label,
                    "probability": p.prob,
                })
            })
            .collect();

        Ok(json!({
            "label": predictions[0].label,
            "probability": predictions[0].prob,
            "predictions": pred_json,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_preprocess_lowercase() {
        assert_eq!(preprocess("Hello World"), "hello world");
        assert_eq!(preprocess("ALLCAPS"), "allcaps");
        assert_eq!(preprocess("already lower"), "already lower");
    }

    #[test]
    fn test_preprocess_empty() {
        assert_eq!(preprocess(""), "");
    }

    #[test]
    fn test_classify_tool_basic() {
        let model_path = match std::env::var("DAGDA_MODEL_PATH") {
            Ok(p) => p,
            Err(_) => {
                eprintln!("DAGDA_MODEL_PATH not set, skipping test");
                return;
            }
        };

        let tool = ClassifyTool::new(&model_path).expect("Failed to load model");
        assert_eq!(tool.name(), "classify");
        assert!(!tool.description().is_empty());
    }

    #[test]
    fn test_classify_tool_execute() {
        let model_path = match std::env::var("DAGDA_MODEL_PATH") {
            Ok(p) => p,
            Err(_) => {
                eprintln!("DAGDA_MODEL_PATH not set, skipping test");
                return;
            }
        };

        let tool = ClassifyTool::new(&model_path).expect("Failed to load model");
        let args = json!({"text": "Water boils at 100C at sea level. Yes, that is correct."});
        let result = tool.execute(args).expect("Classification failed");

        assert!(result["label"].is_string());
        assert!(result["probability"].is_number());
        assert!(result["predictions"].is_array());
        assert!(!result["predictions"].as_array().unwrap().is_empty());

        let label = result["label"].as_str().unwrap();
        assert!(
            label.contains("resolved") || label.contains("unresolved"),
            "Unexpected label: {}",
            label
        );
    }

    #[test]
    fn test_classify_tool_top_k() {
        let model_path = match std::env::var("DAGDA_MODEL_PATH") {
            Ok(p) => p,
            Err(_) => {
                eprintln!("DAGDA_MODEL_PATH not set, skipping test");
                return;
            }
        };

        let tool = ClassifyTool::new(&model_path).expect("Failed to load model");
        let args = json!({"text": "Some claim about science.", "k": 2});
        let result = tool.execute(args).expect("Classification failed");

        let predictions = result["predictions"].as_array().unwrap();
        assert!(predictions.len() <= 2);
    }

    #[test]
    fn test_classify_tool_missing_text() {
        let model_path = match std::env::var("DAGDA_MODEL_PATH") {
            Ok(p) => p,
            Err(_) => {
                eprintln!("DAGDA_MODEL_PATH not set, skipping test");
                return;
            }
        };

        let tool = ClassifyTool::new(&model_path).expect("Failed to load model");
        let result = tool.execute(json!({"wrong_field": "hello"}));
        assert!(result.is_err());

        match result {
            Err(ToolError::InvalidArgs(msg)) => assert!(msg.contains("text")),
            other => panic!("Expected InvalidArgs, got: {:?}", other),
        }
    }

    #[test]
    fn test_classify_tool_invalid_model_path() {
        let result = ClassifyTool::new("/nonexistent/path/model.bin");
        assert!(result.is_err());
    }
}
