# HermesAgentEvaluator: planning document

> **Superseded for review by `hermes_agent_evaluator_plan_v3.md` (2026-09-21).** Kept as history.

Status: **draft for team review — not yet implemented.**

## Context

Clara's frontdesk-poc "model of mind" currently ships two of three named stages:

- **Id** (`lildaemon/goat/app/assistant/rulesets/id_analyst.pl`) — unconditional, generates
  abundant raw impulses, no judgment. See `id_ritual_of_rituals_planning.md`.
- **Superego** (`lildaemon/goat/app/assistant/rulesets/deliberative_analyst.pl`) —
  Robert's-Rules-style deliberation and voting. See `lildaemon/docs/ritual_roberts_rules_example.md`.

The **Ego** stage — the thing that would take Id's raw `Alternatives`, formalize one into an
actionable Motion, and actually *act* on the world (not just talk) — is explicitly called out
as unbuilt in `id_ritual_of_rituals_planning.md` Part 4: "The Ego stage itself is explicitly
out of scope for this pass"; "`id_step/4`'s structured `Alternatives` never escapes its own
Prolog call today."

This document plans a `HermesAgentEvaluator` — a Ritual participant backed by a real
tool-using agent (Hermes, from Nous Research) — to fill that gap. It is regarded as a **gated
executor**: the Ego in an Id/Ego/Superego triad, where the Superego approves the agent's
intentions/tool uses before they're acted on.

A prior brainstorm chat with Clara (`lildaemon/docs/clara_brainstorm_agent_enablement_hermes.md`,
plus its companion `clara_brainstorm_agent_enablement_options.md`) already explored this exact
triad and sketched a gating mechanism. **That chat is explicitly not a decision** — raw
exploratory output to draw on, not settled architecture. This doc treats it as one input among
several, cites it where it's genuinely useful, and calls out where it disagrees.

