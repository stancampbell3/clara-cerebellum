# Offering Auto-Pipe Edges — Implementation Write-up

**Status: implemented and verified live, 2026-07-09.**
Builds on `ritual_typed_edges.md` (landed 2026-07-05). Folds in follow-up
#3 (mailbox hygiene) from `typed_edges_followups.md`.

## What changed, in one paragraph

An `offering` edge S → T can now carry `pipeMode: "auto"`: the edge itself
drives behavior at performance time instead of waiting for authored code to
call the generated `consult_<target>/2` helper. A generated forward-chaining
CLIPS rule matches every incoming Offering coire-event and fires a generated
Prolog wrapper (`caws_auto_pipe_<edge>/1`) that re-publishes the Offering
payload as an addressed, correlated Offering to the target — the
asynchronous analogue of `caws_consult/4`: offer **without** await. The
reply Tephra comes back as a Coire event; generated dispatch rules (emitted
for **both** pipe modes) route it through `caws_edge_reply/3`, which asserts
`edge_result(EdgeId, hohi|tabu, PayloadDict)` and calls the user-overridable
hooks `on_edge_hohi/2` / `on_edge_tabu/2`. Convergence is still guarded by
`pending_offers` + per-offer patience; the root goal is re-proved once the
reply (or timeout) lands, so authored clauses simply *consume* the fact.
`pipeMode` absent = `manual` — every previously saved graph behaves exactly
as before.

## The pieces

### clara-prolog — `the_coire.pl` async pipe/dispatch library
- Shared drain is now lossless: `caws_cache_by_origin` caches incoming
  `ritual/offering` events as `caws_offering/3` instead of dropping them,
  so `caws_await/2` draining first can no longer eat a piped payload.
- `caws_pipe(EdgeId, Target, Topic, Cid)` — forwards the cached Offering
  `Cid` as a fresh `caws_offer/4` to `Target`; memoized per
  `(EdgeId, IncomingCid)` (`caws_piped/2`), outgoing cid recorded in
  `caws_edge_offer/2` for timeout attribution.
- `caws_edge_reply(EdgeId, Kind, Cid)` with `Kind ∈ hohi | tabu |
  tabu_timeout` — reads the cached reply (`caws_result`/`caws_failed`),
  strips `_routing`, asserts `user:edge_result/3` once per `(EdgeId, Cid)`
  and runs the matching hook inside `ignore(catch(...))`. `tabu_timeout`
  is attributed only to edges that actually piped the timed-out offer.
- **`edge_result/3` is `thread_local`, not `dynamic` — load-bearing.**
  SWI-Prolog engines share one global clause store; a plain
  `:- dynamic user:edge_result/3.` leaked replies across deductions in the
  same Dis process (a fresh Run's root goal was satisfied at cycle 0 by the
  *previous* Run's Groq reply — observed live as a Saturn question answered
  with "Lima"/"Phobos" synthesis inputs). `thread_local` scopes the facts
  per engine, like every other caws cache.

### clara-cycle — transduction (`transduce_graph`)
Per offering edge, on the source node:
- **Both modes**: typed reply dispatch CLIPS rules
  `edge-<id>-on-{hohi,tabu}-result` (matched on `topic` + non-empty
  `correlation`) and `edge-<id>-on-timeout-result` (no topic — timeout
  events carry only a cid; Prolog attributes via `caws_edge_offer/2`).
  The legacy `edge-<id>-on-reply` → `edge_replied/1` hook and the
  synchronous `consult_<target>/2` helper are generated unchanged.
- **`pipeMode == "auto"` only**: Prolog wrapper
  `caws_auto_pipe_<id>(Cid) :- [guard,] caws_pipe('<id>', '<target>',
  '<topic>', Cid).` plus catch-all clause (a false boolean-qualifier guard
  is a quiet no-op), and the CLIPS pipe rule matching
  `(origin "ritual/offering")`. Boolean qualifiers compile into the wrapper
  exactly as they do into the consult helper.

### clara-cycle — controller
- New `DeduceRequest`/builder fields (ritual feature):
  - `initial_offering: Option<InitialOffering>` `{payload, topic_path
    (default "run"), source_node_id, correlation_id (fresh UUID when
    absent)}` — injected into **both** engine mailboxes as a
    `ritual/offering` event (with `_routing`) before cycle 0, so the pipe
    rules fire for the payload that started the run, not just for
    peer-published Offerings.
  - `self_node_id: Option<String>` — the graph node this deduction acts as.
