# Ground state: a known baseline for Edgequake and the stack around it

*Status: built, tested and run live 2026-09-24. Baseline v1 (13 design documents, 67 MB) is captured, curated and verified; a full reset-and-restore cycle was run on
`assistant.general` and `verify` passed 14/14. G1 (analysts name their workspace by slug) is deployed and verified live 2026-09-25, strict mode on. `pack`/`unpack`/`mirror` added.*

## Why

Anything that reads Edgequake, whether a test, a demo, or a Clara session, behaves differently depending on what the workspace holds. Until
now nothing said what it *should* hold, so a failure could not be told apart from a stale or half-empty knowledge base. A **ground state** is
a named, versioned baseline: the workspace's exact contents, the bookkeeping that describes them, the database rows tests rely on, and
the Kafka shape, all in one directory that can be captured, restored and verified.

## What a survey found (2026-09-24)

1. **No automated test depends on Edgequake content.** They are mocked, or run on a throwaway `itest.*-<uuid>` workspace and assert shape only.
   So nothing could fail when the workspace lacked our documents. The new `edgequake_baseline` tests can.
2. **Two workspaces were in play.** lildaemon's background paths (research queue, answer leg, ingest consumer, cow evaluator) use
   `assistant.general`. The analysts' *synchronous* tiers went through the Rust `edgequake` tool, which fell back to
   `EDGEQUAKE_DEFAULT_WORKSPACE` in `docker/.env`: `…0003`, the **"Default Workspace"**. Ingesting into `assistant.general` did not ground them.
3. **The existing reset script cleared the Default Workspace**, not `assistant.general`.
4. **State is coupled.** Dropping documents by hand left `assistant_topics` (the "already researched" gate) and `assistant_document_tags` pointing at
   documents that no longer existed, so the assistant could skip research it needed.
5. **Edgequake's delete leaves data behind.** After the manual drop, `assistant.general`'s vector table still held **12,281 rows for 230 deleted
   documents** (and ~495 key/value rows). The graph was clean (0 orphans). Retrieval can therefore cite text from documents that are gone.
   Detect with `ground_state_pg.py orphans`; clear with `clean-orphans`.
6. **Residue is wider than vectors.** Besides orphaned vector and kv rows, Edgequake leaves (a) **stale content-hash dedup entries**
   (`doc:hash:<workspace>:<sha>` pointing at deleted documents, which can make re-ingested identical content look like a duplicate of a
   deleted one) and (b) **entity vectors with no document id and no graph node**. Its bulk delete is also **asynchronous**: it returns while documents
   are still listed, so a cleanup right after it finds nothing. `reset` now waits until the workspace is really empty, then cleans; `orphans` reports all
   four kinds. A baseline captured before these were detected carried 113 stale hash entries, which is why `verify` checks for them.
