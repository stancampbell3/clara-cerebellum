# Typed Edges — Follow-up Work

Candidate next steps after the typed Ritual edges phase
(`ritual_typed_edges.md`, landed 2026-07-05). Ordered roughly by
recommended sequence; the first two items are the ones that bit us
repeatedly during live verification.

## 1. Persist (or self-heal) Dis's ritual registry — *recommended first*

**DONE (2026-07-08).** Chose the persistence option: the registry now
writes through to a `rituals` table in the existing CoireStore
(`clara-coire/src/store.rs`) and reloads it on boot
(`RitualRegistry::restore_from_store`, wired in `clara-api/src/server.rs`).
Active rituals — including keyed-join performance ids — survive Dis
restarts; terminated ones keep answering status/join truthfully (409, not
404). lildaemon auto-rejoin semantics deliberately deferred.

Original problem: Dis kept active Rituals in memory only. Every Dis
restart orphaned the active ritual configs: lildaemon still believed
they're active, but Runs failed until each config was deactivated and
re-created/re-activated by hand.

## 2. Evaluate-cache staleness for LLM calls

**DONE (2026-07-09).** The cache key (`clara-toolbox/src/ffi.rs`,
`cache_key`) is now namespaced by the current deduction id (the
`CURRENT_DEDUCTION_ID` thread-local the controller already installed):
entries never leak across runs, so a repeated query re-executes and a
`set_evaluator` issued by a later deduction actually reaches lildaemon
instead of being served from a previous run's cache — that side-effecting
call being memoized process-wide was the "stale echo after switching the
focused evaluator" symptom. Within one deduction the load-bearing
once-per-deduction memoization is unchanged (re-proved goals stay cache
hits). Neither of the originally suggested fixes was right: a TTL already
existed (`evaluate_cache_ttl_seconds`, 4h CarrionPicker sweep — kept, as
the memory bound) but still serves stale answers inside its window, and a
`ponder_*` opt-out would have broken the once-per-deduction invariant.
The "vary the query between runs" operational gotcha is obsolete.

## 3. Mailbox hygiene: don't ingest foreign Offerings

**DONE (2026-07-09),** folded into the offering auto-pipe iteration (see
`ritual_edge_auto_pipe.md`). `ingest_tephra` now (a) drops non-reply
Tephras whose `performance_id` is its own (self-echo suppression — required
once auto-pipe rules react to incoming Offerings, or a deduction would pipe
its own published Offering forever) and (b) when the deduction was given a
`self_node_id` (new `DeduceRequest` field, threaded from lildaemon), drops
non-reply Tephras addressed to a different `target_node_id`. Unaddressed
Tephras and legacy callers (no `self_node_id`) keep the old
ingest-everything behavior.

## 4. Dual-consumer race on the CLIPS mailbox (investigate)

**DONE (2026-07-09).** Both CLIPS-mailbox consumers are now origin-disjoint,
matching the Prolog mailbox's four filtered consumers:
`relay_clips_to_prolog` polls only `clips`-prefixed origins (the events the
coire-publish-* deffunctions emit for the relay), and the CLIPS engine's
`consume_coire_events` polls the complement via the new
`Coire::poll_pending_excluding_origin_prefix` (`ritual/*`,
`relay-prolog:*`, pushed events). The phase-order dependency is gone: a
`ritual/*` event pending when the relay runs is left untouched (previously
it would have been silently swallowed as `relay-clips:ritual/hohi` and the
generated `(coire-event ...)` dispatch rules would never fire), and a
pending `clips` goal event — whose data is Prolog, not CLIPS — can no
longer be eval'd by the CLIPS engine. Pinned by
`clara-cycle/tests/relay_filter_test.rs`.

## 5. Multi-hop consults — *next capability increment*

The machinery supports a chain (A offers to B; B's inner deduction, joined
to the same Ritual via `peer_consult`, offers to C) but it has never been
exercised. Needs: a 3-node Cobbler scenario, patience budgets that nest
sensibly (inner patience < outer patience), and an E2E test. Good demo
value.

## 6. Fan-out with partial-result merging

One node offering to several targets in parallel (today: one
request/one await per generated helper, sequential). Needs a
`caws_offer_all/awaits` collect form and a policy for partial failures
(some Tabu/timeouts, some Hohi).

## 7. Promote hot logical topics to physical Kafka topics

`topic_path` is filter-only over the single per-Ritual topic. When a
channel gets hot, promote it to its own Kafka topic; envelope stays the
same, participants subscribe by path prefix. (Deferred by design in the
plan.)

## 8. Replay/backpressure instead of fail-fast

Late Hohi/Tabu replies (after tabu-timeout already failed the await) are
logged and dropped. A replay buffer keyed by correlation id would let a
re-evaluated goal still use a late answer within some window.

## 9. CI wiring

None of the new coverage runs automatically: cargo workspace tests,
lildaemon pytest (needs the server *stopped* — duckdb lock), and the
Playwright suite (`dagda/cobbler/frontend/e2e`, needs lildaemon + cobbler
backend + Vite; see `ritual_typed_edges.md` for the run recipe). Even a
single script that brings the stack up/down around the suites would guard
regressions.

## 10. Minor cleanups

- lildaemon: FastAPI `@app.on_event` deprecation → lifespan handlers.
- lildaemon `--reload` drops joined Ritual participants on any file change
  under `goat/` — either document or disable reload for Ritual work.
- Repo-root scratch files in all three repos (test*.pl, x.txt, demo, glogs,
  older planning docs) — manual cleanup pass pending (owner: Stan).