- `ingest_tephra` mailbox hygiene (follow-up #3): non-reply Tephras are
  dropped when they echo our own `performance_id` (a deduction's published
  Offering returns on the shared topic — without this, auto-pipe would
  self-feed forever) or when addressed to a different node. Unaddressed
  Tephras and legacy callers keep ingest-everything.
- Timeouts are now CLIPS-visible: `assert_evaluator_timeout_tabu` writes
  the `ritual/tabu-timeout` event to **both** mailboxes, and
  `has_converged` holds convergence for the cycle that injected a timeout
  so the dispatch rules always get to run.

### lildaemon
- `register_node_source(..., edge_source=)`: generated snippets are
  registered for nodes that authored source **or are the source of an
  edge** — an unauthored middle node's pipe/dispatch rules are load-bearing.
  Plain edge-*target*-only nodes stay raw (their `_validate_input` contract
  is unchanged).
- Run (`run_ritual_config`): passes `self_node_id` (entry node) and
  `initial_offering` (`{"prompt": query}`, topic `run`, fresh UUID cid) to
  `start_deduce`.
- `RitualParticipant` deduce-qualification: always adds `self_node_id`;
  when `peer_consult`, adds `initial_offering` echoing the envelope's
  payload/cid/topic/source so the inner deduction's pipe memo dedups the
  Kafka copy of the same Offering.

### Cobbler (dagda)
- `FlowEdge.pipeMode?: 'auto' | 'manual'` (offering edges; absent = manual);
  round-trips through `graph_layout` without stamping `manual` onto legacy
  graphs. Newly drawn offering edges default to `auto`.
- Edge panel: auto/manual chips (offering edges only) with behavior hints;
  transduce preview shows the generated pipe/dispatch rules.

## Runtime flow (2-cycle pipe latency)

Offering ingested cycle N (phase 5) → CLIPS pipe rule fires N+1 (phase 3)
→ goal relayed (phase 4) → Prolog `caws_pipe` executes N+2 (phase 1) →
Offering published N+2 (phase 5, `pending_offers` blocks convergence) →
reply ingested to both mailboxes → dispatch rule → `edge_result/3` → root
goal re-proved. No convergence hole: relayed goal events count as pending.

## Known limitations (accepted)

- Uncorrelated incoming Offerings are not piped (the CLIPS rules require a
  non-empty correlation).
- Two auto edges to the same target sharing an authored `topicSuffix`
  collapse into one offer (`caws_offer`'s (Target, Topic, Payload) memo).
- `event`/`hohi`/`tabu` edges have their own emit/tee/message runtime,
  built on this substrate — see `docs/ritual_edge_messages.md`.

## Verification

- **Rust**: `cargo test -p clara-cycle --features ritual` (85, incl.
  4 new transduction snapshot tests and 6 new controller tests — keystone
  `run_loop_auto_pipe_round_trip` drives the exact generated snippets
  against a mock peer over the InMemoryBroker; `auto_pipe_timeout_asserts_
  tabu_edge_result` covers the timeout dispatch), `cargo test -p clara-api`.
- **lildaemon**: full pytest, 938 passed (new: unauthored edge-source
  registration, Run payload assertions, participant initial_offering echo).
- **Cobbler Playwright**: 10 passed (new: legacy edge shows `manual`,
  drawn offering edge defaults `auto`, preview shows
  `caws_auto_pipe_e1`/`caws_edge_reply` alongside unchanged
  `consult_clara_groq`).
- **Live E2E** (Kafka + Dis + lildaemon + ollama/gemma4 + Groq), example at
  `clara-prolog/docs/examples/typed_edges_e2e/` now authored with **no**
  consult call:
  - hohi: "Which element has atomic number 79?" → pipe published, Groq Hohi
    resolved the offer, clause 1 synthesized → *"…the best answer … is
    simply: Gold."*
  - tabu: a Groq fast-failure run produced a participant Tabu; dispatch
    asserted `edge_result(e1, tabu, _)` and clause 2 answered from Clara
    alone ("Jupiter") — degraded, didn't hang. Timeout Tabu additionally
    verified in Rust with patience 3 and a silent peer.
  - concurrency: two simultaneous Runs on one config ("ruby color" /
    "spider legs") converged with correct, distinct answers — no
    cross-performance leakage (correlation filtering + thread-local
    `edge_result`).
