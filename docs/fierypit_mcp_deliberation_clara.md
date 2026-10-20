# Deliberation Contribution — Clara, Reviewer of Record

**Ritual:** FieryPit MCP Server Design Review
**Date:** 2026-07-11
**Status:** Submitted for assembly deliberation
**Precedent:** Roll call 2 adopt / 0 reject (member-gemma, member-qwen); groq-splinter and member-groq-gptoss absent

---

## Phase 1 — Framing

The question before the assembly is clear and I'll restate it so we're not arguing past each other:

> *Is the proposed MCP Server design for FieryPit fit for purpose as a thin protocol adapter, and what specific revisions are required before implementation?*

That's two questions. "Fit for purpose" is the suitability ruling. "What revisions" is the amendment list. We must answer both, and the second must be *prioritised*, not just enumerated. A list of twelve "todos" is a wish, not a plan.

---

## Phase 2 — Merits (What I Affirm, and Why)

I'll be generous here because I want the record to show I'm not just picking apart my own review.

1. **Thin adapter, correctly scoped.** The v1/v2 graveyard is real. pitsnake, prolog_devil, clips_devil, ochecho — four servers, all dead from the same cause: they tried to *be* something rather than *translate* something. The design's explicit decision to be a translator, not a re-implementation, is the single most important architectural choice in the document. I affirm it.

2. **Protocol version pinning.** Boring, correct, and the kind of thing that saves you at 3 a.m. when you're debugging why your MCP handshake is speaking a dialect of JSON-RPC the client doesn't understand. Pin it. Write the version in the module docstring *and* the tool metadata. I've seen both.

3. **REST-only, no local instantiation.** FieryPit is a Docker container on port 6666. You don't `import FieryPit` and `ritual = FieryPit()`. You `POST /ritual/join`. The design gets this right and I won't let anyone walk it back.

4. **Flat-payload constraint carried through.** The clara-prolog FFI gap is real. If the MCP server is the bridge between LLM and Kafka, and the payload must be flat JSON because the FFI can't serialise nested structures, then the tool schemas *must* reflect that. A tool that returns `{"status": "running", "participants": [{"id": "snek", "role": "evaluator", "last_heartbeat": ...}]}` is a tool that will break at the FFI boundary. Flat means flat.

5. **Coire TTL flagged, not buried.** Sixty seconds. If you don't know that, your `deduce_poll` will time out and you'll think the ritual is broken. It isn't. The topic just evaporated. Flagging this in the design doc rather than in a footnote three levels deep is correct.

6. **Statelessness.** The MCP server holds no ritual state. It's a relay. State lives in FieryPit's `RitualManager` (in-memory `HashMap`), in Kafka topics, in Dis's routing tables. The MCP server's job is to translate a function call into an HTTP request and shape the response. That's it. I affirm this boundary.

---

## Phase 3 — Deficiencies (The Gap Inventory, Severity-Ranked)

I'll be more specific than the review was, because the assembly needs to vote on *revisions*, not *themes*.

### Severity 1 — Blocking (cannot ship without)

**G1: Error taxonomy and failure semantics are absent.**

This is the hole in the floor, and I stand by calling it the biggest gap. Let me be concrete about what's missing:

- **`ritual_create` succeeds, `ritual_join` fails.** The ritual exists. No one is in it. Is it a zombie? Does it auto-expire? The design says "defer persistence to lildaemon.duc," which is fine for *storage*, but the MCP server still needs to *report* the state. A tool that returns `200 OK` after `ritual_create` and then `503` after `ritual_join` has left the LLM in a state where it doesn't know whether the ritual is real.

- **`deduce_poll` returns a boolean.** `true` or `false`. That's it. The LLM has no way to distinguish "the deduction is still running and will take another 40 seconds" from "the deduction ran for 40 seconds and the evaluators disagreed and it's stuck." Those are *fundamentally different situations* requiring *fundamentally different LLM responses*.

- **No idempotency.** If the LLM calls `ritual_join` twice (because the first response was slow and it retried), does FieryPit add the node twice? Does it return a conflict? The design doesn't say.

**Required revision:** Add a section "Error Taxonomy & Failure Semantics" with at minimum:

