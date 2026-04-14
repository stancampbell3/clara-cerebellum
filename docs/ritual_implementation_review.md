# Ritual Framework — Phases 1–4 Implementation Review

_Branch: `housbonde_lif` · Date: 2026-04-13_

---

## What Was Built

### Phase 1 — Foundations (`clara-ritual`)

New workspace crate `clara-ritual` with no external broker dependency.

| File | Contents |
|---|---|
| `src/error.rs` | `RitualError`: `InvalidTopicName`, `TopicNotFound`, `BrokerError`, `Serialization` |
| `src/envelope.rs` | `TephraEnvelope`, `TephraPayload` (Plaintext / Encrypted stub), `RitualConfig`, `label` constants, TTL logic |
| `src/topic.rs` | `topic_name(dis_domain, ritual_id)` — normalises `/` → `.`, validates Kafka name constraints, returns `"{domain}.ritual.{uuid}"` |
| `src/broker.rs` | `KafkaBridge` trait (`publish` / `poll`) + `InMemoryBroker` (append-only `Vec` per topic behind `Arc<Mutex<_>>`) |

**Test coverage**: 18 unit tests across `envelope.rs`, `topic.rs`, `broker.rs` — TTL edge cases, serde round-trips, offset advancement, clone sharing, topic isolation, negative offset clamping.

### Phase 2 — Registry & Handle

| File | Contents |
|---|---|
| `src/ritual.rs` | `Ritual` struct, `RitualState` enum (`Active` / `Terminated`) |
| `src/registry.rs` | `RitualRegistry::create / join / terminate / get_status / ensure_topic` |
| `src/handle.rs` | `RitualHandle::publish_event / poll_incoming`; `Clone` shares `consumer_offset` Arc |

`AppState` in `clara-api/src/handlers/session_handler.rs` gained `pub ritual_registry: Arc<RitualRegistry>`.  
Initialised in `clara-api/src/server.rs` before `actix_web::rt::System::new()` (same pattern as `FieryPitClient`).

**Test coverage**: 10 registry tests + 7 handle integration tests — join active/terminated ritual, terminate then join, status, TTL filtering, offset advancement, clone sharing.

### Phase 3 — CycleController Integration

Changes to `clara-cycle/src/controller.rs` gated behind `ritual` feature flag:

- Field: `#[cfg(feature = "ritual")] ritual_handle: Option<clara_ritual::RitualHandle>`
- Builder: `pub fn with_ritual(self, handle: RitualHandle) -> Self`
- `evaluator_pass()` dispatches to `evaluator_pass_ritual()` when the feature is active
- `evaluator_pass_ritual()`: polls `RitualHandle::poll_incoming()` → calls `ingest_tephra()` for each; then calls `publish_evaluator_events()`
- `ingest_tephra()`: extracts `TephraPayload::Plaintext` body, constructs `ClaraEvent` with origin `"ritual/{label}"`, writes to global Coire
- `publish_evaluator_events()`: polls Coire for events with prefix `"evaluator/"`, publishes each as a Tephra with label `OFFERING`

`clara-cycle/Cargo.toml`: `clara-ritual` is an optional dependency; `clara-api` enables it via `features = ["ritual"]`.

**Test coverage**: 5 integration tests in `controller.rs` under `#[cfg(all(test, feature = "ritual"))]` — ingest round-trip, encrypted skip, drain+publish, full evaluator_pass, noop without handle.

### Phase 4 — REST API

New files:
- `clara-api/src/handlers/ritual_handler.rs` — four handlers
- `clara-api/src/routes/ritual.rs` — re-exports

Routes added to `clara-api/src/routes/mod.rs`:

```
POST   /ritual               → create_ritual()    201 { ritual_id }
POST   /ritual/{id}/join     → join_ritual()      200 { ritual_id, performance_id, topic, dis_domain }
GET    /ritual/{id}/status   → ritual_status()    200 { ritual_id, state }
DELETE /ritual/{id}          → terminate_ritual() 200 { ritual_id, status: "terminated" }
```

Error responses: 404 (not found), 409 (terminated on join), 400 (invalid topic name), 500 (other).

---

## Testing Gaps

### 1. No manual `curl` smoke test yet

The Phase 4 checkbox for manual smoke testing is still open. Before Phase 5, run against a live `clara-api` instance:

```bash
# Create
curl -s -X POST http://localhost:8080/ritual \
  -H 'Content-Type: application/json' \
  -d '{"name":"test-ritual","participants":[]}' | jq

# Join (use ritual_id from above)
curl -s -X POST http://localhost:8080/ritual/{id}/join | jq

# Status
curl -s http://localhost:8080/ritual/{id}/status | jq

# Terminate
curl -s -X DELETE http://localhost:8080/ritual/{id} | jq

# Join after terminate (should 409)
curl -s -X POST http://localhost:8080/ritual/{id}/join | jq
```

### 2. `start_deduce` never calls `with_ritual()`

~~The bridge between `/deduce` and `/ritual` is unimplemented.~~ **Implemented.** `DeduceRequest` now has an optional `ritual_id: Option<Uuid>` field. When set, `start_deduce` calls `registry.join(ritual_id, None)` inside the blocking task and passes the resulting handle via `controller.with_ritual(handle)`. The `evaluator_pass_ritual` path is exercised in unit tests but has not yet been exercised inside a real `CycleController::run()` loop with a running server.

