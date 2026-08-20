# Edgequake workspace capabilities: what exists today (2026-08-20)

Research note, not a design doc. Written while planning
`lildaemon/goat/app/assistant/` (see `lildaemon/docs/assistant_demo.md`),
which needs workspaces to evolve over time across many research sessions.
Two questions came up that turned out to have definitive, easily-lost
answers — recorded here so nobody re-researches them.

Confirmed against the live OpenAPI spec (`GET /api-docs/openapi.json` on a
running Edgequake instance — not `/openapi.json` or `/docs`, both 404;
Swagger UI is at `/swagger-ui`) and Edgequake's own Rust source, found at
`/mnt/moonpool/tools/edgequake` (nested crates under `edgequake/crates/`,
e.g. `edgequake-api/src/routes.rs`).

## Question 1: can you union/diff/intersect workspaces or their documents?

**No — not at any level.** The only two things that could be mistaken for
"set operations" in the API:

- **`DocumentFilter`** (used by `/api/v1/query`, `/api/v1/query/context`)
  has `document_ids` and `document_pattern`, explicitly documented as
  OR-unioned with each other — but this is a **query-time filter over one
  workspace's own documents**, not an operation that combines two
  workspaces.
- **`POST /api/v1/graph/entities/merge`** — two-named-entity graph
  deduplication (`{source_entity, target_entity, merge_strategy:
  "prefer_source"|"prefer_target"|"merge"}`), scoped to one workspace's
  knowledge graph. Not a document-set or workspace-level operation.

Grepped Edgequake's own source for `union|intersect|merge.*workspace`: every
hit is either the entity-merge/dedup logic above (`edgequake-pipeline/src/
merger/*`, `entity_reconcile.rs`) or low-level Postgres `UNION` SQL used
internally for graph node scans — nothing exposes workspace-to-workspace
set algebra, cross-workspace document copy, or a derived-workspace concept.

**Practical implication**: if you want a shared, ever-growing pool of
knowledge across many separate topic-scoped research runs, the only
available mechanism is **ingesting the same content into more than one
workspace at ingest time** (each `submit_document` call is independent, so
nothing stops submitting one document into both a per-topic workspace and a
second, shared one). That's a real ingest-time duplication, not a real
union — there's no way to later compute "the union of workspace A and B" if
you didn't ingest into both up front.

## Question 2: is there any TTL/expiry/retention on workspaces or documents?

**No — confirmed absent from the API schema and the source, not just
undocumented.** Grepped the full OpenAPI spec and all of Edgequake's Rust
source for `ttl|expir|retention|stale|archiv`. The only hits:

- API-key `expires_at`/`expires_in_days` — auth, unrelated to content.
- JWT/OIDC session expiry — auth, unrelated.
- A cached-retrieval-context 410 expiry — an internal query cache, not
  document/workspace lifecycle.
- `stale: bool` on `WorkspaceStatsResponse` — means "these stats were
  served from cache under load," not "this workspace is stale."
- `WorkspaceResponse`/`CreateWorkspaceApiRequest`/
  `UpdateWorkspaceApiRequest` schemas (read in full) — no expiry, retention,
  or archive field at all.
- Conversations (chat history — a separate concept from documents/
  workspaces) do have `is_archived` + bulk-archive, unrelated to RAG
  content.
- One internal source comment (`edgequake-pipeline/src/prompts/parser/*`,
  "BR0831: Cache entries must have TTL for cleanup") — a query-cache
  implementation detail, not exposed and not about documents/workspaces.

This isn't an oversight nobody noticed either — it's already a documented,
deliberately deferred gap on the ingestion side:
`clara-cerebellum/docs/chewing_the_cud.md:293` explicitly says *"reap/
consolidate stale query-workspaces later — deferred, not needed for a first
version."* Nobody has built the "reap" half yet.

**Practical implication**: any workspace lifecycle management (deleting
old, unused per-topic workspaces; enforcing a retention policy) has to be
built entirely client-side, tracking `created_at`/`last_queried_at`
yourself and calling the one lifecycle primitive Edgequake *does* provide —
`DELETE /api/v1/workspaces/{workspace_id}` (confirmed to exist and work).
There's nothing server-side to lean on.

## Other endpoints worth knowing about, for context

- **Workspaces**: `GET/POST /api/v1/tenants/{tenant_id}/workspaces`,
  `GET /api/v1/tenants/{tenant_id}/workspaces/by-slug/{slug}` (the
  idempotent lookup `EdgequakeClient.get_or_create_workspace` already
  uses), `GET/PUT/DELETE /api/v1/workspaces/{workspace_id}`,
  `.../stats`, `.../metrics-history`, `.../metrics-snapshot`,
  `.../rebuild-embeddings`, `.../rebuild-knowledge-graph`,
  `.../reprocess-documents`, `.../injection(s)`.
- **Documents**: `GET/POST/DELETE /api/v1/documents` — note the top-level
  `DELETE` is a **bulk wipe of every document in the workspace** (202 +
  `wipe_track_id`, tracked async), not selective; selective deletion is
  `DELETE /api/v1/documents/{document_id}`.
- **Tenant quota**: `Default` tenant currently allows 100 workspaces
  (`max_workspaces`), raisable via `PATCH /api/v1/admin/tenants/
  {tenant_id}/quota` — worth knowing if a demo runs many research sessions
  and starts hitting the quota, since there's no auto-reap to keep the
  count down yet.

## Bottom line for anything building on Edgequake workspaces

- Treat "one shared workspace everything gets also ingested into" as the
  honest, deliberate stand-in for real set operations — not a workaround
  pretending to be one. Document it as such wherever it's used.
- Treat workspace lifecycle (TTL, reaping) as entirely your own
  responsibility to build — there is no Edgequake-side mechanism to defer
  to, now or apparently on any near-term roadmap visible in the source.
