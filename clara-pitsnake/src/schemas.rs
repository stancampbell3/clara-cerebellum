use serde_json::{json, Value};

fn position_props() -> Value {
    json!({
        "file_path": {
            "type": "string",
            "description": "Absolute path to the source file"
        },
        "line": {
            "type": "integer",
            "description": "0-indexed line number (LSP convention)"
        },
        "column": {
            "type": "integer",
            "description": "0-indexed column (character) number (LSP convention)"
        }
    })
}

pub fn goto_definition_schema() -> Value {
    json!({
        "type": "object",
        "properties": position_props(),
        "required": ["file_path", "line", "column"]
    })
}

pub fn find_references_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "file_path": { "type": "string", "description": "Absolute path to the source file" },
            "line":      { "type": "integer", "description": "0-indexed line number" },
            "column":    { "type": "integer", "description": "0-indexed column number" },
            "include_declaration": {
                "type": "boolean",
                "description": "Include the declaration itself in results (default: true)",
                "default": true
            }
        },
        "required": ["file_path", "line", "column"]
    })
}

pub fn hover_schema() -> Value {
    json!({
        "type": "object",
        "properties": position_props(),
        "required": ["file_path", "line", "column"]
    })
}

pub fn get_completions_schema() -> Value {
    json!({
        "type": "object",
        "properties": position_props(),
        "required": ["file_path", "line", "column"]
    })
}

pub fn search_symbols_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "query": {
                "type": "string",
                "description": "Symbol name to search for (partial matches supported)"
            }
        },
        "required": ["query"]
    })
}

pub fn get_diagnostics_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "file_path": {
                "type": "string",
                "description": "Absolute path to the file to diagnose"
            }
        },
        "required": ["file_path"]
    })
}
