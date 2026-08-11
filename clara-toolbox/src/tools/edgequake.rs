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
    ListTenants,
    ListWorkspaces,
    ListModels,
}

/// Tool request arguments
#[derive(Debug, Deserialize)]
pub struct EdgequakeArgs {
    pub operation: Operation,

    // Multi-tenancy context. Confirmed live against Edgequake: tenant/workspace
    // scoping is enforced via X-Tenant-ID/X-Workspace-ID headers (not query or
    // body params — those are silently ignored). `tenant` doubles as the
    // required `tenant_id` path segment for `ListWorkspaces`.
    #[serde(default)]
    pub tenant: Option<String>,
    #[serde(default)]
    pub workspace: Option<String>,

    // Query
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub context: Option<String>,
    #[serde(default)]
    pub max_results: Option<i32>,
    // Provider/model override, `Query` operation only. Confirmed live as JSON
    // body fields on POST /api/v1/query (via a deliberately-invalid probe that
    // fails validation before dispatching to any provider).
    #[serde(default)]
    pub llm_provider: Option<String>,
    #[serde(default)]
    pub llm_model: Option<String>,

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
    default_tenant: Option<String>,
    default_workspace: Option<String>,
    http: Client,
}

impl EdgequakeClient {
    pub fn new(
        base_url: impl Into<String>,
        api_key: Option<String>,
        default_tenant: Option<String>,
        default_workspace: Option<String>,
    ) -> Self {
        Self {
            base_url: base_url.into(),
            api_key,
            default_tenant,
            default_workspace,
            http: Client::new(),
        }
    }

    /// Resolve a per-call tenant override against the client's configured
    /// default. `None` means "no tenant scoping" — Edgequake falls back to
    /// its own default tenant/workspace in that case.
    fn resolved_tenant<'a>(&'a self, tenant: Option<&'a str>) -> Option<&'a str> {
        tenant.or(self.default_tenant.as_deref())
    }

    fn resolved_workspace<'a>(&'a self, workspace: Option<&'a str>) -> Option<&'a str> {
        workspace.or(self.default_workspace.as_deref())
    }

