# Ritual Framework — Implementation Review (Phases 1–6)

_Branch: `housbonde_lif` · Last updated: 2026-04-15_

---

## What Was Built

### Phase 1 — Foundations (`clara-ritual`)

New workspace crate `clara-ritual` with no external broker dependency.

| File | Contents |
|---|---|
| `src/error.rs` | `RitualError`: `InvalidTopicName`, `TopicNotFound`, `BrokerError`, `Serialization` |
| `src/envelope.rs` | `TephraEnvelope`, `TephraPayload` (Plaintext / Encrypted stub), `RitualConfig`, `label` constants, TTL logic |
| `src/topic.rs` | `topic_name(dis_domain, ritual_id)` — normalises `/` → `.`, validates Kafka name constraints, returns `"{domain}.ritual.{uuid}"` |
| `src/broker.rs` | `KafkaBridge` trait + `InMemoryBroker` |

**Test coverage**: 18 unit tests across `envelope.rs`, `topic.rs`, `broker.rs`.

### Phase 2 — Registry & Handle

| File | Contents |
|---|---|
| `src/ritual.rs` | `Ritual` struct (incl. `participants: HashMap<String, Uuid>`), `RitualState` enum |
| `src/registry.rs` | `RitualRegistry::create / join / terminate / get_status / ensure_topic` |
| `src/handle.rs` | `RitualHandle::publish_event / poll_incoming`; `Clone` shares `consumer_offset` Arc |

`AppState` gained `pub ritual_registry: Arc<RitualRegistry>`, initialised in `server.rs` before the actix runtime.

`RitualRegistry::join` accepts `participant_key: Option<&str>` — same key always returns the same `performance_id` (idempotent join for FieryPit peers).

**Test coverage**: 13 registry tests + 7 handle integration tests.

### Phase 3 — CycleController Integration

Changes to `clara-cycle/src/controller.rs` gated behind `ritual` feature flag:

- `ritual_handle: Option<RitualHandle>` field + `with_ritual()` builder
- `pending_evaluator_responses: usize` counter — incremented on every Offering published, decremented on each Hohi ingested; `has_converged()` blocks while non-zero. Max-cycles termination is unaffected.
- `evaluator_pass_ritual()`: polls `poll_incoming()` → `ingest_tephra()` for each Tephra, then `publish_evaluator_events()`
- `ingest_tephra()`: unpacks `Plaintext` body → `ClaraEvent` with origin `"ritual/{label}"` → writes to global Coire
- `publish_evaluator_events()`: drains Coire events with prefix `"evaluator/"` → publishes as Offerings to Ritual topic

`clara-cycle/Cargo.toml`: `clara-ritual` is an optional dependency; `clara-api` enables it via `features = ["ritual"]`.

**Test coverage**: 8 integration tests in `controller.rs` under `#[cfg(all(test, feature = "ritual"))]`.

### Phase 4 — REST API

Routes:

```
POST   /ritual               → create_ritual()    201 { ritual_id }
GET    /ritual/{id}/join     → join_ritual()      200 { ritual_id, performance_id, topic, dis_domain }
                               query: ?participant=<stable-key>  (optional — makes join idempotent)
GET    /ritual/{id}/status   → ritual_status()    200 { ritual_id, state }
DELETE /ritual/{id}          → terminate_ritual() 200 { ritual_id, status: "terminated" }
```

`DeduceRequest` gained `ritual_id: Option<Uuid>`. When set, `start_deduce` calls `registry.join(ritual_id, None)` and attaches the handle via `controller.with_ritual()`.

### Phase 5 — RsKafkaClient

`KafkaBridge` trait gained two new methods implemented on both backends:

| Method | InMemoryBroker | RsKafkaClient |
|---|---|---|
| `ensure_topic(topic, partitions, replication)` | no-op | `ControllerClient::create_topic()`; `TopicAlreadyExists` silently ignored |
| `latest_offset(topic)` | `vec.len() as i64` | `PartitionClient::get_offset(OffsetAt::Latest)` |