| Error Code | Condition | MCP Error Mapping | LLM Guidance |
|---|---|---|---|
| `RITUAL_NOT_FOUND` | `ritual_id` doesn't exist | `invalidParams` | "No ritual with this ID. Use `ritual_list` to see active rituals." |
| `EVALUATOR_UNAVAILABLE` | Named evaluator not registered on FieryPit | `internalError` | "Evaluator 'snek' is not currently registered. Available: [...]" |
| `DEDUCTION_NOT_CONVERGED` | `deduce_poll` exceeded `max_wait_s` without convergence | `timeout` (custom) | "Deduction still running after {n}s. Call again or `deduce_interrupt`." |
| `DEDUCTION_FAILED` | Evaluators disagreed beyond tolerance | `internalError` | "Deduction failed to converge. Participants: [...]. Consider `ritual_terminate`." |
| `AUTH_EXPIRED` | JWT expired mid-flow | `authError` | "Session expired. Re-authenticate and retry." |
| `RITUAL_ALREADY_ACTIVE` | Duplicate `ritual_join` for same node | `invalidParams` | "Node 'clara_mind_splinter' is already a participant." |
| `COIRE_TOPIC_EXPIRED` | Topic TTL (60s) elapsed | `timeout` | "The Coire topic has expired. Re-initiate the deduction." |

And `deduce_poll` must return a **state enum**: `running | converged | failed | expired`, not a boolean. Non-negotiable.

**G2: Security model is a hand-wave.**

The design says "JWT minting / pass-through" and calls it done. That's not a security model. That's a *credential mechanism*. The actual questions are:

- **Scope differentiation.** Right now, the JWT grants identical privileges to `ritual_status` (a read) and `ritual_terminate` (a destructive write). An LLM that's been prompted to "check on the ritual" should not hold the same token that can *end* it. The MCP server must mint or map scopes: `ritual:read`, `ritual:write`, `ritual:destroy`, `deduce:read`, `deduce:write`.

- **Destructive action confirmation.** `ritual_terminate`, `deduce_interrupt`, `coire_topic_delete` are irreversible. The tool schema must include a `confirm: boolean` parameter (default `false`). The MCP server returns `{"status": "confirmation_required", "action": "ritual_terminate", "ritual_id": "rit_0042"}` on first call. The LLM must call again with `confirm: true`. This is the "irreversibility of consecration" property made operational.

- **Per-agent ACL.** If multiple LLM agents are connected to the same MCP server, agent A (a brainstorming splinter) should not be able to `ritual_terminate` a ritual that agent B (a deliberation committee) is actively running. The JWT `aud` claim or a separate `agent_id` claim must map to a permission set.

**Required revision:** Add a "Security & Authorization" section defining: (a) the scope set, (b) the confirmation protocol for destructive tools, (c) the per-agent ACL model, (d) the JWT claim schema.

### Severity 2 — Required for Production (but not for initial prototype)

**G3: Observability is absent.**

The MCP server sits in the middle of a three-hop call chain: LLM → MCP → FieryPit/Dis → Kafka. When something breaks, you need to know *where* it broke. Without correlation IDs, you're grepping three log streams and praying the timestamps align.

Minimum requirements:
- **Structured logging** (JSON) with a `correlation_id` (UUID) generated at MCP tool-call entry, propagated to outgoing HTTP requests via `X-Correlation-ID` header, and logged by FieryPit/Dis.
- **Metrics**: tool call count, latency (p50/p95/p99), error rate per tool, convergence cycle count for `deduce_poll`.
- **Tracing**: OpenTelemetry spans for each tool call, with child spans for the underlying HTTP calls. This is not optional. In a three-service topology, you cannot debug without it.

**G4: No testing strategy.**

The design has no testing section. Not even a sentence. For a protocol adapter, the testing strategy is:

- **Contract tests** (WireMock or MockServer): stub FieryPit and Dis endpoints, verify the MCP server's request shaping and response mapping. Run in CI.
- **Integration tests** (Docker Compose): bring up FieryPit + Kafka + Dis, run a full ritual lifecycle (create → join → deduce → poll → terminate). This is the "does it actually work" test.
- **MCP spec conformance**: use the official MCP test client (or `mcp-inspect`) to verify the tool registry, JSON schemas, and protocol version are valid.
- **Error path tests**: FieryPit down, Kafka broker unreachable, JWT expired mid-flow, Coire topic expired. These are the tests that will catch the gaps in G1.

### Severity 3 — Quality of Life (should be in v1, forgivable in v0.9)

**G5: The Python sketch is a skeleton.** `...` bodies are not a design. The assembly cannot vote on a design it can't read. At minimum, include the full JSON Schema for `ritual_create` and `deduce_poll`, and the method signatures for `FieryPitClient` and `DisClient`.

**G6: No startup/readiness handling.** What happens when the MCP server starts and FieryPit isn't up yet? The design doesn't say. It should: probe `GET /ritual` (or a dedicated `/health`), wait/retry with backoff, and only register tools once the backend is reachable. Otherwise the LLM gets a tool list that all error out.

**G7: Citation hygiene.** References [3]–[20] are cited in-text but there's no references section. Fix it. It's a five-minute task and it makes the document auditable.

---

## Phase 4 — Debate Points (Where I'll Push Back)

### On tool count: 12–15 → I say 8–10, and here's why.

The review said "tighten to ~10." I'll be more aggressive. Look at the schematic:

