use anyhow::{Result};
use log::{debug, error, info};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
use uuid::Uuid;

use crate::client::ClipsClient;
use crate::schemas;
use crate::tools;

/// MCP JSON-RPC request
#[derive(Debug, Serialize, Deserialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    id: Value,
    method: String,
    #[serde(default)]
    params: Value,
}

/// MCP JSON-RPC response
#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

/// MCP JSON-RPC error
#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

pub struct McpServer {
    rest_api_url: String,
    session_id: std::sync::Arc<tokio::sync::Mutex<String>>,
}

impl McpServer {
    pub fn new(rest_api_url: String) -> Self {
        Self {
            rest_api_url,
            session_id: std::sync::Arc::new(tokio::sync::Mutex::new(
                format!("mcp-{}", Uuid::new_v4()),
            )),
        }
    }

    pub async fn run(&self) -> Result<()> {
        info!("MCP Server starting");

        // Create the session on startup
        let client = crate::client::ClipsClient::new(
            self.rest_api_url.clone(),
            self.session_id.lock().await.clone(),
        );
        match client.ensure_session("mcp-client").await {
            Ok(real_session_id) => {
                info!("Created session: {}", real_session_id);
                *self.session_id.lock().await = real_session_id;
            }
            Err(e) => {
                error!("Failed to create initial session: {}", e);
                return Err(e);
            }
        }

        let stdin = io::stdin();
        let mut stdout = io::stdout();
        let reader = stdin.lock();
        let mut lines = reader.lines();

        while let Some(Ok(line)) = lines.next() {
            debug!("Received line: {}", line);

            let response = self.handle_line(&line).await;

            // Write response to stdout
            if let Ok(json) = serde_json::to_string(&response) {
                debug!("Sending response: {}", json);
                writeln!(stdout, "{}", json)?;
                stdout.flush()?;
            } else {
                error!("Failed to serialize response");
            }
        }

        info!("MCP Server shutting down");
        Ok(())
    }

    async fn handle_line(&self, line: &str) -> JsonRpcResponse {
        match serde_json::from_str::<JsonRpcRequest>(line) {
            Ok(req) => self.handle_request(&req).await,
            Err(e) => {
                error!("Failed to parse request: {}", e);
                JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: Value::Null,
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32700,
                        message: "Parse error".to_string(),
                        data: Some(json!({"error": e.to_string()})),
                    }),
                }
            }
        }
    }

    async fn handle_request(&self, req: &JsonRpcRequest) -> JsonRpcResponse {
        debug!("Handling method: {}", req.method);

        let result = match req.method.as_str() {
            "initialize" => self.handle_initialize(),
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
            "name": "clara-clips",
            "version": "0.1.0",
            "capabilities": {
                "tools": true,
                "resources": false
            }
        })
    }

    fn handle_tools_list(&self) -> Value {
        json!({
            "tools": [
                {
                    "name": "clips.eval",
                    "description": "Evaluate CLIPS expressions and return results",
                    "inputSchema": schemas::eval_schema()
                },
                {
                    "name": "clips.query",
                    "description": "Query facts from the CLIPS engine",
                    "inputSchema": schemas::query_schema()
                },
                {
                    "name": "clips.assert",
                    "description": "Assert facts into the CLIPS engine",
                    "inputSchema": schemas::assert_schema()
                },
                {
                    "name": "clips.reset",
                    "description": "Reset the CLIPS engine to initial state",
                    "inputSchema": schemas::reset_schema()
                },
                {
                    "name": "clips.status",
                    "description": "Get status of the CLIPS engine",
                    "inputSchema": schemas::status_schema()
                }
            ]
        })
    }

    async fn handle_tools_call(&self, params: &Value) -> Value {
        let name = match params.get("name").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => {
                return json!({
                    "error": "Missing 'name' parameter"
                });
            }
        };

        let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

        let session_id = self.session_id.lock().await.clone();
        let client = ClipsClient::new(self.rest_api_url.clone(), session_id);

        match name {
            "clips.eval" => tools::eval(&client, &arguments).await,
            "clips.query" => tools::query(&client, &arguments).await,
            "clips.assert" => tools::assert_facts(&client, &arguments).await,
            "clips.reset" => tools::reset(&client, &arguments).await,
            "clips.status" => tools::status(&client, &arguments).await,
            _ => json!({
                "error": format!("Unknown tool: {}", name)
            }),
        }
    }
}
