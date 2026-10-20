# Edgequake MCP tools for Evaluators: session summary (2026-10-20)

*For team review. Status: built, deployed on limbic and pineal, regression-checked. Reviewed by Stan during the session; this summary is not itself reviewed.*

## Goal

Let tool-loop Evaluators (FieryPit) call Edgequake's own MCP server as tools, so a model can choose to search the knowledge graph mid-reasoning. Before this, Edgequake was reachable only through two fixed, non-MCP paths: the Prolog path (`the_cow.pl` `ruminate_*` -> Rust toolbox `edgequake` tool -> REST) and the ritual path (`CowEvaluator` -> Python `EdgequakeClient` -> REST). Neither lets a model decide to look something up.

The four earlier design docs (`fierypit_mcp_*`, `edgequake-mcp-node-spawn-from-rust.md`) were treated as **brainstorm input only**. The Dis-spawns-Node design in particular does not match the code (Dis does not host evaluators) and was not followed. They are committed for reference (clara-cerebellum `1dd0eb5`).

## Decisions (Stan)

| Question | Choice |
|---|---|
| Which MCP server | The Rust gateway already built into `edgequake-api` (`POST /mcp`, streamable HTTP). Not the Node stdio server, which was never deployed. |
| Scope | Read-only: search / fetch / retrieve. |
| Client | Fix lildaemon's `MCPToolRegistry` for `mcp` 2.x; `ToolifiedOllama`-based evaluators first, Groq later. |
| Gateway defect found | Fix it in the Edgequake fork (not work around it in lildaemon). |
| Pineal | Point its FieryPit at limbic's Edgequake; API key stays empty. |
| Usage | Qualify the tool in the system prompt so models do not over-use it. |

## What was built

### 1. Edgequake fork: spec-conformant `tools/call` (`stancampbell3/edgequake`, branch `edgequake-main`, `211c8b77`)
- The gateway returned a bare `{"results": [...]}`. The `mcp` 2.x SDK client rejects that (`CallToolResult.content` is required).
- `dispatch.rs::into_call_tool_result` now adds `content`, `structuredContent` and `isError`, and keeps the old top-level fields so existing clients are unaffected.
- Two unit tests added; the existing MCP e2e (21) and transport (21) suites pass.
- The running `edgequake-api` had been the **stock upstream image**, not the fork. It now runs a locally built fork image (`edgequake-clara:g1`) selected by `EDGEQUAKE_API_IMAGE` in `docker-compose.quickstart.yml`. This also brought in the fork's committed SPEC-084 fusion change and an `EDGEQUAKE_HOST/PORT` change in `main.rs`.

