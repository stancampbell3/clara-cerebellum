# HermesAgentEvaluator / Ego integration — addendum (2026-09-20)

Status: **draft for team review — resolves three open questions from `hermes_agent_evaluator_plan.md`
(2026-09-18); still no code written.**

This addendum should be read alongside `docs/hermes_agent_evaluator_plan.md`. It does not replace that
document — it resolves three of its open questions, flags a documentation risk found while reviewing it,
and sharpens the Phase 0/2 sequencing accordingly.

## 1. Documentation risk found this session

A richer, unreviewed brainstorm pass — `hermes_ego_ritual_brainstorm.md` and `hermes_ego_ritual_summary.md`,
both dated 2026-09-18, the same day as the original plan doc — currently exists **only** inside lildaemon's
gitignored container runtime scratch space
(`lildaemon/workspace/instances/clara_mind_splinter-506004bb/{workspace,uploads/ad038a6d-...}/`), not in the
repo. This is ephemeral space (container scratch, cleared on churn), so this material is at real risk of
being lost. It is distinct from the source brainstorm transcripts the plan doc already cites
(`clara_brainstorm_agent_enablement_hermes.md` + `_options.md`), which are safely committed
(`lildaemon@ecd204c`).

This newer pass adds several ideas worth preserving even though it's raw/unreviewed, same caveat as the
original brainstorm docs:
- A **hard-firewalled Id Analyst** seat — separate context window from the Superego seat even though both
  are "Clara," meta-observation only, no vote.
- An **"asynchronous epochs"** model for the Id→Ego→Superego latency asymmetry (Id fast, Ego medium,
  Superego slowest) — each stage runs on its own schedule instead of one synchronous round-trip chain.
- **"Motion to Adjourn"** as a formal Superego-issued termination criterion, replacing "the LLM stopped
  talking" as the implicit stop condition.
- A **Fringe Consensus** algorithm proposed for the Id stage (surface ideas that are widely,
  independently suggested but that no single participant scores highly — "convergence without consensus").
  Interesting, but Id-quality work — out of scope here (see §3 below).

**Recommendation**: copy both files into `clara-cerebellum/docs/` and commit, parallel treatment to the
already-committed source brainstorm transcripts, before they're lost to container cleanup. Not done as part
of this addendum — flagged for a follow-up action, since it's a plain file copy + commit, not a design
decision.

Everything else the original plan doc cites as precedent was re-verified against current code this session
and has not drifted since 2026-09-18: `Evaluator`/`Offering`/`Hohi`/`Tabu`/`Tephra`
(`goat/models/FieryPit.py`), `caws_offer`/`caws_squawk`/`caws_await`/`caws_consult`
(`clara-prolog/prolog-lib/the_coire.pl:161,180,195,208`), `caws_tristate/3`
(`deliberative_analyst.pl:153`), `toolified_ollama.py`'s tool-dispatch/`workspace_dir` pattern, and the
`evaluators.yaml` registration shape. No `AgentEvaluator`/`HermesAgentEvaluator` code exists anywhere yet;
no Phase 0 spike has been run.

## 2. Decision: Superego sharing model — resolves plan doc §3 / open question #3

**One dedicated Superego seat per Ego seat. Not a shared, always-on gate.**

Recorded verbatim, this is the load-bearing reasoning: *"we can't assume that evaluators will be stateless
as they may choose to encapsulate stateful behavior... one superego per ego seat as part of a single ritual
or a ritual of rituals."*

Concretely:
- Every `HermesAgentEvaluator` (Ego) seat gets its own paired `deliberative_analyst`-style Superego seat.
  They join together and leave together, both scoped to the identity (context + configuration) of one
  Ritual performance.
- This extends a pattern already shipped one layer down: per-instance identity isolation (`instance_id` +
  `workspace_dir`, see `fierypit_evaluator_workspace_isolation` — multiple seats spawned from one
  `evaluators.yaml` registration already get distinct filesystem identities) from "isolated filesystem" to
  "isolated Ritual-participant identity/config."
- Practical payoff for §3's `request_action` dispatch: the `caws_offer` target for a given Ego is always
  *its own* paired Superego. No arbitration or queueing across concurrent Egos sharing one gate, and no
  risk of two Egos getting inconsistent verdicts from a shared seat under load.
- **This does not require resolving "single Ritual vs. Ritual of Rituals" as a separate question first.**
  The three-cooperating-Rituals "Ritual of Rituals" vision named in `assistant_demo.md` is **not built
  anywhere yet** — `id_analyst.pl`'s real brainstorm/committee-referral machinery (`brainstorm_step/10`)
  runs entirely within one Ritual performance today, not as spawned sub-Rituals. The 1:1 pairing principle
  holds either way this eventually lands: co-seated in one Ritual now, or paired-and-joined-together across
  sub-Rituals later if/when Ritual-of-Rituals composition actually gets built.