`RsKafkaClient` (`clara-ritual` crate, `rskafka` feature):
- Dedicated single-threaded tokio runtime (same pattern as `FieryPitClient` — safe from `spawn_blocking`)
- Lazy `PartitionClient` cache per topic (partition 0, `UnknownTopicHandling::Retry`)
- `publish`: JSON → `rskafka::record::Record::produce`
- `poll`: `fetch_records(offset, 1..1MiB, 100ms)` → deserialize envelopes
- `RitualHandle` consumer offset seeded at `latest_offset()` on join — new handles skip history

`ClaraConfig.server.kafka_bootstrap: Option<String>`: when set, `server.rs` constructs `RsKafkaClient` and fails fast on connection errors; when absent, `InMemoryBroker` is used.

**Test coverage**: 40 unit tests in `clara-ritual` (incl. 3 new broker tests for `ensure_topic`/`latest_offset`); 57 cycle tests; all API tests pass.

---

### Phase 6 — FieryPit Integration

Cross-language bridge between Dis (Rust, `clara-cerebrum`) and FieryPit (Python, `lildaemon`)
that lets the CycleController delegate evaluations to peer lildaemon instances via Kafka.

#### 6.1  `label::TABU` constant (`clara-ritual/src/envelope.rs`)

Added `pub const TABU: &str = "tabu"` to the `label` module alongside the existing `OFFERING`
and `HOHI` constants.  Updated `ingest_tephra` to treat both `hohi` and `tabu` envelopes as
"peer responded" events — a Tabu is an error response, not silence:

```rust
if tephra.label == clara_ritual::label::HOHI
    || tephra.label == clara_ritual::label::TABU
{
    self.pending_evaluator_responses =
        self.pending_evaluator_responses.saturating_sub(1);
    self.cycles_without_response = 0;
}
```

#### 6.2  Evaluator patience / timeout (`clara-cycle/src/controller.rs`)

Three new fields on `CycleController` (all gated behind `#[cfg(feature = "ritual")]`):

| Field | Default | Purpose |
|---|---|---|
| `pending_evaluator_responses: usize` | 0 | Outstanding Offering count; blocks convergence while > 0 |
| `evaluator_patience_cycles: u32` | 10 | Consecutive silent cycles before timeout fires |
| `cycles_without_response: u32` | 0 | Counter reset whenever `pending` decrements |

New builder method: `pub fn with_evaluator_patience(mut self, patience: u32) -> Self`.

Patience check in `has_converged()` runs before the fixed-point test:

```rust
if self.pending_evaluator_responses > 0 {
    self.cycles_without_response += 1;
    if self.cycles_without_response >= self.evaluator_patience_cycles {
        self.assert_evaluator_timeout_tabu();   // writes ritual/tabu-timeout to Coire
        self.pending_evaluator_responses = 0;
        self.cycles_without_response = 0;
    }
}
```

`assert_evaluator_timeout_tabu()` writes a `ClaraEvent` with origin `"ritual/tabu-timeout"`
and payload `{"error": "evaluator_timeout"}` so CLIPS/Prolog rules can pattern-match and
implement recovery or declare the goal unresolvable.  The cycle is allowed to continue —
the timeout clears the pending counter so convergence can proceed.

#### 6.3  `FieryPitClient::ritual_join` / `ritual_leave` (`fiery-pit-client/src/lib.rs`)

Two new methods on the blocking `FieryPitClient`.  Both use the existing `reqwest::blocking::Client`
and must be called from a `spawn_blocking` context (same pattern as all other client methods).

```rust
pub fn ritual_join(
    &self,
    ritual_id:        uuid::Uuid,
    topic:            &str,
    bootstrap:        &str,
    dis_domain:       &str,
    evaluator:        Option<&str>,
    session_stateful: bool,
    eval_timeout_s:   f64,
) -> Result<Value, FieryPitError>

pub fn ritual_leave(&self, ritual_id: uuid::Uuid) -> Result<Value, FieryPitError>
```

The `RitualJoinRequest` struct serialises to the exact JSON shape the lildaemon
`POST /ritual/join` endpoint expects.  `ritual_leave` sends `DELETE /ritual/{id}`.

