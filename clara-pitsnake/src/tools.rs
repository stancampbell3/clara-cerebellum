use log::{debug, error};
use serde_json::{json, Value};
use tokio::fs;

use crate::lsp_client::{file_path_to_uri, language_id_for, LspClient};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn require_str<'a>(args: &'a Value, key: &str) -> Result<&'a str, Value> {
    args.get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| json!({ "error": format!("Missing required parameter: {}", key) }))
}

fn require_u32(args: &Value, key: &str) -> Result<u32, Value> {
    args.get(key)
        .and_then(|v| v.as_u64())
        .map(|n| n as u32)
        .ok_or_else(|| json!({ "error": format!("Missing required parameter: {}", key) }))
}

fn position_params(file_path: &str, line: u32, column: u32) -> Value {
    json!({
        "textDocument": { "uri": file_path_to_uri(file_path) },
        "position":     { "line": line, "character": column }
    })
}

/// Normalise `Hover.contents` — it can be:
///   - a `MarkupContent` object: `{ kind: "markdown"|"plaintext", value: "..." }`
///   - a `MarkedString`:         `{ language: "rust", value: "..." }` or just a bare string
///   - an array of `MarkedString`
fn normalise_hover_contents(contents: &Value) -> String {
    match contents {
        Value::String(s) => s.clone(),
        Value::Object(map) => {
            // MarkupContent or single MarkedString
            if let Some(v) = map.get("value").and_then(|v| v.as_str()) {
                return v.to_string();
            }
            contents.to_string()
        }
        Value::Array(arr) => arr
            .iter()
            .map(|item| match item {
                Value::String(s) => s.clone(),
                Value::Object(m) => m
                    .get("value")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                _ => String::new(),
            })
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n"),
        _ => String::new(),
    }
}

/// Normalise a location response which may be:
///   - `null`
///   - a single `Location`
///   - an array of `Location`
///   - an array of `LocationLink` (definition only)
fn normalise_locations(value: &Value) -> Vec<Value> {
    match value {
        Value::Null => vec![],
        Value::Array(arr) => arr
            .iter()
            .map(|item| {
                // LocationLink has targetUri/targetRange; map to Location shape.
                if item.get("targetUri").is_some() {
                    json!({
                        "uri":   item["targetUri"],
                        "range": item.get("targetSelectionRange").or_else(|| item.get("targetRange")).cloned().unwrap_or(Value::Null)
                    })
                } else {
                    item.clone()
                }
            })
            .collect(),
        Value::Object(_) => vec![value.clone()],
        _ => vec![],
    }
}

fn severity_label(sev: Option<i64>) -> &'static str {
    match sev {
        Some(1) => "error",
        Some(2) => "warning",
        Some(3) => "information",
        Some(4) => "hint",
        _ => "unknown",
    }
}

// ---------------------------------------------------------------------------
// Tool implementations
// ---------------------------------------------------------------------------

pub async fn goto_definition(client: &LspClient, args: &Value) -> Value {
    let file_path = match require_str(args, "file_path") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let line = match require_u32(args, "line") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let column = match require_u32(args, "column") {
        Ok(v) => v,
        Err(e) => return e,
    };

    debug!("lsp_goto_definition {}:{}:{}", file_path, line, column);

    match client
        .request("textDocument/definition", position_params(file_path, line, column))
        .await
    {
        Ok(result) => {
            let locations = normalise_locations(&result);
            json!({ "definitions": locations })
        }
        Err(e) => {
            error!("goto_definition error: {}", e);
            json!({ "error": e.to_string() })
        }
    }
}

pub async fn find_references(client: &LspClient, args: &Value) -> Value {
    let file_path = match require_str(args, "file_path") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let line = match require_u32(args, "line") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let column = match require_u32(args, "column") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let include_declaration = args
        .get("include_declaration")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    debug!("lsp_find_references {}:{}:{}", file_path, line, column);

    let params = json!({
        "textDocument": { "uri": file_path_to_uri(file_path) },
        "position":     { "line": line, "character": column },
        "context":      { "includeDeclaration": include_declaration }
    });

    match client.request("textDocument/references", params).await {
        Ok(result) => {
            let refs = normalise_locations(&result);
            json!({ "references": refs })
        }
        Err(e) => {
            error!("find_references error: {}", e);
            json!({ "error": e.to_string() })
        }
    }
}

