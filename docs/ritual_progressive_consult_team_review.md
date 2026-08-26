# Progressive-consult Ritual example — status for team review (2026-08-25, updated 2026-08-26)

Executive summary. For full technical detail see
[`ritual_progressive_consult_verification_status.md`](ritual_progressive_consult_verification_status.md)
(implementation + verification history) and
[`dis_sequential_caws_await_bug.md`](dis_sequential_caws_await_bug.md)
(the engine bug: repros, confirmed root cause, the implemented fix) and
[`coire_sync_vs_speculative_design_note.md`](coire_sync_vs_speculative_design_note.md)
(why the gap existed architecturally).

## Where things stand

The progressive-consult Ritual example (`lildaemon/examples_ritual_progressive_consult.py`
— local LLM → Edgequake → Groq → live web research, stopping at the first
tier whose answer is judged sufficient) is implemented and committed.
**All four tiers are now verified live end-to-end (2026-08-26)** — the
engine bug that blocked tiers 2-4 has been root-caused, fixed in
`clara-cycle`, deployed (image rebuild), and verified: the full
integration suite runs 9/9 against the live stack, including forced
escalation through every tier.

## The bug that blocked tiers 2-4 (now fixed)

Within one Dis deduction goal, a **second** `caws_offer`/`caws_await`
round trip — issued after an earlier one in the same goal has already
resolved — never converges to a solution, even though the underlying
request/reply genuinely completes. Confirmed with several independent
minimal repros (different target node, same target node called twice,
inside an if-then-else, with and without `catch/3`). Not specific to this
example: it will block **any** future Ritual design that needs "ask A,
then use A's answer to ask B" within a single deduction. The one existing
pattern that issues two offers (`research_step/8`'s fan-out-then-join,
both offers before either await) is unaffected and still works.

This was found, not by a routine review, but by building deterministic
tests that force the full orchestrator past tier 1 — the previous "5/5
verified" test suite always self-satisfied at tier 1 and so never actually
exercised the code path that's broken.

**Root cause (confirmed 2026-08-26, see the bug doc's "Root cause"
section):** `has_converged`'s own convergence-refresh step
(`re_evaluate_root_goal`) is the only thing that can drive the goal past a
resolved first leg after cycle 0, but it runs *after* the cycle's one
publish/track step (`evaluator_pass`) — so a second, newly-triggered
`caws_offer` was really staged, but the cycle declared convergence before
anything drained and published it, and it was discarded when the run ended.

**The fix (implemented + verified 2026-08-26):** the narrow
convergence-invariant option — `has_converged` now counts undrained
`evaluator/` events after `re_evaluate_root_goal` and holds convergence
while any exist, so the next cycle's `evaluator_pass` publishes them and
the pending offer blocks until the reply. Three new `clara-cycle`
regression tests (invariant-direct, 2-leg dependent chain, 3-leg
recursive chain — each verified to fail with the guard removed); full
crate suite green with and without the `ritual` feature; live suite 9/9.

## What we're asking the team for

1. **Review of the implemented engine fix** (`clara-cycle/src/controller.rs`,
   `has_converged`) — it deliberately chose the narrow ordering fix over
   the larger redesign `coire_sync_vs_speculative_design_note.md` lays out
   (modeling chained `caws_offer`/`caws_await` on the durable/offset
   substrate the topic-poll pattern uses). The design note remains the
   reference if the team prefers the deeper option later; the narrow fix
   does not preclude it.
2. **Awareness of one remaining follow-up (cost, not correctness):**
   under the engine's re-evaluate-fresh-per-cycle model, `consult_step`'s
   `->/;` fallbacks can't tell "no reply yet" from "replied with Tabu"
   (both are plain Prolog failure), so a forced-escalation pass stages the
   remaining tiers' offers eagerly rather than strictly one at a time —
   fan-out cost, correct answers. The fix idiom (tri-state check via
   `caws_result`/`caws_failed`) is described in the bug doc's closing
   note; not yet implemented.

## Also fixed along the way

- `catch/3` wrapping a `caws_offer`/`caws_await` pair was found to be an
  **independent** bug (breaks convergence even for a call that
  successfully completes) and was fixed on the Prolog side — `catch/3`
  now only ever wraps the synchronous decode step after `caws_await`
  returns.
- Four deterministic escalation tests (tiers 2/3/4/exhaustion, mocked
  `sufficient/3` keyed by answer text) were added while the bug was open,
  xfail-marked, and flipped to XPASS on the first run against the fixed
  engine — the markers have since been removed and they are plain tests.