`uuid = { version = "1", features = ["v4", "serde"] }` was added to
`fiery-pit-client/Cargo.toml`.

#### 6.4  Participant bootstrapping (`clara-api/src/handlers/ritual_handler.rs`)

`create_ritual` now bootstraps any FieryPit URLs listed in `config.participants` immediately
after the ritual is registered.  The call happens inside the same `web::block` closure that
calls `registry.create()` — one blocking thread pool task handles both operations:

```rust
for url in &participants {
    let client = FieryPitClient::new(url.as_str());
    match client.ritual_join(ritual_id, &topic, bootstrap, &dis_domain,
                             None, false, 30.0) {
        Ok(_)  => log::info!("bootstrapped participant {} for ritual {}", url, ritual_id),
        Err(e) => log::warn!("failed to bootstrap participant {}: {}", url, e),
    }
}
```

Bootstrap failures are non-fatal — logged as warnings.  The `ritual_id` is still returned
to the caller and participants can rejoin later via `GET /ritual/{id}/join`.

Two new fields were added to `AppState`:

| Field | Type | Source |
|---|---|---|
| `dis_domain` | `String` | `config.server.dis_domain_id` (default: `"dis.local"`) |
| `kafka_bootstrap` | `Option<String>` | `config.server.kafka_bootstrap` |

#### 6.5  `RitualParticipant` (`goat/models/RitualParticipant.py`)

Autonomous Kafka consumer/producer for one Ritual.  Runs as an `asyncio.Task`; all blocking
Kafka calls (`poll`, `produce`, client creation, `close`) are dispatched via
`asyncio.get_event_loop().run_in_executor(None, ...)` to keep the event loop free.

Key behaviours:

- **Filtering**: skips envelopes whose `label != "offering"`, whose `producer_node` matches
  `self.dis_domain` (own echo suppression), or whose `tephra_id` is in `_seen_tephra_ids`.
- **Evaluation**: calls `self._wrangler.eval(offering)` wrapped in `asyncio.wait_for` with
  configurable `eval_timeout_s` (default 30 s).
- **Timeout**: a silent evaluator returns a `Tephra(tabu=Tabu(message="timeout", code=408))`
  which is published as a `"tabu"` envelope.
- **Publishing**: `tephra.is_error()` → label `"tabu"`, otherwise `"hohi"`.  TTL is hardcoded
  to 300 000 ms (5 min).
- **Shutdown**: `stop()` cancels the asyncio task and flushes the producer with a 5-second
  timeout via `executor`.

Consumer group id: `"ritual-{ritual_id}-{dis_domain}"`.  Consumer starts at `latest` offset
so it skips history published before it joined.

#### 6.6  `RitualManager` (`goat/models/RitualManager.py`)

Singleton registry (`get_ritual_manager()`) that maps `ritual_id → RitualParticipant`.  Uses
an `asyncio.Lock` for safe concurrent join/leave in the FastAPI async event loop.

| Method | Raises | Notes |
|---|---|---|
| `join(...)` | `ValueError` if already joined | Creates participant, calls `start()`, registers |
| `leave(ritual_id)` | `KeyError` if not joined | Pops from registry, calls `stop()` |
| `get(ritual_id)` | — | Returns `None` if not joined |
| `active_rituals()` | — | Returns list of joined IDs |

#### 6.7  REST endpoints (`goat/app/ritual/router.py`)

Both endpoints require `Authorization: Bearer <jwt>` via `Depends(get_current_user)`.

**`POST /ritual/join`** → `202 Accepted`

```json
// Request
{ "ritual_id": "...", "topic": "dis.local.ritual.{uuid}",
  "bootstrap_servers": "localhost:9092", "dis_domain": "dis.local",
  "evaluator": "chanter", "session_stateful": false, "eval_timeout_s": 30.0 }

// Response
{ "ritual_id": "...", "status": "joined", "evaluator": "chanter" }
```

Errors: `409 Conflict` (already joined), `400 Bad Request` (evaluator unknown via `disdomain.exists()`), `503` (GoatWrangler not initialised).

**`DELETE /ritual/{ritual_id}`** → `200 OK`

