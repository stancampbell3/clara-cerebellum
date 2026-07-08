# Phase 6 — FieryPit Integration: Implementation Plan

_Branch: `housbonde_lif` · Created: 2026-04-15 · Status: **Approved — ready to implement**_

---

## Vocabulary Reference

| Term | Definition |
|---|---|
| **Offering** | A request sent to an Evaluator |
| **Tephra** | The response envelope from an Evaluator: `Evaluator.evaluate(offering: Offering) -> Tephra` |
| **Hohi** | A success response, carried inside a Tephra |
| **Tabu** | An error response, carried inside a Tephra |
| **TephraEnvelope** | The Dis/Kafka wire format wrapping any message on the ritual topic |

A Tephra normally contains either a Hohi or a Tabu; both can be present but that is
unusual. A Tephra containing a Tabu is always considered an error response.

On the Kafka topic, `TephraEnvelope.label` signals message type:
- `"offering"` — an Offering being broadcast to peer evaluators
- `"hohi"` — a Tephra that carried a successful Hohi
- `"tabu"` — a Tephra that carried a Tabu (error or timeout)

---

## Architecture (Option B — lildaemon owns its Kafka consumer)

```
Browser / Cobbler GUI
       ↕
FieryPit / GoatWrangler  (lildaemon, Python)
       ↕  confluent-kafka consumer + producer
   Kafka topic  (one per Ritual)
       ↕  RsKafkaClient
Dis / CycleController  (clara-cerebrum, Rust)
       ↕  evaluator_pass_ritual
   ClaraMindsplinter (another lildaemon instance)
```

Dis creates the Ritual and topic, registers FieryPit URLs as participants, then publishes
Offerings to the topic as the CLIPS/Prolog inference engine requests them.  Each FieryPit
instance runs its own autonomous Kafka consumer loop: poll → evaluate → publish Tephra
back to the topic.  Dis receives the Tephra and continues its convergence loop.  Neither
party is on the other's critical path after the join handshake.

---

## Part 1 — lildaemon (Python)

### 1.1  Dependency

Add to `requirements.txt`:
```
confluent-kafka>=2.3
```

### 1.2  `RitualParticipant` (`goat/models/RitualParticipant.py`)

Manages one ritual participation: Kafka consumer + producer for a single ritual topic.

```python
class RitualParticipant:
    def __init__(
        self,
        ritual_id: str,
        topic: str,
        bootstrap_servers: str,
        dis_domain: str,
        wrangler: GoatWrangler,
        evaluator_name: str | None = None,   # None → use currently focused evaluator
        session_stateful: bool = False,      # True → reuse a BleatSession across evals
        eval_timeout_s: float = 30.0,
    ): ...

    async def start(self): ...   # start background consumer task
    async def stop(self):  ...   # stop consumer, close Kafka clients

    async def _run_loop(self):
        """
        Poll topic → filter Offerings not produced by self →
        GoatWrangler.eval_async(offering) → publish Tephra (Hohi or Tabu) to topic.
        """
```

**Stateful vs stateless evaluator sessions:**
- `session_stateful=False` (default): each Offering is evaluated fresh; no conversational
  context carried between Offerings in the same ritual.
- `session_stateful=True`: a `BleatSession` is created at join time and reused; the
  evaluator accumulates context across Offerings in the ritual.

**Tephra labelling on publish:**
- `tephra.is_success()` → publish `TephraEnvelope` with label `"hohi"`, payload = `tephra.hohi.response`
- `tephra.is_error()` → publish `TephraEnvelope` with label `"tabu"`, payload = `tephra.tabu` details
- Evaluation timeout → publish `TephraEnvelope` with label `"tabu"`, payload = `{"error": "timeout", "timeout_s": N}`

**Filtering:** a participant must skip Offerings whose `producer_node` matches its own
`dis_domain`, and Offerings produced during a prior Tephra that it already responded to
(deduplicate by `tephra_id` seen set).

### 1.3  `RitualManager` (`goat/models/RitualManager.py`)