## 3. Decision: denial handling — resolves brainstorm docs' open question #2

**Escalate to the user. No silent retry, no auto-loop back to Id.**

When the paired Superego denies a `request_action` (or `caws_await` times out → fail-closed), the Ego
surfaces the denial and its justification directly to the person. This was chosen over both automatic
bounded retry (an agent that gets denied and silently retries is flagged in the brainstorm material as a
materially worse, silent failure mode) and looping the denial back into a fresh Id round (ruled out by §4
below, since that would require Id to be stateful/reactive).

**Concrete UX consequence**: the original plan doc's §4 open question ("should the frontend surface an
intermediate 'pending Superego approval' status?") now has a natural companion answer — add a terminal
**"denied"** outcome, delivered through the same pending-research peek/ack channel
(`clara-frontdesk-poc/src/ws.rs:350-448`) already used for `deferred_query`/`deliberate` outcomes, mirroring
how `deliberate` itself was added as a new `kind` alongside `research` (see `assistant_demo_progress`
memory). No new delivery mechanism needed.

## 4. Decision: Id persistence model — explicit non-goal for this plan

**Id stays a stateless pipeline stage.** `id_analyst.pl`'s existing per-turn `brainstorm_step/10` behavior
is preserved as-is — no persistent Id-voice-with-memory-of-prior-vetoes.

User's own framing: *"we're not generating options from Id with the proper level of quality at this
point... let's preserve the function but understand it's not quite right."* The Ego/Superego (Hermes)
integration work should not block on, or attempt, an Id quality pass (Fringe Consensus or otherwise) —
that stays a clearly separate, not-yet-scoped future item. Landing the Ego work is not an implicit cue to
also fix Id.

## 5. Also flagged, not yet acted on: `caws_tristate/3` promotion

`caws_tristate_promotion_research` already names "next time the Hermes/Ego plan touches `caws_tristate`" as
the trigger to promote it out of being a 4×-copy-pasted ruleset helper and into `the_coire.pl` proper.
Phase 2's `approve_action/4` will be a 5th copy if built the same way as the others — this is that trigger.
**Recommendation**: do the promotion in the same change that writes `approve_action/4`, rather than adding
another copy.

## 6. Sequencing update

Phase 0 and Phase 1 are unchanged in substance from the original plan doc, now sharpened by the decisions
above; Phases 2–4 gain concrete shape from §2/§3.

- **Phase 0 (spike, no repo changes, unchanged goal).** Probe the live Pineal Hermes install for: real
  tool-call wire format, native confirmation hook, and — newly informed by the brainstorm material — any
  cancellation/kill API for in-flight sub-agent trees, since the kill-switch/teardown design (plan doc open
  question #6) depends entirely on that answer. Requires real access to the Pineal host (same class of
  blocker PitBoss hit before SSH access was set up); not executable from a context-less sandboxed session.
  Output: a short addendum to the plan doc, not code.
- **Phase 1 (unchanged).** `AgentEvaluator` base + `HermesAgentEvaluator` skeleton, ungated, single Ego
  seat, no Superego pairing yet (nothing side-effecting to gate). Fan-out safety cap included from day one
  regardless.
- **Phase 2 (now concretely shaped by §2/§3).** `request_action` tool-proxy + `approve_action/4`, built
  directly against the 1:1 Ego:Superego pairing from the start — every test Ego seat in the standalone
  verification Ritual joins with its own dedicated Superego seat, not a shared one retrofitted later.
  Denial path implements "escalate to user" concretely (in the standalone test Ritual: a clear
  logged/printed denial outcome) before Phase 3 wires it to real delivery. Includes the `caws_tristate`
  promotion from §5.
- **Phase 3 (unchanged goal, denial-UX question now resolved).** Wire in as frontdesk-poc's Ego stage.
  Denial escalation reuses the pending-research peek/ack channel with a new terminal "denied" outcome kind
  (§3).
- **Phase 4 (unchanged).** Audit-trail Kafka topics as an observability layer, latency-budget tuning,
  revisiting PitBoss auto-placement.

## Open questions still carried forward, unresolved by this addendum

1. Hermes's real tool-call wire format (Phase 0 spike).
2. Whether Hermes has a usable native confirmation hook (Phase 0 spike).
3. Whether Hermes exposes any sub-agent/task cancellation API — new item, needed for kill-switch design
   (Phase 0 spike).
4. Exact placement/naming of the Superego approval predicate — likely `approve_action/4` alongside the
   `caws_tristate` promotion (§5), but not finalized.
5. Superego decision-latency budget against existing deduction-cycle/httpx/Kafka timeouts (Phase 4, sizing
   still useful earlier).
