// Integration test for ClassifyTool via ToolboxManager

use clara_toolbox::tool::ToolRequest;
use clara_toolbox::tools::ClassifyTool;
use clara_toolbox::ToolboxManager;
use serde_json::json;
use std::sync::Arc;

fn get_model_path() -> Option<String> {
    std::env::var("DAGDA_MODEL_PATH").ok()
}

#[test]
fn test_classify_through_toolbox_manager() {
    let model_path = match get_model_path() {
        Some(p) => p,
        None => {
            eprintln!("DAGDA_MODEL_PATH not set, skipping integration test");
            return;
        }
    };

    let mut mgr = ToolboxManager::new();
    let tool = ClassifyTool::new(&model_path).expect("Failed to load model");
    mgr.register_tool(Arc::new(tool));

    let request = ToolRequest {
        tool: "classify".to_string(),
        arguments: json!({
            "text": "Water boils at 100C at sea level. Yes, that is correct."
        }),
    };

    let response = mgr.execute_tool(&request).expect("Tool execution failed");
    assert_eq!(response.status, "success");
}

#[test]
fn test_classify_resolved_text() {
    let model_path = match get_model_path() {
        Some(p) => p,
        None => {
            eprintln!("DAGDA_MODEL_PATH not set, skipping integration test");
            return;
        }
    };

    let mut mgr = ToolboxManager::new();
    let tool = ClassifyTool::new(&model_path).expect("Failed to load model");
    mgr.register_tool(Arc::new(tool));

    let request = ToolRequest {
        tool: "classify".to_string(),
        arguments: json!({
            "text": "The Earth orbits the Sun. Yes, that is correct, the Earth revolves around the Sun in an elliptical orbit."
        }),
    };

    let response = mgr.execute_tool(&request).expect("Tool execution failed");
    assert_eq!(response.status, "success");

    let result_str = serde_json::to_string(&response).unwrap();
    println!("Resolved text classification: {}", result_str);
}

#[test]
fn test_classify_unresolved_text() {
    let model_path = match get_model_path() {
        Some(p) => p,
        None => {
            eprintln!("DAGDA_MODEL_PATH not set, skipping integration test");
            return;
        }
    };

    let mut mgr = ToolboxManager::new();
    let tool = ClassifyTool::new(&model_path).expect("Failed to load model");
    mgr.register_tool(Arc::new(tool));

    let request = ToolRequest {
        tool: "classify".to_string(),
        arguments: json!({
            "text": "Cats can fly using their tails as propellers. The cosmic energy of the universe propels felines through the astral plane!"
        }),
    };

    let response = mgr.execute_tool(&request).expect("Tool execution failed");
    assert_eq!(response.status, "success");

    let result_str = serde_json::to_string(&response).unwrap();
    println!("Unresolved text classification: {}", result_str);
}
