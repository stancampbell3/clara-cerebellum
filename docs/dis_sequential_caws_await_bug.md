# Dis engine bug: sequential dependent `caws_offer`/`caws_await` calls don't converge

**Status:** open, needs team triage. Found 2026-08-25 while building mocked
full-orchestrator escalation tests for the progressive-consult Ritual
example ([`ritual_progressive_consult_plan.md`](ritual_progressive_consult_plan.md),
[`ritual_progressive_consult_verification_status.md`](ritual_progressive_consult_verification_status.md)).
Not yet fixed — this doc is a handoff for team review, not a patch.

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

## Root-cause investigation notes (hypotheses, not confirmed)

Read `clara-cycle/src/controller.rs` (the Dis cycle controller) and
`clara-prolog/prolog-lib/the_coire.pl` (`caws_offer/4`, `caws_await/2`)
looking for where a second, later-arriving reply could get lost.

**`caws_await/2` (the_coire.pl:195-203)** already caches resolved replies
via a `thread_local` fact (`caws_result/2`, populated by
`caws_drain_ritual_events` from `coire_poll_ritual/2`, a **draining**
poll — see `coire_bridge.rs`), so a later fresh re-evaluation of an
*already-answered* leg should find its cached result immediately rather
than needing to re-drain. This means the naive "single-consumption queue +
full-goal-re-evaluation-per-cycle" race that would otherwise be the
obvious suspect appears to already be guarded against, at least for a
leg that has *already* succeeded once. That doesn't yet explain why the
**second** leg's own `caws_offer` never seems to result in a delivered
Kafka offering at all (confirmed via `docker logs` on every repro above:
the second target node's `RitualParticipant` never logs receiving
anything addressed to it).

**`CycleController::run` (controller.rs:349-443)** re-evaluates the whole
goal fresh every cycle via `prolog_pass`, and `has_converged`
(controller.rs:1125-1225) only refreshes the *tableau's root-goal truth
value* (via `re_evaluate_root_goal`, controller.rs:1288-1376) once
`mailboxes_empty && clips_agenda_empty` — i.e. once nothing is currently
outstanding. `re_evaluate_root_goal` re-queries the literal root goal
string and, on success, sets `self.final_solutions` (the value the final
`DeductionResult` actually reports — see `run`'s `self.final_solutions
.take().or(initial_solutions)` at controller.rs:399/425). **Candidate
hypothesis:** if the first leg's completion causes `mailboxes_empty &&
clips_agenda_empty` to read `true` for one cycle *before* the
Prolog-side re-run has actually reached (and thus emitted) the second
leg's `caws_offer`, `has_converged` could declare convergence off a
tableau that never saw the second offer get made at all — i.e. an
ordering/timing gap between "does the engine consider itself idle" and
"has the Prolog goal actually been re-driven far enough to discover it
needs to make another request." This is a plausible mechanism for
exactly the observed symptom (clean `Converged` status, no Tabu, no
timeout, just an empty solution set) but **has not been confirmed against
a live trace with cycle-by-cycle Coire mailbox contents** — that's the
first thing the team should check.

**Alternative/complementary hypothesis:** something about `caws_offer_sent/2`'s
idempotency key (`Key = offer(Target, Topic, Dict0)`, the_coire.pl:163) or
the JSON round-trip of `Dict0` causes the *second* `caws_offer` call within
a re-evaluated goal to either not re-emit at all, or to emit against a
`Cid`/session state that the cycle controller's `pending_offers` map
(controller.rs:119, referenced throughout `has_converged`) doesn't
actually track as outstanding — worth checking whether `pending_offers`
ever gains an entry for the second leg's `Cid` at all during these repros
(the controller has `log::debug!` convergence lines, `pending_offers={}`,
that would show this directly with `RUST_LOG=debug` on `docker-clara-api-1`).

Neither hypothesis is confirmed. Recommend the team reproduce Repro 4
(simplest: same node, twice) with `RUST_LOG=debug` on `docker-clara-api-1`
and read the per-cycle `CycleController: convergence — ...` log lines
plus whatever Coire mailbox/pending-offer state is visible at that level,
cycle by cycle, to see exactly which cycle the second `caws_offer`'s
`coire_emit` happens on (if ever) and what `has_converged` sees at that
moment.

## Suggested triage options

1. **Root-cause and fix the Dis/clara-cycle engine bug directly.** Correct
   long-term fix — every future multi-step Ritual design needs this
   pattern to work. Cross-repo (`clara-cycle`, possibly `clara-prolog`),
   likely more than one session's work, and needs someone with deeper
   context on the cycle controller's convergence/tableau design than this
   investigation had time to build.
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

No option has been chosen yet — this doc is the handoff for that decision.

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
