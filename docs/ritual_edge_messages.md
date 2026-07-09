# Event/Hohi/Tabu Edge Messages — Implementation Write-up

**Status: implemented and verified live, 2026-07-09.**
Builds on `ritual_typed_edges.md` (landed 2026-07-05) and
`ritual_edge_auto_pipe.md` (offering auto-pipe, same day). Gives the three
non-offering edge `msgType` values — `event`, `hohi`, `tabu` — their own
runtime semantics; previously they generated only a comment.

## What changed, in one paragraph

An `event`/`hohi`/`tabu` edge S → T now generates a manual **emit** helper
on S in both pipe modes (`emit_<target>_<kind>/1`, calling the new
`caws_emit/4`) — a fire-and-forget, addressed, correlated message with
`expects_reply: false`, so it never registers a `PendingOffer` or blocks
convergence. `pipeMode: "auto"` additionally generates a forward-chaining
**tee**: a CLIPS trigger rule (or two, for `tabu` — see below) fires a
generated `caws_auto_tee_<edge>/1` wrapper that re-publishes an
already-cached inbound Hohi/Tabu/event payload onward to T, preserving the
original correlation id — this is what makes relay chains (A → B → C)
possible. On the receive side, T gets a generated CLIPS dispatch rule that
routes the incoming `ritual/<kind>` coire-event through the new
`caws_edge_message/3`, which asserts thread-local `user:edge_message(EdgeId,
Kind, PayloadDict)` and calls the user-overridable `on_edge_message/3` hook
— the same fact-plus-hook shape as `edge_result/3` / `on_edge_hohi`/
`on_edge_tabu`, unified into one hook since there's no reply/timeout
distinction to preserve. Delivery is **live-deductions only**: the
lildaemon `RitualParticipant` still skips non-`"offering"` labels before
addressing, so a participant process never receives these messages
directly — only a live Dis deduction acting as a graph node does.
Participant-side delivery is a deferred follow-up.

## The pieces

### clara-ritual — wire label
- `label::EVENT = "event"` + `MessageKind::Event`, additive to the existing
  offering/hohi/tabu/prolog_fact/clips_fire/clara_fy_hit/deduction_event
  set. There is no wire-level `tabu-timeout` label — see below.

### clara-prolog — `the_coire.pl` emit/tee/message library
- `caws_emit(Target, Topic, Kind, Payload)` — manual entry point; mints a
  fresh correlation id, memoized per `(Target, Topic, Kind, Payload)`
  (`caws_emit_sent/2`) so a re-run of the goal doesn't re-publish.
- `caws_emit_cid(Target, Topic, Kind, Payload, Cid)` — the core publisher
  shared by `caws_emit/4` (fresh cid) and `caws_tee/5` (preserved incoming
  cid); idempotent per `Cid` (`caws_emitted/1`) — wire-level dedup,
  load-bearing for the tee path.
- `caws_tee(EdgeId, Target, Topic, Kind, IncomingCid)` — auto-tee: memoized
  per `(EdgeId, IncomingCid)` (`caws_teed/2`), reads the cached inbound
  payload by kind (`hohi` → `caws_result`, `tabu` → `caws_failed`, `event`
  → `caws_message`), strips `_routing`, republishes preserving the cid.
- `caws_edge_message(EdgeId, Kind, Cid)` — receive dispatch: memoized per
  `(EdgeId, Cid)` (`caws_edge_msg_seen/2`), asserts `user:edge_message/3`
  and calls `user:on_edge_message/3` inside `ignore(catch(...))`.
- `caws_cache_by_origin` gained a `ritual/event` clause (asserting
  `caws_message/3`) and the existing `ritual/hohi`/`ritual/tabu` clauses now
  *additionally* assert `caws_message/3` alongside their existing
  `caws_result`/`caws_failed` — additive, so the offering auto-pipe path
  (`edge_result/3`) is untouched. `ritual/tabu-timeout` stays local-only and
  does not feed `caws_message` — see the tabu wire-label note below.
- `edge_message/3` is `thread_local`, not `dynamic`, for the same reason
  `edge_result/3` is (see `ritual_edge_auto_pipe.md`): SWI-Prolog engines
  share one global clause store, so per-deduction facts must be
  thread-scoped or they leak across Runs.

### clara-cycle — controller
- `CawsDirective` gained a `label` field: `caws_directive` reads `_caws.kind`
  (`"event"`/`"hohi"`/`"tabu"` → the matching wire label; absent → the
  legacy `offering` default) and `publish_evaluator_events` publishes with
  it instead of a hardcoded `OFFERING`.
