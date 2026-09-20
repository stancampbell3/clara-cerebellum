# Feedback on "Hermes-as-Ego: Ritual of Rituals"

## Overall Impression

This is a well-scoped, honest design doc. The phasing discipline is the best part — you're not betting the architecture on the wire-format question, and the "afternoon-scale" Phase 0 acceptance criterion is exactly the right granularity. The single-tool constraint and the hard-veto/advisory scoping rule are genuinely strong decisions. I'd call this a 7/10 draft: solid bones, a few load-bearing gaps that will bite in Phase 2.

---

## What's Strong

**The single-tool constraint is the best decision in the doc.** Making `request_action/3` the *only* side-effecting tool and making bypass "hard by construction, not by audit" is the right call. It collapses the attack surface to one predicate and gives the Superego a single chokepoint to police. Every multi-agent system that tries to be "flexible" about which tools can do what ends up with audit nightmares. You avoided that.

**The scoping invariant (Section 7) is clean.** "Hard veto governs side-effecting dispatch. Advisory philosophy governs creative content. Different axes. Not in tension." This is the sentence that prevents the most common failure mode in these systems — the Superego becoming a thought-police that slows down ideation. Putting it in the header is correct.

**Phase 0 is right-sized.** One afternoon, one HTTP round-trip, one-page addendum. That's a spike, not a project. Good.

**The "Ungated ≠ sandboxed" caveat in Phase 1** is the kind of thing that prevents a week of confusion later. Good instinct.

**The open questions are honestly ranked.** Q1 (Superego sharing) and Q3 (kill-switch) are genuinely the two that will stall you, and you've identified them.

---

## Gaps and Concerns (in priority order)

### 1. The Id is the weakest link and it's not in the open-questions list

The Id is marked "TBD — needs explicit definition" in the table, but it doesn't appear in Section 4. That's a ranking error. You have seven open questions about Superego mechanics, denial paths, and teardown, but zero about *what the Id actually is*. Concretely:

- What model / prompt generates Id proposals?
- What's its output schema? (A list of N proposals? A single raw impulse?)
- How does it terminate? (Max N proposals? Ego says "enough"?)
- Is it a separate LLM call, or a temperature-1.2 pass over the same model?

You can't design the Superego's context (Q4) or the denial path (Q2) without knowing what the Id emits. I'd move this to **Q1** and push the others down.

### 2. The Clara firewall is asserted, not designed

> "These are separate context windows with a hard firewall. State leakage between them invalidates the whole structure."

I agree it invalidates the structure. But the doc doesn't say *how* the firewall is enforced. Options:

- Two separate process instances of Clara_Cerebellum (strongest, but 2x memory)
- Two context windows in one process with a context-manager guard (medium)
- Two separate API sessions to the same model (weakest — same weights, same "personality," and if the model has any shared state or session memory, the firewall is fiction)

This needs a sentence in Phase 2 design. "Hard firewall" should be a testable property, not a hope. I'd suggest: **the Id Analyst's prompt must not contain the Superego's rulebook, and the Superego's prompt must not contain the Id Analyst's notes. Verify by string-matching the system prompts in the test suite.**

### 3. Failure-mode analysis is missing

You have fail-closed via timeout-to-false, which is correct. But the doc doesn't enumerate what happens when:

| Failure | Expected behavior |
|---|---|
| Clara (Superego) is down | Timeout → deny. Ego proceeds without the action. ✓ (implied) |
| Clara (Observer) is down | Should be invisible. Ritual proceeds. But does the *absence* of observer notes get logged? |
| Hermes (Ego) is down | Who notices? Who tears down? The evaluator side? |
| Network between Pineal and Clara drops mid-ritual | In-flight `caws_await` times out → deny. But what about a sub-agent tree already spawned? |
| Id produces a proposal that's a prompt injection targeting the Superego | The Superego is CLIPS/Prolog, not an LLM, so this is less of a risk than it would be. But if the *params* field is free-text, it could carry adversarial content into `approve_action/4`. |

The last one is worth a sentence. If `Params` is a structured type (Prolog term), you're fine. If it's a string, you need sanitization before it hits the rule engine.

### 4. Q4 (Superego context) needs a concrete minimum

