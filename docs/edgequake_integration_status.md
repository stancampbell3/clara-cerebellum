# Edgequake Integration — Status

_Snapshot as of 2026-07-25, updated 2026-07-31. See `docs/the_cow_planning.md` for the original planning doc and `/home/stanc/.claude/plans/prancy-plotting-dewdrop.md` for the full Phase 1 implementation plan._

Phase 1 of the Edgequake integration is implemented: a shared `edgequake` Rust tool in `clara-toolbox` (query + graph reads), `the_cow.pl`/`the_cow.clp` Prolog/CLIPS bridges, and a `goat/mcp/edgequake` stdio MCP server for lildaemon, all wired through `config/evaluators.yaml` and docker-compose env vars.

**Edgequake's real location**: running at `http://10.0.0.192:8082` (host "limbic"), frontend on `:3421`. Reachable directly from both the `clara-api` and `lildaemon` containers as a plain LAN address — no `host.docker.internal` or compose-network bridging needed.

## Resolved (2026-07-31)

1. **`load_rules` fix verified live, and a second, previously-undetected critical bug found and fixed underneath it.**
   - The `load_rules` fix (`is_construct`/`build_or_eval`/`split_clips_constructs`) was confirmed live: POSTing a plain `(defrule ...)` to `/sessions/{id}/rules` now builds successfully (no more `Missing function declaration for 'defrule'`), and the rule genuinely fires on `(run)` (verified via a rule whose RHS asserts a new fact, confirmed present afterward).
   - While loading `the_cow.clp`'s `ruminate-mode` and calling it live, `clara-evaluate` kept returning `{"status":"error","message":"Invalid JSON: EOF while parsing a value at line 1 column 0"}` for *every* payload, even trivial ones (`echo` tool). Root cause: an ABI mismatch that has existed since `clara-evaluate` was first implemented (commit `fe6018c`) — `clara-clips/clips-src/core/userfunctions.c` declares and calls `rust_clara_evaluate(void* env, const char* input)` (two args), but `clara-toolbox/src/ffi.rs` only ever defined `rust_clara_evaluate(input_json: *const c_char)` (one arg). Under the System V x86-64 calling convention, Rust's single parameter reads register `rdi`, which C populates with the CLIPS `Environment*` — the actual JSON string pointer in `rsi` was silently dropped. Every CLIPS-side `clara-evaluate` call has been reading garbage/empty memory as its input for the life of the feature; masked because the only test (`test_clara_evaluate_callback`) asserts `result.is_ok()` but never checks the actual response content.
   - **Fix**: `clara-toolbox/src/ffi.rs` `rust_clara_evaluate` now takes `(_env: *mut libc::c_void, input_json: *const c_char)`, matching the C declaration; the `_env` param is unused (no Rust caller needs it — Prolog's path uses a separate `pl_clara_evaluate` callback, unaffected). Rebuilt `clara-api:latest` and recreated `docker-clara-api-1`.
   - **Verified live end-to-end**: `(ruminate-mode "what is a qubit?" "naive")` from CLIPS now reaches Edgequake and gets back a real structured response (`llm_model: gpt-4.1-mini`, `llm_provider: openai`, timing stats, empty result set since that query isn't in the graph — a data issue, not plumbing). `graph_search_entities` from CLIPS also confirmed working (9,289 total entities). This is the same live round-trip already proven on the Prolog side, now also proven on the CLIPS side.
   - Uncommitted changes so far: `clara-api/src/handlers/session_handler.rs`, `clara-clips/src/backend/ffi/environment.rs` + re-exports (`mod.rs`/`lib.rs`/`backend/mod.rs`), `clara-clips/src/bin/clips-repl.rs` (dedup'd `is_construct`), `clara-toolbox/src/ffi.rs` (this session's ABI fix), `docker/docker-compose.yml` (Edgequake env vars).

## New, smaller findings from this verification pass

- **`the_cow.clp` deffunctions are order-dependent.** Loading the whole file at once fails to build (`CLIPS Build failed (code 3)`) because `ruminate` (defined first) calls `ruminate-mode` (defined third) — CLIPS deffunctions require the callee to already be built, no forward references. Loading `ruminate-mode` alone (or reordering the file bottom-up: `ruminate-mode`, `ruminate-with-context`, `ruminate`) builds fine. Worth fixing the file's definition order, or teaching `load_rules`/callers to build in dependency order.
- **`/sessions/{id}/run` always reports `"rules_fired": 0`, even when a rule genuinely fired.** Confirmed via a rule whose RHS asserted a new fact — the fact appeared, but the API response still said 0. `run_rules` (`clara-api/src/handlers/session_handler.rs`) parses `(run)`'s return value with `result.trim().parse::<u64>().unwrap_or(0)`; likely `env.eval("(run)")` isn't returning the plain fire-count CLIPS returns (e.g. a float string, or captured stdout instead of the return value), silently defaulting to 0 either way. Not investigated further — flagging for a follow-up.
- **`GET /sessions/{id}/facts` (no query params) 500s** with `CLIPS parsing error: [PRNTUTIL2] Syntax Error: Check appropriate syntax for fact-set query function.` — the endpoint likely expects a query parameter that wasn't supplied; not investigated.

2. **Edgequake's default LLM model is now confirmed set and working.** Stan rebuilt Edgequake's containers mid-plan; this session's live `ruminate-mode` call confirmed it — the response came back with `llm_model: gpt-4.1-mini`, `llm_provider: openai`, and full timing stats, no more `Ollama API error (400): model is required`. Graph-read operations remain confirmed working too (9,289 total entities, up from 5,422 at last check — the graph has grown).
[STAN] I rebuilt the docker containers for edgequake and the default should now be set.  We should probably include setting the workspace in our interface?

## Outstanding

1. **`EDGEQUAKE_API_KEY` left blank** in `clara-cerebellum/docker/.env`. Reads worked fine without it during testing, so auth may not be enforced on this Edgequake instance — unconfirmed whether that's intentional.
[STAN] Yes.  The API key is currently blank.  We'll need to fold edgequake into our stack at a later stage and set up dev and prod values for its environment.

2. **Minor, inherited, unconfirmed nuance**: `the_cow.pl`'s error-detection (`get_dict(status, Dict, error) -> fail`) was copied verbatim from `the_rabbit.pl`'s established convention, but attempts to confirm it actually fires on a real error response were inconclusive — the `/devils/sessions/{id}/query` REST endpoint appears to echo back the goal's term structure rather than clean variable bindings, making it hard to observe from outside. Not a regression (same pattern as pre-existing `classify_text/2`), just unverified either way.
[STAN] Definitely something we need to address related to rituals and deduction in general.  Let's not forget this.

Phase 2 (Edgequake graph write ops + lildaemon evaluator write access) and Phase 3 (deeper Prolog/CLIPS dynamic-reasoning integration with graph concepts) are intentionally deferred — sketched only lightly in the plan file, not designed yet.
[STAN] Let's complete these outstanding items as appropriate before moving on to Phase 2.

**How to apply**: when resuming, start by checking `docker ps`/`docker logs docker-clara-api-1` for current container state, then work through the Outstanding items above (API key policy, `the_cow.pl` error-detection) plus the three new smaller findings (deffunction ordering, `rules_fired` always 0, `GET /facts` 500) before moving to Phase 2.
