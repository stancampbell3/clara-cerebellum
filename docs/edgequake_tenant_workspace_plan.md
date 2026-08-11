# Edgequake Tenant/Workspace/Provider/Model Support — Plan

_Written 2026-08-11. Implemented and verified live the same day — see "Implementation status" below. Builds on `docs/edgequake_integration_status.md` (Phase 1 status) and supersedes `docs/gemini_convo_edgequake_extensions.md` (that doc was speculative/AI-generated sketch code against an unverified API shape — this plan replaces its guesses with behavior confirmed live against the running instance)._

## Implementation status (2026-08-11): done

Everything below was implemented and verified live against a rebuilt `docker-clara-api-1` / the real Edgequake instance on limbic, in this order:

- `clara-toolbox/src/tools/edgequake.rs`: `EdgequakeClient` gained `default_tenant`/`default_workspace`, threaded as `X-Tenant-ID`/`X-Workspace-ID` headers on every request; `EdgequakeArgs` gained `tenant`/`workspace` (headers, all ops) and `llm_provider`/`llm_model` (JSON body, `Query` op only); three new read-only ops added (`ListTenants`, `ListWorkspaces`, `ListModels`).
- `clara-toolbox/src/manager.rs`: reads `EDGEQUAKE_DEFAULT_TENANT`/`EDGEQUAKE_DEFAULT_WORKSPACE`, passed through to `ClaraEdgequakeTool::new`.
- `docker/docker-compose.yml`: added the two new env vars next to the existing `EDGEQUAKE_BASE_URL`/`EDGEQUAKE_API_KEY` lines (both `clara-api` and `lildaemon` blocks).
- `clara-prolog/prolog-lib/the_cow.pl`: added `ruminate_opts/3`, a dict-merge predicate (`.put/1`) layering `tenant`/`workspace`/`llm_provider`/`llm_model`/`mode`/etc. over the base query args, alongside the unchanged `ruminate/2`/`ruminate_mode/3`.

**Bug found and fixed during implementation, not in the original plan**: `docker-compose.yml`'s `${EDGEQUAKE_DEFAULT_TENANT:-}` idiom (matching the pre-existing `EDGEQUAKE_API_KEY` pattern) sets the env var to an **empty string** inside the container when unset, not absent. `std::env::var(...).ok()` reads that as `Some("")`, not `None` — silently bypassing the "no tenant configured" guard and building a malformed `/api/v1/tenants//workspaces` URL. Fixed in `manager.rs` by filtering empty strings to `None` for both new env vars.

**Two things confirmed live that the plan below only guessed at:**

1. **`X-Tenant-ID`/`X-Workspace-ID` headers ARE honored on `POST /api/v1/query`**, not just the GET graph-read endpoints — this was the plan's one open question. Confirmed two ways, both cost-free: a bogus workspace header gets a clean `400 Invalid workspace ID: ...` *before* any LLM dispatch (visible in `clara-api`'s logs via the tool's `Execution failed: Edgequake API error 400 ...`), and a valid tenant/workspace pair succeeds with the correct `workspace_id` visible in Edgequake's own resolver logs.
2. **Don't trust the response body's `stats.llm_provider`/`stats.llm_model` fields — they're unreliable on this Edgequake version (`0.20.2`).** A `ruminate_opts` call with explicit `llm_provider: ollama, llm_model: "gemma4:e4b"` returned a response whose `stats` block claimed `"llm_provider":"openai","llm_model":"gpt-4.1-mini"` — alarming at first (looked like an unintended real cloud call) until checking `docker logs edgequake-api`, which shows the actual dispatch: `edgequake_api::providers::resolver: LLM provider created with safety limits provider="ollama" model="gemma4:e4b" source=Request`. The request genuinely ran on local Ollama; the stats field is just mislabeled/stale — a reporting bug on Edgequake's side (no external LLM keys are configured on that instance at all, per Stan, so a real OpenAI call would have failed outright rather than returning a coherent answer). **When verifying which provider actually served a request, check `docker logs edgequake-api | grep resolver`, not the response body's `stats` block.**

## Where this fits

The status doc's Outstanding section already asked for this: "We should probably include setting the workspace in our interface?" (Stan, re: the container rebuild that fixed the default-model issue). This plan is the design for that, generalized to tenant/workspace/provider/model together since they all thread through the same request path.

