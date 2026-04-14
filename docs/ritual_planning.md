# Plan: Ritual Framework — Multi-Evaluator Coordination via rskafka

## Context

A **Ritual** is our metaphor for coordinating work and communication among a group of
Evaluators (FieryPit LilDaemons). The messaging backbone is
[rskafka](https://github.com/influxdata/rskafka), a pure-Rust, async, native Kafka
protocol client (no JVM, no Zookeeper dependency at the client layer). Coire events
generated during a deduction run (Prolog asserted facts, CLIPS rule firings, cache hits,
evaluator Offerings/Hohi) are published to Kafka topics so that peer evaluators
participating in the same Ritual can consume them.

Kafka was chosen over Walrus because:
- rskafka is a mature, production-grade crate with tokio-native async support
- Kafka has first-class Python client libraries (`confluent-kafka-python`, `kafka-python`)
  eliminating the need for any Rust HTTP-to-broker shim on the FieryPit side
- Broker infrastructure (distribution, replication, leader election) is handled by the
  Kafka cluster itself — no raft/gossip protocol to implement
- The `rskafka` API maps naturally to the `WalrusBridge` trait abstraction, requiring
  only a driver-layer swap

This is the first phase of a broader framework that will eventually support rule-gated
finite state automata specifying which evaluator talks to whom, when, and under what
conditions.

---

## Key Vocabulary (project-specific terms)

| Term | Meaning |
|---|---|
| **Ritual** | A named coordination session among N evaluators |
| **Performance** | One specific run of a Ritual (unique PerformanceId per run) |
| **Tephra** | A Ritual message envelope (TTL + label + payload + optional encryption) |
| **Offering** | An evaluate request sent from CLIPS to a peer evaluator |
| **Hohi** | An evaluate response returned by a peer evaluator |
| **Dis** | The clara-api server (the "common Dis server") |
| **FieryPit** | A lildaemon evaluator server (Python, runs LLM/Prolog/CLIPS evaluators) |
| **GoatWrangler** | The lildaemon component managing evaluator sessions and HTTP API |
| **Coire** | The in-process in-memory event bus (DuckDB) bridging Prolog ↔ CLIPS |

---

## Sequence

```
Evaluator A (FieryPit 1) → POST /deduce → Dis (clara-api)
  → DeductionSession created
  → CycleController.run():
      Prolog pass: asserts facts, runs goal
      Relay Prolog → CLIPS
      CLIPS pass: fires production rules
        → CLIPS rule fires Offering (evaluate request) tagged with ritual_id + TTL
        → CycleController.evaluator_pass():
            [NEW] polls incoming Tephras from Ritual consumer offset
            [NEW] publishes outbound Coire events to Kafka topic for this Ritual

On FieryPit 2 (Evaluator B):
  confluent-kafka consumer polls Kafka topic for ritual_id
  Unpacks TephraEnvelope → finds an Offering
  Calls evaluator B's evaluate() method
  Publishes response Tephra back to the same Ritual topic

Back on FieryPit 1 / Dis:
  evaluator_pass polls again next cycle
  Incoming Tephra found → unpacked → Hohi pushed into Coire as a pending event
  Prolog/CLIPS consume the Hohi on the next pass
```

---

## Architecture

### One producer per Dis server

A single `KafkaProducer` lives in `RitualRegistry` in `AppState`. All active Rituals
share this producer. Topics are namespaced to avoid collision.

### Topic naming

```
{dis_domain}.ritual.{ritual_id}
```

- `dis_domain`: stable identifier for this Dis server (e.g. `"dis.local"`, configurable).
  Dots in the domain name are kept as-is. Kafka topic names allow dots, hyphens, and
  alphanumerics; slashes are not allowed — hence `.` as separator throughout.
- `ritual_id`: UUID (with hyphens) identifying the Ritual definition.
- **One topic per Ritual** (not per Performance). `performance_id` and `label` live
  inside the `TephraEnvelope`. Consumers filter in-process. This avoids topic explosion
  across many short-lived Performances.
- Topic auto-creation: configure the Kafka broker with `auto.create.topics.enable=true`
  for dev; in production use `RitualRegistry::ensure_topic()` via the rskafka
  `ControllerClient` admin API to create topics explicitly with desired replication and
  partition counts before the first publish.

### Message envelope: `TephraEnvelope`

```json
{
  "tephra_id":       "uuid",
  "ritual_id":       "uuid",
  "performance_id":  "uuid",
  "label":           "offering",
  "ts_ms":           1731539200123,
  "ttl_ms":          60000,
  "producer_node":   "dis.local",
  "payload": {
    "type": "plaintext",
    "body": { /* ClaraEvent payload */ }
  }
}
```

Encryption envelope stub (Phase 7):
```json
"payload": {
  "type":       "encrypted",
  "cipher":     "XChaCha20-Poly1305",
  "nonce":      "...",
  "ciphertext": "...",
  "aad": {
    "schema":  "offering_v1",
    "domain":  "ritual.example"
  }
}
```

TTL enforcement is the **consumer's** responsibility: drop Tephras where
`now_ms - ts_ms > ttl_ms`.

Label values (non-exhaustive):

| Label | Direction | Description |
|---|---|---|
| `offering` | Dis → FieryPit peer | Evaluate request to a peer evaluator |
| `hohi` | FieryPit peer → Dis | Evaluate response from a peer evaluator |
| `prolog_fact` | Dis → subscribers | A new fact was asserted in Prolog |
| `clips_fire` | Dis → subscribers | A CLIPS production rule fired |
| `clara_fy_hit` | Dis → subscribers | A clara_fy cache hit occurred |
| `deduction_event` | Dis → subscribers | Generic deduction lifecycle event |

---

## New Crate: `clara-ritual`

Workspace member at `clara-ritual/`. No binary.

### `clara-ritual/Cargo.toml`

```toml
[package]
name = "clara-ritual"
version = "0.1.0"
edition.workspace = true
license.workspace = true
description = "Ritual coordination layer — multi-evaluator messaging via Kafka"
publish.workspace = true

[dependencies]
clara-coire   = { path = "../clara-coire" }
serde         = { version = "1", features = ["derive"] }
serde_json    = "1"
uuid          = { version = "1", features = ["v4", "serde"] }
log           = "0.4"
thiserror     = "1"
tokio         = { version = "1", features = ["sync", "rt"] }
rskafka       = { version = "0.5", features = ["transport-tls"] }
```

> Verify the latest published version of `rskafka` on crates.io before coding Phase 5.
> The `transport-tls` feature is optional; include it if the Kafka broker uses TLS.

### Module layout

```
clara-ritual/src/
  lib.rs           — re-exports; public surface
  error.rs         — RitualError (thiserror)
  envelope.rs      — TephraEnvelope, TephraPayload, label constants
  topic.rs         — topic_name() helper; topic parsing
  ritual.rs        — Ritual, RitualConfig, RitualState
  registry.rs      — RitualRegistry: create/join/terminate; owns the producer
  handle.rs        — RitualHandle: cheap Arc clone; publish/poll for CycleController
  broker.rs        — KafkaBridge trait + RsKafkaClient impl (wraps rskafka)
                     + InMemoryBroker for integration tests
```

### `KafkaBridge` trait

Abstraction so we can use an in-memory fake in tests and swap broker implementations
without touching higher-level code:

```rust
pub trait KafkaBridge: Send + Sync {
    /// Publish a serialized TephraEnvelope to the given topic.
    fn publish(&self, topic: &str, envelope: &TephraEnvelope) -> Result<(), RitualError>;
    /// Poll for new envelopes starting at `since_offset`. Returns (envelopes, next_offset).
    fn poll(&self, topic: &str, since_offset: i64) -> Result<(Vec<TephraEnvelope>, i64), RitualError>;
}
```

`RsKafkaClient` implements `KafkaBridge` using `rskafka::client::partition::PartitionClient`:
- `publish`: serializes `TephraEnvelope` to JSON, wraps as `rskafka::record::Record`,
  calls `PartitionClient::produce()`.
- `poll`: calls `PartitionClient::fetch_records(since_offset, ...)`, deserializes each
  record value as `TephraEnvelope`, returns `(envelopes, max_offset + 1)`.

`InMemoryBroker` is a `HashMap<String, Vec<TephraEnvelope>>` behind an `Arc<Mutex<_>>`,
used exclusively in integration tests.

### `RitualRegistry`

```rust
pub struct RitualRegistry {
    dis_domain: String,
    broker:     Arc<dyn KafkaBridge>,
    rituals:    Arc<RwLock<HashMap<Uuid, Ritual>>>,
}

impl RitualRegistry {
    pub fn create(&self, config: RitualConfig) -> Result<Uuid, RitualError>;
    pub fn join(&self, ritual_id: Uuid) -> Result<RitualHandle, RitualError>;
    pub fn terminate(&self, ritual_id: Uuid) -> Result<(), RitualError>;
    pub fn get_status(&self, ritual_id: Uuid) -> Option<RitualState>;
    /// Admin: ensure the Kafka topic exists with the given partition / replication config.
    /// No-op when using InMemoryBroker.
    pub fn ensure_topic(&self, ritual_id: Uuid, partitions: i16, replication: i16) -> Result<(), RitualError>;
}
```

`ensure_topic` uses `rskafka::client::controller::ControllerClient::create_topic()` and
is called by `create()` before publishing the first message.

### `RitualHandle`

Used by `CycleController`. Cheap to clone (wraps `Arc`).

```rust
pub struct RitualHandle {
    ritual_id:       Uuid,
    performance_id:  Uuid,
    dis_domain:      String,
    broker:          Arc<dyn KafkaBridge>,
    consumer_offset: Arc<AtomicI64>,   // i64 to match rskafka offset type
}

impl RitualHandle {
    /// Publish a ClaraEvent outbound to the Ritual topic.
    pub fn publish_event(
        &self,
        event: &ClaraEvent,
        label: &str,
        ttl_ms: Option<u64>,
    ) -> Result<(), RitualError>;

    /// Poll for incoming Tephras since the last consumed offset.
    /// Drops TTL-expired envelopes automatically.
    /// Advances the consumer offset on success.
    pub fn poll_incoming(&self) -> Result<Vec<TephraEnvelope>, RitualError>;
}
```

---

## Modifications to Existing Crates

### `clara-cycle/src/controller.rs`

1. Add field: `ritual_handle: Option<clara_ritual::RitualHandle>`
2. Add builder: `pub fn with_ritual(mut self, handle: RitualHandle) -> Self`
3. Replace the `evaluator_pass` stub:

```rust
fn evaluator_pass(&mut self) {
    let Some(handle) = &self.ritual_handle else {
        log::debug!("CycleController: evaluator_pass (no ritual)");
        return;
    };

    // 1. Drain incoming Tephras from peer evaluators
    match handle.poll_incoming() {
        Ok(tephras) => {
            for tephra in tephras {
                self.ingest_tephra(&tephra);
            }
        }
        Err(e) => log::warn!("evaluator_pass: poll failed: {}", e),
    }

    // 2. Publish outbound Coire events tagged for peer evaluators
    self.publish_evaluator_events(handle);
}
```

`ingest_tephra`: Unpack `TephraEnvelope`, extract the Offering/Hohi payload, write
as a new `ClaraEvent` into the Prolog mailbox so the next Prolog pass picks it up.

`publish_evaluator_events`: Poll `coire.poll_pending_with_origin_prefix(prolog_id, "evaluator/")`,
publish each as a Tephra with label `"offering"`, mark them processed.

**Thread model**: `evaluator_pass` is called from within `CycleController::run()` which
runs inside `tokio::task::spawn_blocking`. The `KafkaBridge::publish` and `poll`
methods on `RsKafkaClient` are synchronous wrappers around async rskafka calls, using
a dedicated single-threaded tokio runtime owned by `RsKafkaClient` (created once at
construction). This matches the existing blocking pattern for `CycleController` and
avoids nested-runtime panics (same lesson as `FieryPitClient`).

### `clara-api/src/app_state.rs`

Add to `AppState`:

```rust
pub ritual_registry: Arc<RitualRegistry>,
```

Initialize in `main()` alongside the Coire singleton, **before**
`actix_web::rt::System::new()`, for the same reason `FieryPitClient` is initialized
there — the `RsKafkaClient` internal runtime must not be dropped inside the actix runtime.

### `clara-api/src/handlers/ritual_handler.rs` (new)

```
POST   /ritual                    → create_ritual()   → returns ritual_id
POST   /ritual/{id}/join          → join_ritual()     → returns performance_id + topic name
DELETE /ritual/{id}               → terminate_ritual()
GET    /ritual/{id}/status        → ritual_status()
```

Wire into `clara-api/src/main.rs` route configuration.

### `clara-cycle/Cargo.toml`

Add: `clara-ritual = { path = "../clara-ritual", optional = true }`

Add feature: `[features] ritual = ["clara-ritual"]`

This keeps `clara-cycle` usable without pulling in Kafka dependencies for callers that
don't need Ritual support.

### `Cargo.toml` (workspace)

Add `"clara-ritual"` to `members`.

---

## FieryPit / Python Side (lildaemon)

Target: GoatWrangler (the lildaemon HTTP API layer).

Python has mature Kafka support — no Rust bridge shim required. Use
`confluent-kafka-python` (recommended: performant, maintained by Confluent, wraps
`librdkafka`) or `kafka-python` (pure Python, simpler install). Either works; prefer
`confluent-kafka-python` in production for throughput and rebalance handling.

New GoatWrangler endpoints:

```
POST /ritual/join        body: { ritual_id, performance_id, dis_domain, bootstrap_servers }
                         → starts a background consumer thread on the ritual topic
GET  /ritual/{id}/poll   → returns list of TephraEnvelope JSON for pending Offerings
POST /ritual/{id}/publish body: TephraEnvelope (Hohi response)
DELETE /ritual/{id}      → stops the consumer thread, unsubscribes
```

Implementation sketch:

```python
from confluent_kafka import Consumer, Producer

class RitualConsumer:
    def __init__(self, ritual_id, performance_id, bootstrap_servers):
        self.ritual_id = ritual_id
        self.performance_id = performance_id
        self.topic = f"{dis_domain}.ritual.{ritual_id}"
        self.inbox: list[dict] = []
        self._consumer = Consumer({
            "bootstrap.servers": bootstrap_servers,
            "group.id": f"fierypit-{performance_id}",
            "auto.offset.reset": "latest",
        })
        self._consumer.subscribe([self.topic])
        self._thread = threading.Thread(target=self._poll_loop, daemon=True)
        self._thread.start()

    def _poll_loop(self):
        while self._running:
            msg = self._consumer.poll(timeout=0.5)
            if msg and not msg.error():
                envelope = json.loads(msg.value())
                if envelope["performance_id"] == str(self.performance_id):
                    now_ms = int(time.time() * 1000)
                    if now_ms - envelope["ts_ms"] <= envelope["ttl_ms"]:
                        self.inbox.append(envelope)
```

The `FieryPitClient` crate (`clara-cerebrum` side) gains:

```rust
impl FieryPitClient {
    pub fn ritual_join(
        &self,
        ritual_id: Uuid,
        performance_id: Uuid,
        bootstrap_servers: &str,
    ) -> Result<Value, FieryPitError>;
    pub fn ritual_poll(&self, ritual_id: Uuid) -> Result<Vec<Value>, FieryPitError>;
    pub fn ritual_publish(&self, ritual_id: Uuid, envelope: &TephraEnvelope) -> Result<(), FieryPitError>;
    pub fn ritual_leave(&self, ritual_id: Uuid) -> Result<(), FieryPitError>;
}
```

The `bootstrap_servers` string is passed through from `ClaraConfig` → `RitualRegistry`
→ `RitualHandle` → `FieryPitClient::ritual_join`, so FieryPit peers connect to the
same Kafka cluster as Dis.

---

## Resolved Open Questions

1. **Kafka Rust client**: Use `rskafka` (pure-Rust, async, no JVM). Verify the latest
   version on crates.io before Phase 5. The `transport-tls` feature enables TLS for
   broker connections. `rskafka` supports producers, consumers, and admin operations
   (topic creation) via `ControllerClient`. No fallback required — Walrus is dropped.

2. **Python Kafka client**: `confluent-kafka-python` is the recommended choice (wraps
   `librdkafka`, production-grade, maintained). `kafka-python` is a pure-Python
   alternative if `librdkafka` is not available in the deployment environment. No Rust
   bridge shim needed — Kafka's Python story is mature.

3. **Which Coire events go outbound**: Events with `origin` prefix `"evaluator/"` are
   published to the Ritual topic. CLIPS production rules that want to invoke a peer
   evaluator write to Coire with origin `"evaluator/offering"`. Other event types
   (`prolog_fact`, `clips_fire`, `clara_fy_hit`) are published on future opt-in labels
   — the `label` field in `TephraEnvelope` lets consumers subscribe selectively without
   topic proliferation.

4. **Ritual ID lifecycle**: A Ritual is per-logical-collaboration-intent and may span
   multiple `/deduce` calls. `PerformanceId` is unique per `/deduce` run. The Kafka
   topic (one per Ritual) persists until `terminate_ritual()` is called or the topic
   is deleted via the admin API. Consumers filter by `performance_id` in-process.

5. **Consumer thread model**: `RsKafkaClient` owns a dedicated single-threaded tokio
   runtime. `publish` and `poll` on `RsKafkaClient` are synchronous (blocking), calling
   into that runtime via `runtime.block_on(...)`. This keeps `CycleController` purely
   blocking (consistent with `spawn_blocking` pattern) and avoids nested-runtime panics.
   On the FieryPit side, `confluent-kafka-python`'s `poll()` runs in a daemon
   `threading.Thread`, also consistent with existing GoatWrangler patterns.

6. **Backpressure**: `poll_incoming` is pull-based — the controller drives the cadence,
   naturally rate-limiting to the cycle frequency. Kafka's offset-based retention means
   slow consumers don't lose messages (within the broker's retention window). No
   additional backpressure design needed for Phase 1–6. Configure Kafka topic retention
   (`retention.ms`) to match the maximum expected Ritual duration.