```json
{ "ritual_id": "...", "status": "left" }
```

Errors: `404 Not Found` (not joined to this ritual).

#### 6.8  `Chanter` evaluator (`goat/evaluators/custom/chanter.py`)

Echo subclass for integration testing.  Tags its Hohi response so consumers can assert
which evaluator responded:

```python
class Chanter(EchoEvaluator):
    def evaluate(self, offering: Offering) -> Tephra:
        tephra = super().evaluate(offering)
        if tephra.hohi is not None:
            tephra.hohi.response["responder"] = "chanter"
        return tephra
```

Registered in `bootstrap_default_evaluators()` as `"chanter"` with metadata
`{"type": "ritual", "builtin": True}`.

---

## Current Test Counts

| Component | Tests | Command |
|---|---|---|
| `clara-ritual` | 40 | `cargo test -p clara-ritual --features rskafka` |
| `clara-cycle` | 57 unit + 3 integration | `cargo test -p clara-cycle --features ritual` |
| `clara-api` | 26 | `cargo test -p clara-api` |
| lildaemon (excl. ws) | 738 passed | `python -m pytest --ignore=tests/test_ws_repl.py` |

---

## Manual Smoke Tests

### Dis ritual API (Phases 4–6)

With a Dis server running on port 8080:

```bash
# Create ritual — no participants yet
curl -s -X POST http://localhost:8080/ritual \
  -H 'Content-Type: application/json' \
  -d '{"name":"smoke-test","participants":[]}' | jq

# Create ritual with FieryPit participant (triggers bootstrap via POST /ritual/join)
curl -s -X POST http://localhost:8080/ritual \
  -H 'Content-Type: application/json' \
  -d '{"name":"smoke-test","participants":["http://localhost:6666"]}' | jq

# Join (idempotent with participant key)
curl -s "http://localhost:8080/ritual/{id}/join?participant=http://fiery-pit-1:8080" | jq

# Join again with same key — must return same performance_id
curl -s "http://localhost:8080/ritual/{id}/join?participant=http://fiery-pit-1:8080" | jq

# Status
curl -s http://localhost:8080/ritual/{id}/status | jq

# Terminate
curl -s -X DELETE http://localhost:8080/ritual/{id} | jq
```

### FieryPit ritual endpoints (Phase 6)

With a lildaemon server running on port 6666 and a JWT `$TOKEN`:

```bash
# Join a ritual (Dis would normally call this automatically on create)
curl -s -X POST http://localhost:6666/ritual/join \
  -H "Authorization: Bearer $TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{
    "ritual_id":         "4d9c0691-8c8c-4d25-8473-ef00bac1cda4",
    "topic":             "dis.local.ritual.4d9c0691-8c8c-4d25-8473-ef00bac1cda4",
    "bootstrap_servers": "localhost:9092",
    "dis_domain":        "dis.local",
    "evaluator":         "chanter",
    "session_stateful":  false,
    "eval_timeout_s":    30.0
  }' | jq

# Should return: {"ritual_id":"...","status":"joined","evaluator":"chanter"}

# Leave a ritual
curl -s -X DELETE "http://localhost:6666/ritual/4d9c0691-8c8c-4d25-8473-ef00bac1cda4" \
  -H "Authorization: Bearer $TOKEN" | jq

# Should return: {"ritual_id":"...","status":"left"}

# Rejoin after leave — 200 OK (not 409)
# Rejoin while still joined — 409 Conflict
# Join with unknown evaluator — 400 Bad Request
```

### Phase 5 E2E (requires a live Kafka broker)

```bash
# docker run -d -p 9092:9092 apache/kafka:latest
# Add to Dis config: kafka_bootstrap = "localhost:9092"
# Then run the Dis smoke test above and verify the topic exists in Kafka
# The FieryPit join will start an autonomous consumer on the topic
```

---

## Open Testing Gaps

### 1. Full round-trip with live Kafka