Stan flagged wanting Phase 1's Outstanding items (API key dev/prod policy, `the_cow.pl` error-detection verification, deffunction ordering, `rules_fired` always 0, `GET /facts` 500) done before Phase 2. This work is config/plumbing on top of the existing read-only tool, not a new capability (no graph writes), so it can proceed in parallel with those — but the actual coding session should knock out the cheap Outstanding fixes first since they touch the same files (`edgequake.rs`, `the_cow.pl`/`.clp`).

## Live API findings (verified 2026-08-11 against `http://10.0.0.192:8082`, no cost incurred — see method notes)

The Edgequake instance (`v0.20.2`, limbic, docker) exposes real multi-tenant/workspace/provider infrastructure that our tool doesn't touch yet:

- **`GET /api/v1/tenants`** — lists tenants. Currently 2: `Default` (`00000000-0000-0000-0000-000000000002`) and `System Default Tenant` (`...0000`). Each carries its own `default_llm_provider`/`default_llm_model`/`default_embedding_*`.
- **`GET /api/v1/tenants/{tenant_id}/workspaces`** — lists workspaces under a tenant. Currently 1: `Default Workspace` (`00000000-0000-0000-0000-000000000003`), which also carries its own provider/model defaults and `entity_types`.
- **`GET /api/v1/models`** — lists all configured providers and their models with capabilities/cost/tags. 15 providers configured (openai, anthropic, gemini, mistral, ollama, lmstudio, xai, openrouter, minimax, vertexai, nvidia, cohere, jina, huggingface, vscode-copilot), all `enabled: true`. Ollama alone has 23 models.
- **Tenant/workspace scoping mechanism is HTTP headers, not query/body params.** Confirmed by contrast test on the read-only `GET /api/v1/graph/entities` endpoint (no LLM call, safe to repeat):
  - `?tenant_id=bogus&workspace_id=bogus` (query params) → **ignored**, still returns the real 9,289-entity default workspace.
  - `X-Tenant-ID: bogus / X-Workspace-ID: bogus` (headers) → **enforced**, returns `{"items":[],"total":0}`.
  - Valid headers or no headers at all → same real data (server falls back to the `default` tenant/workspace, matching `/health`'s `"workspace_id":"default"`).
  - **Conclusion**: the gemini doc's guess to use `X-Tenant-ID`/`X-Workspace-ID` headers was right for tenant/workspace, even though it was unverified when written. Ignore its query-string "alternative" — confirmed not to work.
- **Provider/model override on `POST /api/v1/query` is JSON body fields**, confirmed via a deliberately-invalid probe (`llm_provider: "bogus", llm_model: "bogus"` in the body) that fails validation *before* reaching any model dispatch: `"Cannot use provider 'bogus' with model 'bogus': ... Valid: openai, anthropic, gemini, vertexai, openrouter, xai, huggingface, openai-compatible, ollama, lmstudio, vscode-copilot, mistral, azure, bedrock, mock"`. This is a safe/free way to confirm field names without spending on a real completion — the same probe pattern (obviously-invalid value → read the validation error) is worth reusing during implementation instead of firing real queries.
- **Current defaults are local Ollama** (`gemma4:e4b`, tenant- and workspace-level). Per Stan, this is deliberate — we're running against a local Ollama instance on limbic. The plan below must not introduce a global default that overrides this; provider/model should only change when a caller explicitly asks.

## What to build

### 1. `clara-toolbox/src/tools/edgequake.rs`

- `EdgequakeClient`: add `default_tenant: Option<String>` / `default_workspace: Option<String>` fields (constructor grows to 4 args, matching the gemini doc's shape). In `request()`, set `X-Tenant-ID`/`X-Workspace-ID` headers when a per-call value or client default is present; omit entirely otherwise (let Edgequake fall back to its own default, don't invent one).
- `EdgequakeArgs`: add `tenant: Option<String>`, `workspace: Option<String>` (routed to headers for every operation), plus `llm_provider: Option<String>`, `llm_model: Option<String>` (routed into the JSON body, `Operation::Query` only — the graph-read ops don't invoke an LLM so these fields are meaningless there; reject or silently ignore if set on a read op, reject is more honest).
- New read-only discovery operations, mirroring the existing graph-read pattern:
  - `ListTenants` → `GET /api/v1/tenants`
  - `ListWorkspaces` → `GET /api/v1/tenants/{tenant_id}/workspaces` (needs `tenant_id` — reuse `entity_name`-style required-arg pattern, or add a dedicated `tenant_id` arg)
  - `ListModels` → `GET /api/v1/models`

  These matter because without them, a Prolog rule or a human has no way to discover valid tenant/workspace/provider/model values short of curling the API directly — the whole point of "supporting" these options is picking real values, not just plumbing opaque strings through.
- Keep `context`/`max_results`/etc. exactly as-is; this is additive.

### 2. `clara-toolbox/src/manager.rs` + docker-compose / `.env`

- `ClaraEdgequakeTool::new` grows to take `default_tenant`/`default_workspace`, read from new `EDGEQUAKE_DEFAULT_TENANT`/`EDGEQUAKE_DEFAULT_WORKSPACE` env vars (both `Option`, unset = let Edgequake use its own default — correct for the current single-tenant setup, and forward-compatible once there's more than one tenant in play).
- No new provider/model env var. Global provider/model defaults belong to Edgequake's tenant/workspace config (already set to Ollama there), not duplicated in our stack — avoids the two configs drifting.
- `docker/docker-compose.yml`: add the two new env var lines next to the existing `EDGEQUAKE_BASE_URL`/`EDGEQUAKE_API_KEY` block (both `clara-api` and `lildaemon` service definitions, matching the existing duplication).
- Carries forward, doesn't resolve: status doc's Outstanding #1 (`EDGEQUAKE_API_KEY` blank, dev/prod `.env` split still needed).

### 3. `clara-prolog/prolog-lib/the_cow.pl` and `clara-clips/clp-lib/the_cow.clp`

Keep `ruminate/2` and `ruminate_mode/3` as zero-config thin wrappers (no behavior change, still hit the client's default tenant/workspace/provider/model). Add an options-dict variant alongside them rather than growing positional arity:

```prolog
%% ruminate_opts/3 - Query Edgequake with explicit tenant/workspace/provider/model overrides.
%%   Opts = _{tenant: Tenant, workspace: Workspace, llm_provider: Provider, llm_model: Model, mode: Mode}
%%   All keys optional; omitted keys fall through to the client's configured defaults.
ruminate_opts(Query, Opts, Result) :-
    Args = _{operation: query, query: Query}.put(Opts),
    dict_to_json(_{tool: edgequake, arguments: Args}, Json),
    the_rabbit:clara_evaluate(Json, Raw),
    atom_json_dict(Raw, Dict, []),
    ( get_dict(status, Dict, error) ->
        format(user_error, "edgequake tool error: ~w~n", [Dict.message]),
        fail
    ;
        Result = Raw
    ).
```

`.put(Opts)` merges caller overrides over the base `{operation, query}` dict — same shape whether the caller passes `mode`, `tenant`, `llm_provider`, or any future field, without a new predicate per combination.

CLIPS side: same idea, a `ruminate-opts` deffunction that takes a pre-built JSON fragment string for the optional fields (CLIPS has no dict-merge primitive, so this is closer to string concatenation than Prolog's `.put`) — lower priority than the Prolog side since CLIPS is the less-used path currently, and blocked on the deffunction-ordering fix below anyway.

## Verification plan (kept cheap/local, no cloud spend)

1. Graph-read header scoping — already verified live (see Findings above), no further action needed.
2. End-to-end `llm_provider`/`llm_model` plumbing through the new Rust code — call `ruminate_opts` with `llm_provider: "ollama", llm_model: "gemma4:e4b"` (i.e., the existing default, made explicit). This exercises the whole path — Prolog → tool → Edgequake body field → real dispatch — at zero cost since it's local Ollama, and proves the field-name plumbing without needing to spend on OpenAI/Anthropic/etc.
3. Do **not** live-test cloud providers (openai/anthropic/gemini/...) as part of this work unless Stan wants to spend the credits to confirm end-to-end — the validation-error probe technique above already confirms the field names are correct without dispatching a real completion.

## Open questions / explicitly out of scope for this pass

- **Other request options** (temperature, top_k, max_tokens, embedding provider/model, response streaming): `GET /api/v1/models` exposes per-model capability flags (`supports_streaming`, `supports_json_mode`, context length, etc.) suggesting these might be controllable, but that hasn't been probed. Recommend using the same safe "deliberately invalid value → read the validation error" technique per field when this becomes relevant, rather than guessing.
- ~~Whether `X-Tenant-ID`/`X-Workspace-ID` headers are honored on `POST /api/v1/query`~~ — **resolved, yes**, see "Implementation status" above.
- **Auth policy** (`EDGEQUAKE_API_KEY` blank, dev/prod split) — status doc Outstanding #1, unresolved, unaffected by this plan.
- **Write operations** (Phase 2 proper: graph writes, lildaemon evaluator write access) — still deferred, this plan only touches read/query scoping.