- `ingest_tephra`'s Hohi/Tabu branch now falls through when
  `resolve_pending_offer` fails **and** the Tephra is a foreign
  performance's reply addressed to us (we're a tee/message target, not the
  original offerer) — ingesting it as a `ritual/hohi`/`ritual/tabu` message
  event. An own-performance failure (a self-echo, or a late reply after the
  offer already resolved or timed out) is still dropped, never resurrected
  as a message: `caws_await` checks `caws_result` before `caws_failed`, so a
  stale own-performance reply must not re-enter as a fresh signal.

### clara-cycle — transduction (`transduce_graph`)
Per `event`/`hohi`/`tabu` edge, in **both** pipe modes, on the source node:
- Manual emit helper: `emit_<target_label>_<kind>(Payload) :- [guard,]
  caws_emit('<target>', '<kind>/<topic>', <kind>, Payload).`

On the target node, in both modes:
- Dispatch rule `edge-<id>-on-message`, matched on `(origin
  "ritual/<kind>")`, `(topic ...)`, non-empty `correlation` → publishes
  `caws_edge_message('<id>', <kind>, '<cid>')`.

`pipeMode == "auto"` additionally, on the source node:
- Wrapper `caws_auto_tee_<id>(Cid) :- [guard,] caws_tee('<id>', '<target>',
  '<topic>', <kind>, Cid).` plus a catch-all clause.
- Trigger rule(s) matched on non-empty `correlation`, **no topic
  constraint** (mirroring the auto-pipe convention — the payload lookup is
  keyed by cid, not topic): one rule on `ritual/hohi` for a `hohi` edge, one
  on `ritual/event` for an `event` edge, and **two** for a `tabu` edge — one
  on `ritual/tabu`, one on the local `ritual/tabu-timeout` event. Both tabu
  triggers invoke the *same* wrapper, which always tees with `Kind = tabu`:
  there is no wire-level `tabu-timeout` label
  (`assert_evaluator_timeout_tabu` is local-only), so a timed-out reply
  a tee node observed is forwarded onward exactly as an ordinary Tabu.

Boolean qualifiers compile into both the emit helper and the tee wrapper,
same as the offering consult helper/pipe wrapper. Assertion qualifiers seed
a target-node fact identically for message and offering edges.

### Cobbler (dagda)
- Pipe-mode chips are no longer gated to `msgType === 'offering'` — every
  edge kind shows them, with per-kind hints (offering: auto-pipe-to-await;
  hohi/tabu/event: auto-tee-forwards). Newly drawn edges of *any* kind
  default to `pipeMode: 'auto'` (previously offering-only).
- `types.ts`/`graphSerializer.ts` were already generic over `msgType`; no
  changes needed there.

### lildaemon
- **No changes.** `peer_consulting_node_ids` already collects edge sources
  regardless of `msgType`, so event/hohi/tabu edge sources are already
  registered and addressed correctly. `RitualParticipant` still gates on
  `label == "offering"` before addressing a Tephra — participants never see
  these messages. This is an accepted, documented constraint: delivery of
  event/hohi/tabu messages to a lildaemon participant (rather than a live
  Dis deduction acting as a graph node) is a separate future follow-up.

## Runtime flow

**Manual emit**: authored/generated code calls `emit_<target>_<kind>/1` →
`caws_emit/4` writes an `evaluator/emit` Coire event in the same cycle →
`publish_evaluator_events` (same cycle, later phase) publishes the Tephra.
No `pending_offers` entry, so this never blocks convergence.

**Auto-tee** (2-cycle latency, same shape as auto-pipe): message ingested
cycle N (phase 5, dual-written to both mailboxes) → CLIPS trigger rule
fires N+1 (phase 3) → goal relayed (phase 4) → Prolog `caws_tee` executes
N+2 (phase 1), draining the cached payload and re-publishing → done within
N+2 (phase 5). A relay chain (A → B → C → A) terminates because each hop's
tee fires at most once per `(EdgeId, Cid)` (`caws_teed/2`), and the
terminal receive is idempotent per `(EdgeId, Cid)` (`caws_edge_msg_seen/2`).

## Known limitations (accepted)

- Participant-side (lildaemon) delivery is out of scope — see above.
- Uncorrelated incoming messages are not teed or dispatched (the generated
  rules require a non-empty correlation), same as offering auto-pipe.
