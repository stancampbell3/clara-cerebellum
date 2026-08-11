To support **tenant** and **workspace** context in your Edgequake integration, you need to propagate them at two levels:

1. **Client-level configuration** (stored on `EdgequakeClient` / `ClaraEdgequakeTool` for defaults or global settings).
2. **Request-level overrides / arguments** (passed dynamically via `EdgequakeArgs` if rules or predicates need to switch tenants/workspaces on the fly).

Typically, multi-tenant gateways accept these via **HTTP headers** (e.g., `X-Tenant-ID`, `X-Workspace-ID`) or as **query/body parameters**. Below is how you can update your structs and request builder to support both header-based and parameter-based propagation.

---

### Step 1: Update `EdgequakeClient` and `EdgequakeArgs`

Add `tenant` and `workspace` fields to your argument structs and client configurations so they can be injected into HTTP requests.

```rust
/// Tool request arguments
#[derive(Debug, Deserialize)]
pub struct EdgequakeArgs {
    pub operation: Operation,

    // Multi-tenancy context overrides
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

    // Helper to resolve tenant/workspace falling back to defaults
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

        // Inject tenant and workspace headers (adjust header names as needed for Edgequake)
        let resolved_tenant = tenant.or(self.default_tenant.as_deref());
        if let Some(t) = resolved_tenant {
            req = req.header("X-Tenant-ID", t);
        }

        let resolved_workspace = workspace.or(self.default_workspace.as_deref());
        if let Some(w) = resolved_workspace {
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
    
    // Update individual client methods to accept optional tenant/workspace overrides
    pub fn query(
        &self,
        query: &str,
        mode: &str,
        max_results: Option<i32>,
        tenant: Option<&str>,
        workspace: Option<&str>,
    ) -> Result<Value, String> {
        let mut body = serde_json::json!({ "query": query, "mode": mode });
        if let Some(mr) = max_results {
            body["max_results"] = serde_json::json!(mr);
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
        if let Some(s) = search { q.push(("search", s.to_string())); }
        if let Some(l) = label { q.push(("label", l.to_string())); }
        if let Some(lim) = limit { q.push(("limit", lim.to_string())); }
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
        if let Some(s) = source { q.push(("source", s.to_string())); }
        if let Some(t) = target { q.push(("target", t.to_string())); }
        if let Some(l) = label { q.push(("label", l.to_string())); }
        if let Some(lim) = limit { q.push(("limit", lim.to_string())); }
        self.request(Method::GET, "/api/v1/graph/relationships", &q, None, tenant, workspace)
    }
}

```

---

### Step 2: Pass Context Through `execute_operation`

Update `ClaraEdgequakeTool` so that tenant and workspace parameters extracted from `EdgequakeArgs` flow cleanly into the client calls:

```rust
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
                    .query(&query, &mode, args.max_results, t, w)
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
        }
    }
}

```

---

### Alternative: Body/Query Parameter Convention

If Edgequake expects tenant/workspace inside the JSON body or URL query strings rather than HTTP headers (e.g., `/api/v1/query?tenant=foo`), you can adjust the `request` method to push them into `query` or inject them directly into the `body` `serde_json::Value` object instead of `.header()`.
