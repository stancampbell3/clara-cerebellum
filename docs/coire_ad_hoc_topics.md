# Ad Hoc Coire Topics & Ritual Context — Implementation Write-up

**Status: implemented and verified live, 2026-08-15 through 2026-08-16.**

Three related additions to the Coire/Ritual stack: (1) freeform, non-Ritual
Kafka topics reachable from Prolog and CLIPS — previously the Kafka bridge
(`clara-ritual`) was only constructible inside `clara-api` and only usable
via a formally-joined Ritual; (2) read-only predicates exposing the
current deduction's Ritual identity (`ritual_id`, `performance_id`,
`dis_domain`, `participants`) to authored rule/goal code, which was
previously entirely opaque to Prolog/CLIPS; and (3) the same ad hoc topic
capability extended to FieryPit Evaluators in `lildaemon` (Python), so work
happening outside Dis entirely — a web crawl, an LLM turn, a CLIPS/Prolog
reasoning step — can publish to or poll from a research topic that a Ritual
in Dis might also be touching. See "FieryPit side" below for (3).

## What changed, in one paragraph

`clara-ritual` gained a global `KafkaBridge` singleton (`init_global`/
`global`, mirroring `clara-coire`'s existing pattern) plus an `adhoc` module
implementing create/list/delete/publish/poll against **ad hoc topics** —
named `{dis_domain}.coire.{subject_path}`, distinct from Ritual topics
(`{dis_domain}.ritual.{uuid}`) — with no `RitualRegistry` participant/join
model required. That singleton is now initialized in three places:
`clara-api`'s server startup (so it's available during every deduction),
`prolog-repl`, and `clips-repl` — all three build the same
`KAFKA_BOOTSTRAP`-driven broker via a new shared `bridge_from_env()` helper.
New `coire_topic_*` predicates (Prolog) and `coire-topic-*` deffunctions
(CLIPS) expose it, so a research agent can spin up a topic, publish/poll on
it, and let other agents — in another process, another language, or both —
discover it later via `coire_topic_list`, with no prior coordination.
Separately, `CycleController::with_ritual` now seeds both engines with the
joined Ritual's identity (`ritual_id/1`, `ritual_performance_id/1`,
`ritual_dis_domain/1`, `ritual_participants/1` in Prolog; `(ritual-id)` etc.
in CLIPS), re-injecting the CLIPS globals after every `(reset)` the same way
the session UUIDs already are — giving evaluators the context to address
`caws_offer`/`caws_squawk` messages and name `coire_topic_*` paths sensibly
relative to their own Ritual and performance.

## The pieces

### clara-ritual — global bridge, ad hoc topic naming, admin ops

- `src/lib.rs`: `init_global(bridge, dis_domain)` / `global()` /
  `global_dis_domain()` — a process-wide `KafkaBridge` singleton, mirroring
  `clara_coire::global()`.
- `src/bridge.rs` (new): `bridge_from_env()` — reads `KAFKA_BOOTSTRAP`,
  builds `RsKafkaClient` if set (feature `rskafka`) else `InMemoryBroker`;
  `dis_domain_from_env()` reads `DIS_DOMAIN`, default `"dis.local"`. Shared
  by `clara-api/src/main.rs` and both REPLs so the bootstrap logic can't
  drift between call sites.
- `src/topic.rs`: `coire_topic_name(dis_domain, subject_path)` →
  `{domain}.coire.{subject}`, reusing the existing Kafka-name validation
  (`validate_topic_name`, now `pub(crate)`) without forcing the Ritual/UUID
  shape.
- `src/broker.rs`: `KafkaBridge` trait gained `list_topics()` /
  `delete_topic()`, implemented for `InMemoryBroker` (trivial) and
  `RsKafkaClient` (via rskafka's `Client::list_topics()` /
  `ControllerClient::delete_topic()`, tolerating "already gone" like
  `ensure_topic` already tolerates "already exists"). `InMemoryBroker::
  ensure_topic` now materializes an empty topic entry immediately (previously
  a no-op relying on first-publish) so `list_topics` reflects a just-created,
  not-yet-published-to topic — matching real broker `create_topic` semantics.
- `src/adhoc.rs` (new): the actual create/list/delete/publish/poll logic,
  taking the broker and Dis domain as plain parameters (not the global) so
  it's unit-testable against `InMemoryBroker` directly. `publish_topic`
  stamps `ritual_id`/`performance_id` as `Uuid::nil()` on the wire envelope —
  ad hoc traffic carries no Ritual identity, so consumers written for
  Ritual traffic can tell the two apart. `poll_topic_cursor` tracks an
  auto-advancing offset per `(consumer_id, topic)` pair (a process-wide
  `Mutex<HashMap>`) so independent consumers polling the same topic each get
  their own cursor.