Singleton registry attached to the FastAPI app state.

```python
class RitualManager:
    _participants: dict[str, RitualParticipant]

    async def join(
        self,
        ritual_id: str,
        topic: str,
        bootstrap_servers: str,
        dis_domain: str,
        wrangler: GoatWrangler,
        evaluator_name: str | None = None,
        session_stateful: bool = False,
        eval_timeout_s: float = 30.0,
    ) -> RitualParticipant: ...

    async def leave(self, ritual_id: str) -> None: ...

    def get(self, ritual_id: str) -> RitualParticipant | None: ...
```

### 1.4  Endpoints (`goat/app/ritual/router.py`)

Both endpoints require `Authorization: Bearer <jwt>` (same JWT scheme as REPL sessions).

**`POST /ritual/join`**

Request body:
```json
{
  "ritual_id":         "4d9c0691-8c8c-4d25-8473-ef00bac1cda4",
  "topic":             "dis.local.ritual.4d9c0691-...",
  "bootstrap_servers": "localhost:9092",
  "dis_domain":        "dis.local",
  "evaluator":         "chanter",
  "session_stateful":  false,
  "eval_timeout_s":    30.0
}
```

Response `202 Accepted`:
```json
{
  "ritual_id": "4d9c0691-...",
  "status":    "joined",
  "evaluator": "chanter"
}
```

Errors: `409 Conflict` if already joined, `400` if evaluator unknown.

**`DELETE /ritual/{ritual_id}`**

Response `200 OK`:
```json
{ "ritual_id": "4d9c0691-...", "status": "left" }
```

Errors: `404` if ritual not found.

### 1.5  `Chanter` evaluator (`goat/evaluators/Chanter.py`)

Echo subclass used for integration testing.  Tags its response so tests can assert it
was the responder.

```python
class Chanter(EchoEvaluator):
    """
    Ritual-aware echo evaluator for integration testing.
    Echoes the Offering back as a Hohi, tagged with {"responder": "chanter"}.
    """
    def evaluate(self, offering: Offering) -> Tephra:
        base = super().evaluate(offering)
        if base.hohi:
            base.hohi.response["responder"] = "chanter"
        return base
```

Register in `Disdomain` so `evaluator_name="chanter"` resolves to `Chanter`.

---

## Part 2 — Dis (Rust)

### 2.1  `FieryPitClient` — two new methods

Both use the existing `reqwest::blocking::Client` pattern (sync, called from
`spawn_blocking`).

```rust
/// Register this Dis instance as a participant in `ritual_id`.
/// lildaemon starts its own Kafka consumer + evaluation loop.
pub fn ritual_join(
    &self,
    ritual_id:        Uuid,
    topic:            &str,
    bootstrap:        &str,
    dis_domain:       &str,
    evaluator:        Option<&str>,
    session_stateful: bool,
    eval_timeout_s:   f64,
) -> Result<(), FieryPitError>;

/// Deregister: lildaemon stops its consumer and discards state.
pub fn ritual_leave(
    &self,
    ritual_id: Uuid,
) -> Result<(), FieryPitError>;
```

### 2.2  `RitualRegistry::bootstrap_participant()`

Called from `RitualRegistry::create()` when `config.participants` is non-empty.
Runs in `spawn_blocking` (via `web::block` in the handler, same pattern as `create()`).

```rust
pub fn bootstrap_participant(
    &self,
    ritual_id:  Uuid,
    client:     &FieryPitClient,   // one per FieryPit URL
    topic:      &str,
    bootstrap:  &str,
    evaluator:  Option<&str>,
) -> Result<(), RitualError>;
```

`RitualRegistry::create()` iterates `config.participants`, constructs a
`FieryPitClient` for each URL, calls `bootstrap_participant()`.

### 2.3  `label::TABU` constant

Add to `clara-ritual/src/envelope.rs`:

```rust
pub const TABU: &str = "tabu";
```

