use anyhow::Result;
use log::{debug, error, info};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
use uuid::Uuid;

use crate::client::PrologClient;
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
                format!("mcp-prolog-{}", Uuid::new_v4()),
            )),
        }
    }

    pub async fn run(&self) -> Result<()> {
        info!("Prolog MCP Server starting");

        // Create the session on startup
        let client = PrologClient::new(
            self.rest_api_url.clone(),
            self.session_id.lock().await.clone(),
        );
        match client.ensure_session("mcp-prolog-client").await {
            Ok(real_session_id) => {
                info!("Created Prolog session: {}", real_session_id);
                *self.session_id.lock().await = real_session_id;
            }
            Err(e) => {
                error!("Failed to create initial Prolog session: {}", e);
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

        info!("Prolog MCP Server shutting down");
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
            "name": "clara-prolog",
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
                    "name": "prolog.query",
                    "description": "Execute a Prolog query and return results. Returns first solution by default, or all solutions if specified.",
                    "inputSchema": schemas::query_schema()
                },
                {
                    "name": "prolog.consult",
                    "description": "Load Prolog clauses (facts and rules) into the knowledge base using assertz",
                    "inputSchema": schemas::consult_schema()
                },
                {
                    "name": "prolog.retract",
                    "description": "Remove clauses from the Prolog knowledge base",
                    "inputSchema": schemas::retract_schema()
                },
                {
                    "name": "prolog.status",
                    "description": "Get status of the Prolog engine and session",
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
        let client = PrologClient::new(self.rest_api_url.clone(), session_id);

        match name {
            "prolog.query" => tools::query(&client, &arguments).await,
            "prolog.consult" => tools::consult(&client, &arguments).await,
            "prolog.retract" => tools::retract(&client, &arguments).await,
            "prolog.status" => tools::status(&client, &arguments).await,
            _ => json!({
                "error": format!("Unknown tool: {}", name)
            }),
        }
    }
}