- `tabu-timeout` is intentionally never a wire label; a tee node cannot
  distinguish "peer answered with an error" from "peer timed out" once the
  message reaches a third node — both arrive as an ordinary `tabu`.
- **A passive message-edge receiver has no held-open convergence primitive
  (discovered during live verification).** `has_converged` declares
  convergence as soon as both mailboxes are empty and the tableau is
  stable — offering edges hold that open via `pending_offers` (patience),
  but a node that *only* runs `wait_msg(P) :- edge_message(...)` with
  nothing else pending has nothing analogous, and will converge (give up)
  the moment its mailbox looks quiet for one cycle — which, over a real
  network, can easily be before the awaited message arrives. This was
  invisible in the Rust unit tests (the `InMemoryBroker` has no latency, so
  the full round trip completes inside the same few cycles that also keep
  the mailbox busy) but reproduces reliably over real Kafka. It isn't a
  defect in the emit/tee/dispatch logic itself — every existing "wait"
  pattern in this codebase either holds convergence open via
  `pending_offers` (the offering path) or isn't a bounded-cycle
  `CycleController` at all (a lildaemon participant's polling loop). A node
  that both emits/tees *and* does other real work (further consults,
  ongoing reasoning) won't hit this in practice; a deduction whose *only*
  job is passively listening for one message needs a workaround (e.g. an
  incidental outstanding consult) until a proper fix — a
  `pending_messages`-style hold, analogous to `pending_offers` — lands as a
  follow-up.

## Verification

- **Rust**: `cargo test -p clara-cycle --features ritual` (101, incl. 8 new
  transduction snapshot tests, 5 new controller unit tests for the
  directive-label mapping and ingest fallthrough, and 3 new keystone
  round-trips — `run_loop_event_edge_round_trip` drives a manual emit
  helper end to end between two `CycleController`s over the
  `InMemoryBroker`; `run_loop_event_tee_relay` and
  `run_loop_hohi_tee_forwards_reply` pin the auto-tee forwarding path,
  including the own-performance-echo/foreign-performance-fallthrough
  distinction from `ingest_tephra`), `cargo test -p clara-ritual`,
  `cargo test -p clara-prolog`, `cargo test -p clara-api`.
- **Cobbler Playwright** (`dagda/cobbler/frontend/e2e`): 12 passed (new:
  pipe-mode chips and emit/tee code preview for a non-offering edge, a
  freshly drawn event edge defaults to `auto`).
- **lildaemon**: full pytest, 938 passed, 2 skipped (unrelated: pylint not
  installed) — regression only, no lildaemon changes.
- **Live E2E** (Kafka + Dis, real broker — not the `InMemoryBroker`, two
  independent `POST /deduce` calls joined to the same live Ritual):
  - **Event-edge emit round trip**: node n1's generated
    `emit_n2_event/1` helper published a real, addressed `label::EVENT`
    Tephra; node n2 (joined first, per the offset-skip note above) received
    it via the generated `edge-e1-on-message` dispatch rule and converged
    with `edge_message(e1, event, _{msg: hello})` bound — confirming the
    new wire label round-trips correctly through the real `KafkaBridge`
    (not just the in-memory one the Rust tests use).
  - **Tabu auto-tee, timeout branch** (the W4 case): a node with an
    outstanding `caws_consult` to a peer that never replies hit its
    `evaluator_patience_cycles` timeout, firing the *local-only*
    `ritual/tabu-timeout` event; the generated
    `edge-e2-auto-tee-tabu-timeout` CLIPS rule invoked the same
    `caws_auto_tee_e2` wrapper a genuine reply would, and it published a
    real Tephra with wire label **`tabu`** (never a timeout-specific
    label) to a third node, which received it via `edge_message(e2, tabu,
    _{error: evaluator_timeout})` — confirming the W4 design (both tabu
    triggers converge on one wire label) end to end.
  - Both scenarios needed the "known limitations" workaround above (an
    incidental outstanding `caws_offer`/`caws_consult` to hold convergence
    open across real network latency) to observe delivery live — itself
    the main finding of this verification pass.
  - Not exercised live (covered instead by the Rust keystones/transduction
    snapshots, which pin the same generated code deterministically): the
    direct (non-timeout) hohi/tabu wire-reply tee trigger, a three-node
    event ring, and concurrent-Run cross-deduction isolation (the latter is
    structural — `edge_message/3` is declared `thread_local` exactly like
    `edge_result/3`, whose cross-deduction isolation was already live-
    verified in the auto-pipe follow-up).
