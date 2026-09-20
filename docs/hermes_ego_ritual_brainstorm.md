# Hermes-as-Ego: Ritual of Rituals — Brainstorm Summary

> Generated from working session, 2026-09-18
> Status: Draft for team review. Not a spec. A thinking document.

---

## 1. Architecture

A four-seat deliberative structure:

| Seat | Who | Role |
|------|-----|------|
| **Id** | *(TBD — see §6)* | Divergent ideation — fast, raw, first |
| **Ego** | Hermes (Nous/Pineal) | Executive state management, sub-ritual spawning, reality tests |
| **Superego** | Clara (Deliberative, CLIPS/Prolog) | Constraint, veto, narrowing |
| **Id Analyst** | Clara (Observer) | Meta-observation, no vote, hard firewall from Superego |

Clara holds two seats in **separate context windows** — state leakage between them invalidates the structure.

### Why "Ego" fits Hermes

Psychopomp. The one who stands *between* and says "yes, but *how*, *when*, and *what are the consequences*." Not the storm (Id), not the voice from the attic (Superego). The messenger who also does the reality check.

---

## 2. Key Decisions (Agreed)

- **Gating:** `caws_offer` / `caws_await` — no Kafka control plane. Fail-closed on timeout (timeout → false).
- **Single side-effect tool:** `request_action(Action, Params, Justification)`. Bypass is structurally impossible by construction, not by audit.
- **Scoping invariant (one-liner for doc header):**
  > Hard veto governs side-effecting dispatch. Advisory philosophy governs creative content. Different axes. Not in tension.
- **No auto-placement.** No full Robert's Rules per action.
- **Superego reuse:** `caws_tristate/3` for approve / deny / abstain.
- **No new coordination mechanism** next to the one Rituals already use.

---

## 3. Phased Rollout

| Phase | Goal | Acceptance Criterion |
|-------|------|----------------------|
| **0 — Spike** | Probe Pineal Hermes wire format | Send msg via HTTP → receive tool-call response → execute → produce 1-page addendum with exact request/response shapes. Should take an afternoon. If it doesn't, wire format is more complex than expected and Phase 1 timeline shifts. |
| **1 — Ungated** | Prove cross-host text-in/text-out | **Also:** confirm Pineal Hermes native tool set is empty or read-only. Document what it is. "Ungated" ≠ "sandboxed" on the Pineal side. |
| **2 — Gated** | Wire Superego veto path (`approve_action/4`) | End-to-end: Id proposes → Ego evaluates → Superego votes → action fires or is denied. |

---

## 4. Open Questions (Ranked by Stall-Risk)

1. **Superego sharing model** — one shared seat vs. one per Ego.
   - Shared → serialisation bottleneck at the gate.
   - Per-seat → N deliberative analysts running concurrently; cost + risk of inconsistent verdicts on the same action.
   - **Must decide the *shape* before Phase 2 starts**, even if exact predicate placement waits.

2. **Denial handling** — what does the Ego do when Superego says no?
   - Retry with different params?
   - Report back to Id for re-consideration?
   - Escalate to user?
   - Affects loop termination *and* user-facing experience.
   - An agent that gets denied and silently retries 3× is a different failure mode than one that escalates.

3. **Kill-switch / teardown** — the 1,393-subagent incident makes the fan-out cap non-negotiable.
   - Cap is evaluator-side (we control it).
   - Teardown is Hermes-side (at the mercy of whatever API the agent runtime exposes).
   - If there's no cancel API, "cap" = "stop sending new requests and hope the existing ones finish." Weaker guarantee. Needs its own design pass in Phase 1.

4. **Superego context quality** — what does `approve_action/4` actually see?
   - Minimum: action, params, justification.
   - Better: Id's original impulse, Ego's reasoning chain, full deliberative context.
   - Quality of the gate = quality of the context handed to it.

5. **Latency asymmetry** — Id is fast, Ego is slow, Superego is slowest.
   - Synchronous ritual = bottleneck factory.
   - Recommendation: asynchronous epochs. Id → Ego → Superego, each on its own schedule. Discrete, finite, bounded.

6. **Who governs the governors?** — tie-break rule before wiring agents.
   - What happens when Id and Ego deadlock and Superego is silent?
   - Mediators mediating between mediators, one layer deep.

