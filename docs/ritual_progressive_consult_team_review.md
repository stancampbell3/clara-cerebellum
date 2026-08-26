# Progressive-consult Ritual example — status for team review (2026-08-25)

Executive summary. For full technical detail see
[`ritual_progressive_consult_verification_status.md`](ritual_progressive_consult_verification_status.md)
(implementation + verification history) and
[`dis_sequential_caws_await_bug.md`](dis_sequential_caws_await_bug.md)
(the blocking bug, repro steps, root-cause hypotheses) and
[`coire_sync_vs_speculative_design_note.md`](coire_sync_vs_speculative_design_note.md)
(why the gap exists architecturally, and what that implies for scoping a
fix).

## Where things stand

The progressive-consult Ritual example (`lildaemon/examples_ritual_progressive_consult.py`
— local LLM → Edgequake → Groq → live web research, stopping at the first
tier whose answer is judged sufficient) is implemented and committed.
**Tier 1 is verified live end-to-end.** Tiers 2-4 are currently blocked by
an open engine bug, not a bug in this example's own Prolog.

## The blocker

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

**Root cause is now confirmed** (2026-08-26, see the bug doc's "Root cause"
section): `has_converged`'s own convergence-refresh step
(`re_evaluate_root_goal`) is the only thing that can drive the goal past a
resolved first leg after cycle 0, but it runs *after* the cycle's one
publish/track step (`evaluator_pass`) — so a second, newly-triggered
`caws_offer` is really staged, but the cycle declares convergence before
anything drains and publishes it, and it's discarded when the run ends.
Caught live with `RUST_LOG=debug` on a single clean repro run, no engine
code changes or rebuild needed — existing debug logging already showed the
exact cycle.

## What we're asking the team for

1. **A decision between the two fix directions already scoped in the bug
   doc**: a narrow ordering fix inside `has_converged` (drain/publish
   anything `re_evaluate_root_goal` just staged before finalizing
   convergence for that cycle), or the larger redesign
   `coire_sync_vs_speculative_design_note.md` lays out (model chained
   `caws_offer`/`caws_await` on the same durable/offset substrate the
   topic-poll pattern already proved safe). This is no longer an
   open-ended investigation — it's a scoping decision plus implementation.
2. **A decision on how to proceed with this example in the meantime** —
   the bug doc lays out three options: fix the engine (correct, bigger,
   cross-repo), restructure this example's orchestration into one
   `/deduce` call per tier from Python (unblocks this example without an
   engine change, doesn't fix the underlying limitation), or leave tiers
   2-4 documented as known-broken for now.

## What's already done, not blocked on the above

- `catch/3` wrapping a `caws_offer`/`caws_await` pair was found to be an
  **independent** bug (breaks convergence even for a call that
  successfully completes) and has been fixed — `catch/3` now only ever
  wraps the synchronous decode step after `caws_await` returns. Real,
  worthwhile fix, but not sufficient by itself to unblock tiers 2-4.
- Four new tests exist for tiers 2/3/4/exhaustion, using a deterministic
  mock in place of the real `clara_fy` sufficiency check so escalation can
  be forced reproducibly rather than depending on the local model choosing
  not to self-satisfy. They're marked `xfail` against the open bug so the
  suite documents the known gap rather than either hiding it or breaking
  CI; they'll flip to real passing tests as soon as the engine bug is
  fixed, with no test-code changes needed.
