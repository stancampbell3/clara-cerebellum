# Typed Edges & Proper Evaluator Messaging in Rituals — Implementation Write-up

**Status: implemented and verified live, 2026-07-05.**
Plan: `deduction_redux.md` (design), `ritual_multi_evaluator.md` (scenario),
`ritual_multi_evaluator_plan.md` (folded in as Phase 0 and superseded by this work).

## What changed, in one paragraph

Edges in a Ritual graph now *mean* something at performance time. An
`offering` edge `S → T` compiles (via edge transduction) into a source-side
Prolog consult helper and a target-side CLIPS reply hook; at runtime the
source's deduction publishes an addressed, correlated Offering Tephra over
the Coire/Kafka topic, the target node's participant evaluates it and replies
with a Hohi/Tabu echoing the correlation id, and the source's `caws_await/2`
either binds the reply or **fails on timeout** (timeout-to-false) so
convergence always completes. Convergence itself is blocked per outstanding
offer, not by a global counter.

## The pieces

### clara-ritual — typed envelope
- `TephraEnvelope` gained five optional, serde-defaulted routing fields:
  `source_node_id`, `target_node_id` (design-time graph addresses — deployment
  identity stays in `producer_node`), `correlation_id`, `topic_path`
  (logical hierarchical channel over the single per-Ritual Kafka topic), and
  `tags`. Old JSON still deserializes; `None` fields are omitted on the wire.
- `MessageKind` enum (`Offering | Hohi | Tabu | …`) + `kind()` accessor;
  the string `label` stays for wire compat.
- `RitualHandle::publish_event_routed` / `publish_body_routed(body, label,
  ttl, Routing)` stamp routing onto the envelope. `publish_body_routed`
  publishes a *clean user payload* as the body (the `_caws` transport block
  is stripped by the controller) so a plain evaluator's `_validate_input`
  sees exactly the authored payload, e.g. `{"prompt": ...}`.

### clara-cycle — correlated convergence
- `pending_offers: HashMap<Uuid, PendingOffer>` replaces the bare counter.
  Convergence requires `pending_offers.is_empty()`.
- `publish_evaluator_events` drains `evaluator/`-origin Coire events; events
  carrying a `_caws` block (written by `caws_offer/4` etc.) are published
  routed with their correlation id; `expects_reply: false` (squawks) skips
  the pending registration.
- `ingest_tephra` accepts a Hohi/Tabu only for this performance and a known
  correlation id (legacy uncorrelated replies resolve the oldest
  order-matched offer); winners are written to *both* mailboxes as
  `ritual/{label}` events with a `_routing {correlation_id, source_node_id,
  topic_path}` block merged into the payload.
- Per-offer patience (`DeduceRequest.evaluator_patience_cycles`, default 10)
  injects a synthetic `ritual/tabu-timeout` carrying the timed-out
  correlation id. Idle waiting cycles are paced (~250 ms) so patience is
  roughly a wall-clock bound.

### clara-prolog / clara-clips — the caws predicates
- New FFI: `coire_poll_ritual/2` (drains only `ritual/`-prefixed mailbox
  events, so it can never starve the relay) and `caws_uuid/1`.
- `the_coire.pl`: `caws_offer/4` (memoized per engine by
  `(target, topic, payload)` — safe across goal re-evaluation),
  `caws_await/2` (binds the correlated Hohi payload; **fails** on Tabu or
  timeout), `caws_consult/4` (offer + await), `caws_squawk/3`
  (fire-and-forget, never blocks convergence).
- `the_coire.clp`: `coire-event` template gained `topic` / `correlation` /
  `source-node` slots (populated from `_routing` by the CLIPS consume path);
  `caws-offer` / `caws-squawk` deffunctions mirror the Prolog side.

### clara-cycle transduction + Dis — edges become code
- `transduce_graph(graph_json)` consumes the Cobbler `graph_layout` and
  emits per-node `{prolog, clips}` snippets. Per `offering` edge `S → T`:
  - source Prolog: `consult_<target-label>(Payload, Result) :-
    caws_consult('<target-id>', 'consults/<edge-id|topicSuffix>', Payload, Result).`
  - source CLIPS: `edge-<id>-on-reply` defrule matching the correlated
    `ritual/hohi` coire-event and asserting `edge_replied('<id>')`.
  - qualifiers: `assertion` → fact clause in the target's Prolog; `boolean`
    → `clara_fy` guard (multi-word) or literal splice.
- `decorate_source` now injects the `the_rabbit`/`the_rat`/`the_coire`
  imports when absent.
- New Dis endpoint `POST /transduce/graph`; CLI `clara-transduction --graph`.
- Bug fix en route: `resolve_clips_source` used line-splitting for registered
  CLIPS sources; now uses `clara_clips::ffi::split_clips_constructs`, so
  multi-line constructs and comments survive registration.

### lildaemon — multi-participant addressing & activation
- `RitualManager` keys participants by `(ritual_id, node_id)`; multiple
  local nodes per process are now allowed (the old hard-409 is gone).
