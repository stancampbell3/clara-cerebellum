# The Coire Module Summary

## Overview

`the_coire/1` is a SWI-Prolog module implementing a distributed event/message system with typed edge messaging, offering/request-response patterns, and fire-and-forget publishing. It integrates with a Rust-based relay (via `coire_emit`) to forward events across multiple engines including CLIPS peers.

## Core Concepts

### Thread-Local State
The module uses extensive thread-local predicates for per-engine state:
- **Session management**: `coire_session_id/1` tracks the active Prolog engine session
- **Offer tracking**: `caws_offer_sent/2`, `caws_result/2`, `caws_failed/2` cache outstanding offers and their outcomes
- **Edge memoization**: Various caches prevent duplicate publishing/replies within an engine

### Key Predicates

| Predicate | Purpose |
|-----------|---------|
| `coire_session(Id)` | Get/set current session ID (thread-local) |
| `coire_publish(Type, DataTerm)` | Publish typed event via relay |
| `caws_offer/4` | Addressed offering with correlation tracking |
| `caws_await/2` | Resolve outstanding offer result or timeout |
| `caws_consult/4` | Request/response round-trip to peer node |
| `caws_squawk/3` | Fire-and-forget publish (no reply expected) |
| `caws_pipe/4` | Auto-pipe incoming offering along an edge |
| `caws_tee/5` | Forward cached payload onward preserving CID |

### Event Types & Routing

The system handles three message kinds:
- **Hohi**: Successful responses (cached in `caws_result`)
- **Tabu**: Failed/rejected offers (cached in `caws_failed`)  
- **Event**: General messages via new cache (`caws_message`)

All events carry a `_routing` block with correlation IDs that gets stripped before user handlers see the payload.

### User Hooks

Three overridable hooks allow custom event handling:
```prolog
:- dynamic user:on_edge_hohi/2.      % Success callbacks
:- dynamic user:on_edge_tabu/2.      % Failure callbacks  
:- dynamic user:on_edge_message/3.   % General message handlers
```

Edge results are asserted as `user:edge_result(EdgeId, Kind, Payload)` for inspection.

### Idempotency Guarantees

The design ensures idempotent operations through memoization keyed on `(TargetNodeId, TopicPath, Payload)`:
- Re-running a goal reuses outstanding correlation IDs instead of publishing duplicates
- Results/failures are cached so resolved consults stay resolved across cycles
- Wire-level dedup via `caws_emitted/1` prevents duplicate publishes

### Built-in Handlers

```prolog
coire_dispatch_type(assert,  D) :- assertz(user:Fact).
coire_dispatch_type(retract, D) :- retract(user:Fact).
coire_dispatch_type(goal,    D) :- user:call(Goal).
```

These handle `assert`, `retract`, and `goal` types automatically; other types fall through.

### Consume Loop

`coire_consume/0` polls the inbound mailbox for events from `"relay-*"` origins (self-emitted `"prolog"` events stay local for CLIPS relay forwarding). Events are dispatched via `coire_dispatch_event/1`.

### Ad hoc topics (`coire_topic_*`)

Everything above operates on the local, per-session `Coire` mailbox (an in-memory/DuckDB store — no Kafka involved) or on a single Ritual's addressed traffic via `caws_offer`/`caws_squawk`/`caws_emit`, which requires a `RitualHandle` joined by the cycle controller. `coire_topic_*` is a third, independent path: it talks directly to `clara-ritual`'s global `KafkaBridge` singleton, publishing to and polling freeform topics named `{dis_domain}.coire.{SubjectPath}` — segregated from Ritual topics (`{dis_domain}.ritual.{uuid}`) by the `coire` designation. No Ritual, registry entry, or joined participant is required.

| Predicate | Purpose |
|-----------|---------|
| `coire_topic_create(SubjectPath)` | Ensure an ad hoc topic exists (idempotent) |
| `coire_topic_list(-Topics)` | List every ad hoc topic's subject path in the ambient Dis domain |
| `coire_topic_delete(SubjectPath)` | Delete an ad hoc topic (not an error if absent) |
| `coire_topic_publish/2,3` | Publish a JSON payload; the 3-arg form takes `label`/`ttl_ms`/routing options |
| `coire_topic_poll(SubjectPath, -Envelopes)` | Poll with an auto-advancing cursor, keyed per `(coire_session, SubjectPath)` |
| `coire_topic_poll/4` | Poll from an explicit offset — no cursor tracked |

This is what makes ad hoc, cross-agent conversation possible outside a Ritual: a research agent can `coire_topic_create/1` a topic, `coire_topic_publish/2` onto it, and any other agent (Prolog, CLIPS, or an external consumer) can `coire_topic_list/1` to discover it and `coire_topic_poll/2` to read it, with no prior coordination. The published `TephraEnvelope` stamps `ritual_id`/`performance_id` as nil UUIDs, signaling "no Ritual identity" to consumers written for both kinds of traffic. See `clara-ritual/src/adhoc.rs` for the underlying (unit-tested) Rust logic shared with the CLIPS side, and `clara-ritual/src/lib.rs` for `init_global`/`global` — the singleton injected into the deduction process, `prolog-repl`, and `clips-repl` alike.

## Architecture Notes

- **Thread-local vs Dynamic**: State predicates use thread_local to avoid cross-engine contamination; hooks remain dynamic as authored definitions
- **Rust Integration**: All publishes go through `coire_emit(Session, 'evaluator/*', Json)` which the Rust relay forwards appropriately
- **Cycle Controller**: The cycle controller's pending_offers entry blocks convergence until correlated Hohi/Tabu or timeout arrives
- **Coire vs clara-ritual**: "Coire" as used by this module is the local, in-memory/DuckDB mailbox (`clara-coire`) — Kafka itself lives one layer up, in the separate `clara-ritual` crate, reached only via `caws_*` (Ritual-scoped) or `coire_topic_*` (ad hoc) predicates, never directly

## Summary

The Coire module provides a robust distributed messaging layer for Prolog engines with:
1. Typed JSON event publishing via external relays
2. Correlation-based request/response patterns  
3. Auto-pipe and auto-tee forwarding chains
4. Idempotent operations through memoization caches
5. Extensible user hooks for custom handling
6. Ad hoc, non-Ritual Kafka topics (`coire_topic_*`) for freeform cross-agent conversation

The design prioritizes correctness (no stale replies leaking across runs) while enabling flexible event-driven architectures in heterogeneous Prolog/CLIPS environments.