---

## Phased Delivery

### Phase 1 — Foundations (no external broker required) ✓
- [x] New crate `clara-ritual` with types: `TephraEnvelope`, `TephraPayload`, `RitualConfig`
- [x] `KafkaBridge` trait + `InMemoryBroker` (append-only `Vec` per topic behind `Arc<Mutex<_>>`)
- [x] `topic_name()` helper: replaces `/` with `.`, validates Kafka name constraints
- [x] Unit tests for envelope TTL filtering and topic naming

### Phase 2 — RitualRegistry & Handle ✓
- [x] `RitualRegistry::create/join/terminate/ensure_topic` (ensure_topic no-op on InMemoryBroker)
- [x] `RitualHandle::publish_event` / `poll_incoming` (TTL drop, offset advance)
- [x] Add `ritual_registry: Arc<RitualRegistry>` to `AppState` (wired to InMemoryBroker initially)
- [x] Integration test: two handles on the same `InMemoryBroker`, publish/poll round-trip
- [x] Integration test: TTL expiry — expired Tephras are dropped by `poll_incoming`

### Phase 3 — CycleController Integration ✓
- [x] Add `ritual_handle: Option<RitualHandle>` + `with_ritual()` builder to `CycleController`
- [x] Replace `evaluator_pass` stub with full implementation
- [x] `ingest_tephra`: push Hohi payload into Prolog Coire mailbox
- [x] `publish_evaluator_events`: drain `"evaluator/"` Coire events → Tephra publish
- [x] Integration test: two `CycleController`s sharing an `InMemoryBroker` ritual, full offering/hohi round-trip

