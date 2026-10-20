# Review: FieryPit MCP Server Design Document

**Reviewer:** Clara
**Date:** 2026-07-11
**Status:** For team review

---

## Overall Assessment

Solid architectural bones. The thin-adapter pattern, constraint awareness, and explicit avoidance of the prior v1/v2 MCP server failure are all correct. The gaps are in operational details: error handling, security, observability, testing, and a tool surface that's slightly too wide.

---

## What's Genuinely Good

- **Thin protocol adapter framing** — correct call. Prior servers died because they tried to be *things* rather than *translators*.
- **Grounding in the v1/v2 failure** and explicitly pinning the protocol version.
- **Flat-payload constraint** carried through to tool design. The `clara-prolog` FFI gap is correctly elevated to a design constraint.
- **"What to Avoid" section** reads like hard-won wisdom.
- **Coire TTL gotcha** (60 s default) flagged in the design doc rather than buried in a footnote.

---

## Structural / Architectural Gaps

### 1. No Error Handling or Failure Semantics *(Biggest Hole)*

- What happens when FieryPit is down or times out?
- Partial failures in multi-step flows (`ritual_create` succeeds, `ritual_join` fails → zombie state?)
- Retry semantics — is `ritual_create` idempotent? No idempotency key anywhere.
- `deduce_poll` timeout: need a `state` enum (`running` / `failed` / `converged`), not just a boolean.

**Suggestion:** Add an error taxonomy section. At minimum: `timeout`, `not_converged`, `evaluator_unavailable`, `auth_expired`, `ritual_not_found`. Each maps to a distinct MCP error code.

### 2. Security / Authorization Is Hand-Waved

- **Prompt injection → destructive action.** No confirmation step, no dry-run mode, no "irreversible" flag in the tool schema.
- **JWT scope.** Same privileges for `ritual_terminate` as `ritual_status`? Should differ.
- **Per-agent access control.** Multiple LLM agents → all-or-nothing?

**Suggestion:** Add `destructive: true` flag + `confirm: true` parameter for `ritual_terminate`, `deduce_interrupt`, `coire_topic_delete`.

### 3. Concurrency and Session Isolation

- Two LLM sessions managing the same ritual simultaneously.
- `prolog_clauses` thread-local isolation — state the MCP server is stateless explicitly.

### 4. Observability Is Missing

- No structured logging (correlation IDs across MCP → REST → Kafka).
- No metrics (tool call latency, error rates, convergence cycle counts).
- No tracing (OpenTelemetry).

---

## Tool Surface Feedback

### 5. 12–15 Tools Is Too Many

Target **8–10 tools** max.

- Group `deduce_snapshot` / `deduce_trace` / `deduce_trace_entries` / `deduce_trace_dot` into one `deduce_inspect` with a `view` parameter.
- Group Coire tools into `coire_topic_manage` with an `action` parameter.
- Remove `evaluator_list` if it's not a real endpoint.

### 6. Tool Descriptions Should Encode Semantic Intent

Not `"POST /ritual"` but:

> "Consecrate a new Ritual: a bounded, peer-judgment session in which registered evaluators deliberate on a stated goal. This action is irreversible once participants have joined."

### 7. Response Contract Is Inconsistent

`prolog_solutions` is Prolog-specific. Namespace evaluator-specific payloads under a `type` discriminator:

```json
{
  "result": "...",
  "ritual_id": "a2b3c4d5-...",
  "state": "active",
  "converged": true,
  "cycles": 3,
  "evaluator_output": {
    "type": "prolog",
    "solutions": [...]
  }
}
```

---

## Smaller Issues

| # | Issue | Suggestion |
|---|-------|-----------|
| 8 | Python sketch is too schematic (`...` bodies) | Include actual JSON schema for at least one tool; define `FieryPitClient` / `DisClient` method signatures |
| 9 | No testing strategy | Contract tests (WireMock), integration tests (Docker Compose), MCP spec test client |
| 10 | No startup / readiness handling | Define `/health` check; startup sequence; behaviour when FieryPit isn't up |
| 11 | Citation hygiene | References [3]–[19] used but no references section |
| 12 | "Suitability" question asked but not answered | Map the four load-bearing properties to concrete design decisions |

---

## TL;DR

Tighten the tool count to ~10, add an error taxonomy, encode semantic intent in descriptions, and actually *answer* the suitability question rather than assuming the answer. The design is correct in shape; it needs to go from "here's the shape" to "here's what happens when things go wrong."
