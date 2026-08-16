# Chewing the Cud: SnekEvaluator results → Edgequake knowledge base

**Status: planning draft, feedback incorporated (2026-08-16), not yet
implemented.** All four open questions below are now resolved — see
"Resolutions" under each. Scope deliberately narrowed
per direction: **document submission only**. Asserting graph entities/
relationships directly is a separate, later step — Edgequake's own
ingestion pipeline builds the graph from submitted documents automatically,
so getting documents in is a complete, useful step on its own, not a
half-measure blocked on graph design.

(Name: a cow chews its cud — swallows food, brings it back up, digests it
properly. Matches `the_cow.pl`'s own naming for Edgequake integration, and
what this pipeline literally does to Snek's crawled pages.)

## Goal

`SnekEvaluator` already identifies which crawled pages are relevant to a
research query (LLM relevance judgment, `_judge_relevance`) and persists
each one, as found, to a durable ad hoc Coire topic
(`snek.{query_slug}` — see `coire_ad_hoc_topics.md` and
`lildaemon/docs/ritual_snek_splinter_example.md`). Right now that's where
the trail ends: the data sits in Kafka, readable only by something that
knows to poll that specific topic. The goal is to turn "Snek found this
page relevant" into "this page is now in Edgequake's knowledge base and
queryable via `ruminate`/`ruminate_opts`/the graph read API" —
automatically, without a human copying URLs around.

## What Edgequake's ingestion API actually looks like

Confirmed against the real upstream source
(`~/moonpool/tools/edgequake/edgequake/crates/edgequake-api/src/routes.rs`
— the top-level `~/moonpool/tools/edgequake/crates/` is a decoy/empty dir,
same trap noted in `edgequake_integration_status.md`) and
`~/moonpool/tools/edgequake/docs/api-reference/document-upload-quick-reference.md`,
not guessed:

**`POST /api/v1/documents`** (`application/json`) is the one that matters
here — programmatic text ingestion, no file involved:

```json
{
  "content": "...",              // required — the document text
  "title": "...",                // optional
  "metadata": { ... },           // optional — arbitrary object
  "async_processing": true,      // recommended true for anything non-trivial
  "enable_gleaning": true,       // optional, default true — multi-pass extraction
  "max_gleaning": 1,             // optional
  "use_llm_summarization": true  // optional, default true
}
```

`async_processing: true` (the recommended, non-default setting) returns a
`task_id` immediately; progress via `GET /api/v1/ingestion/{task_id}/progress`
or `ws://.../ws/progress/{task_id}`. `async_processing: false` (the
document's OpenAPI default, kept only for backward compat) processes
inline — fine for a quick smoke test, not for production volume.

Auth/scoping matches what `clara-toolbox/src/tools/edgequake.rs`'s
`EdgequakeClient` already does for reads: `X-API-Key`, `X-Tenant-ID`,
`X-Workspace-ID` headers. `EDGEQUAKE_DEFAULT_TENANT`/
`EDGEQUAKE_DEFAULT_WORKSPACE` are already set in `docker/.env` (real UUIDs,
per the workspace-scoping bug fixed in `edgequake_integration_status.md`)
and already forwarded to both the `clara-api` and `lildaemon` containers in
`docker-compose.yml` — no new config needed on that front.

**Explicitly not this step**: `/api/v1/documents/pdf`, `/upload`, `/scan`
(file-based paths — irrelevant, Snek's output is already plain text) and
anything under `/api/v1/graph/*` writes (deferred, per direction).

## What exists today, and what doesn't

- **Reads only, Rust/Prolog/CLIPS side**: `clara-toolbox/src/tools/
  edgequake.rs`'s `EdgequakeClient`/`ClaraEdgequakeTool` supports `Query`,
  four `Graph*` read ops, and three list ops. **No document-ingestion
  operation exists there at all.**
- **Zero Edgequake references anywhere in lildaemon** (`grep -rl
  edgequake` across `goat/`, case-insensitive, empty) — confirmed, not
  assumed. `the_cow_planning.md`'s original plan named "extending support
  to the lildaemon side... so Evaluators can create and modify knowledge
  in Edgequake workspaces" as a later goal; this is that goal's first
  concrete slice.
- **`SnekEvaluator`** has the page data (`url, title, text_sample,
  confidence, reason`) and the ad hoc topic (`snek.{slug}`) already
  building, from this session's earlier work.

## Design

### Where the new code lives: decoupled consumer, not baked into Snek

Recommended: a **new, separate Python consumer** that polls a `snek.*` ad
hoc topic (reusing `CoireTopicClient.poll`, same as `ritual_snek_splinter_example.md`'s
`snek_pull_and_assert` does on the Prolog side) and submits each envelope's
payload to Edgequake — **not** a change to `SnekEvaluator._save_page_to_topic`
itself.

Why decoupled rather than "Snek also pushes to Edgequake inline, next to
its ad hoc topic save": ad hoc topics exist specifically so a producer
doesn't need to know who (if anyone) is consuming its output — Snek's job
stays "crawl, judge, persist"; "persisted pages become Edgequake
documents" is a second, independent concern with its own failure modes
(Edgequake being slow/down shouldn't slow down or fail a crawl) and its own
consumer identity (its own `coire_topic_poll`-style cursor, independent of
any Ritual consumer that might also be reading the same topic — recall the
cursor is keyed per `(consumer_id, topic)`, exactly built for this). A
decoupled consumer can also, later, ingest ad hoc topics from evaluators
other than Snek without touching Snek's code again.

**Alternative considered**: embed the Edgequake push directly in
`SnekEvaluator._save_page_to_topic`, parallel to the existing
`CoireTopicClient.publish` call. Simpler (one less moving part, no new
consumer process/loop to run), but couples Snek's crawl loop to
Edgequake's availability/latency and only ever covers Snek's own output.
Worth reconsidering if the decoupled consumer turns out to be more
plumbing than value in practice — flagging here rather than silently
picking one.

[STAN] I agree with the new separate python consumer approach.

### Sketch: `goat/models/EdgequakeClient.py` (new, lildaemon)

Mirrors `CoireTopicClient`'s shape — a thin, purpose-built async HTTP
client, not a general Edgequake SDK:

```python
class EdgequakeClient:
    def __init__(self, base_url: str, api_key: str | None = None,
                 tenant: str | None = None): ...
    # No longer a fixed `workspace` at construction — see Q4: workspace is
    # resolved per query slug, not fixed for the client's lifetime.

    async def get_or_create_workspace(self, slug: str, name: str | None = None) -> str:
        """GET /api/v1/tenants/{tenant}/workspaces/slug/{slug}; on 404,
        POST /api/v1/tenants/{tenant}/workspaces. Returns workspace_id.
        Idempotent — safe to call every time, no local creation-state
        tracking needed."""

    async def submit_document(
        self, workspace_id: str, content: str, title: str | None = None,
        metadata: dict | None = None,
    ) -> str:
        """POST /api/v1/documents (scoped via X-Workspace-ID), async_processing=true.
        Returns task_id. Dedup (Q2) is automatic on Edgequake's side, keyed
        by content hash within workspace_id — nothing extra to do here."""

    async def ingestion_progress(self, task_id: str) -> dict:
        """GET /api/v1/ingestion/{task_id}/progress. Unused in the
        fire-and-forget first version (Q3) — kept for later if task
        tracking is ever added."""
```

Config: `EDGEQUAKE_BASE_URL`, `EDGEQUAKE_API_KEY`,
`EDGEQUAKE_DEFAULT_TENANT`, `EDGEQUAKE_DEFAULT_WORKSPACE` — all already
present as env vars in `docker-compose.yml`'s `lildaemon` service, just
unread by any Python code today.

### Sketch: the consumer

A small script/module in the same spirit as `examples_ritual_snek_splinter.py`
— polls `snek.{slug}` via `CoireTopicClient.poll(subject, consumer_id="edgequake-ingest")`.
Per envelope: derive the workspace slug (`{ritual_id-or-"adhoc"}.{query_slug}`
— the envelope's `query` field, slugified the same way `topic_subject_for`
already does, plus whatever ritual/adhoc qualifier the envelope's
provenance carries), resolve it via `EdgequakeClient.get_or_create_workspace(slug)`
(idempotent — cheap to call on every envelope, no cache needed for a first
version), then call `EdgequakeClient.submit_document(workspace_id, ...)`
with:

| Edgequake field | From Snek's payload |
|---|---|
| `content` | full page text (**resolved — was `text_sample`, see Q1 below**) |
| `title` | `title` |
| `metadata.source_url` | `url` |
| `metadata.query` | `query` |
| `metadata.confidence` | `confidence` |
| `metadata.reason` | `reason` |
| `metadata.fierypit_id` | `fierypit_id` (provenance, same field already added this session) |

Since `content` now carries the full page (Q1), submission also needs a
target workspace resolved *before* the `submit_document` call — see Q4
below for the workspace-per-query naming scheme and how the consumer
resolves/creates it idempotently via `EdgequakeClient`.

## Open questions — resolved 2026-08-16

All four resolved, per your review; each inline note below is followed by a
**Resolution** grounded in the real Edgequake source. Kept in place
(rather than deleted) so the reasoning stays visible for whoever
implements this.

1. **`text_sample` is 500 characters** (`SnekEvaluator._crawl_and_judge`
   truncates `text[:500]` before building `PageResult`). Is a 500-char
   snippet enough for Edgequake's entity extraction/gleaning to produce
   anything meaningful, or does this need the **full page text** captured
   separately (Snek's crawler already has the full `html_to_text()` output
   at judgment time — currently discarded after truncation)? If full text
   is needed, that's a small `SnekEvaluator` change (keep/pass full text
   alongside the sample) — decide this before building the consumer, not
   after.

[STAN] That's a good point.  The snippet approach was left over from the initial implementation and isn't really appropriate when we're using an LLM to judge the document.  Let's submit the whole thing.  It should probably fit in context.

**Resolution:** submit the full page text. This requires a small
`SnekEvaluator` change: `_crawl_and_judge`'s per-page handler already has
the full `html_to_text()` output in scope at judgment time, before it gets
truncated to build `PageResult.text_sample`. `_save_page_to_topic`'s
payload needs a `text` field (full text) alongside (or instead of)
`text_sample` — `PageResult` itself and the truncated `text_sample` field
can stay as-is for Snek's own Hohi response (that's a separate concern,
UI/API-facing, not Edgequake's input), only the ad hoc topic payload needs
the extra field. This is now in scope for the implementation plan, not a
follow-up.

2. **Dedup / idempotency**: does resubmitting the same URL across repeated
   crawls create duplicate Edgequake documents, or does Edgequake dedup by
   content hash the way `POST /source` already does on the Dis side? The
   batch-upload response shape (`"status": "duplicate", "duplicate_of":
   ...`) suggests file uploads dedup; unconfirmed whether `POST /documents`
   (JSON path) does the same. Check before assuming either way.

[STAN] I'm not sure about this, but I believe the pipeline can handle deduplication.  Probably worth a quick web/documentation search (we have the source locally) to verify.

**Resolution — confirmed against the real source, not guessed.**
Edgequake does content-hash dedup on the JSON ingestion path, and it's
**workspace-scoped**: `edgequake-api/src/services/workspace_content_hash_dedup.rs`
and migration `023_workspace_scoped_content_hash.sql` key dedup records as
`doc:hash:{workspace_id}:{sha256_of_content}` — a duplicate submission
(same content hash) *within the same workspace* is recognized and
resolved via that KV key, not re-ingested as a second document. It is
**not** cross-workspace: two different workspaces holding the same page
content each get their own copy, no dedup between them. This is a
non-issue for the workspace-per-query design below (Q4) — the case that
actually happens (re-running the same query, Snek re-crawling and
re-finding the same page) lands in the *same* workspace both times, so
dedup applies exactly where it's needed. No extra code needed on our side
to get this — it's automatic on Edgequake's ingestion path.

3. **Task tracking**: does the consumer need to *wait* for ingestion to
   complete (poll `task_id` to done) before considering a page "handled",
   or is fire-and-forget (submit, move on, don't track completion)
   acceptable for a first version? Fire-and-forget is simpler and matches
   the ad hoc topic's own "best-effort, don't block on this" philosophy
   (`_save_page_to_topic`'s own resilience pattern) — leaning toward that
   unless there's a concrete reason to track completion.

[STAN] Fire and forget is the intended approach.

4. **Tenant/workspace**: submit into the same default tenant/workspace
   everything else uses, or should crawl-sourced knowledge land in its own
   workspace (keeping "things a crawler found" queryable/auditable
   separately from hand-curated knowledge)? No opinion yet — needs a
   decision, not a default assumption.

[STAN] Our topics should be qualified by a "query slug", so let's use that as part of a new workspace per query.  We may need to examine how heavy an Edgequake workspace's footprint is.  If they don't come with much overhead, then using a separate workspace per query makes sense.  We could qualify the workspace by combining any ritual id (or ad hoc) and the query slug to provide a predicatable topic in kafka and workspace in edgequake.

**Resolution — checked against the real source, footprint and quota both
confirmed:**

- **Per-workspace footprint is light.** `POST /api/v1/tenants/{tenant_id}/workspaces`
  (`edgequake-api/src/handlers/workspaces/workspace_crud.rs::create_workspace`)
  just writes a metadata row, inheriting the parent tenant's LLM/embedding
  config unless overridden — no heavy provisioning step, no separate
  infra spun up per workspace. Workspace-per-query is cheap on this axis.
- **But there's a hard tenant-level quota that a workspace-per-query
  strategy will hit.** `TenantPlan::default_max_workspaces()` caps
  workspaces per tenant: Free 10, Basic 100, Pro 500, Enterprise 500
  (`workspaces_types/mod.rs`, SPEC-028). A research session that runs even
  a modest number of distinct queries against a Free/Basic-tier tenant
  will exhaust that quota fast. Two ways to handle it, not mutually
  exclusive: (a) raise the quota via `PATCH /api/v1/admin/tenants/{tenant_id}/quota`
  — this is a **real, already-implemented** admin endpoint
  (`edgequake-api/src/handlers/admin.rs`), not a draft spec; confirm which
  tenant our `EDGEQUAKE_DEFAULT_TENANT` is provisioned under and what tier
  it's on before implementing, since this is the actual blocker, not
  workspace cost. (b) reap/consolidate stale query-workspaces later —
  deferred, not needed for a first version.
- **Naming scheme, per your direction**: workspace slug =
  `{ritual_id-or-"adhoc"}.{query_slug}` — mirrors the Kafka ad hoc topic's
  own `snek.{query_slug}` naming exactly, just with the ritual/adhoc
  qualifier prepended so a workspace slug is predictable from the same
  inputs that produce the topic name. The consumer resolves this
  idempotently rather than tracking creation state itself: try
  `GET /api/v1/tenants/{tenant}/workspaces/slug/{slug}`
  (`get_workspace_by_slug`, already exists in `workspace_crud.rs`) first,
  `POST .../workspaces` on 404. `EdgequakeClient`'s sketch below needs a
  `get_or_create_workspace(slug)` method to cover this, not just
  `submit_document`.


## Explicitly out of scope (for now)

- Any `/api/v1/graph/*` write operations (entities, relationships) —
  deferred per direction; Edgequake's own ingestion pipeline builds the
  graph from submitted documents without clara-cerebellum needing to
  assert anything directly.
- Extending `clara-toolbox/src/tools/edgequake.rs`/`the_cow.pl`/
  `the_cow.clp` with an ingestion operation — the design above is
  Python/lildaemon-only. Worth revisiting for symmetry once the Python
  path is proven (so `assert_page_result/1` in
  `ritual_snek_splinter_example.md`'s Prolog pipeline could ingest
  directly too), but not needed for this slice.
- PDF/file-based ingestion paths — irrelevant to Snek's plain-text output.

## Next step

Open questions are resolved; this is ready to move from planning draft to
an implementation plan (Plan-mode pass covering `SnekEvaluator`'s full-text
change, `EdgequakeClient`, and the consumer) whenever you're ready — not
started yet, since the doc review was the checkpoint.

## Related reading

- `~/moonpool/tools/edgequake/docs/api-reference/document-upload-quick-reference.md`
  — the source of truth this plan is grounded in.
- `~/moonpool/tools/edgequake/edgequake/crates/edgequake-api/src/services/workspace_content_hash_dedup.rs`,
  migration `023_workspace_scoped_content_hash.sql` — source for the Q2
  dedup resolution (workspace-scoped content-hash dedup).
- `~/moonpool/tools/edgequake/edgequake/crates/edgequake-api/src/handlers/workspaces/workspace_crud.rs`,
  `~/moonpool/tools/edgequake/edgequake/crates/edgequake-api/src/handlers/admin.rs`,
  `~/moonpool/tools/edgequake/edgequake/crates/edgequake-api/src/handlers/workspaces_types/mod.rs`
  — source for the Q4 resolution (workspace creation cost, `get_workspace_by_slug`,
  `max_workspaces` tenant quota tiers, `PATCH .../quota` admin endpoint).
- `docs/edgequake_integration_status.md`, `docs/the_cow_planning.md`,
  `docs/edgequake_tenant_workspace_plan.md` — existing Edgequake
  integration history and the tenant/workspace scoping bug/fix this
  design relies on already being resolved.
- `docs/coire_ad_hoc_topics.md` — ad hoc Coire topics, including
  SnekEvaluator's save-as-you-go adoption this plan builds on.
- `lildaemon/docs/ritual_snek_splinter_example.md` — the existing
  Prolog-side ad hoc-topic consumer pattern (`coire_topic_poll` +
  per-consumer cursor) this plan's Python consumer mirrors.
- `clara-prolog/docs/examples/ruminate/ruminate.pl` — related experimental
  work (grounding/verifying an LLM answer via `ruminate_opts`/`ponder_text`
  + `clara_fy`), worth a look before designing anything Prolog-side here.
