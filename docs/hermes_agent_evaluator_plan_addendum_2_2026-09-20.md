# HermesAgentEvaluator / Ego integration — addendum 2 (2026-09-20)

Status: **draft for team review — folds Clara's feedback into the plan; still no code written.**

Read alongside `hermes_agent_evaluator_plan.md` and `hermes_agent_evaluator_plan_addendum_2026-09-20.md`
(addendum 1). Input reviewed: `clara_feedback_hermes_agent_integration_plan.md` and
`clara_feedback_hermes_agent_integration_190926.md`. Clara reviewed `hermes_ego_ritual_summary.md`, which
predates addendum 1, so some of her advice contradicts decisions already made.

> **Invariant:** Hard veto governs side-effecting dispatch. Advisory philosophy governs creative content.
> Different axes. Not in tension.

> **Note:** The Freudian labels (Id, Ego, Superego) are mnemonics only. They impose no behavioral constraint
> beyond each seat's functional spec.

## 1. Rejected: contradicts decisions already made in addendum 1

- **Shared, serialized Superego (Clara's #2).** Addendum 1 §2 decided one Superego seat per Ego seat, because
  evaluators may be stateful. A Prolog-`assert` FIFO queue would also reintroduce the clause-accumulation
  hazard already hit in the rumination-ingest work.
- **Denial retry / loop to Id.** Addendum 1 §3 decided: escalate to the user.
- **Id contract (`id_proposal/4`, Clara's #1).** Addendum 1 §4 keeps the Id stateless and out of scope, and
  `id_analyst.pl` already emits `Alternatives`. Clara's contract also puts `SideEffects` on the Id; side
  effects only appear when the Ego calls `request_action`.
- **`failure_mode/3` as enforcement.** The `caws_await` timeout is the enforcement. A table is
  documentation only.
- **The 4-week estimate.**

## 2. New decisions

1. **Hermes built-in toolsets disabled is a hard Phase 0 gate.** `request_action` as the only side-effecting
   tool is "hard by construction" only if Hermes' native toolsets (terminal, web, file, etc.) are off. Phase 0
   must show Hermes can run with them disabled (config or container). If it cannot, the gate design is
   revisited before any Phase 1 code. This replaces the Phase 1 "confirm native tool set" check.
2. **Kill-switch: one Hermes container per Ego seat**, hard-stopped by the evaluator and bounded by cgroup
   limits. This makes the evaluator-side fan-out cap a real guarantee independent of Hermes internals. It is
   the assumed design unless Phase 0 finds a real sub-agent cancel API. This resolves plan doc open question #6.

## 3. Adopted from Clara

- **"Structurally deterministic; content-stochastic."** Replace "deterministic" in the epoch description.
  Tests assert structure (turn order, bounds, ids), never content.
- **Tiered Superego context, independently sourced.** Tier 1 (always): action, params, justification, ritual
  state. Tier 2 (on request): richer context. Tier 3 (rare): full transcript. The Ego is the gated party, so
  the Superego must not take Tier 2 from it: the user's original request and the Id's `Alternatives` come
  from the Ritual channel. Only action, params, and justification come from the Ego. The per-Ego Superego
  seat can track its own ritual state.
- **`ritual_id` + `seq` message envelope from Phase 1**, so failed rituals can be replayed. Full audit
  topics stay Phase 4.
- **Params handling.** Hermes emits JSON; parse to data, validate against a per-action allowlist and param
  schema, never interpolate into goals. A free-text `Params` string is rejected. Plan around the known
  clara-prolog FFI dict-serialization gap.
- **Firewall is a structural property**: distinct seats and `instance_id`s, no shared Coire subscriptions, no
  shared Prolog state. A prompt string-match test is a cheap extra, not the evidence. The Observer / Id
  Analyst seat is deferred; it is not on the Ego critical path.
- **User override** is an authenticated frontdesk WS action that re-issues the request. It is not a Prolog
  predicate reachable by the Ego, and it does not change the Superego's rules.
- **Governor question:** answer is the user plus timeout-to-deny. No fourth agent.

## 4. Phase 0 gate criteria (updated)

Phase 0 passes only if the spike documents all of:
1. Real tool-call wire format (exact request/response shapes).
2. Native toolsets can be disabled (new hard gate, §2.1).
3. Whether a native confirmation hook exists and what it does.
4. Whether any sub-agent cancellation API exists (if not, §2.2 applies).

Output remains a short addendum, not code. It requires real access to Pineal.

## 5. Sequencing impact

- Phase 1 gains: container-per-Ego-seat evaluator deployment, and the `ritual_id`/`seq` envelope.
- Phase 2 gains: allowlist/schema param validation, independently sourced Superego context, structural
  firewall tests. `caws_tristate` promotion (addendum 1 §5) still lands in the same change as `approve_action/4`.
- Phase 3: user override added to the denial-escalation path in `ws.rs`.
