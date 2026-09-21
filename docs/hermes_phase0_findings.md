# Hermes Agent Phase 0 findings (limbic, source review)

Status: **draft for review. Source review is read-only. One live run was made on 2026-09-21 (see "Live run" under A2); it
read a file and changed nothing except creating one Hermes session/run record.**
Checklist: `hermes_phase0_research_checklist.md`. Plan: `hermes_agent_evaluator_plan_v3.md`.

**Subject:** Hermes Agent (Nous Research), v0.21.3 (2026.9.14), upstream 8ffc2f03, docker install,
container `hermes` on limbic (ports 8642, 9119; `/home/stanc/.hermes` mounted at `/opt/data`).
**Method:** read the installed source at `/opt/hermes` via `docker exec`, `hermes tools list`, and the effective
`/opt/data/config.yaml` (secret-bearing lines filtered out). Paths below are inside the container.
Confidence: **confirmed** (read in source/config), **inferred**, **unknown**. All answers apply to v0.21.3 on
limbic. Pineal's version is not recorded and may differ.

## C1. What we are running

- Version above. Model `qwen-clara-hermes:latest`, `provider: custom`, `base_url http://172.17.0.1:11434/v1`
  (host Ollama). Confirmed (`config.yaml`).
- Container has **no memory or CPU limit** (`HostConfig.Memory=0`, `NanoCpus=0`), bridge network, one bind mount
  `~/.hermes:/opt/data` read-write. Confirmed (`docker inspect`).
- `config.yaml` has **no `platform_toolsets.api_server` entry** (only cli, telegram, discord, whatsapp, slack, signal,
  homeassistant, qqbot, yuanbao, teams, google_chat). Confirmed. So the API server uses its preset default (A1).
- Not recorded: Pineal's version, image digest of the Pineal container. Unknown.

## A1. Native toolsets (hard gate)

Answer: **Toolsets can be disabled, and disabled toolsets are removed from the model's tool list. But the API
server's default preset is nearly the full toolset, so the gate is only real if we configure it explicitly.**