    fn request(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, String)],
        body: Option<Value>,
        tenant: Option<&str>,
        workspace: Option<&str>,
    ) -> Result<Value, String> {
        let url = format!("{}{}", self.base_url.trim_end_matches('/'), path);
        let mut req = self.http.request(method, &url);
        if let Some(key) = &self.api_key {
            req = req.header("X-API-Key", key);
        }
        if let Some(t) = self.resolved_tenant(tenant) {
            req = req.header("X-Tenant-ID", t);
        }
        if let Some(w) = self.resolved_workspace(workspace) {
            req = req.header("X-Workspace-ID", w);
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

    pub fn query(
        &self,
        query: &str,
        mode: &str,
        max_results: Option<i32>,
        llm_provider: Option<&str>,
        llm_model: Option<&str>,
        tenant: Option<&str>,
        workspace: Option<&str>,
    ) -> Result<Value, String> {
        let mut body = serde_json::json!({ "query": query, "mode": mode });
        if let Some(mr) = max_results {
            body["max_results"] = serde_json::json!(mr);
        }
        if let Some(p) = llm_provider {
            body["llm_provider"] = serde_json::json!(p);
        }
        if let Some(m) = llm_model {
            body["llm_model"] = serde_json::json!(m);
        }
        self.request(Method::POST, "/api/v1/query", &[], Some(body), tenant, workspace)
    }

    pub fn graph_search_entities(
        &self,
        search: Option<&str>,
        label: Option<&str>,
        limit: Option<i32>,
        tenant: Option<&str>,
        workspace: Option<&str>,
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
        self.request(Method::GET, "/api/v1/graph/entities", &q, None, tenant, workspace)
    }

    pub fn graph_get_entity(
        &self,
        name: &str,
        tenant: Option<&str>,
        workspace: Option<&str>,
    ) -> Result<Value, String> {
        self.request(
            Method::GET,
            &format!("/api/v1/graph/entities/{name}"),
            &[],
            None,
            tenant,
            workspace,
        )
    }

    pub fn graph_entity_neighborhood(
        &self,
        name: &str,
        tenant: Option<&str>,
        workspace: Option<&str>,
    ) -> Result<Value, String> {
        self.request(
            Method::GET,
            &format!("/api/v1/graph/entities/{name}/neighborhood"),
            &[],
            None,
            tenant,
            workspace,
        )
    }

    pub fn graph_search_relationships(
        &self,
        source: Option<&str>,
        target: Option<&str>,
        label: Option<&str>,
        limit: Option<i32>,
        tenant: Option<&str>,
        workspace: Option<&str>,
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
        self.request(Method::GET, "/api/v1/graph/relationships", &q, None, tenant, workspace)
    }

    /// List all tenants. Not tenant/workspace-scoped — this is a cross-tenant
    /// admin listing, the whole point of which is discovering tenant IDs.
    pub fn list_tenants(&self) -> Result<Value, String> {
        self.request(Method::GET, "/api/v1/tenants", &[], None, None, None)
    }

    /// List workspaces under a tenant. `tenant` resolves the same way as
    /// everywhere else (per-call value falling back to the client default) —
    /// here it fills the `{tenant_id}` path segment rather than a header.
    pub fn list_workspaces(&self, tenant: Option<&str>) -> Result<Value, String> {
        let tenant_id = self.resolved_tenant(tenant).ok_or_else(|| {
            "list_workspaces requires a tenant (pass `tenant` or configure \
             EDGEQUAKE_DEFAULT_TENANT)"
                .to_string()
        })?;
        self.request(
            Method::GET,
            &format!("/api/v1/tenants/{tenant_id}/workspaces"),
            &[],
            None,
            None,
            None,
        )
    }

    /// List all configured LLM/embedding providers and their models.
    pub fn list_models(&self) -> Result<Value, String> {
        self.request(Method::GET, "/api/v1/models", &[], None, None, None)
    }
}

/// ClaraEdgequakeTool - Bridge to the Edgequake RAG API
pub struct ClaraEdgequakeTool {
    client: EdgequakeClient,
}

impl ClaraEdgequakeTool {
    pub fn new(
        base_url: impl Into<String>,
        api_key: Option<String>,
        default_tenant: Option<String>,
        default_workspace: Option<String>,
    ) -> Self {
        Self {
            client: EdgequakeClient::new(base_url, api_key, default_tenant, default_workspace),
        }
    }

    fn execute_operation(&self, args: EdgequakeArgs) -> Result<Value, ToolError> {
        let t = args.tenant.as_deref();
        let w = args.workspace.as_deref();

        match args.operation {
            Operation::Query => {
                let query = args
                    .query
                    .ok_or_else(|| ToolError::InvalidArgs("'query' required".into()))?;
                let mode = args.mode.unwrap_or_else(|| "hybrid".to_string());
                self.client
                    .query(
                        &query,
                        &mode,
                        args.max_results,
                        args.llm_provider.as_deref(),
                        args.llm_model.as_deref(),
                        t,
                        w,
                    )
                    .map_err(ToolError::ExecutionFailed)
            }

            Operation::GraphSearchEntities => self
                .client
                .graph_search_entities(
                    args.search.as_deref(),
                    args.label.as_deref(),
                    args.limit,
                    t,
                    w,
                )
                .map_err(ToolError::ExecutionFailed),

            Operation::GraphGetEntity => {
                let entity_name = args
                    .entity_name
                    .ok_or_else(|| ToolError::InvalidArgs("'entity_name' required".into()))?;
                self.client
                    .graph_get_entity(&entity_name, t, w)
                    .map_err(ToolError::ExecutionFailed)
            }

            Operation::GraphEntityNeighborhood => {
                let entity_name = args
                    .entity_name
                    .ok_or_else(|| ToolError::InvalidArgs("'entity_name' required".into()))?;
                self.client
                    .graph_entity_neighborhood(&entity_name, t, w)
                    .map_err(ToolError::ExecutionFailed)
            }

            Operation::GraphSearchRelationships => self
                .client
                .graph_search_relationships(
                    args.source.as_deref(),
                    args.target.as_deref(),
                    args.label.as_deref(),
                    args.limit,
                    t,
                    w,
                )
                .map_err(ToolError::ExecutionFailed),

            Operation::ListTenants => self.client.list_tenants().map_err(ToolError::ExecutionFailed),

            Operation::ListWorkspaces => {
                self.client.list_workspaces(t).map_err(ToolError::ExecutionFailed)
            }

            Operation::ListModels => self.client.list_models().map_err(ToolError::ExecutionFailed),
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
        let tool = ClaraEdgequakeTool::new("http://localhost:8082", None, None, None);
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
    fn test_query_args_with_provider_override() {
        let json = r#"{"operation": "query", "query": "hi", "llm_provider": "ollama", "llm_model": "gemma4:e4b"}"#;
        let args: EdgequakeArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.llm_provider, Some("ollama".to_string()));
        assert_eq!(args.llm_model, Some("gemma4:e4b".to_string()));
    }

    #[test]
    fn test_graph_get_entity_args() {
        let json = r#"{"operation": "graph_get_entity", "entity_name": "Clara"}"#;
        let args: EdgequakeArgs = serde_json::from_str(json).unwrap();
        assert!(matches!(args.operation, Operation::GraphGetEntity));
        assert_eq!(args.entity_name, Some("Clara".to_string()));
    }

    #[test]
    fn test_list_workspaces_args() {
        let json = r#"{"operation": "list_workspaces", "tenant": "00000000-0000-0000-0000-000000000002"}"#;
        let args: EdgequakeArgs = serde_json::from_str(json).unwrap();
        assert!(matches!(args.operation, Operation::ListWorkspaces));
        assert_eq!(args.tenant, Some("00000000-0000-0000-0000-000000000002".to_string()));
    }

    #[test]
    fn test_missing_operation_fails() {
        let json = r#"{"query": "hello"}"#;
        let result: Result<EdgequakeArgs, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_list_workspaces_requires_tenant() {
        let client = EdgequakeClient::new("http://localhost:8082", None, None, None);
        let err = client.list_workspaces(None).unwrap_err();
        assert!(err.contains("requires a tenant"));
    }
}