pub async fn hover(client: &LspClient, args: &Value) -> Value {
    let file_path = match require_str(args, "file_path") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let line = match require_u32(args, "line") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let column = match require_u32(args, "column") {
        Ok(v) => v,
        Err(e) => return e,
    };

    debug!("lsp_hover {}:{}:{}", file_path, line, column);

    match client
        .request("textDocument/hover", position_params(file_path, line, column))
        .await
    {
        Ok(Value::Null) => json!({ "contents": null }),
        Ok(result) => {
            let contents_raw = result.get("contents").unwrap_or(&Value::Null);
            let contents = normalise_hover_contents(contents_raw);
            let range = result.get("range").cloned().unwrap_or(Value::Null);
            json!({ "contents": contents, "range": range })
        }
        Err(e) => {
            error!("hover error: {}", e);
            json!({ "error": e.to_string() })
        }
    }
}

pub async fn get_completions(client: &LspClient, args: &Value) -> Value {
    let file_path = match require_str(args, "file_path") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let line = match require_u32(args, "line") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let column = match require_u32(args, "column") {
        Ok(v) => v,
        Err(e) => return e,
    };

    debug!("lsp_get_completions {}:{}:{}", file_path, line, column);

    match client
        .request("textDocument/completion", position_params(file_path, line, column))
        .await
    {
        Ok(result) => {
            // Response may be CompletionList { isIncomplete, items } or Item[].
            let items: &Vec<Value> = &match &result {
                Value::Array(arr) => arr.clone(),
                Value::Object(map) => map
                    .get("items")
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default(),
                _ => vec![],
            };

            let completions: Vec<Value> = items
                .iter()
                .map(|item| {
                    let doc = match item.get("documentation") {
                        Some(Value::String(s)) => s.clone(),
                        Some(Value::Object(m)) => m
                            .get("value")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        _ => String::new(),
                    };
                    json!({
                        "label":         item.get("label").cloned().unwrap_or(Value::Null),
                        "kind":          item.get("kind").cloned().unwrap_or(Value::Null),
                        "detail":        item.get("detail").cloned().unwrap_or(Value::Null),
                        "documentation": doc,
                    })
                })
                .collect();

            json!({ "completions": completions })
        }
        Err(e) => {
            error!("get_completions error: {}", e);
            json!({ "error": e.to_string() })
        }
    }
}

pub async fn search_symbols(client: &LspClient, args: &Value) -> Value {
    let query = match require_str(args, "query") {
        Ok(v) => v,
        Err(e) => return e,
    };

    debug!("lsp_search_symbols query={:?}", query);

    match client
        .request("workspace/symbol", json!({ "query": query }))
        .await
    {
        Ok(result) => {
            let items = match &result {
                Value::Array(arr) => arr.clone(),
                _ => vec![],
            };

            let symbols: Vec<Value> = items
                .iter()
                .map(|item| {
                    // WorkspaceSymbol (LSP 3.17) vs SymbolInformation (3.16):
                    // - WorkspaceSymbol: location may be { uri } (no range)
                    // - SymbolInformation: location always { uri, range }
                    json!({
                        "name":           item.get("name").cloned().unwrap_or(Value::Null),
                        "kind":           item.get("kind").cloned().unwrap_or(Value::Null),
                        "container_name": item.get("containerName").cloned().unwrap_or(Value::Null),
                        "location":       item.get("location").cloned().unwrap_or(Value::Null),
                    })
                })
                .collect();

            json!({ "symbols": symbols })
        }
        Err(e) => {
            error!("search_symbols error: {}", e);
            json!({ "error": e.to_string() })
        }
    }
}

pub async fn get_diagnostics(client: &LspClient, args: &Value) -> Value {
    let file_path = match require_str(args, "file_path") {
        Ok(v) => v,
        Err(e) => return e,
    };

    debug!("lsp_get_diagnostics file={}", file_path);

    let text = match fs::read_to_string(file_path).await {
        Ok(t) => t,
        Err(e) => {
            return json!({ "error": format!("Cannot read file '{}': {}", file_path, e) });
        }
    };

    let uri = file_path_to_uri(file_path);
    let lang = language_id_for(file_path);

    match client.fetch_diagnostics(&uri, &text, lang).await {
        Ok(raw_diags) => {
            let diagnostics: Vec<Value> = raw_diags
                .iter()
                .map(|d| {
                    let sev = d.get("severity").and_then(|s| s.as_i64());
                    json!({
                        "severity": severity_label(sev),
                        "range":    d.get("range").cloned().unwrap_or(Value::Null),
                        "message":  d.get("message").and_then(|m| m.as_str()).unwrap_or(""),
                        "source":   d.get("source").cloned().unwrap_or(Value::Null),
                        "code":     d.get("code").cloned().unwrap_or(Value::Null),
                    })
                })
                .collect();

            json!({
                "file_path":   file_path,
                "diagnostics": diagnostics,
            })
        }
        Err(e) => {
            error!("get_diagnostics error: {}", e);
            json!({ "error": e.to_string() })
        }
    }
}
