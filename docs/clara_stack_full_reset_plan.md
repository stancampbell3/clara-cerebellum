# Clara stack full-RESET tooling + data ERD

**Status:** Proposed — pending team review

## Context

The physical hardware hit a temperature limit a few days ago, and until
there's a more stable physical setup, stale state left over between runs
is a real risk — half-finished Rituals, orphaned research-task rows,
Edgequake documents/embeddings from an interrupted ingest, leftover Kafka
consumer groups, etc. All of that needs a reliable way to go back to a
known-empty baseline. This plan designs (a) an orchestrator script, living
in `clara-cerebellum` since it's the one repo with a system-wide view of
the whole stack, that resets every component's data to empty, and (b) an
ERD documenting how the stack's DuckDB, Postgres, and Kafka resources
relate, since untangling that was necessary groundwork anyway and is
independently useful.

Four research passes plus direct verification against the live host
(`docker inspect`, `docker compose config` run through the actual `clara`/
`clara.sh` wrapper) produced a complete, verified inventory of every
persistent data resource in the stack.

## Data landscape (verified)

**clara-cerebellum (Rust, clara-api)** — no Postgres; all persistence is
DuckDB. `CoireStore` (`clara-coire/src/store.rs`) owns `data/coire.duckdb`
(bind-mounted `../data:/app/data`), 6 tables: `coire_events`,
`deduction_snapshots`, `tableau_changes`, `source_registry`,
`source_artifacts`, and **`rituals`** — `RitualRegistry` write-throughs to
this table and reloads active *and terminated* rituals on every boot (a
past incident let 250 dead rituals accumulate, since `terminate()` never
deletes rows). `FieryPitRegistry` is confirmed pure in-memory. Kafka has
**no volume** — container recreate alone wipes every topic and consumer
offset. Existing partial tooling in `clara-cerebellum/scripts/`:
`flush_coire.sh` + `drop_coire_data.sql` (truncates 5 of the 6 CoireStore
tables — **missing `rituals`**), `reset_kafka_topics.sh` (per-topic
delete, has `--dry-run`), `reset_adhoc_workspaces.sh` (pattern-matched
Edgequake workspace deletion via its REST API).

**lildaemon (Python)** — one shared DuckDB file, `lildaemon.duc`
(`LILDAEMON_DB_PATH`), 8 tables: `users`, `repl_sessions`,
`assistant_sessions`, `assistant_topics`, `assistant_document_tags`,
`assistant_pending_research`, `ritual_configs`, `ritual_participants`. No
SQLite anywhere. lildaemon holds only a *tag* mirror of Edgequake document
ids, not the documents themselves. Each joined Ritual creates a Kafka
consumer group `ritual-{ritual_id}-{node_id}` that is **never explicitly
deleted** on leave — confirmed via `AdminClient` grep (zero hits) — so
recreating the Kafka container (which has no volume) is the clean fix
rather than chasing groups one by one. Two small existing scripts:
`scripts/purge_output.sh` (wipes `output/`), `scripts/purge_pycache.sh`
(dev cache, irrelevant). `users` is safely fully-wipeable: `POST
/auth/register` (`goat/app/users/router.py:64`) requires no prior auth, so
a wiped instance is immediately self-service-recoverable, no admin-reseed
step needed.

**dagda's Cobbler** — owns zero persistent data of its own (confirmed by
its own architecture doc: "never stores or validates JWTs, never stores
session state") — every router is a thin proxy to lildaemon/clara-api.
Its one local write path (session bookmarks) edits lildaemon's own
`output/` tree in place. **No reset step needed for Cobbler** — wiping
lildaemon's `output/` already covers it. The rest of the `dagda` repo
(`scripts/`, `test/`, `dataset/`, `db/`, `models/`, `output/`) is
unrelated ingestion/training work per the repo's own `.gitlab-ci.yml` —
explicitly out of scope, except `scripts/cobbler.sh` and
`scripts/codex_cobbler.py`, which are Cobbler's own despite the location.

