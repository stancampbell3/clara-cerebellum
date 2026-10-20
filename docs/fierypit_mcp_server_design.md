# Designing an MCP Server for FieryPit Ritual Management

Based on the architecture and existing infrastructure documented in the context, here's a grounded design proposal for an MCP server that exposes FieryPit's Ritual management functionality.

## 1. Rationale & Precedent

The context makes clear that this is a live design question already under active consideration:

- The assembly resolution explicitly directs research into *"whether that surface is suitable for direct exposure to an LLM function-calling interface"* and calls for a JSON tool definition conforming to the OpenAI function-calling schema with parameters including `ritual_name`, `initial_goal`, and `participants` [9].
- A prior generation of MCP servers (`pitsnake`, `prolog_devil`, `clips_devil`, `ochecho`) was unwired on 2026-09-01 because they hit an MCP v1/v2 API mismatch and were never functional in the deployed container [4]. The stated plan was to *"bring Prolog/CLIPS/TTS access back as their own Ritual participants rather than as MCP servers on one splinter"* [4].
- The deployment shape mandates that FieryPit is driven exclusively through its REST API; evaluators are never constructed locally [6][8].

This means the MCP server should be a **thin protocol adapter** over the existing REST surface, not a re-implementation of the logic.

## 2. Architectural Position

```text
┌─────────────────────────────────────────────────────────┐
│  LLM / Agent (function-calling client)                  │
└──────────────────────┬──────────────────────────────────┘
                       │  MCP protocol (JSON-RPC / stdio or SSE)
┌──────────────────────▼──────────────────────────────────┐
│  MCP Server (new component)                             │
│  ┌───────────────────────────────────────────────────┐  │
│  │  Tool Registry (JSON schema per tool)             │  │
│  │  Auth: JWT minting / pass-through                 │  │
│  │  Response shaping → JSON_RESPONSE contract        │  │
│  └───────────────────────────────────────────────────┘  │
└──────────────────────┬──────────────────────────────────┘
                       │  HTTP (REST)
┌──────────────────────▼──────────────────────────────────┐
│  FieryPit (lildaemon) — dockerized, port 6666          │
│  POST /ritual/join · POST /evaluators/set · POST /evaluate│
│  GET /ritual · GET /ritual/{id}/status                  │
│  POST /deduce · GET /deduce/{id} · DELETE /deduce/{id}  │
│  Kafka consumer/producer loops (RitualManager)          │
└──────────────────────┬──────────────────────────────────┘
                       │  Kafka (dis.local.ritual.<ritual_id>)
┌──────────────────────▼──────────────────────────────────┐
│  Dis (clara-api) — topic naming, routing, /coire/topics │
└─────────────────────────────────────────────────────────┘
```

The MCP server sits between the LLM and the existing REST API. It does not replace FieryPit, Dis, or Kafka; it translates MCP tool calls into the HTTP calls the system already expects.

## 3. Tool Surface (Mapped from Existing Endpoints)

The context enumerates the full endpoint set [19]. Grouping them into MCP tools:

### 3a. Ritual Lifecycle

| MCP Tool | Maps To | Notes |
|---|---|---|
| `ritual_create` | `POST /ritual` | Parameters: `name`, `participants[]` (base URLs), `evaluator`, `eval_timeout_s`, `kafka_bootstrap`, `dis_url` [11][16] |
| `ritual_join` | `POST /ritual/join` (FieryPit) / `GET /ritual/{id}/join` (Dis) | Spawns a fresh evaluator instance per `node_id` via `RitualManager.join → GoatWrangler.spawn_evaluator` [4] |
| `ritual_status` | `GET /ritual/{id}/status` | Returns current state (e.g. `'active'`), inspectable via dedicated status endpoint [RITUAL entity] |
| `ritual_list` | `GET /ritual` | Lists all Ritual IDs the server is currently participating in [12] |
| `ritual_terminate` | `DELETE /ritual/{id}` | Triggers Kafka topic cleanup [KAFKA entity] |

### 3b. Deduction / Evaluation