### 2. lildaemon: `MCPToolRegistry` ported to mcp 2.x (`goat/models/MCPToolRegistry.py`)
- v2 transport and snake_case result fields (the old code imported v1 names, so discovery had been failing silently).
- Per-server `headers`, `fixed_args`, `workspace_slug` (resolved to a `workspace_id`, pinned as an argument and `X-Workspace-ID`, hidden from the model's schema, not overridable by the model), `max_result_chars` (default 6000), `${VAR}` expansion.
- A down server is logged at ERROR with "NOT offered" and retried lazily every 30 s (`failed_servers` is inspectable).
- Single JSON text results are parsed rather than double-encoded; results are `json.dumps(..., default=str)`-safe.
- New evaluator `clara_mind_splinter_edgequake` (no local tools, Edgequake MCP only). 9 registry tests against a real mcp 2.x server.

### 3. Usage policy so the tool is not over-used
- `config/prompts/edgequake_tool_policy.txt`, attached through a per-server `usage_guidance_file`. The registry returns it and the evaluator appends it to its system prompt, so **every evaluator that includes the server gets the same policy**; it is skipped if the server was unreachable.
- Policy: search for project-specific context (Clara/Dis/FieryPit/Rituals, a named document, "what our system does", the current task's context); do not use it for chit-chat, general knowledge, creative requests, or what is already in the conversation. Budget: one search per question, at most two fetches, no repeated queries, stop and say so when nothing relevant comes back.
- Check on `qwen-clara` (small sample): greeting, trivia, a binary-search explanation and a haiku made **0** calls; two project questions made **1 search + 1 fetch** each. Other models may behave differently; the loop guard (25 calls per turn by default) is still the only hard cap.

### 4. Pineal's FieryPit wired to limbic's Edgequake (`docker-compose.ego-remote.yml`)
- `EDGEQUAKE_BASE_URL=http://limbic:8082`, a default tenant for slug resolution, `EDGEQUAKE_API_KEY` left empty (Edgequake needs no key here).
- Setting a base URL would have started the shared assistant-documents reaper on pineal (the reason it had been blanked). New `ASSISTANT_REAP_ENABLED` override (mirrors `ASSISTANT_COMPOSED_WARM`): `0`/`1` forces, otherwise follows the base URL. Pineal sets `0`; its log confirms the reaper is off.

## Commits

| Repo | Commit | Content |
|---|---|---|
| edgequake fork | `211c8b77` | gateway fix, quickstart compose, `main.rs` host/port |
| lildaemon | `2505399` | registry port, evaluator, tests, doc (rebased onto upstream `cbf742e`) |
| lildaemon | `af48bb7` | `ASSISTANT_REAP_ENABLED` |
| lildaemon | `d2b54f0` | usage policy |
| clara-cerebellum | `1dd0eb5` | the four brainstorm docs |
| clara-cerebellum | `26962bd` | pineal ego-remote compose |

All pushed to GitHub. `stancampbell3/edgequake` was created for this (**private**, branch `edgequake-main` only).

## Deployment

- **limbic**: `edgequake-api` -> `edgequake-clara:g1`; `docker-lildaemon-1` rebuilt (twice; latest includes the policy).
- **pineal**: `ego-lildaemon-ego-1` runs a prebuilt `lildaemon:ego` image that is built on limbic and shipped with `docker save | docker load`; it is **not** built from pineal's git checkout. Recreated from the updated compose.
- Both images were checked: no `/app/.env` baked in; inside each container the registry finds all 3 tools, the policy is in the system prompt, and a live search works.
- Rollback tags: limbic `lildaemon:pre-edgequake-mcp`, `lildaemon:pre-edgequake-policy`, `lildaemon:ego-prev-edgequake`, `lildaemon:ego-pre-policy`; pineal `lildaemon:ego-pre-edgequake`, `lildaemon:ego-pre-policy`. For Edgequake: unset `EDGEQUAKE_API_IMAGE` and `bash go.sh "up -d --no-deps api"`.

## Regression check

| Check | Result |
|---|---|
| lildaemon suite | 2172 passed, 81 skipped |
| Edgequake MCP gateway tests (e2e, transport, 2 new) | pass |
| Edgequake baseline canaries | 2 passed |
| Ground-state verify (14 checks) | pass after restore |
| Functional routing corpus | 42 passed, 5 documented known-gap xfails |
| Functional live paths (composed analyst) | 9 of 9 passed |

Not run: the full `edgequake-api` cargo suite (its build pushed load average past 140 and starved the LLM tests; it probably needs a database). `spec028_routes_registered` fails; it is a source-text grep on `routes.rs` and is unrelated to this change, though not confirmed against a clean tree.

## Incidents and findings

- **lildaemon left stopped.** `ground_state.sh verify` stops lildaemon (DuckDB is single-process). My first run hit a shell timeout and was killed before its restart step, so the container stayed down for about 20 minutes. Restarted; the re-run passed. Worth hardening the script's cleanup against being killed.
- **Baseline drift and wipes.** The Edgequake workspace had drifted from `baseline_v1` (18 documents vs 13), so it was restored (wipe permitted by Stan), and restored again after the live-path run added 2 research-crawl documents. Queue rows from the test run were exported to `/mnt/moonpool/clara-archives/queue_export_20261020` first.
- **Snap docker on pineal.** `docker cp` from a host `/tmp` path fails (`lstat ... no such file`) because the snap's confinement does not see the host `/tmp`; paths under `~` work. Same family as the earlier data-root issue. Workaround: put files under `~` or pipe through `docker exec -i`.
- **Credential in git config.** `origin` URLs of lildaemon, clara-cerebellum and the edgequake fork embed a GitLab token in plain text. Rotate it or use a credential helper.
- The Edgequake fork's `go_mcp.sh` (untracked) has a broken line continuation and sets variables nothing reads; left alone.

## Known limitations / open items

1. **Groq tier** ignores `mcp_servers` (`groq_evaluator.py` builds its payload from local tools only). Follow-up.
2. **Read-only by design.** No ingest, tagging, delete or ruminate tools. Those would need the Rust gateway extended and, for mutating tools, the Superego / ledger gate pattern from `goat/mcp/ego_gate`.
3. **No per-evaluator allowlist for MCP tools** (`tools_allow` covers local tools only), and no approval gate on MCP calls; the loop guard and the usage policy are the only bounds.
4. **Prolog path unchanged.** `the_cow.pl` and the `CowEvaluator` still use REST. Migrating them to MCP was out of scope.
5. **Pineal has no seat that uses the new evaluator yet.** It is loadable there; nothing calls it.
6. **Limbic's Ego compose** (`docker-compose.ego.yml`) still blanks `EDGEQUAKE_*`; only pineal was changed, as requested.
7. **Stashed on pineal:** a local edit to `config/evaluators.yaml` (nsfw evaluator model changed to `qwen-clara`) and a deleted `workspace/README.md` in `~/widebody/Development/lildaemon`. `git stash pop` restores them.
8. **Prompt evolution** (e.g. pulling in the Google harness2 code) is deferred.
9. Models other than `qwen-clara` have not been checked against the usage policy.

## Where to look

- `lildaemon/docs/edgequake_mcp_tools.md` (wiring note), `config/prompts/edgequake_tool_policy.txt`, `goat/models/MCPToolRegistry.py`, `tests/test_mcp_tool_registry.py`
- `edgequake/crates/edgequake-api/src/mcp/gateway/dispatch.rs` in the fork
- `clara-cerebellum/docker/docker-compose.ego-remote.yml`
