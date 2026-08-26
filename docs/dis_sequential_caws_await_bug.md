# Dis engine bug: sequential dependent `caws_offer`/`caws_await` calls don't converge

**Status: FIXED and VERIFIED LIVE (2026-08-26).** Root cause confirmed
live via `RUST_LOG=debug` trace, fixed the same day with the narrow
convergence-invariant option (see "The fix" at the end of this doc),
`clara-api:latest` rebuilt, and verified end-to-end against the live
stack: the minimal two-leg repro now converges with both answers bound,
and lildaemon's full progressive-consult suite ran 5 passed + 4 XPASS
(the four formerly-xfail escalation tests, including the tier-4 real-crawl
chain, 0 failures, 5m55s). The xfail markers have been removed — they are
plain tests now. Found 2026-08-25 while building mocked
full-orchestrator escalation tests
for the progressive-consult Ritual example
([`ritual_progressive_consult_plan.md`](ritual_progressive_consult_plan.md),
[`ritual_progressive_consult_verification_status.md`](ritual_progressive_consult_verification_status.md)).

**Read [`coire_sync_vs_speculative_design_note.md`](coire_sync_vs_speculative_design_note.md)
before scoping a fix.** It frames why this gap exists (the correlated
`caws_offer`/`caws_await` pattern was only ever designed/exercised for one
round trip per goal, unlike Coire's other, already-proven-safe-under-
re-evaluation topic-poll pattern) and lays out two different kinds of fix
worth choosing between deliberately, not just patching the one failing
test case.

## TL;DR

Within **one** Dis deduction goal, a **second** `caws_offer`/`caws_await`
round trip — issued *after* an earlier one in the same goal has already
resolved — never completes. The deduction reports `status: Converged` but
with **zero** `prolog_solutions`, even though the first round trip's answer
was real and correct. This reproduces with:

- two different target participants (`consult_local` then `consult_cow`), and
- the **same** target participant called twice with different payloads
  (`consult_local` twice, different queries).

