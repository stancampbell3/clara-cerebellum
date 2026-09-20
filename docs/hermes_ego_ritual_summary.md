# Hermes-as-Ego: Ritual of Rituals — Design Summary

## 1. The Triad + Observer

| Seat | Agent | Function | Thinking Mode |
|------|-------|----------|---------------|
| **Id** | *(TBD — needs explicit definition)* | Divergent, high-entropy ideation. Raw associative proposals. Fast, impulsive, *first*. | Divergent |
| **Ego** | **Hermes** (Nous Research runtime on Pineal) | Stateful executive. Owns Meta-Ritual lifecycle, spawns sub-rituals, tracks state, enforces termination, runs reality tests. | Convergent / Executive |
| **Superego** | **Clara — Deliberative Mode** (CLIPS/Prolog via Clara_Cerebellum) | Procedural constraint. Robert's Rules. Veto-power. Narrowing. | Convergent / Normative |
| **Id Analyst** *(meta)* | **Clara — Observer Mode** | Takes notes. Does not vote. Watches the other three. Hard firewall from Superego context. | Meta-observation |

> **Key structural note:** Clara occupies *two* seats (Superego + Id Analyst). These are separate context windows with a hard firewall. State leakage between them invalidates the whole structure.

## 2. Load-Bearing Architectural Decisions

- **Gating mechanism:** `caws_offer` / `caws_await` (existing Rituals plumbing). No Kafka control plane. Fail-closed via timeout-to-false. Superego reuses `caws_tristate/3`.
- **Single-tool constraint:** `request_action(Action, Params, Justification)` is the *only* tool with a real side effect. Bypass is hard by construction, not by audit.
- **Scoping rule (invariant):** Hard veto applies to **side-effecting tool dispatch only**. Advisory philosophy applies to all creative/brainstorming content. These are not in tension; they operate on different axes.
- **No auto-placement.** No full Robert's-Rules per action. No Kafka. Scope stays tight.

## 3. Phased Rollout

| Phase | Goal | Acceptance Criterion |
|-------|------|----------------------|
| **0 — Spike** | Probe live Pineal Hermes install for tool-call wire format. | Send HTTP message → receive documented tool-call response → execute → produce one-page addendum with exact request/response shapes. (Afternoon-scale.) |
| **1 — Ungated** | Prove cross-host text-in/text-out works. | *Also:* confirm Pineal Hermes native tool set is empty or read-only. Document what it is. "Ungated" ≠ "sandboxed" on the Pineal side. |
| **2 — Gated** | Wire the Superego veto path. | `approve_action/4` predicate live. Denial-handling path defined. Superego sharing model resolved. |

## 4. Open Questions (Ranked by Stall-Risk)

1. **Superego sharing model.** One Superego seat shared across concurrent Ego seats (serialisation bottleneck) vs. one per seat (N deliberative analysts, cost, inconsistent verdicts). *Must decide the shape before Phase 2 starts.*
2. **Denial handling.** On Superego deny / timeout-deny: does Ego retry with different params? Escalate to Id for re-consideration? Report to user? Affects loop termination *and* UX.
3. **Kill-switch / teardown.** Fan-out cap is evaluator-side (we control it). Teardown of already-running sub-agent tree is **Hermes-side** (at their mercy). If no cancel API exists, "cap" = "stop sending, hope they finish." Weaker guarantee. Needs its own design pass in Phase 1. *(Motivation: the 1,393-subagent incident.)*
4. **Superego context quality.** What does `approve_action/4` actually *see*? Just `(action, params, justification)` is a weak gate. Should it see Id's original impulse, Ego's reasoning chain, full deliberative context? Must be explicit in Phase 2 design.
5. **Latency asymmetry.** Id is fast. Ego is medium. Superego is slowest (consulting its rulebook). Synchronous ritual = bottleneck factory. → **Asynchronous epochs:** Id proposes async → Ego evaluates in its epoch → Superego veto/approves on its own schedule. Discrete, finite, bounded, deterministic.
6. **"Who governs the governors?"** If Id and Ego deadlock and Superego is silent, what happens? Need a tie-break rule *before* wiring agents, not after.
7. **Termination criteria.** Formal "Motion to Adjourn" from Superego, not "the LLM stopped talking." Ego must be able to say "no, that plan doesn't survive contact with the actual API" — reality test, not just mediation.

## 5. What the Document Gets Right

- `caws`-based gating over a new Kafka control plane.
- Single-tool constraint (bypass hard by construction).
- Phased rollout (don't bet the stack on the wire-format question).
- Explicit "what we won't do" list.
- Scoping discipline that keeps this from becoming a six-month project.

## 6. Recommended Team-Focus for Review

> The team should focus energy on **the seven open questions + Phase 0 spike design**. Treat the rest as "agreed, build it."
>
> The four things that will actually stall delivery:
> 1. Superego sharing model
> 2. Denial-handling path
> 3. Kill-switch / teardown mechanics
> 4. Wire format (Phase 0 spike)
>
> Resolve those four and the rest is plumbing.

## 7. One-Line Invariant (for the doc header)

> **Hard veto governs side-effecting dispatch. Advisory philosophy governs creative content. Different axes. Not in tension.**