- `src/clips_bridge.rs` (new, feature `ffi`): `extern "C" fn
  rust_ritual_topic_{create,list,delete,publish,poll,poll_from}` — the C-ABI
  glue linked into CLIPS's `userfunctions.c`, mirroring
  `clara-coire/src/clips_bridge.rs`'s existing pattern exactly.

### clara-api — HTTP admin surface (`/coire/topics*`)

Added in a follow-up commit so non-Prolog/CLIPS callers (any HTTP client,
in particular lildaemon's FieryPit Evaluators — see "FieryPit side" below)
can reach ad hoc topics without a live Prolog/CLIPS session:

- `src/handlers/coire_topics_handler.rs` (new): thin `web::block` wrappers
  around `clara_ritual::adhoc::{create_topic,list_topics,delete_topic}` +
  `clara_ritual::global()`, mirroring `handlers/ritual_handler.rs`'s
  `create_ritual`/`list_rituals`/`terminate_ritual` exactly (`InvalidTopicName`
  → 400, everything else → 500). Deliberately separate from
  `handlers/coire_handler.rs`, which is the unrelated `/cycle/coire/*`
  in-memory-mailbox surface (`clara_coire`, no Kafka).
- `src/routes/coire_topics.rs` (new) + `routes/mod.rs`: `POST /coire/topics`
  (body `{subject_path, num_partitions?, replication_factor?}` → 201
  `{topic, dis_domain, bootstrap_servers}`), `GET /coire/topics` (→
  `{topics: [...]}`, subject paths only), `DELETE /coire/topics/{subject:.*}`
  (greedy tail segment since subject paths contain dots; not an error if the
  topic is already gone). `bootstrap_servers` is the operator's real Kafka
  address (or `null` under the in-memory broker, dev mode) — the same
  "Dis hands out routing info, caller connects to Kafka directly" precedent
  `GET /ritual/{id}/join` already sets for Ritual topics.

### clara-prolog — foreign predicates + `the_coire.pl`

- `src/backend/ffi/ritual_bridge.rs` (new): registers `ritual_topic_create/1`,
  `ritual_topic_list/1`, `ritual_topic_delete/1`, `ritual_topic_publish/4`,
  `ritual_topic_poll/3`, `ritual_topic_poll_from/4` into the `the_coire`
  module, called from `ensure_prolog_initialized()` alongside the existing
  `coire_bridge` registration.
- `prolog-lib/the_coire.pl`: user-facing wrappers —
  `coire_topic_create/1`, `coire_topic_list/1`, `coire_topic_delete/1`,
  `coire_topic_publish/2,3` (payload is a dict/`json([K=V,...])`, same
  normalization `caws_offer` already uses; the 3-arg form takes an `Options`
  dict with `label`/`ttl_ms`/`target_node_id`/`source_node_id`/
  `correlation_id`/`tags`), `coire_topic_poll/2` (auto-advancing cursor keyed
  on this engine's own `coire_session/1`), `coire_topic_poll/4`
  (explicit-offset variant).
- Also adds `ritual_id/1`, `ritual_performance_id/1`, `ritual_dis_domain/1`,
  `ritual_participants/1` — thread-local facts
  (`ritual_id_fact/1` etc.) asserted only when `CycleController::with_ritual`
  seeds them; `ritual_id/1` and friends simply **fail** (not raise) when the
  deduction never joined a Ritual, and `ritual_participants/1` always
  succeeds, `[]` when there's nothing to report.

### clara-clips — UDFs + `the_coire.clp`

- `clips-src/core/userfunctions.c`: `RitualTopic{Create,List,Delete,Publish,
  Poll,PollFrom}Wrapper` static functions, registered via `AddUDF` as
  `ritual-topic-create`, `ritual-topic-list`, `ritual-topic-delete`,
  `ritual-topic-publish`, `ritual-topic-poll`, `ritual-topic-poll-from` —
  same low-level-UDF-then-deffunction-wrapper split the existing
  `coire-emit`/`coire-poll` already use. `ritual-topic-poll-from`'s offset
  argument is read as `INTEGER_BIT` and stringified via `snprintf` before
  crossing into Rust.
- `clp-lib/the_coire.clp`: `coire-topic-create`, `coire-topic-list`,
  `coire-topic-delete`, `coire-topic-publish` (fixed 3-arity — CLIPS
  deffunctions can't be overloaded like Prolog predicates, so the options
  argument is always required; pass `""` for defaults), `coire-topic-poll`,
  `coire-topic-poll-from`. CLIPS can't parse JSON natively, so `list`/`poll`
  return raw JSON text, same as `(coire-poll ...)` already does.
- Also adds `?*ritual-id*`, `?*ritual-performance-id*`, `?*ritual-dis-domain*`
  (default `""`) and `?*ritual-participants*` (default `(create$)`, an empty
  multifield) plus `(ritual-id)`, `(ritual-performance-id)`,
  `(ritual-dis-domain)`, `(ritual-participants)` deffunctions reading them.

### clara-cycle — seeding Ritual context into both engines

- `src/session.rs`: `DeductionSession::seed_ritual_context(ritual_id,
  performance_id, dis_domain, participants)` — asserts the four Prolog facts
  and evals the four CLIPS `(bind ...)` forms; stores the context so
  `reset_clips_wm()` (already responsible for re-injecting
  `?*prolog-session-id*`/`?*coire-session-id*` after any CLIPS `(reset)`)
  also re-injects the ritual globals.
- `src/controller.rs`: `CycleController::with_ritual(handle)` now calls
  `seed_ritual_context` using the handle's `ritual_id`, `performance_id`,
  `dis_domain`, and new `participants` field, before storing the handle.
  Builder-style — seeding failure logs a warning rather than propagating an
  error, consistent with the other infallible `with_*` methods.

### clara-ritual — `RitualHandle.participants`

- `src/handle.rs`: `RitualHandle` gained `pub participants: Vec<String>`,
  populated by `RitualRegistry::join()` from `Ritual.config.participants` —
  the roster a Ritual was *created* with (`RitualConfig.participants`), not
  the live join map (`Ritual.participants: HashMap<String, Uuid>`, which
  tracks issued `performance_id`s per participant key and stays internal to
  `RitualRegistry`).

### Wiring — deduction process, `prolog-repl`, `clips-repl`

- `clara-api/src/main.rs`: one call to `clara_ritual::init_global(
  ritual_broker.clone(), dis_domain)` right after building the broker —
  every deduction in that process now has ad hoc topics available
  automatically, with no change needed to `deduce_handler.rs`.
- `clara-prolog/src/bin/prolog-repl.rs`, `clara-clips/src/bin/clips-repl.rs`:
  each calls `clara_ritual::bridge_from_env()` +
  `clara_ritual::dis_domain_from_env()` + `clara_ritual::init_global(...)` at
  startup, right after the existing `clara_coire::init_global()`. Both
  `Cargo.toml`s gained a `clara-ritual` dependency (`rskafka` feature for
  Prolog; `ffi` + `rskafka` for CLIPS). `clara-clips/src/lib.rs` re-exports
  the new `rust_ritual_topic_*` symbols the same way it already re-exports
  `clara-coire`'s, to stop the linker garbage-collecting them (nothing in
  Rust code calls them by name — they're only ever reached from
  `userfunctions.c`).

## FieryPit side — lildaemon's CoireTopicClient

`lildaemon` (`/mnt/moonpool/Development/lildaemon`) is the Python FastAPI
server hosting CLIPS/Prolog/LLM Evaluators inside FieryPits. Its only
pre-existing Kafka code, `goat/models/RitualParticipant.py`, is scoped
entirely to formal, already-joined Ritual topics (one continuous
consume-evaluate-publish loop per `(ritual_id, topic, bootstrap_servers,
dis_domain)` — all four handed to it by Dis's `GET /ritual/{id}/join`), and
lildaemon already overloads the word "Coire" for something unrelated —
`KindlingEvaluator`'s `coire_emit`/`coire-poll` bridge and `SnekEvaluator`'s
`POST /cycle/coire/push`, both talking to clara-cerebellum's per-session
in-memory mailbox (`clara_coire`, no Kafka). New code keeps the ad hoc
Kafka-topic concept namespaced as `coire_topic_*` throughout, on both sides
of the stack, to keep the two mechanisms unambiguous.

Architecture mirrors the Ritual precedent exactly: Dis owns topic naming
and admin (the new `/coire/topics*` endpoints above); lildaemon resolves
`{topic, dis_domain, bootstrap_servers}` from Dis and then talks to that
Kafka broker **directly** via `confluent-kafka` for publish/poll — no HTTP
proxying of message traffic.

- `goat/models/CoireTopicClient.py` (new): one-shot (not a background
  loop) `ensure_topic`/`list_topics`/`delete_topic`/`publish`/`poll`.
  `publish` builds a `TephraEnvelope`-shaped dict matching
  `clara-ritual/src/envelope.rs` field-for-field (`ritual_id`/
  `performance_id` stamped as the nil UUID, same ad hoc-traffic convention
  `clara_ritual::adhoc::publish_topic` uses). `poll` uses **manual
  partition assignment + seek** (`assign()`/`seek()`, no `subscribe()`, a
  fresh throwaway `group.id` per call) rather than a consumer group, so ad
  hoc polls never join or interfere with a `RitualParticipant`'s
  auto-committed group on the same broker; an optional `consumer_id`
  tracks an auto-advancing per-`(consumer_id, topic)` cursor, mirroring
  `clara_ritual::adhoc::poll_topic_cursor`.
- `goat/app/dis_client.py`: `create_coire_topic`/`list_coire_topics`/
  `delete_coire_topic`, mirroring the existing `create_ritual`/
  `join_ritual`/`delete_ritual` methods.
- `goat/evaluators/custom/kindling_evaluator.py`: new `coire_topic_create`/
  `coire_topic_list`/`coire_topic_delete`/`coire_topic_publish`/
  `coire_topic_poll` Offering keys, dispatched in both `evaluate()` (via a
  small sync-over-async bridge, `asyncio.run` when no loop is already
  running) and `evaluate_async()` (awaited directly). `ClaraMindSplinter`
  (a `KindlingEvaluator` subclass) inherits the capability for free.
  Default poll `consumer_id` is the evaluator's own `shared_session_id`, so
  repeated `coire_topic_poll` calls behave like a per-evaluator stream
  cursor with no offset bookkeeping required by the caller.
- `goat/repl/fishes/StickFish.py`: matching `coire-topic-create <path>`,
  `coire-topic-list`, `coire-topic-delete <path>`,
  `coire-topic-publish <path> <json>`, `coire-topic-poll <path> [offset]`
  shorthand, alongside the pre-existing `coire-emit`/`coire-poll` (mailbox)
  shorthand.

## Example

```prolog
?- coire_topic_create('research.edge-detection').
true.
?- coire_topic_publish('research.edge-detection', _{finding: "sharpen kernel works"}).
true.
?- coire_topic_poll('research.edge-detection', Envelopes).
Envelopes = [_{label:event, payload:_{body:_{finding:"sharpen kernel works"}, ...}, ...}].
```

```clips
(coire-topic-create "research.edge-detection")
(coire-topic-publish "research.edge-detection" "{\"finding\":\"sharpen kernel works\"}" "")
(coire-topic-poll "research.edge-detection")
```

Published from one process (e.g. `prolog-repl`), the same topic is visible
from a completely separate `clips-repl` process over real Kafka — no shared
memory, no prior coordination, just the topic name.

```prolog
?- ritual_id(Id), ritual_participants(Ps).
Id = '3f9a...', Ps = ['http://fp1:8080', 'http://fp2:8080'].
```

```clips
(ritual-id)         ; => "3f9a..."
(ritual-participants) ; => ("http://fp1:8080" "http://fp2:8080")
```

Outside a joined Ritual, `ritual_id(Id)` fails and `(ritual-id)` returns
`""`.

## Verification

- `cargo test -p clara-ritual`: 71 tests (topic naming, `list_topics`/
  `delete_topic` on both broker impls, full `adhoc` module coverage
  including cross-consumer cursor isolation).
- `cargo test -p clara-cycle --features ritual`: 103 tests, including
  `with_ritual_seeds_ritual_context_into_prolog_and_clips` (real
  `RitualRegistry::create`→`join`→`with_ritual`, then queries both engines)
  and `without_ritual_context_predicates_reflect_absence`.
- `cargo build`/`test` across `clara-prolog`, `clara-clips`, `clara-api`
  together — clean.
- Live smoke test: published from a running `prolog-repl` process, polled
  from a separate `clips-repl` process, over a real Kafka broker — confirmed
  the full ad hoc, cross-process, cross-language round trip this feature was
  built for.
- `cargo test -p clara-api`: full suite green after adding
  `coire_topics_handler.rs`.
- lildaemon: `pytest` (984 passed) with everything mocked, plus a
  `@pytest.mark.integration` suite (`tests/test_coire_topic_client_integration.py`,
  skipped unless `KAFKA_BOOTSTRAP` is set, excluded from CI the same way the
  existing Ollama integration tests are) exercising `CoireTopicClient`
  against a real broker directly — publish/poll round trip, cursor
  advancement without redelivery, independent per-consumer cursors.
- Live cross-repo smoke test: rebuilt and restarted the `clara-api` and
  `lildaemon` containers (`docker compose`) from current source; created an
  ad hoc topic via `POST /coire/topics`; published through the
  containerized `lildaemon`'s `clara_mind_splinter` evaluator
  (`coire_topic_publish`); polled the same message back from two separate
  **host** processes, `prolog-repl` (`coire_topic_poll/2`) and `clips-repl`
  (`(coire-topic-poll ...)`) — both decoded the envelope correctly, with
  `producer_node` showing the `kindling-<uuid>` id the FieryPit Evaluator
  stamped it with. Confirms the full path: containerized Python Evaluator →
  real Kafka → separate host Prolog/CLIPS processes, no shared memory, no
  prior coordination beyond the topic's subject path.
