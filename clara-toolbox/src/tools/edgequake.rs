//! ClaraEdgequakeTool - Bridge between CLIPS/Prolog and the Edgequake RAG API
//!
//! This tool enables CLIPS rules (via `clara-evaluate`) and Prolog predicates
//! (via `clara_evaluate/2`, see `the_cow.pl`) to query Edgequake's graphical
//! RAG system and read its knowledge graph.

use crate::tool::{Tool, ToolError};
use reqwest::blocking::Client;
use reqwest::Method;
use serde::Deserialize;
use serde_json::Value;

/// Operations supported by the Edgequake tool
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    Query,
    GraphSearchEntities,
    GraphGetEntity,
    GraphEntityNeighborhood,
    GraphSearchRelationships,
}

/// Tool request arguments
#[derive(Debug, Deserialize)]
pub struct EdgequakeArgs {
    pub operation: Operation,

    // Query
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub context: Option<String>,
    #[serde(default)]
    pub max_results: Option<i32>,

    // Graph entities/relationships
    #[serde(default)]
    pub entity_name: Option<String>,
    #[serde(default)]
    pub search: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub limit: Option<i32>,
}

/// Thin blocking HTTP client for the Edgequake REST API.
pub struct EdgequakeClient {
    base_url: String,
    api_key: Option<String>,
    http: Client,
}

impl EdgequakeClient {
    pub fn new(base_url: impl Into<String>, api_key: Option<String>) -> Self {
        Self {
            base_url: base_url.into(),
            api_key,
            http: Client::new(),
        }
    }

    fn request(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, String)],
        body: Option<Value>,
    ) -> Result<Value, String> {
        let url = format!("{}{}", self.base_url.trim_end_matches('/'), path);
        let mut req = self.http.request(method, &url);
        if let Some(key) = &self.api_key {
            req = req.header("X-API-Key", key);
        }
        if !query.is_empty() {
            req = req.query(query);
        }
        if let Some(b) = body {
            req = req.json(&b);
        }

        let resp = req.send().map_err(|e| format!("Edgequake request failed: {e}"))?;
        let status = resp.status();
        let json: Value = resp
            .json()
            .map_err(|e| format!("Edgequake response was not valid JSON: {e}"))?;

        if !status.is_success() {
            return Err(format!("Edgequake API error {status}: {json}"));
        }
        Ok(json)
    }

    pub fn query(&self, query: &str, mode: &str, max_results: Option<i32>) -> Result<Value, String> {
        let mut body = serde_json::json!({ "query": query, "mode": mode });
        if let Some(mr) = max_results {
            body["max_results"] = serde_json::json!(mr);
        }
        self.request(Method::POST, "/api/v1/query", &[], Some(body))
    }

    pub fn graph_search_entities(
        &self,
        search: Option<&str>,
        label: Option<&str>,
        limit: Option<i32>,
    ) -> Result<Value, String> {
        let mut q = Vec::new();
        if let Some(s) = search {
            q.push(("search", s.to_string()));
        }
        if let Some(l) = label {
            q.push(("label", l.to_string()));
        }
        if let Some(lim) = limit {
            q.push(("limit", lim.to_string()));
        }
        self.request(Method::GET, "/api/v1/graph/entities", &q, None)
    }

    pub fn graph_get_entity(&self, name: &str) -> Result<Value, String> {
        self.request(
            Method::GET,
            &format!("/api/v1/graph/entities/{name}"),
            &[],
            None,
        )
    }

    pub fn graph_entity_neighborhood(&self, name: &str) -> Result<Value, String> {
        self.request(
            Method::GET,
            &format!("/api/v1/graph/entities/{name}/neighborhood"),
            &[],
            None,
        )
    }

    pub fn graph_search_relationships(
        &self,
        source: Option<&str>,
        target: Option<&str>,
        label: Option<&str>,
        limit: Option<i32>,
    ) -> Result<Value, String> {
        let mut q = Vec::new();
        if let Some(s) = source {
            q.push(("source", s.to_string()));
        }
        if let Some(t) = target {
            q.push(("target", t.to_string()));
        }
        if let Some(l) = label {
            q.push(("label", l.to_string()));
        }
        if let Some(lim) = limit {
            q.push(("limit", lim.to_string()));
        }
        self.request(Method::GET, "/api/v1/graph/relationships", &q, None)
    }
}