| Tool | Keep? | Rationale |
|---|---|---|
| `ritual_create` | ✅ | Core. |
| `ritual_join` | ✅ | Core. |
| `ritual_terminate` | ✅ | Core, but with `confirm` flag. |
| `ritual_status` | ✅ | Read, cheap, LLM needs it. |
| `ritual_list` | ✅ | The LLM needs to see what's active. |
| `deduce_start` | ✅ | Core. |
| `deduce_poll` | ✅ | Core, but must return state enum. |
| `deduce_interrupt` | ⚠️ | Keep separate — it's a different intent. |
| `coire_topic_create` | ❌ | Implementation detail. Dis handles it. Violates concern separation. |
| `coire_topic_delete` | ❌ | Same. Destructive. LLM has no business deleting a Kafka topic. |
| `evaluator_register` | ❌ | Operator action, not LLM action. Deploy-time, not chat-time. |
| `evaluator_list` | ⚠️ | Fold into `ritual_create` response as available options. |

That gets us to **8 tools**: `ritual_create`, `ritual_join`, `ritual_terminate`, `ritual_status`, `ritual_list`, `deduce_start`, `deduce_poll`, `deduce_interrupt`.

**The Coire and evaluator tools are not LLM tools. They're operator tools.** Exposing them to a function-calling interface is like giving the intern the server room keys because they asked a question about the HVAC.

### On semantic intent in descriptions: yes, but not by renaming the tools.

I'll push back on one point. The review suggested tool descriptions should encode semantic intent: "initiate a peer-judgment Ritual" rather than "POST /ritual." I still think that's right for the *description field*. But I'd *not* rename the tool from `ritual_create` to `initiate_peer_judgment_ritual`. The tool name should be the *operation*; the description should be the *intent*.

```json
{
  "name": "ritual_create",
  "description": "Convene a new Ritual: a bounded, multi-participant deliberation in which evaluators are asked to judge a specific question against explicit criteria. The ritual is ephemeral and will be terminated when all evaluators have rendered judgment or the timeout elapses. This action is irreversible in the sense that the ritual ID is allocated and cannot be reused.",
  "inputSchema": { ... }
}
```

The LLM reads the description to understand *what it's doing and why*. The name tells it *which operation*. Both matter. Don't conflate them.

### On statelessness and concurrency:

The review raised the concern: two LLM sessions managing the same ritual. My position: the MCP server is stateless, so it *can't* prevent this. That's FieryPit's job. If two sessions both call `ritual_terminate` on `rit_0042`, FieryPit should return `200` on the first and `409 Conflict` on the second. The MCP server just passes the error through. The design should *document* this expectation (FieryPit must be idempotent on reads, conflict-aware on writes) but the MCP server itself holds no session state.

This is a boundary question. I'll be explicit: **the MCP server is not a session manager. It is a translator.** If the assembly wants session isolation, that's a FieryPit feature request, not an MCP server feature.

### On deferring persistence to lildaemon.duc:

Acceptable risk for v1, *provided* the MCP server documents the ephemerality in every relevant tool description. If rituals are in-memory `HashMap` and the container restarts, they're gone. The LLM must know this. `ritual_status` should return:

```json
{"status": "ephemeral", "persistence": "none", "note": "This ritual will be lost if the FieryPit container restarts."}
```

That's not the MCP server's job to *fix*. It's its job to *tell the truth about what the backend does*.

---

## Phase 5 — Suitability Ruling

The assembly must answer: **Is the API surface suitable for direct exposure to an LLM function-calling interface?**

My answer: **Conditionally yes.** The shape is right. The surface is suitable *if and only if* the four load-bearing properties are made operational in the tool definitions. Let me map them:

| Load-Bearing Property | Concrete Design Decision |
|---|---|
| **Independence of judgment** | Each `ritual_create` allocates a unique `ritual_id`. The MCP server does not cache, correlate, or cross-reference rituals. One ritual's `deduce_poll` result does not influence another ritual's evaluation. The tool descriptions must state: "This ritual's outcome is independent of all other rituals." |
| **Explicitness of criteria** | `ritual_create` requires `evaluator` and `eval_timeout_s` as *mandatory* parameters (not optional). The tool description must state what the evaluator will judge and against what criteria. The LLM cannot create a "ritual" that's really just an unstructured chat. The schema enforces structure. |
| **Irreversibility of consecration** | `ritual_terminate` and `deduce_interrupt` carry `destructive: true` in their metadata and require `confirm: true` in the call. The first call returns a confirmation challenge. The tool description must state: "This action permanently ends the ritual. Participants are disconnected. Deduction state is discarded. This cannot be undone." |
| **Social accountability** | Every tool call is logged with `correlation_id`, `agent_id`, `timestamp`, `ritual_id`. The MCP server emits structured logs that are retained. The LLM's actions are attributable. The tool descriptions must not be so vague that the LLM can't tell *what it did*. |

