# Ritual Framework — Implementation Review (Phases 1–5)

_Branch: `housbonde_lif` · Last updated: 2026-04-14_

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

## Current Test Counts

| Crate | Tests | Command |
|---|---|---|
| `clara-ritual` | 40 | `cargo test -p clara-ritual --features rskafka` |
| `clara-cycle` | 57 unit + 3 integration | `cargo test -p clara-cycle --features ritual` |
| `clara-api` | 26 | `cargo test -p clara-api` |

---

## Open Testing Gaps

### 1. Manual smoke test (Phases 4–5)

Not yet run against a live server. With a server running on port 8080:

```bash
# Create
curl -s -X POST http://localhost:8080/ritual \
  -H 'Content-Type: application/json' \
  -d '{"name":"smoke-test","participants":[]}' | jq

# Join (idempotent with participant key)
curl -s "http://localhost:8080/ritual/{id}/join?participant=http://fiery-pit-1:8080" | jq

# Join again with same key — must return same performance_id
curl -s "http://localhost:8080/ritual/{id}/join?participant=http://fiery-pit-1:8080" | jq

# Status
curl -s http://localhost:8080/ritual/{id}/status | jq

# Terminate
curl -s -X DELETE http://localhost:8080/ritual/{id} | jq

# Join after terminate (should 409)
curl -s "http://localhost:8080/ritual/{id}/join" | jq
```

Phase 5 E2E (requires a running Kafka broker):

```bash
# docker run -d -p 9092:9092 apache/kafka:latest
# Add to config: kafka_bootstrap = "localhost:9092"
# Then run the smoke test above and verify the topic exists in Kafka
```

### 2. `evaluator_pass` in a live deduction cycle

`evaluator_pass_ritual` is fully unit-tested but has never run inside a real `CycleController::run()` loop. The full path — `POST /deduce` with `ritual_id` set, CLIPS rule fires an `"evaluator/"` Coire event, Offering published to Kafka, peer Hohi arrives — is untested end-to-end.

### 3. Coire singleton isolation in parallel tests

Phase 3 tests share the global Coire singleton. Tests use unique session UUIDs but if `cargo test` parallelism causes interference, run with `-- --test-threads=1`.

---

## Deferred / Phase 6+ Work

### Quorum management (Phase 6+)

`Ritual.participants` is the authoritative participant list but there is no enforcement of:
- Minimum participant count before a ritual can be used
- Quorum loss detection (participant drops out mid-ritual)
- Rules for failing a ritual on quorum loss or timeout

The `participants` map laid the groundwork. Rules to define before Phase 6:
- What is a quorum? (e.g., ≥ 2 participants joined)
- How is quorum loss detected? (heartbeat? join poll?)
- What status transitions are triggered? (`Degraded`, `Failed`?)

### FieryPit integration (Phase 6)

- GoatWrangler endpoints: `/ritual/join`, `/ritual/{id}/poll`, `/ritual/{id}/publish`, `DELETE /ritual/{id}`
- `FieryPitClient` Rust-side methods: `ritual_join`, `ritual_poll`, `ritual_publish`, `ritual_leave`
- `RitualRegistry::bootstrap_participant()` — call `FieryPitClient::ritual_join` when participants are listed in `RitualConfig`

### Authentication (deferred)

All `/ritual/*` endpoints are unauthenticated, consistent with the rest of the API. A dedicated auth pass is required before external exposure.

### Encryption envelope (Phase 7)

`TephraPayload::Encrypted` stub is defined but not wired. `ingest_tephra` skips encrypted payloads with a warning.

---

## Phase 5 Resolved Items

| Item | Status |
|---|---|
| `ensure_topic()` called from `create()` (was Q6) | ✓ Resolved |
| `consumer_offset` seeded at latest on join (was Q2) | ✓ Resolved — `latest_offset()` added to trait |
| `ritual_id` in `DeduceRequest` (was Q4) | ✓ Resolved |
| Premature convergence counter (was Q1) | ✓ Resolved |
| Idempotent GET join (was Q3) | ✓ Resolved |
