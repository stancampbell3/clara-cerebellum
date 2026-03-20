use log::{debug, error};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
use std::sync::Arc;

use crate::lsp_client::LspClient;
use crate::{schemas, tools};

// ---------------------------------------------------------------------------
// JSON-RPC types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    #[allow(dead_code)]
    jsonrpc: String,
    #[serde(default)]
    id: Value,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

// ---------------------------------------------------------------------------
// McpServer
// ---------------------------------------------------------------------------

pub struct McpServer {
    lsp: Arc<LspClient>,
}

impl McpServer {
    pub fn new(lsp: LspClient) -> Self {
        Self {
            lsp: Arc::new(lsp),
        }
    }

    /// Stdio transport loop.
    pub async fn run_stdio(&self) -> anyhow::Result<()> {
        let stdin = io::stdin();
        let mut stdout = io::stdout();
        let reader = stdin.lock();
        let mut lines = reader.lines();

        while let Some(Ok(line)) = lines.next() {
            let response = self.handle_line(&line).await;
            if let Ok(json) = serde_json::to_string(&response) {
                writeln!(stdout, "{}", json)?;
                stdout.flush()?;
            } else {
                error!("Failed to serialize MCP response");
            }
        }

        Ok(())
    }

    /// HTTP transport entry point: process a parsed JSON value.
    pub async fn handle_json(&self, request: Value) -> Value {
        match serde_json::from_value::<JsonRpcRequest>(request) {
            Ok(req) => {
                let resp = self.handle_request(&req).await;
                serde_json::to_value(resp)
                    .unwrap_or_else(|_| json!({"error": "serialization error"}))
            }
            Err(e) => serde_json::to_value(JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: Value::Null,
                result: None,
                error: Some(JsonRpcError {
                    code: -32700,
                    message: "Parse error".to_string(),
                    data: Some(json!({ "error": e.to_string() })),
                }),
            })
            .unwrap(),
        }
    }

    async fn handle_line(&self, line: &str) -> JsonRpcResponse {
        match serde_json::from_str::<JsonRpcRequest>(line) {
            Ok(req) => self.handle_request(&req).await,
            Err(e) => {
                error!("MCP parse error: {}", e);
                JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: Value::Null,
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32700,
                        message: "Parse error".to_string(),
                        data: Some(json!({ "error": e.to_string() })),
                    }),
                }
            }
        }
    }

    async fn handle_request(&self, req: &JsonRpcRequest) -> JsonRpcResponse {
        debug!("MCP method: {}", req.method);

        let result = match req.method.as_str() {
            "initialize" => self.handle_initialize(),
            "notifications/initialized" => {
                return JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: req.id.clone(),
                    result: Some(Value::Null),
                    error: None,
                };
            }
            "tools/list" => self.handle_tools_list(),
            "tools/call" => self.handle_tools_call(&req.params).await,
            _ => {
                return JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: req.id.clone(),
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32601,
                        message: format!("Method not found: {}", req.method),
                        data: None,
                    }),
                };
            }
        };

        JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: req.id.clone(),
            result: Some(result),
            error: None,
        }
    }

    fn handle_initialize(&self) -> Value {
        json!({
            "protocolVersion": "2024-11-05",
            "serverInfo": {
                "name": "clara-pitsnake",
                "version": "0.1.0"
            },
            "capabilities": {
                "tools": {}
            }
        })
    }

    fn handle_tools_list(&self) -> Value {
        json!({
            "tools": [
                {
                    "name": "lsp_goto_definition",
                    "description": "Jump to the definition of the symbol at the given position. Returns a list of locations.",
                    "inputSchema": schemas::goto_definition_schema()
                },
                {
                    "name": "lsp_find_references",
                    "description": "Find all references to the symbol at the given position across the workspace.",
                    "inputSchema": schemas::find_references_schema()
                },
                {
                    "name": "lsp_hover",
                    "description": "Get type information and documentation for the symbol at the given position.",
                    "inputSchema": schemas::hover_schema()
                },
                {
                    "name": "lsp_get_completions",
                    "description": "Get code completion suggestions at the given position.",
                    "inputSchema": schemas::get_completions_schema()
                },
                {
                    "name": "lsp_search_symbols",
                    "description": "Search for symbols (functions, types, variables) by name across the workspace.",
                    "inputSchema": schemas::search_symbols_schema()
                },
                {
                    "name": "lsp_get_diagnostics",
                    "description": "Get errors, warnings, and hints for a source file as reported by the language server.",
                    "inputSchema": schemas::get_diagnostics_schema()
                }
            ]
        })
    }

    async fn handle_tools_call(&self, params: &Value) -> Value {
        let name = match params.get("name").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => return json!({ "error": "Missing 'name' in tools/call params" }),
        };
        let arguments = params.get("arguments").cloned().unwrap_or(json!({}));
        let lsp = &self.lsp;

        let content = match name {
            "lsp_goto_definition" => tools::goto_definition(lsp, &arguments).await,
            "lsp_find_references" => tools::find_references(lsp, &arguments).await,
            "lsp_hover" => tools::hover(lsp, &arguments).await,
            "lsp_get_completions" => tools::get_completions(lsp, &arguments).await,
            "lsp_search_symbols" => tools::search_symbols(lsp, &arguments).await,
            "lsp_get_diagnostics" => tools::get_diagnostics(lsp, &arguments).await,
            _ => json!({ "error": format!("Unknown tool: {}", name) }),
        };

        // Wrap in MCP content envelope.
        json!({
            "content": [{
                "type": "text",
                "text": serde_json::to_string_pretty(&content).unwrap_or_default()
            }]
        })
    }
}
