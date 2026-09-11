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
use std::time::Duration;

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

/// Per-request Mix mode weight override for the `query` operation, forwarded
/// as Edgequake's `mix_weights` request field (SPEC-022 P-H6). Mirrors
/// Edgequake's own `MixWeightRequest` shape.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct MixWeightsArg {
    #[serde(default)]
    pub local: Option<f32>,
    #[serde(default)]
    pub global: Option<f32>,
    #[serde(default)]
    pub naive: Option<f32>,
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
    /// Retrieval-only: return context/sources without generating an answer.
    /// `Query` operation only. Forwarded as Edgequake's `context_only` body
    /// field.
    #[serde(default)]
    pub context_only: Option<bool>,
    /// Pre-supplied high-level keywords (LightRAG-shaped). `Query` operation
    /// only, Mix mode. Skips Edgequake's keyword-extraction LLM call when set.
    #[serde(default)]
    pub hl_keywords: Option<Vec<String>>,
    /// Pre-supplied low-level keywords. See `hl_keywords`.
    #[serde(default)]
    pub ll_keywords: Option<Vec<String>>,
    /// Per-request Mix mode arm weight override. `Query` operation only,
    /// Mix mode.
    #[serde(default)]
    pub mix_weights: Option<MixWeightsArg>,
    /// Per-request RRF `k` override (SPEC-084 FR-001). `Query` operation
    /// only, Mix mode. No-op outside Mix mode.
    #[serde(default)]
    pub rrf_k: Option<f32>,
    /// Per-request Mix fusion strategy override: `rrf` (default), `fringe`,
    /// `round_robin`, `max_after_minmax` (SPEC-084 FR-002). `Query`
    /// operation only, Mix mode. An unrecognized or reserved (`bandpass`)
    /// value is rejected by Edgequake with an HTTP 400 — surfaces here as
    /// `ToolError::ExecutionFailed`.
    #[serde(default)]
    pub fusion: Option<String>,

    // Graph entities/relationships
    #[serde(default)]
    pub entity_name: Option<String>,
    #[serde(default)]
    pub search: Option<String>,
    /// Filters `GraphSearchEntities` by entity type (Edgequake wire param
    /// `entity_type`) and `GraphSearchRelationships` by relationship type
    /// (wire param `relationship_type`) — different underlying query keys
    /// per operation, same "category label" concept, so one arg name here.
    #[serde(default)]
    pub label: Option<String>,
    /// `GraphSearchRelationships` only. **Not currently enforced** —
    /// confirmed live 2026-09-11 that Edgequake's `GET /graph/relationships`
    /// list endpoint (`ListRelationshipsQuery`) has no source/target filter,
    /// only pagination + `relationship_type`. Accepted here (not removed,
    /// to avoid an API break) but silently has no effect server-side until
    /// Edgequake grows that capability. Prefer `GraphEntityNeighborhood` to
    /// find relationships touching a specific entity in the meantime.
    #[serde(default)]
    pub source: Option<String>,
    /// See `source`.
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub limit: Option<i32>,
    /// `GraphEntityNeighborhood` only. Traversal depth, clamped server-side
    /// to `[1, 3]`. Defaults to Edgequake's own default (1) when unset.
    #[serde(default)]
    pub depth: Option<i32>,
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
            // Explicit generous timeout — confirmed live (2026-08-20) that
            // reqwest's default is too short for a hybrid-mode query
            // against a large/growing workspace (e.g. goat/app/assistant/'s
            // ever-growing cumulative workspace): TCP connectivity to
            // Edgequake was fine, but the query itself took long enough to
            // trip the default and return "operation timed out" on every
            // single call, not intermittently. 90s leaves headroom under
            // the two other LLM legs in answer_step/9's own ~180s budget
            // (lildaemon/goat/app/assistant/rulesets/general_assistant.pl).
            http: Client::builder()
                .timeout(Duration::from_secs(90))
                .build()
                .unwrap_or_else(|_| Client::new()),
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

    #[allow(clippy::too_many_arguments)]
    pub fn query(
        &self,
        query: &str,
        mode: &str,
        max_results: Option<i32>,
        llm_provider: Option<&str>,
        llm_model: Option<&str>,
        context_only: Option<bool>,
        hl_keywords: Option<&[String]>,
        ll_keywords: Option<&[String]>,
        mix_weights: Option<&MixWeightsArg>,
        rrf_k: Option<f32>,
        fusion: Option<&str>,
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
        if let Some(co) = context_only {
            body["context_only"] = serde_json::json!(co);
        }
        if let Some(hl) = hl_keywords {
            body["hl_keywords"] = serde_json::json!(hl);
        }
        if let Some(ll) = ll_keywords {
            body["ll_keywords"] = serde_json::json!(ll);
        }
        if let Some(mw) = mix_weights {
            let mut obj = serde_json::Map::new();
            if let Some(l) = mw.local {
                obj.insert("local".to_string(), serde_json::json!(l));
            }
            if let Some(g) = mw.global {
                obj.insert("global".to_string(), serde_json::json!(g));
            }
            if let Some(n) = mw.naive {
                obj.insert("naive".to_string(), serde_json::json!(n));
            }
            body["mix_weights"] = Value::Object(obj);
        }
        if let Some(k) = rrf_k {
            body["rrf_k"] = serde_json::json!(k);
        }
        if let Some(f) = fusion {
            body["fusion"] = serde_json::json!(f);
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
        // Confirmed live 2026-09-11: `ListEntitiesQuery`'s real filter field
        // is `entity_type`, not `label`, and its page-size field is
        // `page_size`, not `limit` — either wrong key was previously
        // silently ignored server-side (no `deny_unknown_fields`).
        if let Some(l) = label {
            q.push(("entity_type", l.to_string()));
        }
        if let Some(lim) = limit {
            q.push(("page_size", lim.to_string()));
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
        depth: Option<i32>,
        tenant: Option<&str>,
        workspace: Option<&str>,
    ) -> Result<Value, String> {
        let mut q = Vec::new();
        if let Some(d) = depth {
            q.push(("depth", d.to_string()));
        }
        self.request(
            Method::GET,
            &format!("/api/v1/graph/entities/{name}/neighborhood"),
            &q,
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
        // `source`/`target` are accepted but not sent — see `EdgequakeArgs::source`
        // doc comment: Edgequake's `/graph/relationships` list endpoint has no
        // such filter (confirmed live 2026-09-11), so sending them would just be
        // dead query params. Kept as no-op params here (not removed) to avoid an
        // API break; `_ = source; _ = target;` documents the intentional drop.
        let _ = source;
        let _ = target;
        // Real filter field is `relationship_type`, not `label`; real
        // page-size field is `page_size`, not `limit` (same class of
        // mismatch as `graph_search_entities`'s fix above).
        if let Some(l) = label {
            q.push(("relationship_type", l.to_string()));
        }
        if let Some(lim) = limit {
            q.push(("page_size", lim.to_string()));
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
                        args.context_only,
                        args.hl_keywords.as_deref(),
                        args.ll_keywords.as_deref(),
                        args.mix_weights.as_ref(),
                        args.rrf_k,
                        args.fusion.as_deref(),
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
                    .graph_entity_neighborhood(&entity_name, args.depth, t, w)
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
    fn test_query_args_with_divergent_fusion_fields() {
        // SPEC-084 / leannan_sidhe Tier 2: the fields the_leannan.pl's
        // leannan_spark/4 needs on a Mix-mode context_only query.
        let json = r#"{
            "operation": "query",
            "query": "what is clara?",
            "mode": "mix",
            "context_only": true,
            "ll_keywords": ["Clara", "Cerebellum"],
            "hl_keywords": [],
            "mix_weights": {"local": 3.0, "global": 1.0, "naive": 0.2},
            "rrf_k": 500,
            "fusion": "fringe"
        }"#;
        let args: EdgequakeArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.context_only, Some(true));
        assert_eq!(
            args.ll_keywords,
            Some(vec!["Clara".to_string(), "Cerebellum".to_string()])
        );
        assert_eq!(args.hl_keywords, Some(vec![]));
        assert_eq!(args.rrf_k, Some(500.0));
        assert_eq!(args.fusion, Some("fringe".to_string()));
        let mw = args.mix_weights.expect("mix_weights must parse");
        assert_eq!(mw.local, Some(3.0));
        assert_eq!(mw.global, Some(1.0));
        assert_eq!(mw.naive, Some(0.2));
    }

    #[test]
    fn test_query_args_divergent_fusion_fields_default_none() {
        let json = r#"{"operation": "query", "query": "hi"}"#;
        let args: EdgequakeArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.context_only, None);
        assert_eq!(args.hl_keywords, None);
        assert_eq!(args.ll_keywords, None);
        assert_eq!(args.mix_weights, None);
        assert_eq!(args.rrf_k, None);
        assert_eq!(args.fusion, None);
    }

    #[test]
    fn test_graph_search_entities_args_label_and_depth() {
        // `label` on GraphSearchEntities and `depth` on
        // GraphEntityNeighborhood share the same underlying arg struct.
        let json = r#"{"operation": "graph_search_entities", "label": "PERSON", "limit": 10}"#;
        let args: EdgequakeArgs = serde_json::from_str(json).unwrap();
        assert!(matches!(args.operation, Operation::GraphSearchEntities));
        assert_eq!(args.label, Some("PERSON".to_string()));
        assert_eq!(args.limit, Some(10));

        let json = r#"{"operation": "graph_entity_neighborhood", "entity_name": "Clara", "depth": 2}"#;
        let args: EdgequakeArgs = serde_json::from_str(json).unwrap();
        assert!(matches!(args.operation, Operation::GraphEntityNeighborhood));
        assert_eq!(args.depth, Some(2));
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