7. **Termination criteria** — formal "Motion to Adjourn" from Superego, not "the LLM stopped talking."

---

## 5. What's Solid

- `caws`-based gating (not a new control plane)
- Single-tool constraint (bypass hard by construction)
- Phased rollout (don't bet the stack on one unknown)
- Explicit "what we won't do" list
- Scoping discipline
- The two load-bearing decisions (caws gating, single-tool) are the right ones

---

## 6. The Id — Current State & Design Tensions

### 6.1 Fringe Consensus (new algorithm)

**Idea:** Drive low-scoring but widely-popular (among participant suggestions) responses into the creative thinking process.

**Why it's clever:**
- Detects *convergence without consensus*.
- Finds the idea where no single participant thinks it's great but many independently arrived at it.
- "Distributed insight" — the idea is in the cultural fringe, just below "I'd propose this to the room," but it's *there* in a lot of heads.
- Not the best idea. The idea that's *almost* consensus. More useful target for an Id.

**Risks:**
- **Self-referential scoring.** If scoring and popularity are both LLM-derived, "low scoring but widely popular" is partially a measure of *model uncertainty*, not idea quality. Watch for Fringe Consensus surfacing the same 3–4 "uncertain but familiar" idea families → accidental diversity *reducer*.
- **Volume vs. latency.** N independent generation passes before fringe analysis can run. Expensive and slow. Consider a **fast path** for low-stakes actions (single-pass, skip Fringe Consensus) and reserve full multi-pass for high-stakes or novel situations.

### 6.2 Pipeline Stage vs. Voice

**Current framing:** Id = "drive suggestions for action to be considered by Ego." That's a *pipeline stage*.

**Problem:** The Id in the triad isn't a pipeline stage — it's a *voice*. A pipeline stage produces output and stops. A voice *continues to exist*, keeps having impulses, keeps reacting to what Ego does and what Superego vetoes.

**Key question:** Does the Id persist across the Ritual? Does it react to the Superego's veto with a new impulse? Does it get *frustrated* when its fringe ideas keep getting rejected and that frustration feeds the next generation?

- If yes → dynamic system where Id's *state* changes based on deliberation. Genuinely interesting.
- If no → a very sophisticated autocomplete.

### 6.3 Output Shape Recommendation

Let the Id output **proposals with impulse-rationale**, not just actions:

```
proposal: contact the vendor about the SLA breach
impulse: frustration with being treated as a low-priority account
fringe_signal: 4/6 agents independently raised "escalate externally"
               without any of them scoring it above 0.4
```

This gives:
- **Ego** the *action* to evaluate and the *impulse* to reason about.
- **Superego** a richer surface: not just "is this action permissible?" but "is this impulse *proportionate*?"

### 6.4 Structural Mismatch

Id is supposed to be *divergent, impulsive, wild*. But "suggest an action for Ego to consider" is a *convergent, bounded* task.

**Resolution:** Id proposes *shapes of response* (divergent), Ego narrows to a specific action (convergent). The Id doesn't produce `send_email(to=X, body=Y)`. It produces "escalate externally / reframe the relationship / walk away / counter-propose with different terms / …"

---

## 7. Id Analyst — What to Watch

The interesting observation isn't "the Id proposed X." It's **why Fringe Consensus selected X and not Y**.

The Analyst should observe the *selection process*:
- Which ideas were in the fringe
- Which got cut
- What the score/popularity distribution looked like

If the Analyst only sees final proposals, it's a stenographer, not an analyst.

---

## 8. Focus for Next Steps

> Resolve these four and the rest is plumbing:
> 1. **Superego sharing model** (shape decision — ripples into cost, consistency, latency)
> 2. **Denial-handling path** (affects termination + UX)
> 3. **Kill-switch / teardown mechanics** (Hermes-side, not ours to control)
> 4. **Wire format** (Phase 0 spike — should take an afternoon)

Plus:
> 5. **Id persistence model** — pipeline stage or voice? Decide before Phase 2.
> 6. **Fringe Consensus fast path** — define which actions skip multi-pass analysis.

---

## 9. One-Line Invariant

> **Hard veto governs side-effecting dispatch. Advisory philosophy governs creative content. Different axes. Not in tension.**

---

*End of summary. Next: team review, then Phase 0 spike.*