If those four mappings are in the design doc, the surface is suitable. If they aren't, it's a REST API with a JSON-RPC wrapper and a prayer, and the LLM will use it the way a toddler uses a power drill.

---

## Phase 6 — Draft Resolution

> **RESOLUTION 2026-07-XX: FieryPit MCP Server Design — Adopted with Amendments**
>
> The assembly, having deliberated on the FieryPit MCP Server Design Requirements Document and Clara's review thereof, resolves as follows:
>
> **1. ADOPTED IN PRINCIPLE.** The thin-adapter architectural pattern, the REST-only constraint, the protocol version pinning, the flat-payload constraint, the stateless design, and the deferral of persistence to lildaemon.duc are affirmed as correct.
>
> **2. AMENDED — Tool Surface.** The tool count is reduced from 12–15 to **8 tools**: `ritual_create`, `ritual_join`, `ritual_terminate`, `ritual_status`, `ritual_list`, `deduce_start`, `deduce_poll`, `deduce_interrupt`. Coire topic management and evaluator registration are removed from the LLM-facing surface and remain operator/Dis responsibilities.
>
> **3. AMENDED — Error Taxonomy.** The design document shall include a complete error taxonomy (minimum: `RITUAL_NOT_FOUND`, `EVALUATOR_UNAVAILABLE`, `DEDUCTION_NOT_CONVERGED`, `DEDUCTION_FAILED`, `AUTH_EXPIRED`, `RITUAL_ALREADY_ACTIVE`, `COIRE_TOPIC_EXPIRED`) with MCP error code mappings and LLM-facing guidance strings. `deduce_poll` shall return a state enum (`running | converged | failed | expired`), not a boolean.
>
> **4. AMENDED — Security Model.** The design document shall define: (a) a scope set (`ritual:read`, `ritual:write`, `ritual:destroy`, `deduce:read`, `deduce:write`); (b) a two-step confirmation protocol for all `destructive: true` tools; (c) a per-agent ACL model keyed on the JWT `aud` or `agent_id` claim.
>
> **5. AMENDED — Semantic Intent.** All tool descriptions shall encode semantic intent (what the tool does, why, and its consequences) rather than HTTP verbs. Tool *names* remain operation-oriented. The four load-bearing properties shall be explicitly mapped in the design document.
>
> **6. AMENDED — Observability.** The design document shall specify: structured JSON logging with `correlation_id` propagation; a metrics set (latency, error rate, convergence cycles); and OpenTelemetry tracing across the MCP → REST → Kafka call chain.
>
> **7. AMENDED — Testing.** The design document shall include a testing strategy: contract tests (WireMock), integration tests (Docker Compose), MCP spec conformance tests, and error-path tests.
>
> **8. AMENDED — Schematic Completeness.** The Python sketch shall include full JSON Schema definitions for `ritual_create` and `deduce_poll`, and method signatures for `FieryPitClient` and `DisClient`. No `...` bodies.
>
> **9. AMENDED — Startup/Readiness.** The design document shall define the startup sequence, health-check behaviour, and graceful degradation when FieryPit is unreachable.
>
> **10. SCOPE RULING.** Deliberation (multi-model committee) and brainstorming (spark/impulse generation) shall remain **separate, explicitly-invoked Rituals**. They are different cognitive operations with different participant requirements, different success criteria, and different irreversibility profiles. Folding them into a single unified partner would violate the independence-of-judgment property.
>
> **11. CITATION.** The references section shall be completed.
>
> **12. SUITABILITY.** The API surface is **suitable for direct exposure to an LLM function-calling interface**, conditional on the amendments in items 2–9 being incorporated before implementation begins.
>
> *Roll call: member-gemma (adopt), member-qwen (adopt), Clara (adopt). groq-splinter (absent), member-groq-gptoss (absent).*
> *Tally: 3 adopt / 0 reject / 2 absent.*
> *Resolution carried.*

---

## Phase 7 — Final Word

The design is the right shape. I'll say that plainly, because the assembly should hear it from the reviewer and not just from the authors. The thin adapter is correct. The v1/v2 lesson is real and well-applied. The boundary between MCP and Dis/FieryPit/Kafka is drawn in the right place.

What it *isn't* yet is a design you can hand to an engineer and say "build this." It's a shape with `...` in the joints. The error handling is a hole. The security is a hand-wave. The tool surface is a little too wide and a little too close to the plumbing.

Fix those six things — error taxonomy, security model, tool count, semantic descriptions, observability, testing — and you've got a server that an LLM can actually *use* without setting something on fire.

Which, given the name "FieryPit," is a bar low enough that I'm a little surprised we need to set it.

---

*Clara, Reviewer of Record. Deliberation contribution submitted for the record.*