Update `ingest_tephra` in `controller.rs` to handle both response labels:
- `HOHI` → decrement `pending_evaluator_responses`, write success context to Coire
- `TABU` → decrement `pending_evaluator_responses` (peer did respond, with error),
  assert a failure fact into Coire so CLIPS/Prolog rules can react

A Tabu still counts as "responded" — the peer is not silent, it failed.  The cycle
continues toward the limit; CLIPS rules may fire on the failure assertion and attempt
recovery or declare the goal unresolvable.

### 2.4  Evaluator patience / timeout

New fields on `CycleController` (ritual feature only):

```rust
evaluator_patience_cycles: u32,   // default: 10
cycles_without_hohi:       u32,   // reset to 0 whenever pending changes
```

In `has_converged`, before checking `pending_responses_zero`:

```rust
if self.pending_evaluator_responses > 0 {
    self.cycles_without_hohi += 1;
    if self.cycles_without_hohi >= self.evaluator_patience_cycles {
        // Peer is silent — assert failure into Coire, clear pending.
        self.assert_evaluator_timeout_tabu();
        self.pending_evaluator_responses = 0;
        self.cycles_without_hohi = 0;
    }
}
```

`assert_evaluator_timeout_tabu()` writes a `ClaraEvent` with origin
`"ritual/tabu-timeout"` and payload `{"error": "evaluator_timeout"}` so CLIPS/Prolog
rules can react.

### 2.5  CLIPS rule stub (for integration test)

Load into the deduction session via `seed_clips`:

```clips
; Fire when a fact signals that peer evaluation is needed.
(defrule ask-peer-evaluator
    (need-peer-eval ?goal)
    =>
    (coire-emit evaluator "ask-chanter"
        (create$ goal ?goal source "clara")))
```

The `coire-emit` call writes to the `evaluator/ask-chanter` Coire channel.
`publish_evaluator_events` drains it and publishes an Offering to the Kafka topic.

---

## Part 3 — Integration Test

### Scenario

```
ClaraMindsplinter lildaemon (port 6666)  +  Chanter lildaemon (port 6667)
       both joined as ritual participants via POST /ritual/join
Dis CycleController:
  - CLIPS rule asserts (need-peer-eval "is_prime(7)") on cycle 0
  - evaluator_pass_ritual publishes Offering to Kafka
  - Chanter consumer receives it, echoes Hohi with {"responder": "chanter", ...}
  - Dis receives Hohi TephraEnvelope, pending → 0
  - Coire writes hohi fact → cycle converges
```

### Test assertions

1. `result.status == Converged` — cycle did not hit max_cycles or timeout
2. `result.cycles >= 2` — pending counter blocked early convergence
3. Coire contains a `ritual/hohi` event whose payload includes `"responder": "chanter"`
4. The Kafka topic has exactly 2 messages: one `offering`, one `hohi`

### Infrastructure required

- Docker Kafka on `localhost:9092` (already running per Phase 5 smoke test)
- Two lildaemon instances (or two evaluator slots in one instance) registered as participants
- `kafka_bootstrap = "localhost:9092"` in `config/local.toml`

---

## Open Items (deferred)

| Item | Phase |
|---|---|
| Quorum management — minimum participants, quorum loss detection | Phase 7 |
| `session_stateful` evaluation across multi-turn rituals | Phase 7 |
| Cobbler / Dagda GUI wiring | Phase 7+ |
| Full rule-based FSA for evaluator selection and message coordination | Phase 7+ |
| Authentication hardening for external exposure | Pre-production |

---

## Implementation Order

1. `label::TABU` constant + `ingest_tephra` Tabu handling (Rust, small)
2. `evaluator_patience_cycles` timeout (Rust)
3. `FieryPitClient::ritual_join` / `ritual_leave` (Rust)
4. `RitualRegistry::bootstrap_participant()` + wiring into `create()` (Rust)
5. `RitualParticipant` + `RitualManager` (Python)
6. `/ritual/join` + `DELETE /ritual/{id}` endpoints + JWT guard (Python)
7. `Chanter` evaluator + Disdomain registration (Python)
8. CLIPS rule stub + integration test (both)