7. **Postgres gotcha:** the `edgequake` schema (the DB user's own, first in the search path) holds compatibility *views* with fewer columns
   (documents, chunks, entities, relationships, tasks). Unqualified table names read the view. The snapshot tool schema-qualifies everything as `public.`.
8. **DuckDB allows one process per file.** Anything that opens `lildaemon.duc` needs the lildaemon service stopped (the tool does it and restarts it).
   Do not run it while an ingest that goes through lildaemon is in flight.

## What a baseline is

One directory (default `/mnt/moonpool/clara-archives/<stamp>_<slug>/`, a place `reset_clara_stack.sh` never purges, visible to the lildaemon container):

| Path | Layer | Use |
|---|---|---|
| `baseline.json` | summary | when, which workspace, which commits, row counts per layer |
| `manifest.json`, `documents/` | Edgequake content | text + content hash per document; **re-ingestable** (`seed`) |
| `pg/` | Edgequake Postgres | **exact** workspace-scoped snapshot: relational tables, key/value rows, the workspace's vector table, the AGE graph; **instant restore, no LLM** |
| `assistant_*.jsonl` | lildaemon bookkeeping | document tags, topics, research queue with replies and citations |
| `duckdb/` (mode 0700) | lildaemon baseline tables | users, ritual configs and children, assistant tables |
| `kafka.json` | Kafka | expected topics with partitions and configs; a baseline holds **no** ritual/coire topics |
| `canaries.json` (you write it) | acceptance | questions the workspace must answer, with phrases and source files each must produce |

`seed` re-ingests through the LLM, which is slow and **not deterministic** (entity extraction varies), so it can never reproduce a baseline exactly. That is why
the Postgres layer exists: `restore` brings the workspace back byte for byte. Use `seed` to build or extend a baseline from documents, `restore` to return to one.

## Commands

    scripts/ground_state.sh status  [--workspace-id W] [--with-db]
    scripts/ground_state.sh capture [--to DIR] [--allow-orphans] [--allow-busy]
    scripts/ground_state.sh reset   [--to DIR | --no-archive] [--components edgequake,kafka,dis] [--all-kafka] [--dry-run] [--yes]
    scripts/ground_state.sh restore --from DIR [--components pg,bookkeeping,kafka,duckdb] [--dry-run]
    scripts/ground_state.sh seed    --from DIR
    scripts/ground_state.sh verify  --baseline DIR
    scripts/ground_state.sh curate  --baseline DIR --keep-users a,b --empty table1,table2
    scripts/ground_state.sh pack    --baseline DIR [--out DIR]      # <name>.tar.zst + .sha256 + .json
    scripts/ground_state.sh unpack  --from PACK --to DIR             # checksum verified before extracting
    scripts/ground_state.sh mirror  --pack PACK --to host:/path      # rsync, then sha256sum -c on the far host
    scripts/ground_state.sh queue   status | expire-stuck --older-than 6h | archive --to DIR | export --to DIR | reset-coupled --to DIR | promote ...

`curate` makes a *captured* baseline a clean one (a capture records whatever accumulated: 65 users, 43 draft/terminated ritual configs, topics that say
"researched" for documents that are not there): it keeps only the named users and empties the named tables in the baseline's DuckDB layer and bookkeeping files.

The pieces: `ground_state.py` (host orchestrator), `ground_state_pg.py` (Postgres), `ground_state_kafka.py` (Kafka), and, in lildaemon,
`goat/app/assistant/ground_state.py` (Edgequake content, bookkeeping, DuckDB) and `maintenance.py` (research queue). The container-side modules run
in a throwaway `lildaemon` container with the working tree's `goat/` mounted, so they run the code you are looking at.

**Safety rules the tool enforces**
- `capture` and `reset` archive first; `--no-archive` must be typed.
- The workspace is named by **slug** (default `assistant.general`, `ASSISTANT_WORKSPACE_SLUG`); the canonical workspace is emptied, **never deleted**
  (a Postgres restore needs the same workspace id). Other workspaces under the tenant (`plight`, `calfresh`) are never touched.
- A snapshot **refuses** a workspace that is still ingesting or that carries orphaned rows (`--allow-busy`, `--allow-orphans` override, deliberately).
- Restore is one transaction that replaces only that workspace's rows; a wrong or missing workspace id aborts before anything changes.
- `hermes-user-memory` and `hermes-continuity` (learning data) are never touched.
- `reset` after the Edgequake API delete also runs `clean-orphans`, so "empty" is really empty.
- Kafka reset deletes ritual/coire topics **only when no live Ritual owns them** unless `--all-kafka` (use that after resetting Dis).

## The research queue: archive, don't discard

A queue row holds a landed reply and its citations; the crawled, LLM-validated pages behind it exist only as Edgequake documents. So:

- `queue status` shows rows by kind/status and age; `expire-stuck` marks rows stuck in `researching`/`answering` as `failed` (kept, not deleted; rows parked
  for a scheduled retry are left alone).
- `queue archive --to DIR --older-than 7d` writes delivered/failed rows with replies and citations to JSONL, then deletes them (never before the file is written).
- `queue promote --from FILE --seed DIR ID…` copies chosen archived results into a baseline's `promoted_results.jsonl`, so a result worth keeping becomes baseline
  material instead of being lost. Curation (which results deserve it) is a human decision.
- `reset-coupled` clears tags, topics and open queue rows together, always exporting first.

## Analysts name their workspace (G1)

`the_cow.pl` gains `assistant_workspace_slug/1` (reads `ASSISTANT_WORKSPACE_SLUG`, default `assistant.general`); `progressive_research.pl` and `the_leannan.pl`
(`none` now means "the assistant's workspace by slug", not "whatever the tool defaults to") pass `workspace_slug`, and the Rust `edgequake` tool resolves it through
`GET /tenants/{t}/workspaces/by-slug/{slug}` (cached per process, so it survives a workspace being dropped and recreated).
`EDGEQUAKE_REQUIRE_EXPLICIT_WORKSPACE=true` (compose, clara-api; default **false**) makes a call that names no workspace an error and ignores
`EDGEQUAKE_DEFAULT_WORKSPACE`. **Rolled out 2026-09-25:** the flag now defaults to `true` in compose and `EDGEQUAKE_DEFAULT_WORKSPACE` is gone from `docker/.env`, so an unnamed-workspace call fails loudly.
While verifying, a pre-existing bug surfaced: `progressive_research.pl` passed a *list* as the tool's `context` (a String), so tier b failed with "invalid type: sequence"; the option was removed (`tests/test_progressive_edgequake_call.py` guards it).

## Who cleans up Kafka (decided 2026-09-25)

Dis owns a Ritual's topic and deletes it; participants clean up their own state.
- **Dis** (`RitualRegistry::reap_topics`, a `ritual-topic-reaper` thread): deletes the topic of a Ritual terminated more than `ritual_topic_grace_seconds` ago (default 600), and, with `ritual_topic_reap_orphans` (default true), `{domain}.ritual.{uuid}` topics the registry does not know, once seen on two consecutive sweeps. Never touches active Rituals or non-ritual topics. Sweeps every `ritual_topic_sweep_interval_seconds` (300); grace 0 disables it.
- **FieryPit** (`goat/app/participant_reaper.py`, every `RITUAL_PARTICIPANT_REAP_INTERVAL_SECONDS`, default 60, 0 disables): polls Dis for each joined Ritual and, on `terminated` or 404, leaves it (stops the consumer, which leaves its group, and frees the evaluator slot). A Dis outage is "unknown" and does nothing.
- Consumer groups are created by the participants' consumers and expire on the broker once empty; Dis does not manage them.
- Live 2026-09-25: the first sweep after deploy deleted 300 terminated rituals' topics, 0 errors. `ground_state_kafka.py reset` remains the manual override.

## Packing a baseline

`pack` writes `<name>.tar.zst` (zstd; baseline_v1 is 25 MB) beside a SHA-256 sidecar and a JSON summary. `unpack` refuses an archive with no sidecar or a mismatched digest, and refuses to overwrite an existing directory.
`mirror` copies the three files to another host and verifies there. limbic and pineal share `/mnt/moonpool`, so that copy is already visible to both; `mirror` is for an off-site or GitLab-LFS-side copy (git-lfs is not installed, so committing the blob is not set up).

## Tests

- Unit: `lildaemon/tests/test_ground_state.py`, `test_assistant_maintenance.py` (a stateful fake Edgequake); `clara-cerebellum/scripts/test_ground_state_scripts.py`
  (pure logic: restore script shape, orphan and kafka planning); `clara-toolbox` slug resolution tests.
- **`edgequake_baseline`** marker, `lildaemon/tests/test_edgequake_baseline.py`: skipped unless `EDGEQUAKE_BASELINE_DIR` names a seeded baseline; then it fails if the
  workspace lacks the baseline's documents or cannot answer its canaries.
- The Postgres export/restore was exercised for real against a schema-only scratch database built from the same image (never against the live database):
  export from live, restore into scratch, counts match, second restore idempotent, no dangling graph edges, orphan cleanup, wipe-and-restore.

## Known limits

- The Postgres snapshot needs a **quiet** workspace and the **same workspace id** on restore. It moves knowledge tables and the workspace's own vector/graph/kv rows only;
  job history, conversations and audit tables are deliberately excluded.
- If Edgequake caches workspace data in memory, restart `edgequake-api` after a restore and re-run `verify`.
- Edgequake leaves a per-workspace vector table behind for every workspace ever created (85 exist); this tool does not reclaim them.
- `verify`'s canaries call the real query path (an LLM), so it takes as long as your slowest question.