The Hermes install itself is **experimental and lives only on host Pineal** — it is not
checked into this repo at all. The only trace of it here is a context-window tweak,
`clara-cerebellum/models/Modelfile.qwen-clara-hermes` (bumps `num_ctx` to 65536 over
`qwen-clara:latest`'s 32768 — nothing about chat template or tool-call format). That means
this integration is **cross-host from day one**. The existing FieryPit/PitBoss registry
(`fierypit_registration_plan.md`) covers cross-host *discovery* but explicitly not
*auto-placement* — a Ritual node must still hand-type the remote FieryPit's URL today. This is
a known, separately-tracked gap ("Hermes→FieryPit/Dis mapping," flagged 2026-09-17, not yet
scoped) — this plan routes around it rather than solving it.

Throughout, this doc tags each point as:
- **[precedent]** — already exists, reused as-is,
- **[recommendation]** — this doc's own proposed choice, not yet built,
- **[open]** — genuinely unresolved.

## 1. `AgentEvaluator` (abstract base) — [recommendation]

New file: `lildaemon/goat/evaluators/agent_evaluator.py`.

Subclasses `Evaluator` (`lildaemon/goat/models/FieryPit.py:109`), implementing the same
contract every evaluator uses **[precedent]**:
- `Offering` (`FieryPit.py:26`) in, `Tephra` (`FieryPit.py:70`) out, wrapping either a `Hohi`
  (`FieryPit.py:41`, success) or a `Tabu` (`FieryPit.py:57`, error).
- `evaluate(self, offering) -> Tephra` is the only abstract method (`FieryPit.py:192`);
  `evaluate_async` is convention, not enforced by the ABC.

Because the contract is unchanged, `AgentEvaluator` drops into `RitualParticipant`,
`GoatWrangler`, and `Disdomain` with **zero Ritual-side changes** — the same way
`CowEvaluator` (non-LLM) and `GroqEvaluator` (different LLM backend) already do.

What distinguishes `AgentEvaluator` from `ToolifiedOllamaEvaluator`
(`lildaemon/goat/evaluators/toolified_ollama.py`) is that it delegates its whole
reasoning+tool loop to an **external agent runtime**, rather than hand-rolling a `think()`
loop (`toolified_ollama.py:1040`) against a raw chat-completions API. `AgentEvaluator` itself
should own, regardless of which concrete agent backend it wraps:

- A **per-turn fan-out safety cap** — max tool calls and/or max sub-agent spawns, enforced
  evaluator-side and never trusted to the agent runtime. This directly mitigates a real
  incident cited in the brainstorm doc: Hermes spawned 1,393 subagents in a single task with
  no operator-side backstop. This is non-negotiable for phase 1 — an unbounded agent loop is
  the single largest operational risk in this whole plan.
- Per-instance `workspace_dir` sandboxing, reusing the existing convention
  **[precedent]** (`toolified_ollama.py:763-791`, keyed by `instance_id` so many Ritual seats
  spawned from one YAML registration don't collide).

## 2. `HermesAgentEvaluator` (concrete) — [recommendation], wire format [open]

Talks to the Pineal-hosted Hermes agent process over HTTP. Registered in `evaluators.yaml`
the same way as `clara_mind_splinter_groq` **[precedent]**:

```yaml
# config/evaluators.yaml:150-192, pattern to follow (values illustrative)
- name: hermes_agent
  module: goat.evaluators.agent_evaluator
  class: HermesAgentEvaluator
  parameters:
    base_url: http://<pineal-host>:<hermes-port>
    model: qwen-clara-hermes:latest
    max_tool_calls: <TBD from phase 0 spike>
  metadata:
    tools: []   # request_action is the only tool — see §3
```

Key difference from every other registration today: this evaluator should run as **its own
FieryPit process on Pineal**, not colocated with the main lildaemon stack, since Hermes is
GPU-bound there. Cross-host reach goes through the already-shipped registry **[precedent]**:

- `FieryPitRegistry` (`clara-api/src/fierypit_registry.rs`) — in-memory, heartbeat-repopulated
  registration keyed by `uuid5(base_url)`.
- `DisClient.register_fiery_pit()` (`lildaemon/goat/app/dis_client.py`) auto-advertises on
  startup/shutdown via a periodic heartbeat.
- Joining is still manual: a Ritual node names Pineal's FieryPit URL explicitly and joins via
  `fiery_pit_peer_client.join_remote()`, the same pattern documented and live-verified
  2026-09-02 in `docs/remote_fierypit_deployment.md`. Auto-placement ("PitBoss's fuller
  vision") is out of scope for this plan.

**[open] Hermes's actual tool-call wire format is unknown from this repo.** Nothing checked
in shows whether it speaks OpenAI-style `tool_calls`, its own XML/JSON tool-call convention,
or something else entirely — `Modelfile.qwen-clara-hermes` only changes context length. Do not
assume it mirrors `ToolifiedOllamaEvaluator`'s `tool_calls`/xLAM/JSON-in-text fallback chain
(`toolified_ollama.py:620-943`) without checking. This must be resolved by a live-Pineal spike
(Phase 0, below) before `HermesAgentEvaluator`'s request/response translation layer is written.

**[open]** Whether Hermes Agent already has its own native "confirmation hook" for gating tool
use. The brainstorm doc dismissed this as "least invasive, most fragile" and recommended
building an independent tool-proxy gate instead (§3) — but that call was made without
verifying what the native hook actually does. Re-examine it during the Phase 0 spike; if it
turns out to be solid, it may simplify §3 rather than being redundant with it.

## 3. Gating: Ego → Superego over `caws_offer`, not a bespoke Kafka control plane — [recommendation]

The brainstorm doc proposes a set of Kafka topics as the gating mechanism itself
(`id.impulses`, `ego.requests`, `superego.decisions`, `ego.results`, `superego.audit`). This
doc recommends **against** that as a *control* plane. This codebase's own prior conclusion,
in the brainstorm's companion options doc, already leans toward using Clara's native
`caws_offer`/`caws_await` primitives as "the plumbing *inside* Clara" rather than bolting on
an external orchestration framework (LangGraph/CrewAI/AutoGen/etc. were all considered and
rejected for the same reason). The same logic should apply to internal gating: don't build a
second coordination mechanism next to the one Rituals already use.

Concretely:

- `AgentEvaluator`'s only avenue to real effects is a single tool,
  `request_action(action, params, justification)` — the brainstorm doc's "Option A / Tool
  Proxy" shape, chosen there specifically because it's structurally hard to bypass (the agent
  is never handed any other tool with a real side effect). This part of the brainstorm is
  worth keeping.
- Dispatching `request_action` performs a `caws_offer/4` (`the_coire.pl:161`) to a named
  Superego participant and `caws_await/2`s (`the_coire.pl:195`) the verdict.
- `caws_await`'s existing **timeout-to-false** semantics give fail-closed/default-deny-on-timeout
  for free, with zero new code — this matches the brainstorm doc's desired fail-closed
  behavior exactly.
- Superego side: reuse `deliberative_analyst.pl`'s existing `caws_tristate/3`
  (`deliberative_analyst.pl:153`, built on `the_coire:caws_result/2` /
  `the_coire:caws_failed/2`) and yield idioms, via a **new thin single-shot predicate** (e.g.
  `approve_action/4`) rather than running a full Robert's-Rules session per action (too slow
  for a tool-dispatch-rate gate) or writing a from-scratch `superego.pl`. **[open]** exact
  predicate name/placement — new file vs. a new clause set added to
  `deliberative_analyst.pl` — is a Phase 2 decision, not resolved here.

**[open] tension to name explicitly, not paper over:** `docs/creative_process_rituals.md`
deliberately treats gates as "advisory, not hierarchical — a deliberate anti-approval-loop."
This plan's hard-veto gate should be scoped specifically to real side-effecting tool dispatch
(the kind of action you can't take back — sending, deleting, spending, publishing), not to
brainstorming/creative content, where the existing advisory philosophy should continue to
apply unchanged. The doc should say this out loud so the two don't read as silently
contradicting each other.

**[open] Superego sharing model.** The brainstorm doc's own answer, credited to the user
inline: "we'll always be running several evaluators in one or more Fiery Pits through one Dis
domain for a given ritual" — i.e. the Superego "emerges from the ritual performance" rather
than being a separate always-on service. This plan adopts that framing but does not resolve
the mechanics of it (one Superego seat shared across concurrently-running Ego seats in the
same Ritual vs. one per seat) — flagged for Phase 2.

## 4. Fitting into frontdesk-poc's model of mind — [recommendation]

Position `HermesAgentEvaluator` as the concrete filler for the named Ego gap in
`id_ritual_of_rituals_planning.md` Part 4: it receives Id's raw impulses, formalizes/selects
one into a Motion (or answers directly for simple, non-side-effecting cases), and routes
anything with a real side effect through the `request_action` gate (§3) before execution.

Delivery to the user stays on the existing choke points in `clara-frontdesk-poc/src/ws.rs`
**[precedent]** — no new delivery mechanism is needed, because the gate lives *inside* the
Ritual, before a result ever reaches these points:

- Sync replies: `impl Handler<TurnResult> for FrontDeskActor` (`ws.rs:451`), pushed via
  `ctx.text(msg.to_string())` at `ws.rs:476`.
- Async replies (deferred/deliberate/brainstorm-style turns): the pending-research peek/ack
  path (`ws.rs:350-448`), which already uses safe peek-then-explicit-ack delivery (the
  "frontdesk delivery-robustness fix" from `id_ritual_of_rituals_planning.md` Part 2).

**[open]** Whether the frontend should surface an intermediate "pending Superego approval"
status to the user while a `request_action` round-trip is in flight, versus the gate being
fully invisible until a final answer/action lands. Not designed here; likely a small addition
to the existing WS message types in `ws.rs` if wanted, deferred to Phase 3.

## 5. Phased rollout — [recommendation]

- **Phase 0 (spike, no repo changes).** Probe the live Pineal Hermes install for its actual
  tool-call wire format and whether it has a usable native confirmation hook (see §2's two
  `[open]` items). Output: a short addendum to this doc, not new code.
- **Phase 1.** `AgentEvaluator` base + `HermesAgentEvaluator` skeleton, registered as a
  Pineal-hosted FieryPit, manually joined into a throwaway test Ritual, **ungated**. Goal:
  prove cross-host text-in/text-out works at all before adding gating complexity. Include the
  fan-out safety cap from day one even though nothing dangerous is wired up yet.
- **Phase 2.** `request_action` tool-proxy (§3) + minimal `approve_action/4`-style Superego
  gate predicate. Verify approve/deny/timeout-deny paths independently of the frontdesk demo
  (a standalone test Ritual, same style as the existing Robert's Rules example).
- **Phase 3.** Wire in as the Ego stage of the Id/Superego flow for clara-frontdesk-poc (§4).
  Decide the pending-approval UX question above.
- **Phase 4 (explicitly deferred — name, don't design yet).** Optional audit-trail
  Kafka topics (`superego.audit` etc., as an observability/replay layer *on top of* the
  `caws_offer`-based gate, not instead of it); latency-budget tuning for the Superego
  round-trip against existing deduction-cycle/httpx/Kafka timeouts; revisiting PitBoss
  auto-placement so Ritual nodes stop hand-typing Pineal's FieryPit URL.

## Critical files

| File | Relevance |
|---|---|
| `lildaemon/goat/models/FieryPit.py:26,41,57,70,109` | `Offering`/`Hohi`/`Tabu`/`Tephra`, `Evaluator` ABC |
| `lildaemon/goat/evaluators/toolified_ollama.py:620-943,763-791,1040` | tool-call dispatch precedent, `workspace_dir`, `think()` loop |
| `lildaemon/goat/evaluators/custom/groq_evaluator.py:18,107,209` | precedent for wrapping a different backend while reusing dispatch machinery |
| `clara-cerebellum/clara-prolog/prolog-lib/the_coire.pl:161,180,195,208` | `caws_offer`/`caws_squawk`/`caws_await`/`caws_consult` |
| `lildaemon/goat/app/assistant/rulesets/deliberative_analyst.pl:153` | `caws_tristate/3` |
| `lildaemon/docs/ritual_roberts_rules_example.md` | Superego precedent, yield/tristate idioms |
| `clara-cerebellum/docs/id_ritual_of_rituals_planning.md` (Part 4) | Id precedent + the named Ego gap this plan fills |
| `clara-cerebellum/docs/fierypit_registration_plan.md`, `clara-api/src/fierypit_registry.rs`, `docs/remote_fierypit_deployment.md` | cross-host registration/join |
| `lildaemon/docs/clara_brainstorm_agent_enablement_hermes.md`, `clara_brainstorm_agent_enablement_options.md` | source brainstorm — not a decision |
| `clara-cerebellum/clara-frontdesk-poc/src/ws.rs:451,476,350-448` | delivery choke points |
| `lildaemon/config/evaluators.yaml:150-192` | registration pattern to follow |
| `clara-cerebellum/docs/creative_process_rituals.md` | existing "advisory, not hierarchical" gating philosophy — scope this plan's hard gate against it |
| `clara-cerebellum/models/Modelfile.qwen-clara-hermes` | only known repo artifact about Hermes; context-length only |

## Open questions carried forward (do not drop)

1. Hermes's real tool-call wire format (§2).
2. Whether Hermes has a usable native confirmation hook, vs. building the tool-proxy gate
   independently (§2, §3).
3. Superego sharing model across concurrently-running Ego seats in one Ritual (§3).
4. Exact placement/naming of the Superego approval predicate (§3).
5. Pending-approval UX in the frontdesk WS protocol (§4).
6. Kill-switch mechanics for an in-flight agent that exceeds its fan-out cap mid-turn — this
   doc specifies *that* a cap must exist (§1) but not how an already-running Hermes-side
   sub-agent tree gets torn down once the cap trips. Needs its own design pass in Phase 1.
7. Superego decision-latency budget against existing deduction-cycle/httpx/Kafka timeouts
   (Phase 4, but worth sizing early so Phase 2's design isn't built around an unrealistic
   round-trip time).