You're right that `(action, params, justification)` is a weak gate. But "should it see everything?" is too open-ended. I'd propose a tiered context:

- **Tier 1 (always):** `Action`, `Params`, `Justification`, current ritual state (what's been approved/denied so far)
- **Tier 2 (if Superego requests):** Id's original proposal, Ego's reasoning chain
- **Tier 3 (rare, explicit escalation):** Full deliberative transcript

The Superego should be *able* to request Tier 2 via a `request_context/2` predicate, but not *required* to see it by default. This keeps the common case fast and the edge case thorough.

### 5. Q5 (latency) — drop "deterministic"

> "Discrete, finite, bounded, deterministic."

LLM-based agents are not deterministic. You can make the *epoch structure* deterministic (fixed number of epochs, fixed turn order), but the *content* of each epoch is stochastic. I'd reframe:

> "Discrete, finite, bounded. Structurally deterministic; content-stochastic."

This matters because if someone reads "deterministic" and builds a test harness that asserts exact outputs, they'll be very confused.

### 6. Q6 (governors) — the answer is probably "the user"

You ask "who governs the governors?" The cleanest answer is: **the user, via timeout-to-deny.** If Id and Ego deadlock and Superego is silent (or times out), the action is denied (fail-closed), the ritual is logged, and the user is notified. The user can override.

This is less elegant than a "Meta-Superego" or "Chair of the House," but it's honest. You don't need a fourth LLM to break a tie between three. You need a timeout and a notification. Keep it simple.

### 7. No observability / debugging plan

Three agents talking means the interesting bugs are in the *interactions*, not any single agent. The Id Analyst helps, but the doc doesn't say:

- What's the log format? (Structured JSON per message? Prolog trace?)
- Can you replay a ritual from the log?
- Is there a "ritual ID" that threads through all three agents' logs?

This isn't a Phase 2 concern, but it should be in Phase 1. If you can't replay a failed ritual, you can't debug it.

---

## Smaller Notes

- **The Freudian metaphor is a label, not a constraint.** Make sure the team doesn't start thinking "the Id should be irrational" or "the Superego should be moralistic." The functional spec is in the table; the Freudian names are just mnemonics. Worth a one-line note so nobody over-identifies.

- **Cost model is absent.** Three LLM calls per action (Id + Ego + Superego, even if Superego is Prolog) is meaningfully more expensive than a single agent. At what action-per-minute rate does this get painful? A rough estimate in the doc would help set expectations.

- **The "1,393-subagent incident" reference is great context.** If this is going into a shared doc, make sure the incident is linked or summarized in an appendix. New readers won't have that scar tissue.

- **Phase 2 acceptance criterion says "Superego sharing model resolved" but doesn't say *which* model.** I'd pick one now (I'd say: **shared Superego, serialized** — consistency > throughput at your scale) and write it in. You can revisit, but "resolved" in an acceptance criterion should mean "decided," not "discussed."

---

## Suggested Re-Ranking of Open Questions

| Rank | Question | Why |
|------|----------|-----|
| **1** | **Id definition** *(new — promote from table footnote)* | Can't design the other two seats without knowing what the Id emits |
| **2** | Superego sharing model | Must be decided before Phase 2; I'd recommend shared/serialized |
| **3** | Denial handling | Affects loop termination and UX; needs a concrete state machine |
| **4** | Kill-switch / teardown | The 1,393 incident makes this urgent, not theoretical |
| **5** | Wire format (Phase 0) | Afternoon-scale, but it gates everything |
| **6** | Superego context quality | Tiered context (see above) |
| **7** | Latency / epochs | Important but solvable with a simple turn scheduler |
| **8** | Tie-break / governor | Answer is "user + timeout-to-deny" |
| **9** | Termination criteria | "Motion to Adjourn" + Ego reality-test veto |

---

## Bottom Line

The architecture is sound, the phasing is disciplined, and the scoping is honest. The two things I'd fix before the team review: **(1)** define the Id (or at least pin down its output contract), and **(2)** make the Clara firewall a testable property, not a stated intention. Everything else is Phase 2 work that you've correctly deferred.

The one-line invariant in Section 7 should go in the doc header. It's the sentence that prevents the most common scope-creep failure in this kind of system.