| MCP Tool | Maps To | Notes |
|---|---|---|
| `deduce_start` | `POST /deduce` | Parameters: `ritual_id`, `initial_goal`, optional `context` [DEDUCTION entity] |
| `deduce_poll` | `GET /deduce/{id}` | Poll to convergence; budget for nested `/evaluate` wall-clock time [8] |
| `deduce_interrupt` | `DELETE /deduce/{id}` | |
| `deduce_resume` | `POST /deduce/resume` | Persisted deduction identified by UUID, resumable [DEDUCTION entity] |
| `deduce_snapshot` | `GET /deduce/{id}/snapshot` | |
| `deduce_trace` | `GET /deduce/{id}/trace` + `/{change_id}/entries` + `/{change_id}/dot` | Trace inspection [19] |

### 3c. Coire / Ad-Hoc Topics

| MCP Tool | Maps To | Notes |
|---|---|---|
| `coire_topic_create` | `POST /coire/topics*` (Dis) | Naming: `{dis_domain}.coire.{subject_path}` [3] |
| `coire_topic_publish` | `CoireTopicClient.publish()` | Gotcha: default `ttl_ms` is 60 s; durable saves need explicit longer TTL [3] |
| `coire_topic_poll` | `CoireTopicClient.poll()` | |
| `coire_topic_list` / `coire_topic_delete` | `GET`/`DELETE /coire/topics*` | |

### 3d. Evaluator Management

| MCP Tool | Maps To | Notes |
|---|---|---|
| `evaluator_set` | `POST /evaluators/set` | Register by name in the Disdomain [FIERYPIT entity] |
| `evaluator_list` | *(implied by Disdomain)* | Registered evaluators: `snek`, `clara_mind_splinter`, etc. [FIERYPIT entity] |

## 4. Authentication & Session Handling

- FieryPit mints short-lived JWTs (`iss=fierypit`, `aud=dis`, `exp=+5 min`) [FIERYPIT entity, relations]. The MCP server must either:
  - Mint its own JWT per session (Option B pattern) and attach it to outgoing REST calls, **or**
  - Pass through a user-supplied JWT from the LLM client.
- Dis joins rituals by *"attaching cached JWTs to requests"* [DIS entity].
- The MCP server should maintain a session token cache so that repeated tool calls within a single LLM turn don't re-mint on every request.

## 5. Response Contract

The context defines `JSON_RESPONSE` as the canonical output structure, containing fields such as `"result"` and `"prolog_solutions"` [JSON_RESPONSE entity]. The MCP server should:

- Normalize every tool response into a consistent envelope:

  ```json
  {
    "result": "...",
    "prolog_solutions": [...],
    "ritual_id": "a2b3c4d5-...",
    "state": "active",
    "cycles": 3,
    "converged": true
  }
  ```

- For deduction polling, surface the convergence status explicitly (e.g., the example that *"converged after 3 cycles"* [DEDUCTION entity]) so the LLM knows when to stop polling.
- For trace endpoints, return the structured entries and optional DOT graph [19].

## 6. Key Design Constraints from Context

| Constraint | Source |
|---|---|
| Never instantiate evaluators locally — always drive via REST | [6][8], FIERYPIT entity |
| Kafka consumer groups must use distinct `self_node_id` to avoid collisions | [KAFKA entity] |
| Nested `/evaluate` calls cost real wall-clock time; budget poll timeouts accordingly | [8] |
| `prolog_clauses` are session-isolated (`thread_local`); avoid name collisions with static `.pl` libraries | [8] |
| Ad-hoc Coire topics have no registry, no join step — any agent can create/publish/poll by subject path alone | [3] |
| Rituals are currently ephemeral (in-memory `RitualManager` HashMap); persistence in `lildaemon.duc` is planned | [16][17] |
| Payload keys stay flat (no nested options dict through `caws_offer`) to avoid FFI serialization gaps | [4] |
| MCP v1/v2 mismatch was the failure mode of the prior MCP servers — pin the protocol version explicitly | [4] |

## 7. Implementation Sketch (Python, per the Resolution)

The resolution calls for a Python implementation that abstracts the Ritual lifecycle behind a single callable interface [9]. Concretely:

```python
# mcp_server/ritual_tools.py  (schematic)

class FieryPitRitualMCP:
    """Thin MCP adapter over FieryPit's REST API.

    Deployment shape: FieryPit is dockerized, driven via REST only.
    This class NEVER constructs an evaluator or RitualManager locally.
    """

    def __init__(self,
                 fierypit_base_url: str = "http://localhost:6666",
                 dis_url: str = "http://localhost:8080",
                 kafka_bootstrap: str = "localhost:9094",
                 jwt: str | None = None):
        self.fp = FieryPitClient(fierypit_base_url, jwt)   # REST wrapper
        self.dis = DisClient(dis_url, jwt)                  # REST wrapper
        self.kafka_bootstrap = kafka_bootstrap

    # --- Tool: ritual_create ---
    def ritual_create(self,
                      name: str,
                      participants: list[str],
                      evaluator: str | None = None,
                      eval_timeout_s: float = 30.0) -> dict:
        """POST /ritual on Dis."""
        ...

    # --- Tool: ritual_join ---
    def ritual_join(self,
                    ritual_id: str,
                    node_id: str,
                    evaluator: str) -> dict:
        """POST /ritual/join on FieryPit.

        Spawns fresh evaluator instance per node_id [4].
        """
        ...

    # --- Tool: deduce_start ---
    def deduce_start(self,
                     ritual_id: str,
                     initial_goal: str,
                     context: dict | None = None) -> dict:
        """POST /deduce on Dis."""
        ...

    # --- Tool: deduce_poll ---
    def deduce_poll(self,
                    deduce_id: str,
                    max_wait_s: float = 120.0) -> dict:
        """GET /deduce/{id} — budget for nested /evaluate wall-clock [8]."""
        ...

    # --- Tool: ritual_terminate ---
    def ritual_terminate(self, ritual_id: str) -> dict:
        """DELETE /ritual/{id} — triggers Kafka topic cleanup."""
        ...
```

The MCP server wraps these methods as JSON-schema-annotated tools conforming to the OpenAI function-calling schema, with at minimum the parameters `ritual_name`, `initial_goal`, and `participants` [9].

## 8. What to Avoid (Lessons from Context)

1. **Don't replicate the prior MCP server pattern** of embedding Prolog/CLIPS/TTS as MCP tools on a single splinter. The context explicitly states the plan is to bring those back *"as their own Ritual participants rather than as MCP servers on one splinter"* [4]. The MCP server should expose Ritual management (create, join, deduce, poll, terminate), not re-implement evaluator internals.

2. **Don't assume the LLM can see Kafka directly.** The MCP server is the only bridge; Kafka topics (`dis.local.ritual.<ritual_id>`) are internal to the Dis/FieryPit/Kafka triangle [KAFKA entity].

3. **Don't drop the `ttl_ms` on Coire publishes.** The 60-second default will silently expire durable messages [3].

4. **Don't nest options dicts in Offering payloads** — keep keys flat to avoid the `clara-prolog` FFI serialization gap [4].

## 9. Open Questions the Context Flags

- **Persistence:** Rituals are currently ephemeral in the `RitualManager`'s in-memory HashMap [16]. The MCP server should expose a `ritual_save` / `ritual_load` pair that maps to the planned `lildaemon.duc` CRUD (`ritual_configs` table with `status IN ('draft','active','terminated')`) [11][16][17], but this is not yet built.
- **Cobbler integration:** The planned Ritual editor in Cobbler (Dagda's browser front end) will expose a separate page for configuring participants and Offering I/O paths [16]. The MCP server and Cobbler should share the same `RitualConfig` model to avoid divergence [17].
- **Suitability assessment:** The resolution explicitly asks whether the API surface is *"suitable for direct exposure to an LLM function-calling interface"* [9]. The design above assumes yes, but the four load-bearing properties (independence of judgment, explicitness of criteria, irreversibility of consecration, social accountability) [5][14] suggest the MCP tool descriptions should encode semantic intent (e.g., *"initiate a peer-judgment Ritual"*) rather than raw HTTP verbs.

## Summary

The MCP server is best designed as a thin, protocol-translating adapter over FieryPit's existing REST API and Dis's coordination endpoints. It should:

- Expose ~12–15 tools covering Ritual CRUD, deduction lifecycle, Coire topic management, and evaluator registration.
- Handle JWT minting/caching transparently.
- Normalize responses into the `JSON_RESPONSE` contract.
- Respect the flat-payload and no-local-instantiation constraints.
- Pin the MCP protocol version to avoid the v1/v2 mismatch that killed the prior MCP servers.
- Defer to the planned `lildaemon.duc` persistence layer for durable Ritual configs rather than managing state itself.

This keeps the MCP server a concern-separation layer (consistent with architecture principle #1 [15]) that adds LLM-facing ergonomics without duplicating the coordination logic that already lives in Dis, FieryPit, and Kafka.
