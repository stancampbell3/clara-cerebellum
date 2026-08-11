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

## Architecture Notes

- **Thread-local vs Dynamic**: State predicates use thread_local to avoid cross-engine contamination; hooks remain dynamic as authored definitions
- **Rust Integration**: All publishes go through `coire_emit(Session, 'evaluator/*', Json)` which the Rust relay forwards appropriately
- **Cycle Controller**: The cycle controller's pending_offers entry blocks convergence until correlated Hohi/Tabu or timeout arrives

## Summary

The Coire module provides a robust distributed messaging layer for Prolog engines with:
1. Typed JSON event publishing via external relays
2. Correlation-based request/response patterns  
3. Auto-pipe and auto-tee forwarding chains
4. Idempotent operations through memoization caches
5. Extensible user hooks for custom handling

The design prioritizes correctness (no stale replies leaking across runs) while enabling flexible event-driven architectures in heterogeneous Prolog/CLIPS environments.