- The API server (8642) uses toolset `hermes-api-server`: "full agent tools accessible via HTTP (no interactive UI
  tools like clarify or send_message)", defined as `_core_without("text_to_speech", "clarify", "computer_use",
  kanban=False)`. Confirmed (`toolsets.py:192`). It therefore includes terminal, file, web, browser,
  code_execution, delegation, cronjob, memory, skills and the rest of the core set (`hermes tools list` for the
  `cli` platform shows the same set enabled).
- Disable path: `hermes tools disable <toolset>` and `platform_toolsets.<platform>` in `config.yaml`
  (`hermes_cli/tools_config.py:553-698`). Confirmed. `hermes tools --summary` needs a TTY and was not run.
  **Unverified:** the exact platform key for the API server (`api_server` is the likely name; no entry exists
  today to confirm the spelling).
- Filtering: `get_tool_definitions(enabled_toolsets, disabled_toolsets)` builds the model-facing list, disabled
  subtracted after enabled (`model_tools.py:213-221`). Confirmed. This is the tool schema list, so a disabled tool
  is not offered to the model. Whether execution is *also* blocked is not confirmed.
- Custom tool registration: MCP servers via `mcp_servers:` in `config.yaml`, stdio (`command`/`args`) or HTTP
  (`url`/`headers`), with per-server `timeout`, `connect_timeout`, `lazy` (`cli-config.yaml.example:1469-1510`).
  Confirmed. MCP tools are addressed as `server:tool` in `hermes tools enable|disable`. Plugins exist under
  `/opt/hermes/plugins` (unexamined). So `request_action` can plausibly be an HTTP MCP server that forwards to the
  evaluator. Inferred.
- Runtime tool-acquisition surfaces (all present in v0.21.3, none yet shown to be closed by disabling the core
  toolsets): `skills`, `connections`, `cronjob`, `delegation`, `a2a` plugin toolset, `peer`, `webhook`, `kanban`,
  and MCP **sampling**, which is "enabled by default" (`cli-config.yaml.example`), i.e. an MCP server can request
  LLM calls. Confirmed for existence; behaviour with toolsets disabled is unknown.
- Delegated children: `delegation.inherit_mcp_toolsets` defaults true (`delegate_tool_config.py:167`). A child
  inherits MCP tools. Confirmed.

## A2. Wire format and API surface

Answer: **Port 8642 is the API server; it runs the whole agent loop server-side. Tool calls are executed inside
Hermes, not returned to the caller.** Exact wire format confirmed by a live run (below).

- 8642: OpenAI-compatible server (`gateway/platforms/api_server.py`). Routes: `POST /v1/chat/completions`,
  `POST /v1/responses`, `GET /v1/models`, `GET /v1/capabilities`, `GET /v1/toolsets`, `GET /v1/skills`,
  `/api/sessions` (+ `/{id}/chat`, `/chat/stream`, `/fork`, `/messages`), `/api/jobs`, `/health`,
  `/health/detailed`, and the run API below. Confirmed (`api_server.py:76-100, 1537-1577`).
- Auth: Bearer `API_SERVER_KEY`; the adapter refuses to start without one. Confirmed (`api_server.py:1375-1390`).
- **Durable run API (best fit for an evaluator):** `POST /v1/runs`, `GET /v1/runs/{id}` (status),
  `GET /v1/runs/{id}/events` (SSE), `POST /v1/runs/{id}/approval`, `/steer`, `/stop`. Supports an
  `Idempotency-Key`. Confirmed (`api_server_runs.py`). Events include `tool.started`, `tool.completed`
  (tool, duration, error, redacted 500-char preview), `reasoning.available`, subagent lifecycle fields
  (subagent_id, depth, status, tokens, cost), and terminal statuses `completed | failed | cancelled`.
- Sessions: `X-Hermes-Session-Id` (continuity) and `X-Hermes-Session-Key` headers; sessions are addressable and
  deletable (`/api/sessions/{id}`). One session per Ego seat is possible. Confirmed for existence.
- Client-supplied `tools` in `/v1/chat/completions`: appears only in the idempotency fingerprint
  (`api_server_openai_routes.py:557`). Whether they are honored is **unknown**; do not rely on it.
- Limits: `gateway.api_server.max_concurrent_runs` default 10 (0 disables) (`api_server.py:1270`);
  `agent.max_turns: 500`; `delegation.max_iterations: 250`; `code_execution.max_tool_calls: 50`;
  `tool_loop_guardrails` warn thresholds on, `hard_stop_enabled: false`, non-interactive hard stop enabled.
  Confirmed (`config.yaml`).
- 9119: dashboard. Not inspected. Unknown.

### Live run (confirmed, 2026-09-21, limbic, v0.21.3)

Called from inside the container (the API binds `127.0.0.1` there; the published port is unreachable from the host).

Request: `POST /v1/runs`, headers `Authorization: Bearer <API_SERVER_KEY>`, `Content-Type: application/json`,
`Idempotency-Key: phase0-wire-1`:

    {"model":"hermes-agent","input":"Use your file tool to read the first 3 lines of /opt/hermes/LICENSE, then tell me what they say. Do not use any other tool."}

Response: `202 {"run_id":"run_96dd...","status":"started","replayed":false}`.

`GET /v1/runs/{id}/events` (SSE, `data: {json}` frames, closes with `: stream closed`), in order:

    {"event":"tool.started","run_id":...,"timestamp":...,"tool":"read_file","preview":"LICENSE"}
    {"event":"tool.completed",...,"tool":"read_file","duration":0.272,"error":false,"preview":"{\"content\": \"1|MIT License\\n2|\\n3|Copyright ...\", \"total_lines\": 21, ...}"}
    {"event":"message.delta",...,"delta":"..."}            (many)
    {"event":"reasoning.available",...,"text":"<final text>"}
    {"event":"run.completed",...,"output":"<final text>","usage":{"input_tokens":26909,"output_tokens":151,"total_tokens":27060},"completed":true,"partial":false,"interrupted":false}

`GET /v1/runs/{id}` returns `{"object":"hermes.run","status":"completed","session_id":"<same as run_id>","output":...,"usage":...}`.

What this establishes:
- **Tool calls are internal.** The caller never receives a `tool_calls` object to execute. It sees only
  `tool.started` (tool name plus a short `preview`) and `tool.completed` (duration, error flag, redacted preview
  capped at 500 chars). **Full tool arguments are not exposed in events.** So the evaluator cannot audit
  `request_action` arguments from the event stream; it must receive them directly, which an evaluator-hosted HTTP
  MCP `request_action` server does.
- **Model behaviour:** `qwen-clara-hermes` made a correct single native tool call and answered from the result.
  About 12 s wall time including model load.
- **Prompt overhead is large:** 26,909 input tokens for a one-line task, dominated by the 25 tool schemas and system
  prompt. Cutting toolsets to one tool should shrink this sharply; measure it after the gate config is applied.
- Each run gets its own `session_id` (equal to `run_id` here), so a per-seat session is the default, not something
  to configure. Whether memory still leaks across sessions through the shared data dir is unchanged (B1).
- Still unverified: a run in which the model calls an MCP tool, and the tool-call format for a non-native path.


## A3. Native confirmation hook

Answer: **A real, external-drivable approval gate exists, but it targets dangerous shell/`execute_code` actions
and plugin-flagged tools, not every tool call. Default is fail-closed.**

- `tools/approval.py`: guard entry points `check_all_command_guards`, `check_execute_code_guard`,
  `request_tool_approval`. Modes `manual | smart | off` (`approvals.mode`, default `manual`)
  (`approval_context.py:228`). Timeout `approvals.timeout` default 300 s and **fails closed** ("the command did not
  run", `approval.py:507`). Confirmed.
- External resolution: gateway pending approvals are resolved over HTTP by
  `POST /v1/runs/{id}/approval` with `choice` in `once | session | always | deny` and optional `request_id`
  (`api_server_runs.py:815`). So approval can be delegated to a service. Confirmed.
- **`always` and `session` choices persist an allowlist** (`_permanent_approved`); a caller could widen approval
  permanently. Confirmed. Any evaluator-side approver must only ever send `once` or `deny`.
- Plugin `pre_tool_call` hooks can block, modify args, or return `{"action": "approve"}` to escalate to the same
  gate (`model_tools.py:760-780`, `approval.py:1094`). Confirmed. A plugin is therefore a second enforcement point
  behind `request_action`, if we ever want belt and braces.
- Bypasses by design: `HERMES_YOLO_MODE` (frozen at import), session `/yolo`, `approvals.mode: off`. A `--yolo`
  flag exists. Confirmed. These must be verified absent per seat.
- Delegated children auto-deny by default (`delegation.subagent_auto_approve` false)
  (`delegate_tool_config.py:39-56`). Confirmed.
- Known bugs or bypass reports: not searched (no network review done). Unknown.

## A4. Cancellation and fan-out

Answer: **A cancel API exists but it is cooperative and best-effort. The container hard-stop remains the only
guarantee, which supports decision 5.**

- Spawn limits: `delegation.max_concurrent_children` default **10**, `max_spawn_depth` default **1** (flat),
  `orchestrator_enabled` default true (a kill switch that forces leaf children), `child_timeout_seconds` default
  **none** ("No default wall-clock cap on children"). Confirmed (`delegate_tool_config.py:17-27, 85-160`).
  So the 1,393-sub-agent incident is not prevented by defaults: the concurrency cap bounds parallelism, not total
  spawns over a long run. Total-spawn cap: not found (unknown).
- Cancel: `POST /v1/runs/{id}/stop` calls `request_hard_interrupt(agent, ...)` and reaps that run's background
  processes; status goes `stopping` then `cancelled` (`api_server_runs.py:900-925`). Interrupt is "cooperative:
  the flag propagates to in-flight tools and recurses into grandchildren via AIAgent.interrupt()", honoured "at its
  next iteration boundary" (`delegate_tool_registry.py:92`). A blocked approval wait may still hold an executor
  thread after stop. Confirmed. Guarantee: **best-effort**, not guaranteed.
- `hermes pause`: "Emergency stop: pause cron/kanban dispatch and new gateway turns". It does not say it cancels
  in-flight work. Inferred not to.
- On container stop: not tested. Cgroup hard-kill is the assumed guarantee. Inferred.

## B1. Sandboxing and blast radius

- The agent can reach everything the container can: `terminal.backend: local` (commands run inside the
  container), the mounted `~/.hermes` (config, credentials, memory, skills, kanban db, sessions), the bridge
  network including host Ollama at 172.17.0.1. Confirmed. Note `terminal.container_*` keys apply to a
  docker terminal backend, not this one.
- **State persists and is shared:** `memory.memory_enabled: true`, `user_profile_enabled: true`, `skills`
  creation nudges, session store, `kanban.db`, all under the one mounted data dir. Two Ego seats sharing a data dir
  would leak state into each other. Confirmed. Container-per-seat needs a **separate data dir per seat**, or memory
  and skills off; sharing one mount would defeat the isolation.
- Outbound calls beyond configuration: `updates.check: true` (update checks); `telemetry.shared_metrics` is
  disabled (`enabled: false, send: false`). Provider fallbacks: `hermes fallback` exists; none configured in
  the filtered config. Confirmed for these keys only.
- Official container isolation guidance: not reviewed. Unknown.

## B2. Model requirements and tool-call reliability

- 64K context minimum: confirmed as behaviour in earlier notes (Hermes' hard minimum, memory
  `hermes_agent_limbic_setup`); the source location was not re-read here. `agent.reasoning_overrides: {}` is empty
  in our config; its semantics were not read. Unknown.
- Qwen-through-Ollama tool-call parsing issues: not researched. Unknown.

## B3. Extension points

- External orchestrator drives Hermes through the OpenAI-compatible API and the run API (A2). This is the
  supported pattern visible in source. Confirmed.
- Custom tool calling back to an HTTP service: an HTTP MCP server with `url` and `headers` is the direct route
  (A1). Confirmed for the config shape; not exercised.

## Blockers

None found that stop the single-tool design. Conditions that must hold before Phase 1:
1. The API server's default preset is near-full tooling; the gate is real only with an **explicit
   `platform_toolsets` entry for the API platform** (spelling to confirm) containing no side-effecting toolsets,
   plus the acquisition surfaces in A1 closed. A per-seat config must be tested by reading `GET /v1/toolsets` and
   the run's tool events, not assumed.
2. Per-seat data dir (B1), otherwise memory, skills and sessions leak between Ego seats.

## Surprises (contradict or sharpen plan assumptions)

- Native approval and cancel APIs **do** exist and are HTTP-driven (plan open questions 2 and 3 are answered "yes").
  Cancel is cooperative only, so the container kill-switch stays.
- Default delegation has no total-spawn cap and no child timeout; only concurrency (10) and depth (1) are bounded.
- The limbic container has no cgroup limits today, so decision 5's "cgroup-bounded" is new work, not existing state.
- The approval gate can persist `always`/`session` allowlists.
- Tool events arrive on an SSE stream with redacted previews, which gives the evaluator an observation channel
  for free (useful for the fan-out cap and for `ritual_id`/`seq` logging).
- MCP sampling is on by default.

## New questions

1. ~~Exact request and response bodies for a run with a tool call.~~ Answered by the live run under A2. Remaining:
   the same for an MCP-provided tool.
2. Platform key for the API server's `platform_toolsets`, and whether disabled tools are also blocked at execution.
3. Does MCP sampling, `a2a`, `peer` or `webhook` remain reachable once the core toolsets are off?
4. Is there a total-spawn or total-tool-call cap per run, or must the evaluator count via the events stream?
5. Does a plugin `pre_tool_call` hook add value as a second gate behind `request_action`?
6. Pineal's Hermes version and config, and whether it matches limbic (C1).
7. What is on 9119 and does it expose anything writable?
8. Whether `docker stop` on a seat container reliably reaps its child processes (needs a test).