### 3. `evaluator_pass` timing within a real deduction cycle

In unit tests, `evaluator_pass` is called directly. In a real cycle inside `CycleController::run()`, it is called once per iteration after the CLIPS pass. The interaction between cycle pacing and Tephra round-trip latency (Kafka RTT for Phase 5) has not been exercised.

### 4. `ensure_topic()` is a no-op

`RitualRegistry::create()` does NOT call `ensure_topic()` — this is deferred to Phase 5. If Phase 5 wires rskafka but forgets to add the `ensure_topic()` call inside `create()`, the first `publish` will fail with a Kafka "unknown topic" error.

### 5. Integration test isolation depends on global Coire singleton

Phase 3 tests call `let _ = clara_coire::init_global();` to initialise the singleton, ignoring `AlreadyInitialized`. Tests use unique session UUIDs for isolation, but they share a single `InMemoryBroker` instance within a test. If `cargo test` parallelism causes interference between tests in different modules that both touch global Coire, spurious failures are possible. Run with `-- --test-threads=1` if flaky behaviour appears.

---

## Open Design Questions

### Q1 — Premature convergence while awaiting a Hohi ✓ Resolved

**Resolution**: `pending_evaluator_responses: usize` counter added to `CycleController` (ritual feature only).
- `publish_evaluator_events` increments by the number of Offerings successfully published.
- `ingest_tephra` decrements by 1 (`saturating_sub`) when a `HOHI`-labelled envelope arrives.
- `has_converged` returns `false` while the counter is non-zero.
- Max-cycles termination is unaffected — the cycle still returns `Err(MaxCyclesExceeded)` after exhausting its budget regardless of pending count.

3 new tests cover increment, decrement, and underflow protection.

### Q2 — Late joiner offset (Phase 5 implementation note)

Not an open question. `InMemoryBroker` replays from offset 0 by design. In Phase 5, `RsKafkaClient` should initialise the `PartitionClient` at the latest offset for new `CycleController` handles so Dis-side joins don't replay prior messages. FieryPit peers using `confluent-kafka-python` are unaffected — they already use `"auto.offset.reset": "latest"`.

**Phase 5 task**: Initialise `consumer_offset` from the broker's latest offset in `RitualHandle::new()` when backed by `RsKafkaClient`.

### Q3 — GET /ritual/{id}/join (idempotent) ✓ Resolved

**Resolution**: Changed `POST /ritual/{id}/join` → `GET /ritual/{id}/join`.

`Ritual` now tracks a `participants: HashMap<String, Uuid>` map (participant key → `performance_id`). The handler accepts an optional `?participant=<key>` query parameter:
- With a key: the same caller always receives the same `performance_id` (idempotent).
- Without a key: a fresh `performance_id` is generated (anonymous join — used internally by `start_deduce`).

**Deferred**: Quorum management (minimum participant count, quorum loss detection, ritual failure rules) is future work. The `participants` map lays the groundwork — it is the authoritative participant list.

### Q4 — `ritual_id` in `DeduceRequest` ✓ Resolved

**Resolution**: `DeduceRequest` now has `ritual_id: Option<Uuid>`. When set, `start_deduce` calls `registry.join(ritual_id, None)` (anonymous — fresh `performance_id` per run) inside the blocking task and passes the handle to `CycleController::with_ritual()`. Failed joins are logged and the deduction continues without a ritual handle.

### Q5 — No authentication on `/ritual/*` endpoints (deferred)

All endpoints remain unauthenticated. Authentication across all public routes is deferred until the API is stable. **Tracked**: API-wide authentication pass is a pre-production requirement.

### Q6 — `ensure_topic()` call missing from `create()` (Phase 5 task)

`RitualRegistry::create()` does NOT call `ensure_topic()`. For Phase 5, the call sequence must be:

```rust
pub fn create(&self, config: RitualConfig) -> Result<Uuid, RitualError> {
    let ritual_id = Uuid::new_v4();
    let topic = topic_name(&self.dis_domain, ritual_id)?;
    self.ensure_topic(ritual_id, 1, 1)?;   // ← add this before inserting
    // ... insert ritual ...
}
```

Without this, the first `publish_event` call against rskafka will fail with an unknown-topic error.

---

## Pre-Phase-5 Checklist

- [x] Run `cargo test -p clara-ritual` — 37 tests pass
- [x] Run `cargo test -p clara-cycle --features ritual` — 57 unit + 3 integration tests pass
- [x] Run `cargo test -p clara-api` — all tests pass
- [ ] Manual `curl` smoke test of all 4 `/ritual/*` endpoints against a running server
  - Note: `POST /ritual/{id}/join` is now `GET /ritual/{id}/join?participant=<key>`
- [x] Pending evaluator responses counter (Q1) — implemented and tested
- [x] `ritual_id: Option<Uuid>` in `DeduceRequest` (Q4) — implemented
- [ ] Add `ensure_topic()` call inside `RitualRegistry::create()` (Q6) — required for Phase 5
- [ ] Phase 5: initialise `consumer_offset` at latest broker offset for Dis-side handles (Q2)
- [ ] Phase 5: quorum management — minimum participant count, quorum loss detection, ritual failure rules (Q3 deferred)
