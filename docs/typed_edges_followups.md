# Typed Edges — Follow-up Work

Candidate next steps after the typed Ritual edges phase
(`ritual_typed_edges.md`, landed 2026-07-05). Ordered roughly by
recommended sequence; the first two items are the ones that bit us
repeatedly during live verification.

## 1. Persist (or self-heal) Dis's ritual registry — *recommended first*

Dis keeps active Rituals in memory only. Every Dis restart orphans the
active ritual configs: lildaemon still believes they're active, but Runs
fail until each config is deactivated and re-created/re-activated by hand.
This is the single most annoying operational trap in day-to-day dev.

Options (pick one):
- Persist the registry in the existing CoireStore (DuckDB) and reload on
  boot.
- Have lildaemon re-join automatically: on a failed Run/join due to unknown
  ritual_id, re-run the activation flow for configs it thinks are active
  (idempotent thanks to content-addressed source registration).

## 2. Evaluate-cache staleness for LLM calls

Dis's toolbox `evaluate` cache (`clara-toolbox/src/ffi.rs`) memoizes by
payload. Correct for deterministic tools, wrong for `ponder_*` LLM calls:
re-running the same query returns the previous answer (during E2E, a re-run
returned a stale echo response even after the focused evaluator changed).
Add a TTL, or let splinteredmind `evaluate` opt out of the cache.

## 3. Mailbox hygiene: don't ingest foreign Offerings

`ingest_tephra` writes every non-Hohi/Tabu tephra into both engine
mailboxes — including Offerings *addressed to other nodes* (observed as a
`ritual/offering` event drained alongside the awaited `ritual/hohi`).
Harmless today (caws ignores non-hohi/tabu origins) but it accumulates
noise in every performance's mailboxes. Filter on `target_node_id` before
ingesting, mirroring the participant-side skip.

## 4. Dual-consumer race on the CLIPS mailbox (investigate)

`relay_clips_to_prolog` drains the CLIPS mailbox **unfiltered**
(`poll_pending(clips_id)`, `clara-cycle/src/relay.rs:75`) while the CLIPS
engine's own `consume_coire_events` also reads that mailbox. Whichever runs
first in a cycle wins. It happens to work with the current phase ordering,
but the invariant is implicit — worth documenting or making explicit with
origin-prefix filters like the Prolog side.

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