The path — `POST /deduce` with `ritual_id`, CLIPS rule fires an `"evaluator/"` Coire event,
Offering published to Kafka topic, lildaemon `RitualParticipant` consumes it, evaluates with
`Chanter`, publishes `"hohi"` envelope back, Dis ingests it via `ingest_tephra`, `pending → 0`,
cycle converges — has not been tested end-to-end.  Requires a running Kafka broker and at least
one lildaemon instance joined to the same ritual.

### 2. Tabu path under patience timeout

`assert_evaluator_timeout_tabu()` is unit-tested but has not been exercised with a real silent
peer.  A test could use `with_evaluator_patience(1)` and a mock broker that never delivers a
response to force the timeout in a single cycle.

### 3. Coire singleton isolation in parallel tests

Phase 3 tests share the global Coire singleton. Tests use unique session UUIDs but if `cargo test`
parallelism causes interference, run with `-- --test-threads=1`.

### 4. CLIPS rule stub + integration test (Phase 6, item 8 — deferred)

A CLIPS rule that writes to the `evaluator/` Coire prefix (triggering `publish_evaluator_events`)
has not yet been written.  The integration test scenario described in `phase6_implementation_plan.md`
(§ Part 3) is deferred pending live Kafka availability.

---

## Deferred / Phase 7+ Work

### Quorum management

`Ritual.participants` is the authoritative participant list but there is no enforcement of:
- Minimum participant count before a ritual can be used
- Quorum loss detection (participant drops out mid-ritual)
- Status transitions on quorum loss (`Degraded`, `Failed`)

### `session_stateful` evaluation

The `session_stateful` field is plumbed through to `RitualParticipant` but has no effect yet
(marked "reserved for Phase 7" in the code).  When true it should reuse a `BleatSession` across
all Offerings in the same ritual so the evaluator accumulates conversational context.

### Authentication hardening

`/ritual/*` Dis endpoints are unauthenticated, consistent with the rest of the API.  The
FieryPit endpoints are JWT-guarded.  A dedicated auth pass is required before external exposure
on both sides.

### Encryption envelope

`TephraPayload::Encrypted` stub is defined but not wired. `ingest_tephra` skips encrypted
payloads with a warning.

---

## Phase 6 Resolved Items

| Item from plan | Status |
|---|---|
| `label::TABU` constant | ✓ Added to `clara-ritual/src/envelope.rs` |
| Tabu handling in `ingest_tephra` | ✓ Tabu decrements pending counter (peer responded, with error) |
| Evaluator patience timeout | ✓ `evaluator_patience_cycles` (default 10) + `assert_evaluator_timeout_tabu()` |
| `FieryPitClient::ritual_join` | ✓ Implemented — calls `POST /ritual/join` |
| `FieryPitClient::ritual_leave` | ✓ Implemented — calls `DELETE /ritual/{id}` |
| Participant bootstrapping in `create_ritual` | ✓ Iterates `config.participants`, calls `ritual_join` per URL; failures non-fatal |
| `RitualParticipant` | ✓ Autonomous consumer/producer; all blocking Kafka calls in executor |
| `RitualManager` singleton | ✓ `join / leave / get / active_rituals`; asyncio.Lock for safety |
| `/ritual/join` (POST, JWT-guarded) | ✓ 202 Accepted; 409 if already joined; 400 if evaluator unknown |
| `DELETE /ritual/{id}` (JWT-guarded) | ✓ 200 OK; 404 if not joined |
| `Chanter` evaluator | ✓ EchoEvaluator subclass tagged with `{"responder": "chanter"}` |
| `confluent-kafka>=2.3` dependency | ✓ Added to pyproject.toml, setup.cfg, setup.py |
| CLIPS rule stub + integration test | ✗ Deferred — requires live Kafka environment |

---

## Phase 5 Resolved Items

| Item | Status |
|---|---|
| `ensure_topic()` called from `create()` (was Q6) | ✓ Resolved |
| `consumer_offset` seeded at latest on join (was Q2) | ✓ Resolved — `latest_offset()` added to trait |
| `ritual_id` in `DeduceRequest` (was Q4) | ✓ Resolved |
| Premature convergence counter (was Q1) | ✓ Resolved |
| Idempotent GET join (was Q3) | ✓ Resolved |