### Phase 4 — REST API ✓
- [x] `ritual_handler.rs`: create / join / terminate / status
- [x] Wire into `main.rs` routing
- [ ] Manual smoke test via `curl`

### Phase 5 — rskafka Client ✓ (pending E2E smoke test)
- [x] Add `rskafka` dependency to `clara-ritual/Cargo.toml` (optional, `rskafka` feature)
- [x] Implement `RsKafkaClient: KafkaBridge` with dedicated internal tokio runtime
- [x] `RsKafkaClient::new(bootstrap_servers)` → connects, creates `PartitionClient` (lazy per topic)
- [x] `ensure_topic` via `ControllerClient::create_topic()` — `TopicAlreadyExists` is silently ignored
- [x] `KafkaBridge::latest_offset()` — seeds new handle consumer_offset at latest to skip history
- [x] `KafkaBridge::ensure_topic()` — called from `RitualRegistry::create()` (was deferred from Phase 2)
- [x] Wire `RsKafkaClient` into `RitualRegistry` via `ClaraConfig.server.kafka_bootstrap`
- [x] `InMemoryBroker` now used when `kafka_bootstrap` is absent (logged at startup)
- [ ] End-to-end smoke test with a local Kafka broker (Docker: `confluentinc/cp-kafka` or `apache/kafka`)