- `GoatWrangler.eval_slot(slot, offering)` dispatches to the named spawned
  evaluator, bypassing the process-global focus.
- `RitualParticipant`: skips envelopes addressed to another `target_node_id`;
  per-node Kafka group id `ritual-{ritual_id}-{self_node_id}` (true
  broadcast); replies echo `correlation_id` and address the offering's
  source; deduce-capable nodes with outgoing edges join the inner deduction
  to the same Ritual (`peer_consult`) and derive patience/max_cycles from
  `eval_timeout_s`.
- Activation calls Dis `POST /transduce/graph` and registers
  `generated + authored` source per node (content-addressed, idempotent);
  new `POST /ritual-configs/transduce-preview` passthrough for the UI.

### Cobbler (dagda) — typed edge UI
- `FlowEdge.msgType` (`offering|hohi|tabu|event`) replaces free-text label
  as source of truth (legacy `envelopeLabel` still read); `topicSuffix`,
  `tags`; node `slots {incoming, outgoing}` with chip toggles.
- Edge panel: msgType chips, topic suffix, tags, qualifier UI, and
  **Preview generated rules** — renders the per-node transduced Prolog/CLIPS
  exactly as activation will register it.
- All round-tripped through the opaque `graph_layout` JSON — no backend
  schema change.

## Wire contract (what a peer must do)

Reply to an Offering by publishing a Tephra with `label: "hohi"` (or
`"tabu"`), the same `performance_id`, and top-level `correlation_id` echoed
from the Offering envelope. Optional: `source_node_id` (yours),
`target_node_id` (the offerer), `topic_path`. Payload body is
`{"type": "plaintext", "body": {...}}`; the body surfaces to the awaiting
Prolog goal as a dict with `_routing` merged in.

## Verification

- **Rust**: full workspace `cargo test` green (clara-ritual 47 incl.
  envelope round-trip/compat; clara-cycle 75 incl. caws consult round trip,
  timeout-to-false, correlation mismatch drops; transduction snapshots).
- **lildaemon**: full pytest suite green (addressing, eval_slot, correlation
  echo, multi-node activation, transduce merge, preview endpoint).
- **Live E2E** (real Kafka + Dis + one lildaemon process + ollama/gemma4 +
  Groq): Cobbler config `Clara (clara_mind_splinter) → offering edge e1 →
  Clara/Groq (groq_evaluator)`, authored `reasoned_response/3` on the source
  calling the *generated* `consult_clara_groq/2`. Query: "Which chemical
  element has the symbol Fe? Answer in one word." Run converged with
  response: *"After reviewing both answers, the most concise and accurate
  single-word answer is **Iron**."* — a genuine synthesis of the local
  gemma4 answer and Groq's edge-consulted answer. Log trail (one run):
  Offering published `correlation=9519b52b…`, `pending_offers=1` blocks
  convergence → `ingest_tephra hohi resolved offer 9519b52b… — 0 still
  pending` → generated CLIPS hook asserts `edge_replied('e1')` → converged.
  The exact authored source, graph, and a runnable script are committed at
  `clara-prolog/docs/examples/typed_edges_e2e/`.
- **Cobbler UI (Playwright)**: new `dagda/cobbler/frontend/e2e/` suite
  (7 tests, all passing headless): login, canvas render, node slots panel,
  edge typed fields, transduce preview (asserts the generated
  `consult_clara_groq` / `edge-e1-on-reply` text renders), draw-edge →
  msgType dialog, save/reload round-trip of `topicSuffix`. Venv:
  `dagda/.venv-e2e`; run `../../.venv-e2e/bin/python -m pytest e2e -q` from
  `cobbler/frontend` with lildaemon (:6666), cobbler backend (:5001) and
  Vite (:5173) up. Screenshots land in `e2e/screenshots/`. A dev-only
  `window.__cobblerCy` hook in `GraphCanvas.tsx` exposes the editor
  instance for the tests.

## Operational notes (bit us during verification)

- Dis needs `LD_LIBRARY_PATH` pointing at
  `target/debug/build/clara-prolog-*/out/swipl-build/src` for libswipl.
- Dis's ritual registry is in-memory: restarting Dis orphans active configs —
  deactivate and re-activate after a restart.
- The toolbox `evaluate` cache in Dis memoizes LLM calls by payload; re-runs
  of the *same* query return cached answers.
- `POST /evaluate` (splinteredmind) uses the process-global focused
  evaluator — focus `clara_mind_splinter` (`POST /evaluators/set`) or the
  local side answers with the echo evaluator.
- lildaemon's pytest integration tests need the server *stopped* (duckdb
  file lock on `lildaemon.duc`).

## Out of scope (unchanged from plan)

Physical topic per channel; backpressure/replay; the CAWS language proper;
multi-hop cascading consults and fan-out merging; redundant-pool balancing
(sharing a `self_node_id` remains the opt-in); Elastic-style message
clustering.
