# Progressive-consult Ritual example — status for team review (2026-08-25)

Executive summary. For full technical detail see
[`ritual_progressive_consult_verification_status.md`](ritual_progressive_consult_verification_status.md)
(implementation + verification history) and
[`dis_sequential_caws_await_bug.md`](dis_sequential_caws_await_bug.md)
(the blocking bug, repro steps, root-cause hypotheses).

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

**Root cause is not yet pinned down.** Two candidate hypotheses are
written up in the bug doc, both pointing into `clara-cycle`'s cycle
controller (`has_converged`/`re_evaluate_root_goal`) or `the_coire.pl`'s
`caws_offer`/`caws_await` idempotency handling — neither confirmed against
a live trace yet.

## What we're asking the team for

1. **Someone with deeper context on `clara-cycle`'s convergence/tableau
   design** to take the two hypotheses in `dis_sequential_caws_await_bug.md`
   and either confirm one or find the actual mechanism. Suggested first
   step is already written up there (rerun the simplest repro with
   `RUST_LOG=debug` and read cycle-by-cycle convergence/mailbox state).
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