### Phase 6 — FieryPit Producers/Consumers
- [ ] GoatWrangler: implement `/ritual/join`, `/ritual/{id}/poll`, `/ritual/{id}/publish`, `/ritual/{id}` DELETE
- [ ] Python `RitualConsumer` class with `confluent-kafka-python` background thread
- [ ] `FieryPitClient` ritual methods (`ritual_join`, `ritual_poll`, `ritual_publish`, `ritual_leave`)
- [ ] `RitualRegistry::bootstrap_participant()` — call `FieryPitClient::ritual_join` to register a peer FieryPit
- [ ] End-to-end test: Dis initiates a Ritual, FieryPit 2 receives an Offering, publishes Hohi back, Dis ingests it

### Phase 7 — Encryption Envelope
- [ ] Add `chacha20poly1305` crate to `clara-ritual`
- [ ] Implement `TephraPayload::Encrypted` encrypt/decrypt via `XChaCha20-Poly1305`
- [ ] Key distribution: shared secret in `ClaraConfig` for now; key distribution design deferred
- [ ] Unit tests for encrypt/decrypt round-trip

### Phase 8 — Finite State Automata (future)
- [ ] Design rule-gated FSA schema: states, transitions, guards (Prolog predicates or CLIPS rules)
- [ ] `RitualConfig` extended with FSA definition
- [ ] Evaluator routing driven by FSA state rather than static participant list
- [ ] Replace explicit `participants` list with FSA-discovered peers (may leverage gossip or a registry)