**Edgequake** — corrects an earlier wrong assumption: it runs **locally
on limbic itself** (`10.0.0.192`, this host), not on pineal, as its own
separate docker-compose project (`~/moonpool/tools/edgequake/docker-
compose.quickstart.yml`, project name `edgequake`, outside
`~/moonpool/Development`). Three services (`postgres`, `api`, `frontend`);
only `postgres` is stateful, via **one named Docker volume**
(`edgequake-pg-data`, not a bind mount like everything else in this
stack). That one Postgres 18 instance bundles **pgvector 0.8.5** (the
"vector db") and **Apache AGE 1.8.0** (graph) alongside `pg_trgm`,
`btree_gin`, `uuid-ossp` — relational + vector + graph in one place, no
separate vector service. It's genuinely multi-tenant: every core table
(`documents`, `chunks`, `entities`, `relationships`, `tasks`, `pdf_
documents`, `document_originals`, `document_mm_assets`) carries
`tenant_id`/`workspace_id`, enforced by Row-Level Security. Confirmed
`assistant.general` (lildaemon's `ASSISTANT_WORKSPACE_SLUG`) and
lildaemon's `EDGEQUAKE_DEFAULT_TENANT` resolve to Edgequake's own built-in
default UUIDs (tenant `...0002`, workspace `...0003`) when using the
`"default"` alias — so Clara's data is cleanly identifiable, but the
instance may serve other tenants too and must not be treated as
exclusively Clara's.

Edgequake ships its own reset tooling, but the blunt ones
(`make db-reset`/`db-clean`/`db-clean-force`) are **whole-instance,
cross-tenant** — wrong for a shared Postgres. The right primitive is the
**scoped** one: `DELETE /api/v1/documents` with `X-Tenant-ID`/
`X-Workspace-ID` headers + `X-EdgeQuake-Confirm: delete-all-documents`,
which drives a durable `WorkspaceWipe` task
(`edgequake/crates/edgequake-api/src/services/workspace_document_wipe.rs`)
clearing that workspace's AGE graph nodes, vector rows/tables (including
the dynamically-named `eq_*_vectors` tables a plain `TRUNCATE` would
miss), KV blobs (PDFs/assets), and relational rows — and nothing outside
that workspace. `clara-cerebellum/scripts/reset_adhoc_workspaces.sh`
already exists to sweep pattern-matched leftover demo workspaces (e.g.
`adhoc.*`) the same way.

**Mount-path verification** — ran `docker compose ... config` through the
exact invocation `clara.sh` uses; every bind mount resolves to the
expected path (coire.duckdb, lildaemon.duc, output/, workspace/, cobbler's
shared output mount). `/mnt/moonpool` and `/home/stanc/moonpool` are
confirmed the same filesystem (symlink, matching device:inode) — no
divergent-copy risk today. The stack was fully down on this host at the
time of this review (no clara containers existed, running or stopped).

## Proposed design

**One orchestrator, `clara-cerebellum/scripts/reset_clara_stack.sh`**,
calling into small per-component scripts (new ones matching the existing
`flush_coire.sh`/`reset_kafka_topics.sh` style) rather than one monolithic
script — keeps each piece independently runnable/testable, matches the
convention already in the repo, and lets the orchestrator be mostly a
sequenced, confirmed, dry-runnable driver:

1. **Safety gate**: print exactly what will be wiped, require `--yes` (or
   an interactive typed confirmation) unless `--dry-run`. `--dry-run`
   threads through to every sub-step, matching the existing scripts'
   convention.
2. **Stop the stack**: shell out to the repo's own `./clara down` (the
   user's existing wrapper) rather than re-deriving a compose invocation
   — keeps this script honoring whatever `clara.sh` does, forever, with
   no drift. DuckDB files can't be safely touched while clara-api/
   lildaemon hold them open.
3. **CoireStore**: fix `drop_coire_data.sql` to add `DELETE FROM rituals;`
   (closing the real gap found above), then call `flush_coire.sh`.
4. **lildaemon DuckDB**: new `lildaemon/scripts/flush_lildaemon_db.sh` +
   `drop_lildaemon_data.sql`, mirroring `flush_coire.sh`'s shape exactly
   (same `duckdb <path> < script.sql` pattern), truncating all 8 tables
   listed above.
5. **lildaemon filesystem state**: reuse `scripts/purge_output.sh`; add
   an equivalent wipe of `workspace/` (per-evaluator scratch, e.g.
   `workspace/instances/*` — exactly the kind of in-flight-consult
   residue this task is about).
6. **Kafka**: recreate the `kafka` service via `./clara up -d
   --force-recreate --no-deps kafka` (or `rm`+`up`) — since it has no
   volume this is a complete, one-shot wipe of topics *and* consumer
   group offsets, cleaner than `reset_kafka_topics.sh`'s per-topic
   deletion (kept as a documented fallback for when the stack must stay
   up, e.g. mid-Ritual on a remote FieryPit).