/// ClaraEdgequakeTool - Bridge to the Edgequake RAG API
pub struct ClaraEdgequakeTool {
    client: EdgequakeClient,
}

impl ClaraEdgequakeTool {
    pub fn new(base_url: impl Into<String>, api_key: Option<String>) -> Self {
        Self {
            client: EdgequakeClient::new(base_url, api_key),
        }
    }

    fn execute_operation(&self, args: EdgequakeArgs) -> Result<Value, ToolError> {
        match args.operation {
            Operation::Query => {
                let query = args
                    .query
                    .ok_or_else(|| ToolError::InvalidArgs("'query' required".into()))?;
                let mode = args.mode.unwrap_or_else(|| "hybrid".to_string());
                self.client
                    .query(&query, &mode, args.max_results)
                    .map_err(ToolError::ExecutionFailed)
            }

            Operation::GraphSearchEntities => self
                .client
                .graph_search_entities(
                    args.search.as_deref(),
                    args.label.as_deref(),
                    args.limit,
                )
                .map_err(ToolError::ExecutionFailed),

            Operation::GraphGetEntity => {
                let entity_name = args
                    .entity_name
                    .ok_or_else(|| ToolError::InvalidArgs("'entity_name' required".into()))?;
                self.client
                    .graph_get_entity(&entity_name)
                    .map_err(ToolError::ExecutionFailed)
            }

            Operation::GraphEntityNeighborhood => {
                let entity_name = args
                    .entity_name
                    .ok_or_else(|| ToolError::InvalidArgs("'entity_name' required".into()))?;
                self.client
                    .graph_entity_neighborhood(&entity_name)
                    .map_err(ToolError::ExecutionFailed)
            }

            Operation::GraphSearchRelationships => self
                .client
                .graph_search_relationships(
                    args.source.as_deref(),
                    args.target.as_deref(),
                    args.label.as_deref(),
                    args.limit,
                )
                .map_err(ToolError::ExecutionFailed),
        }
    }
}

impl Tool for ClaraEdgequakeTool {
    fn name(&self) -> &str {
        "edgequake"
    }

    fn description(&self) -> &str {
        "Bridge to the Edgequake RAG API for graph-backed knowledge queries"
    }

    fn execute(&self, args: Value) -> Result<Value, ToolError> {
        log::debug!("EdgequakeTool executing with args: {}", args);

        let parsed_args: EdgequakeArgs = serde_json::from_value(args)
            .map_err(|e| ToolError::InvalidArgs(format!("Failed to parse arguments: {}", e)))?;

        self.execute_operation(parsed_args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_name() {
        let tool = ClaraEdgequakeTool::new("http://localhost:8082", None);
        assert_eq!(tool.name(), "edgequake");
    }

    #[test]
    fn test_query_args_default_mode() {
        let json = r#"{"operation": "query", "query": "what is clara?"}"#;
        let args: EdgequakeArgs = serde_json::from_str(json).unwrap();
        assert!(matches!(args.operation, Operation::Query));
        assert_eq!(args.query, Some("what is clara?".to_string()));
        assert_eq!(args.mode, None);
    }

    #[test]
    fn test_graph_get_entity_args() {
        let json = r#"{"operation": "graph_get_entity", "entity_name": "Clara"}"#;
        let args: EdgequakeArgs = serde_json::from_str(json).unwrap();
        assert!(matches!(args.operation, Operation::GraphGetEntity));
        assert_eq!(args.entity_name, Some("Clara".to_string()));
    }

    #[test]
    fn test_missing_operation_fails() {
        let json = r#"{"query": "hello"}"#;
        let result: Result<EdgequakeArgs, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }
}
