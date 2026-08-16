# Chewing the Cud: SnekEvaluator results → Edgequake knowledge base

**Status: planning draft, not yet implemented.** Scope deliberately narrowed
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

### Sketch: `goat/models/EdgequakeClient.py` (new, lildaemon)

Mirrors `CoireTopicClient`'s shape — a thin, purpose-built async HTTP
client, not a general Edgequake SDK:

```python
class EdgequakeClient:
    def __init__(self, base_url: str, api_key: str | None = None,
                 tenant: str | None = None, workspace: str | None = None): ...

    async def submit_document(
        self, content: str, title: str | None = None,
        metadata: dict | None = None,
    ) -> str:
        """POST /api/v1/documents, async_processing=true. Returns task_id."""

    async def ingestion_progress(self, task_id: str) -> dict:
        """GET /api/v1/ingestion/{task_id}/progress."""
```

Config: `EDGEQUAKE_BASE_URL`, `EDGEQUAKE_API_KEY`,
`EDGEQUAKE_DEFAULT_TENANT`, `EDGEQUAKE_DEFAULT_WORKSPACE` — all already
present as env vars in `docker-compose.yml`'s `lildaemon` service, just
unread by any Python code today.

### Sketch: the consumer

A small script/module in the same spirit as `examples_ritual_snek_splinter.py`
— polls `snek.{slug}` via `CoireTopicClient.poll(subject, consumer_id="edgequake-ingest")`,
and for each envelope calls `EdgequakeClient.submit_document(...)` with:

| Edgequake field | From Snek's payload |
|---|---|
| `content` | `text_sample` (**see open question below**) |
| `title` | `title` |
| `metadata.source_url` | `url` |
| `metadata.query` | `query` |
| `metadata.confidence` | `confidence` |
| `metadata.reason` | `reason` |
| `metadata.fierypit_id` | `fierypit_id` (provenance, same field already added this session) |

## Open questions to resolve before implementing

1. **`text_sample` is 500 characters** (`SnekEvaluator._crawl_and_judge`
   truncates `text[:500]` before building `PageResult`). Is a 500-char
   snippet enough for Edgequake's entity extraction/gleaning to produce
   anything meaningful, or does this need the **full page text** captured
   separately (Snek's crawler already has the full `html_to_text()` output
   at judgment time — currently discarded after truncation)? If full text
   is needed, that's a small `SnekEvaluator` change (keep/pass full text
   alongside the sample) — decide this before building the consumer, not
   after.
2. **Dedup / idempotency**: does resubmitting the same URL across repeated
   crawls create duplicate Edgequake documents, or does Edgequake dedup by
   content hash the way `POST /source` already does on the Dis side? The
   batch-upload response shape (`"status": "duplicate", "duplicate_of":
   ...`) suggests file uploads dedup; unconfirmed whether `POST /documents`
   (JSON path) does the same. Check before assuming either way.
3. **Task tracking**: does the consumer need to *wait* for ingestion to
   complete (poll `task_id` to done) before considering a page "handled",
   or is fire-and-forget (submit, move on, don't track completion)
   acceptable for a first version? Fire-and-forget is simpler and matches
   the ad hoc topic's own "best-effort, don't block on this" philosophy
   (`_save_page_to_topic`'s own resilience pattern) — leaning toward that
   unless there's a concrete reason to track completion.
4. **Tenant/workspace**: submit into the same default tenant/workspace
   everything else uses, or should crawl-sourced knowledge land in its own
   workspace (keeping "things a crawler found" queryable/auditable
   separately from hand-curated knowledge)? No opinion yet — needs a
   decision, not a default assumption.

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

## Related reading

- `~/moonpool/tools/edgequake/docs/api-reference/document-upload-quick-reference.md`
  — the source of truth this plan is grounded in.
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