The only previously-existing code that issues two `caws_offer`s in one goal
(`research_step/8`) never hit this because it uses **fan-out-then-join**
(`offer, offer, await, await`), not **sequential-dependent**
(`offer, await, offer, await`, where the second offer's payload depends on
the first await's result). Every other shipped example only ever does a
single `caws_offer`/`caws_await` pair per goal. The progressive-consult
design (`consult_step/15` in `lildaemon/examples_ritual_progressive_consult.py`)
is the first code to need the sequential-dependent pattern for real,
end-to-end (its earlier "5/5 verified" test suite always self-satisfied at
tier 1, so tiers 2-4's second/third/fourth `caws_offer` were never actually
exercised until now).

**Impact:** `consult_step`'s tier 2/3/4 legs are currently non-functional
in production, not just in tests — every real run silently degrades to
"keep the previous tier's answer" instead of ever incorporating cow's or
Groq's answer, because the second `caws_offer` in the chain never resolves.
Any *future* Ritual design that needs "ask A, then use A's answer to ask B"
in a single deduction will hit the same wall.

## Reproduction

All repros run against the live dev stack (`docker-lildaemon-1`,
`docker-clara-api-1`) via `lildaemon`'s existing
`tests/test_ritual_progressive_consult_example.py::_run_isolated_goal`
helper, which joins only the given participants and submits a raw
`initial_goal` + `prolog_clauses` list to `POST {fierypit}/evaluate`
(`{"data": {"deduce": {...}}}`).

### Repro 1 — bare, single call: works

```prolog
consult_cow('what is a qubit?', [], WorkspaceId, ollama, 'gemma4:e4b',
            Answer, Cites, CiteCount)
```

Real Edgequake round trip, converges in ~1-2 cycles with a real `Answer`.
(This is `test_tier2_cow_queries_edgequake`, already in the suite and
passing.)

### Repro 2 — the same call wrapped in `catch/3`: fails

```prolog
catch(consult_cow('what is a qubit?', [], WorkspaceId, ollama, 'gemma4:e4b',
                   Answer, Cites, CiteCount),
      Err, (Answer = caught(Err), Cites = [], CiteCount = -1))
```

Converges (`status: Converged`) with **zero** `prolog_solutions`. Same
query, same fresh workspace as Repro 1 — only the `catch/3` wrapper
differs. This looked at first like the root cause of the mocked-test
failures (see "What did NOT fix it" below) — it is a **real, independent**
bug, but turned out not to be sufficient to explain the actual failures.

Control: `catch(member(X,[1,2,3]), _, fail)` against the same stack
succeeds fine with 3 solutions in 1 cycle — so `catch/3` is not broken in
general, only specifically when it wraps a goal containing a
`caws_offer`/`caws_await` pair.

**Root-caused, fixed, and verified live 2026-08-26** (this exact shape —
a catch-wrapped single leg with an operator-syntax recovery — converges
with the real answer bound against the rebuilt image) — and it turned out
not to be about `catch/3` at all. `re_evaluate_root_goal` parsed the root-goal string with
`transpile.rs`'s template mini-parser *before* re-querying, and returned
on parse failure — but that parser has no support for infix operators or
`(...)` grouping, so any root goal written in operator syntax (this
repro's `(Answer = caught(Err), ...)` recovery arg; equally an
if-then-else or a bare `A = B`) failed to parse and was therefore **never
re-run after cycle 0 at all**. The async leg's reply would land, the run
would quiesce, and it converged with cycle-0's empty solutions. The
control worked because it succeeded *on* cycle 0, where `prolog_pass`
runs the raw goal string with no parsing involved; a fully-parseable
`catch(g(X), _, fail)` would also have worked. Fix: `re_evaluate_root_goal`
now re-queries first and captures `final_solutions` unconditionally on
success; the parse/template/tableau step is best-effort bookkeeping after
the fact. Regression test:
`run_loop_operator_syntax_root_goal_captures_solutions` (verified failing
before the fix with exactly this symptom, `solutions=[]`).

### Repro 3 — the actual root cause: two sequential dependent calls, no catch at all

```prolog
diag_seq(Query, WorkspaceId, Answer1, Answer2) :-
    consult_local(Query, [], Answer1),
    consult_cow(Query, [], WorkspaceId, ollama, 'gemma4:e4b',
                Answer2, _Cites, _CiteCount).
```

Joined participants: `local-splinter`, `cow`. No `catch/3` anywhere.
Converges with **zero** solutions after 15 cycles. Container logs for the
run confirm `cow` never received an offering at all (only
`local-splinter` did) — the second leg's `caws_offer` never actually gets
delivered to its target.

### Repro 4 — same target twice: also fails

```prolog
diag_seq_same(Q1, Q2, Answer1, Answer2) :-
    consult_local(Q1, [], Answer1),
    consult_local(Q2, [], Answer2).
```

Only `local-splinter` joined, called twice with two different queries.
Converges with **zero** solutions after 13 cycles. This rules out
"per-target Kafka wiring only handles the first distinct target" as the
cause — the second call fails even against an already-wired,
already-successfully-contacted node.

### Repro 5 — same pattern inside an if-then-else (matches production shape): also fails

```prolog
consult_step_2tier(Query, Ctx, WorkspaceId, LlmProvider, LlmModel, Answer) :-
    consult_local(Query, Ctx, Answer1),
    (   sufficient(Answer1, Query, Ctx)
    ->  Answer = Answer1
    ;   (   consult_cow(Query, Ctx, WorkspaceId, LlmProvider, LlmModel,
                         EdgeAnswer2, _Cites2, _CiteCount2)
        ->  Answer = EdgeAnswer2
        ;   Answer = fallback(Answer1)
        )
    ).
```

With a deterministic mock forcing `sufficient(Answer1,...)` to fail (see
"Mock design" below), this *does* converge — but `Answer` is bound to
`fallback(Answer1)`, i.e. the `consult_cow(...) -> ...` condition itself
failed. Container logs confirm `cow` again never received an offering.
This is the closest structural match to `consult_step/15`'s real tier-2
branch, confirming the bug reproduces inside an if-then-else, not just a
flat conjunction.

### Repro 6 — the full `consult_step/15` mocked to force escalation to tier 2: fails the same way

Full 5-participant orchestrator, `sufficient/3` replaced with a
deterministic mock (see below), `mock_target_stage(2)` (forces tier 1
insufficient, tier 2 should then satisfy). Converges with zero solutions;
logs show only `local-splinter` ever receives an offering — `cow`,
`groq-splinter`, `snek`, `edgequakeingest` never do, regardless of whether
`catch/3` wraps the tier-2/3 legs or not (tested both ways).

### Repro 7 — root cause caught live with `RUST_LOG=debug` (2026-08-26)

Simplest possible repro (Repro 4's shape): `local-splinter` joined once,
goal `consult_local('what is 2 + 2?', [], R1), consult_local('what is 3 + 3?',
[], R2)`, no `catch/3`, no mock, no other participants. `docker-clara-api-1`
recreated with `RUST_LOG=info,clara_cycle=debug,clara_coire=debug` for this
one run only (reverted immediately after, no code changes, no rebuild).

Converged after **21 cycles**, zero solutions — same symptom as every other
repro. The full debug trace shows exactly what happens, cycle by cycle:

- **Cycle 0:** `prolog_pass` runs the full goal once, reaches
  `consult_local(R1)`, stages+publishes its `caws_offer` (`Coire: writing
  event ... origin evaluator/offering ... prompt: "what is 2 + 2?"`,
  followed by `publish_evaluator_events published 1 Offering(s)`), then
  fails on `caws_await(R1)` (no reply yet) — the conjunction fails *before
  ever reaching* `consult_local(R2)`. `pending_offers=1` correctly blocks
  convergence.
- **Cycles 1-18:** `prolog_pass` is a no-op after cycle 0 (`query_once("true")`
  — it never re-runs the goal). `has_converged` calls `re_evaluate_root_goal`
  every cycle (mailboxes/agenda are otherwise empty), but leg 1's reply
  hasn't arrived yet, so the re-query still fails at `caws_await(R1)` before
  reaching `consult_local(R2)` — no new offer, nothing logged (this specific
  failure path in `re_evaluate_root_goal` is a silent `return`, see below).
  `pending_offers=1` keeps blocking convergence throughout.
- **Cycle 19 (03:45:24.640):** leg 1's Hohi finally arrives, ingested into
  both mailboxes. `pending_offers` drops to 0, but `clips_pending=1` this
  cycle — still not converged.
- **Cycle 20 (03:45:24.642-.750), the pivotal cycle:** `prolog_pass` is
  still a no-op. `evaluator_pass` runs and finds nothing new to publish
  (correct — nothing has called `caws_offer` again yet). Then, inside
  `has_converged`'s convergence check, `re_evaluate_root_goal` fires — and
  this time leg 1's answer is cached, so the re-run genuinely reaches
  `consult_local(R2)` for the first time. The log catches it in the act:

  ```
  [03:45:24.749] Coire: writing event 2935da9f-... (session 5f8d9969-...,
  origin evaluator/offering, status pending, payload {"_caws":
  {"correlation_id":"656285e8-...","target_node_id":"local-splinter",
  "topic_path":"consult/local"},"context":[],"prompt":"what is 3 + 3?"})
  [03:45:24.750] CycleController: convergence — prolog_pending=0,
  clips_pending=0, agenda_empty=true, snapshot_stable=false,
  tableau_stable=true, root_resolved=true, pending_offers=0 → true
  [03:45:24.750] CycleController: converged after 21 cycle(s)
  ```

  The second `caws_offer` for R2 ("what is 3 + 3?") really does get staged
  — one line later, the same cycle declares `Converged`, with
  `pending_offers=0` (Rust's bookkeeping never learns about the event that
  was *just* staged) and no further `evaluator_pass` ever runs to drain and
  publish it. `run()` returns immediately; `evict_coire_sessions` clears the
  session moments later. The staged offer for R2 is discarded, unsent —
  this is the literal mechanism behind "the second leg's offering never
  gets delivered to Kafka."

## Mock design used for these tests (for context, not itself the bug)

To force escalation deterministically without relying on `clara_fy`'s real
judgment (unreliable to trigger past tier 1 — `clara_mind_splinter` has its
own `web_search` tool and tends to self-satisfy), `sufficient/3` was
replaced with:

```prolog
sufficient(Answer, _Query, _Ctx) :-
    (   mock_verdict(Answer, Verdict)
    ->  true
    ;   aggregate_all(count, mock_verdict(_, _), SeenCount),
        NextStage is SeenCount + 1,
        mock_target_stage(Target),
        (   NextStage >= Target -> Verdict = true ; Verdict = false ),
        assertz(mock_verdict(Answer, Verdict))
    ),
    Verdict == true.
```

keyed by **answer text**, not a raw call counter, because Dis re-evaluates
the whole submitted goal fresh every retry cycle (confirmed separately)
— a naive counter would over-count across retries of the same logical
check. `mock_verdict/2` must be `thread_local`, not `dynamic` (confirmed
live: `dynamic` leaks across separate `/deduce` calls in the same
long-lived `clara-api` process). This mock is not the bug; it's the tool
that exposed it, and it works correctly in isolation (verified retry-safe:
3 distinct stage-advancements across 6 total calls with repeats).

## What did NOT fix it

The `catch/3`-around-async-call bug (Repro 2) is real and was fixed in
`lildaemon/examples_ritual_progressive_consult.py`: `extract_caws_response/2`
and `consult_cow/8` now wrap only the *synchronous, post-`caws_await`*
decode step (`is_dict`/`atom_json_dict`/`get_dict`) in `catch/3`, never the
`caws_offer`/`caws_await` pair itself; the three call sites in
`consult_step/15` that used to wrap `consult_cow`/`consult_groq` in
`catch(...,_,fail)` now call them plain (they already degrade to normal
Prolog failure internally on error, so the outer `catch` was redundant
once the inner decode is guarded). This is a worthwhile fix to keep
regardless — it's cheaper and more correct — but **it does not unblock
tiers 2-4**: Repro 3-6 above all reproduce the zero-solutions failure with
no `catch/3` anywhere in the picture. The two bugs happen to produce an
identical symptom (`status: Converged`, `prolog_solutions: []`), which is
what made the `catch/3` bug look sufficient before Repro 3 ruled it out.

(The engine side of the `catch/3` bug has since been root-caused and fixed
too — see the note under Repro 2. The lildaemon Prolog workaround remains
good style regardless: narrow `catch/3` scopes around decode steps are
cheaper and clearer than goal-wide ones.)

## Root cause (confirmed 2026-08-26)

Confirmed live via Repro 7 above, cross-referenced against
`clara-cycle/src/controller.rs`. No engine code was changed to find this —
existing `log::debug!`/`log::info!` instrumentation, read with
`RUST_LOG=debug` for one run, was already enough.

**The mechanism, precisely:**

1. `CycleController::run`'s per-cycle loop (controller.rs:349-443) is:
   `prolog_pass` → relay → `clips_pass` → relay → `evaluator_pass` (drains
   staged Coire events and *actually publishes them to Kafka*, populating
   `self.pending_offers`) → `has_converged`.
2. **`prolog_pass` (controller.rs:551-581) only re-runs the actual goal on
   cycle 0.** Every cycle after that, it's a no-op tick
   (`query_once("true")`). So after cycle 0, the *only* code path that ever
   re-attempts the whole goal is `has_converged` → `re_evaluate_root_goal`
   (controller.rs:1288-1376), called once mailboxes/agenda are empty
   (controller.rs:1144-1146) — which happens **after** `evaluator_pass`
   already ran for that cycle.
3. `caws_offer/4` (the_coire.pl:161-174) is a plain `assertz` + stage-into-
   Coire's global per-session event queue — an ordinary, irreversible
   Prolog side effect. It does not care whether it's called from
   `prolog_pass`'s cycle-0 attempt or from `re_evaluate_root_goal`'s
   later, otherwise-throwaway re-query.
4. So: once leg 1's answer is cached, the *next* `re_evaluate_root_goal`
   call re-runs the whole goal, resolves leg 1 from cache, and reaches
   `consult_local`/`consult_cow`/etc for **leg 2** — staging a real,
   brand-new `caws_offer` event — before failing overall on leg 2's fresh
   `caws_await` (nothing has replied yet, it was *just* staged). This
   overall failure is a normal "goal produced zero solutions" outcome, not
   a Prolog exception, so `re_evaluate_root_goal`'s own `query_with_bindings`
   call returns `Ok("[]")` rather than `Err` — which lands on the **silent**
   `_ => return` branch at controller.rs:1328 (empty solutions array), not
   the logged "still fails" `Err` branch at controller.rs:1315-1318. This is
   why nothing about this ever appears in the logs by itself — the function
   runs, correctly does nothing to the tableau, and returns quietly.
5. Back in `has_converged`, `pending_responses_zero` (`self.pending_offers
   .is_empty()`, controller.rs:1192) and `tableau_stable` are both computed
   from state that **predates** step 4's side effect — `pending_offers` is
   Rust-side bookkeeping that only `evaluator_pass`/`publish_evaluator_events`
   populate, and that already ran earlier this same cycle. Nothing in this
   cycle's remaining code re-checks whether `re_evaluate_root_goal` just
   staged something new. Convergence is declared `true` on the very cycle
   the second offer was minted.
6. `run()` returns `Converged` immediately (controller.rs:393-413).
   `evaluator_pass` never runs again. The staged "evaluator/offering" event
   for leg 2 is never drained, never becomes a Kafka-published Tephra, and
   is discarded moments later by `evict_coire_sessions`
   (`Coire: clearing session ...`, visible in Repro 7's trace right after
   `converged after 21 cycle(s)`).

**In short:** `has_converged`'s own convergence-refresh step
(`re_evaluate_root_goal`) is the *only* thing capable of driving the goal
past a resolved first leg, but it runs downstream of the cycle's one
publish/track step (`evaluator_pass`) — so any new `caws_offer` it triggers
is real (a real Kafka message would eventually go out for it) but is
guaranteed to be orphaned in the exact cycle it's created, because nothing
in that cycle or any later one re-invokes `evaluator_pass` for it before
`has_converged`'s stale-relative-to-that-side-effect check declares the
run finished.

**What a fix needs to do:** make sure that whenever `re_evaluate_root_goal`
causes new Coire events to be staged, `has_converged` either (a) drains and
publishes them (an `evaluator_pass`-equivalent call) *before* computing
`pending_responses_zero`/`converged` for that cycle, so a genuinely new
offer is seen as pending and blocks convergence like any other, or
(b) explicitly treats "a re-evaluation just staged new events" as a
not-converged signal for that cycle, guaranteeing at least one more cycle
in which the normal `evaluator_pass` step picks it up. Read
[`coire_sync_vs_speculative_design_note.md`](coire_sync_vs_speculative_design_note.md)
before picking between this narrow fix and the larger architectural
options it lays out — this mechanism is a plain ordering bug, but whether
the *right* fix is "reorder/re-check in `has_converged`" versus "redesign
the correlation cache" is exactly the choice that doc is about.

## Suggested triage options

1. **Fix the Dis/clara-cycle engine bug directly.** Correct long-term fix —
   every future multi-step Ritual design needs this pattern to work. Root
   cause is now precisely located (`has_converged`/`re_evaluate_root_goal`
   ordering relative to `evaluator_pass`, see "Root cause" above) — this is
   no longer an open-ended investigation, just a decision between the
   narrow ordering fix and the larger design-note options, then
   implementation + a 3+-leg/loop stress test (not just 2) in `clara-cycle`.
2. **Route around it at the orchestration layer.** Split `consult_step`
   into one `/deduce` call per tier from Python instead of one Prolog goal
   spanning all four tiers — `run_demo()` already does the join/cleanup
   orchestration, so it could feed each tier's `Answer` into the next
   tier's prompt itself. Keeps every individual goal to a single
   `caws_offer`/`caws_await` pair (the one proven-working pattern),
   unblocking the example without an engine change. Does not fix the
   underlying limitation for other future designs.
3. **Scope-limit for now.** Leave `consult_step`'s tiers 2-4 as documented
   known-broken, ship only a tier-1-only or fan-out-only version of
   anything relying on this pattern until the engine is fixed.

**Decision made 2026-08-26: option 1, the narrow engine fix — implemented.**

## The fix (implemented 2026-08-26)

`clara-cycle/src/controller.rs`, `has_converged`: after
`re_evaluate_root_goal()` runs, count undrained `evaluator/`-prefixed
events in the Prolog session's Coire queue
(`count_pending_with_origin_prefix`, the same prefix
`publish_evaluator_events` drains) and add `!evaluator_events_staged` to
the convergence conjunction. Invariant: *convergence is never declared in
a cycle whose root-goal re-evaluation staged new outbound events* — the
next cycle's `evaluator_pass` publishes them and the resulting
`pending_offers` entry takes over blocking convergence until the reply.
Gated on `ritual_handle.is_some()` (without a handle nothing ever drains
that prefix, and holding would burn the cycle budget on dead letters).
An N-leg chain now resolves in O(N) quiescence rounds; `caws_offer/4`'s
existing idempotency keeps each leg to exactly one published Offering
across all the re-evaluations in between.

Covered by three new regression tests in `controller.rs`'s
`ritual_tests` (all verified to fail with the guard removed):

- `undrained_evaluator_event_blocks_convergence` — the invariant directly.
- `run_loop_sequential_dependent_caws_consults_converge` — full `run()`
  with a 2-leg dependent chain (`offer, await, offer, await`, leg 2's
  payload built from leg 1's answer) against an InMemoryBroker echo peer;
  asserts both answers in the solutions and exactly 2 Offerings published.
- `run_loop_chained_caws_consults_in_recursion_converge` — 3 dependent
  legs driven by a recursive chain predicate (the N-deep/loop shape the
  design note warned a "works for exactly two" fix would miss); asserts
  the fully-chained answer and exactly 3 Offerings.

Full `clara-cycle` suite passes with and without the `ritual` feature.

**Live verification (2026-08-26, complete):** `clara-api:latest` rebuilt
with the fix and redeployed; the minimal two-leg repro (Repro 7's goal)
converged with both `R1` and `R2` bound (115 cycles — each leg pacing on
a real local-LLM reply); lildaemon's full
`test_ritual_progressive_consult_example.py` suite then ran **5 passed +
4 XPASS, 0 failed** — all four formerly-xfail escalation tests (tier 2,
tier 3, tier 4 with a real crawl, and exhausted) passed on the first run
against the fixed engine, and their xfail markers have been removed.

Note a separate follow-up surfaced by scoping this fix:
`consult_step`'s `->/;` fallbacks can't distinguish "no reply yet" from
"replied with Tabu" (both are plain Prolog failure), so with the engine
fixed its tiers will fan out rather than stop early — it needs the
tri-state idiom (`caws_result`/`caws_failed`/neither-then-fail) before
its stop-early economics are real.

## Appendix: what's confirmed clean / not implicated

- `research_step/8`'s fan-out-then-join pattern (`offer, offer, await,
  await`) is unaffected — already verified working in the original 5/5
  suite and not touched by any of this.
- Single `caws_offer`/`caws_await` pairs, anywhere, in any of the four
  existing `examples_ritual_*.py` scripts: unaffected.
- The mock (`sufficient/3` override) mechanism itself: verified
  independently retry-safe and not implicated (Repro 4 has no mock at all
  and still fails).
- Workspace-ID-vs-slug handling in `CowEvaluator`/`EdgequakeClient` (it
  treats whatever's in the offering's `workspace` field strictly as a
  slug, so passing a resolved workspace *ID* causes it to look up/create a
  workspace literally named after the UUID) is a separate, minor, latent
  issue noticed in passing during this investigation — not the cause of
  the zero-solutions failures (Repro 1 uses the identical ID-as-slug
  pattern and still succeeds when it's the *only* leg in the goal). Worth
  a follow-up cleanup but out of scope here.
