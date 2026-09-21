# Hermes-as-Ego v3: Deliberative Analyst review (2026-09-21)

Status: **advisory input, not a decision.** Output of the "Deliberative Analyst" ruleset run over
`hermes_agent_evaluator_plan_v3.md`. The user has separately reviewed v3. Where this document and v3's status
tags disagree, the tags win.

Outcome: adopted, 4 adopt / 0 reject (member-gemma, member-qwen, groq-splinter, member-groq-gptoss).
The resolution ID below says 2025; the correct year is 2026.

## Resolution 2025-HERMES-03 (verbatim)

Be it resolved, that the assembly hereby adopts the Hermes-as-Ego Consolidated Implementation Plan, version 3, as
the governing specification for all subsequent work, in the following particulars:

**1. Decision Ledger.** Items 1 through 22 of the decision ledger are ratified as stated and incorporated by
reference.

**2. Ego Architecture.** Each Ego seat shall be instantiated as a single Hermes container, bounded by cgroup
resource limits and subject to hard-stop authority by the evaluator. The functions `caws_offer` and `caws_await`
shall constitute the sole side-effecting gate; any offer not acknowledged within the prescribed timeout shall be
treated as denied.

**3. Superego Context Tiers.** The Superego context shall be organized in tiers, with Tier 2 sourced independently
of the Ego runtime state, such that the evaluator's context cannot be influenced by Ego-supplied data.

**4. Parameter Handling (§4).** The prior allowlist-only rule is superseded. Two approval routes are established:
(a) deterministic schema validation for allowlisted actions; (b) semantic review via `clara_fy` or an equivalent
mechanism for free-form or otherwise unknown actions. The following invariant is binding and non-waivable:
**Prolog goals shall never be constructed from text supplied by Hermes.**

**5. Phased Rollout.** Phase 0: read-only verification on the limbic substrate, a hard gate; no repository code
shall be merged until Phase 0 is completed and attested. Phases 1 through 3: incremental integration per the plan.
Phase 4: audit, latency-budget enforcement, and auto-placement.

**6. Effective Date.** Takes effect upon adoption and supersedes all prior specifications inconsistent with the
foregoing.

Roll call: all four ADOPT. Rationale given: the Phase 0 hard gate, cgroup isolation, and the non-waivable
goal-construction invariant.

## Reviewer notes (added when filed)

- "Ratified items 1-22" includes the rejected (16-21), superseded (15) and deferred (22) rows. Read as ratifying
  their status, not as reviving them.
- Phase 4 wording ("enforcement", "auto-placement") is broader than v3, which only names Phase 4. v3 stands.
- Not addressed by the vote, still open in v3 §7: stricter bar for irreversible free-form actions, FFI
  serialization path, `approve_action/4` placement, cost model.
