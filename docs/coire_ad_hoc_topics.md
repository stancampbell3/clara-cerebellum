# Ad Hoc Coire Topics & Ritual Context — Implementation Write-up

**Status: implemented and verified live, 2026-08-15.**

Two related additions to the Coire/Ritual stack: (1) freeform, non-Ritual
Kafka topics reachable from Prolog and CLIPS — previously the Kafka bridge
(`clara-ritual`) was only constructible inside `clara-api` and only usable
via a formally-joined Ritual; and (2) read-only predicates exposing the
current deduction's Ritual identity (`ritual_id`, `performance_id`,
`dis_domain`, `participants`) to authored rule/goal code, which was
previously entirely opaque to Prolog/CLIPS.

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
