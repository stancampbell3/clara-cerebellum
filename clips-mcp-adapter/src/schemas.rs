use serde_json::{json, Value};

/// Schema for clips.eval tool
pub fn eval_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "expression": {
                "type": "string",
                "description": "CLIPS expression to evaluate (e.g., '(+ 1 2)' or '(printout t \"hello\")')"
            }
        },
        "required": ["expression"]
    })
}

/// Schema for clips.query tool
pub fn query_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "template": {
                "type": "string",
                "description": "CLIPS template/fact pattern to query (e.g., '(myfact ?x ?y)')"
            },
            "limit": {
                "type": "integer",
                "description": "Maximum number of results to return (optional)",
                "default": 100
            }
        },
        "required": ["template"]
    })
}

/// Schema for clips.assert tool
pub fn assert_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "facts": {
                "type": "array",
                "description": "List of facts to assert",
                "items": {
                    "type": "string",
                    "description": "Fact template (e.g., '(myfact 1 \"value\")')"
                }
            }
        },
        "required": ["facts"]
    })
}

/// Schema for clips.reset tool
pub fn reset_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "preserve_globals": {
                "type": "boolean",
                "description": "Whether to preserve global variables (optional)",
                "default": false
            }
        }
    })
}

/// Schema for clips.status tool
pub fn status_schema() -> Value {
    json!({
        "type": "object",
        "properties": {}
    })
}