7. **Edgequake**: resolve lildaemon's actual `EDGEQUAKE_DEFAULT_TENANT`/
   `ASSISTANT_WORKSPACE_SLUG` from its real (not example) env, resolve
   the workspace UUID, then call the scoped `DELETE /api/v1/documents`
   wipe described above — touching only Clara's own workspace. Follow
   with `reset_adhoc_workspaces.sh` to sweep any leftover demo workspaces
   from crashed `examples_ritual_*.py` runs. Explicitly documented as
   **do not use** `make db-reset`/`db-clean`/`db-clean-force` here — they
   would wipe every tenant Edgequake hosts, not just Clara's. No
   container lifecycle needed for Edgequake — it stays running throughout
   since only its HTTP API is called.
8. **Cobbler**: no step — already covered by #5.
9. **Summary**: print what was wiped vs. explicitly left alone (dagda's
   unrelated ingestion/training data, any other Edgequake tenant), so the
   run is auditable. Leave the stack down by default; add a `--start`
   flag to bring it back up via `./clara up` for convenience.

## ERD deliverable

New doc `clara-cerebellum/docs/clara_stack_data_erd.md` with Mermaid
diagrams:
- One `erDiagram` for CoireStore's DuckDB (6 tables above).
- One `erDiagram` for lildaemon's DuckDB (8 tables above).
- One `erDiagram` for Edgequake's Postgres (tenants, users, workspaces,
  memberships, documents, chunks, entities, relationships, tasks,
  folders, conversations, messages, audit_logs, pdf_documents,
  document_originals, document_mm_assets — noting AGE's graph data and
  the dynamically-named `eq_*_vectors` tables as call-outs rather than
  static entities, since they're not fixed schema).
- One connecting `flowchart` (not a true ERD, since these are
  cross-system correlations, not enforced FKs) showing: Kafka topic
  `{domain}.ritual.{ritual_id}` linking CoireStore's `rituals` row and
  lildaemon's `ritual_configs`/`ritual_participants`; Kafka topic
  `{domain}.coire.{path}` linking to `source_registry`/
  `deduction_snapshots`; lildaemon's `assistant_document_tags.document_id`
  correlating (loosely, cross-system, unenforced) to Edgequake's
  `documents.document_id`; Ritual consumer groups `ritual-{ritual_id}-
  {node_id}` as ephemeral Kafka-internal state with no DB row at all.

## Files this touches once implemented

- `clara-cerebellum/scripts/reset_clara_stack.sh` (new, orchestrator)
- `clara-cerebellum/scripts/drop_coire_data.sql` (add `rituals`)
- `lildaemon/scripts/flush_lildaemon_db.sh` + `drop_lildaemon_data.sql`
  (new, mirrors `flush_coire.sh`)
- `lildaemon/scripts/purge_output.sh` or a new sibling script (extend to
  also cover `workspace/`)
- `clara-cerebellum/docs/clara_stack_data_erd.md` (new)
- `clara-cerebellum/docs/reset_clara_stack.md` (new, short usage doc —
  what gets wiped, what doesn't, the Edgequake scoping rationale, the
  "do not use `make db-reset` against this shared instance" warning)

## Verification plan (once implemented)

1. `--dry-run` end-to-end against the live stack: bring it up (`./clara
   up -d`), create a throwaway user/ritual/research task/Edgequake
   document via existing example scripts, run `reset_clara_stack.sh
   --dry-run` and confirm it lists every real row/topic/document it would
   touch without changing anything.
2. Real run: same seeded state, run without `--dry-run`, confirm: `coire.
   duckdb` and `lildaemon.duc` tables all empty (including `rituals`,
   previously missed), Kafka has zero topics/consumer groups after
   restart, Edgequake's `assistant.general` workspace has zero documents
   via `GET /api/v1/documents`, while a manually-created second Edgequake
   tenant/workspace (simulating an unrelated consumer) is confirmed
   untouched.
3. Confirm `/auth/register` still works immediately after reset (no
   lockout).
4. Confirm Cobbler needs no changes: bookmark a session before reset,
   confirm it's gone after (since it lived in the wiped `output/` tree).
5. ERD doc: spot-check each Mermaid block renders and that every table
   name matches the live schema (`duckdb <path> ".tables"` / `psql -c
   "\dt"`).

## Open questions for review

- Should the orchestrator default to leaving the stack down after reset,
  or auto-restart it? Currently proposed: leave down by default, `--start`
  flag for convenience.
- Is Edgequake ever expected to serve tenants other than Clara's own on
  this host? The scoped-wipe design works correctly either way, but it's
  worth confirming the assumption isn't accidentally wrong in the other
  direction (i.e. that Clara's `"default"` tenant/workspace couldn't ever
  collide with a real second tenant someone stands up later).
- Any objection to fully wiping the `users` table given open
  self-registration makes it recoverable? No admin-reseed step is
  otherwise proposed.
