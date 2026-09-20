# Hermes Agent: Phase 0 research checklist (for Clara)

Purpose: answer the questions that gate the Hermes-as-Ego integration, using public documentation, source
code, and issue trackers, before anyone touches the live install. Findings feed a short Phase 0 addendum.
Context docs: `hermes_agent_evaluator_plan.md`, `hermes_agent_evaluator_plan_addendum_2026-09-20.md`,
`hermes_agent_evaluator_plan_addendum_2_2026-09-20.md`.

## Ground rules

- **Subject:** Hermes Agent, the tool-using agent runtime from Nous Research (`nousresearch/hermes-agent`).
  This is **not** the Hermes family of language models. Do not conflate them.
- **Our deployment:** the agent runs in a container named `hermes` (ports 8642 and 9119, data at
  `~/.hermes` mounted at `/opt/data`), backed by Ollama through an OpenAI-compatible endpoint
  (`model.provider: custom`, `qwen-clara-hermes:latest`, context 65536). It requires a model context of at
  least 64K.
- **Evidence standard.** For every answer give: the claim, the source (URL, file path plus line or section,
  or commit), and a confidence of **confirmed** (read in source or authoritative docs), **inferred**
  (reasoned from indirect evidence), or **unknown** (could not find). "I could not find this" is a valid and
  useful answer. Do not guess to fill a gap.
- **Version.** State which Hermes Agent version or commit each answer applies to. We do not yet know our
  installed version (see item 0.1), so flag any answer that may differ across versions.
- **Do not** propose architecture. Report facts. Design happens afterward.

## Part A: Gate questions (each blocks a specific decision)

### A1. Can the native toolsets be disabled? (hard gate)

Why: our design allows exactly one side-effecting tool, `request_action(action, params, justification)`.
That is only enforceable if Hermes' built-in tools cannot act on their own.

- List every built-in tool or toolset Hermes ships (terminal/shell, file read/write, web/browser, code
  execution, memory, sub-agent/delegation, scheduling, messaging integrations, and so on). Mark each as
  read-only or side-effecting.
- Which toolsets are on by default? How are they enabled or disabled: config keys, CLI flags, environment
  variables, per-platform toolset presets? Quote the exact keys.
- Can Hermes run with **zero** native tools plus one custom tool that we define? How is a custom tool
  registered: MCP server, plugin API, config-declared HTTP tool, Python entry point?
- Is there any path by which the agent can acquire tools at runtime (skills, self-authored tools, MCP
  auto-discovery, downloaded plugins)? Can that be turned off?
- Does disabling a toolset remove it from the model's tool list, or only block execution? (Matters for
  prompt injection: a listed-but-blocked tool can still be attempted.)

### A2. Wire format and API surface

Why: `HermesAgentEvaluator` needs a request/response contract. Nothing in this repo documents it.

- What HTTP interfaces does the container expose on 8642 and 9119? Which is the API gateway and which the
  dashboard? Endpoints, methods, auth.
- Is there an OpenAI-compatible chat-completions endpoint served by Hermes itself? If so, does it run the
  full agent loop server-side, or just proxy the model?
- How is a task submitted, and how are results returned: synchronous response, streaming (SSE or
  WebSocket), or job id plus polling?
- How are tool calls represented (native `tool_calls`, XML or JSON tags, other)? Give an exact example
  request and response, including a tool call and its result.
- How are conversation state and sessions identified? Can each Ego seat get an isolated session?
- What are the timeout and iteration-limit knobs (max turns, max tool calls, max tokens per turn)?

### A3. Native confirmation or approval hook

Why: the earlier brainstorm dismissed this as "most fragile" without checking. If it is solid it may
simplify or double the Superego gate.

- Does Hermes have a native approval, confirmation, or permission-gating mechanism for tool use? Describe
  its exact semantics: which tools, what happens on deny, what happens on timeout.
- Can approval be delegated to an external service (webhook, callback, or MCP-side hook), or is it
  human-in-the-loop in the terminal only?
- Is the default fail-open or fail-closed?
- Known bypasses, bugs, or issues reported against it.

### A4. Cancellation and fan-out

Why: motivates the kill-switch design. A prior run spawned 1,393 sub-agents in one task.

- How does Hermes spawn sub-agents or parallel tasks? Are there configurable limits on depth, count, or
  concurrency? Defaults?
- Is there a cancel or abort API for an in-flight task and its sub-agent tree? What does it stop, and is
  stopping guaranteed or best-effort?
- What happens to child processes and tool executions on cancel or on container stop?
- Look for the 1,393-sub-agent incident (see `lildaemon/docs/clara_brainstorm_agent_enablement_hermes.md`
  for our own record of it) or similar public reports of runaway delegation, and what mitigations exist.

## Part B: Questions the gate answers depend on

### B1. Sandboxing and blast radius

- What can a Hermes instance touch by default: filesystem paths, network, host resources, credentials in
  `~/.hermes`? What does the official container deployment guide recommend for isolation?
- Does Hermes persist memory or skills between sessions? Where, and can it be disabled or scoped per
  session? (Our Ego seats should not leak state into each other.)
- Does Hermes make outbound calls we did not configure (telemetry, update checks, model-provider
  fallbacks)?

### B2. Model requirements and tool-call reliability

- Which model families and tool-call formats does Hermes officially support? Any known problems with
  Qwen-family models served through Ollama's OpenAI-compatible endpoint (tool-call parsing, thinking-mode
  spirals, reasoning-effort handling)?
- What does `agent.reasoning_overrides` do, and when is it needed?
- Why the 64K minimum context, and is it configurable?

### B3. Extension points

- What is the supported way to embed Hermes as a component in a larger system (SDK, library import, RPC,
  MCP)? Is there an official pattern for "external orchestrator drives Hermes"?
- Can a custom tool's implementation call back to an external HTTP service? This is where `request_action`
  would forward to the Superego.

## Part C: Housekeeping

### C1. Identify what we are actually running (do this first if you can reach it)

0.1: Record the installed Hermes Agent version or image digest, and the effective config
(`/opt/data/config.yaml`, with secrets removed). If you cannot reach the container, say so and mark
every version-specific answer above as unverified. Relevant notes: `lildaemon/docs/hermes_setup_notes.md`.

## Output format

One markdown document, `hermes_phase0_findings.md`, with a section per checklist item (A1 to A4, B1 to
B3, C1). Each item: a short answer, then evidence lines as described in the ground rules. End with three
lists:

1. **Blockers:** anything that would prevent the single-tool design or the per-seat container kill-switch.
2. **Surprises:** facts that contradict our assumptions in the plan docs.
3. **New questions:** gaps this research exposed that need their own investigation.

## Note for the human (not for Clara)

Hermes is also running on limbic (a reproduction of Pineal's setup, verified 2026-09-17), so most of
A1, A2, and C1 can be confirmed hands-on locally without Pineal access. Treat Clara's answers as leads to
verify against the live container, especially anything marked "inferred".
