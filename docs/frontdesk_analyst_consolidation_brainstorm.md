# Frontdesk analyst consolidation — initial brainstorm

This is a **brainstorm document, not a locked design**. It exists to get Clara's own input (via the
frontdesk's analyst rulesets) and the human team's feedback before anything is decided or built. Where it
states a fact about the codebase, that fact was checked directly. Where it lays out an option, it is
deliberately not picking a winner yet.

## Why now

"Consolidate copy-pasted logic across analyst rulesets" has sat in project notes as blocked on the Ego
work landing. That gate is now open: Hermes Ego Phases 1-3, ritual-keyed session continuity, the shared
NFS mount, the per-user memory tier, and the new skill/action-classification gate route are all built,
live-verified, and merged. This is the first pass at what's next.

## The vision, in the user's own words

> within Rituals, we can specify prolog. the analyst should ideally be a complete ritual with associated
> prolog and defined participants.

> we'd basically combine the prolog and rituals of the existing analysts into a coherent conversational
> partner capable of spawning research, consulting knowledge, doing tasks (approved by superego), etc.

This is a bigger idea than deduplicating helper predicates across five files. It's collapsing today's
**classify-then-pick-one-ruleset-for-the-whole-session** model into **one Ritual** with several
capabilities available inside it, decided as needed rather than committed to up front for an entire
session. Today, `ruleset_key` is chosen once per assistant session and every turn goes down that single
path (`lildaemon/goat/app/assistant/runtime.py`'s `_RULESETS` map).

Mapping the three named capabilities to what already exists:

| Capability | Existing implementation |
|---|---|
| Spawning research | `progressive_research.pl`'s tiered escalation (`tier_groq/9`, the `deferred_query` action, background crawl via `PendingResearchPoller`) |
| Consulting knowledge | Edgequake RAG via the shared Prolog lib's `the_cow.pl`, `ruminate/2` — already loaded server-side, no cross-repo blocker for this specific piece |
| Doing tasks (approved by Superego) | `ego_analyst.pl`'s `ego_step/6` + `goat/mcp/ego_gate`'s Superego-mediated `request_action` gate, just extended with the new quick-content-review route (see `ego_skill_tier_plan.md`) |

Notably absent from that list: **deliberation** (`deliberative_analyst.pl`'s multi-model committee) and
**brainstorming** (`id_analyst.pl`'s spark/impulse generation). Whether those fold into the same unified
partner as more capabilities, or stay separate, explicitly-invoked Rituals ("convene a committee"), is a
genuinely open question this doc does not answer — see Open Questions.

## What already exists to build this on

`lildaemon/goat/app/ritual_configs/` (`models.py`, `router.py`, `store.py`, `lifecycle.py` — the
Cobbler-facing system) already lets a user define a `RitualConfig`: a `graph_layout` (nodes/edges), named
`participants` (each a `url` + `role`), and an `evaluator`. `activate_ritual_config`/`run_ritual_config`
transduce that graph into per-node Prolog/CLIPS text and run it as a real Dis Ritual. This is real, working
infrastructure today, not a future idea — the same mechanism behind the earlier "Cobbler ritual
transduction" exploration.

Every current analyst is instead a hand-written flat `.pl` file, registered as one Prolog source ad hoc
per turn — never a `RitualConfig`, never a standing multi-participant Ritual the way the Ego (its own
per-session Ritual) or the multi-participant examples (rumination-ingest/answer, progressive-consult,
Robert's Rules) already are.

## What this does NOT resolve on its own

1. `register_node_source` still produces one flat generated Prolog blob per node. Real cross-node code
   sharing still depends on the transducer's own node/edge library being reusable across configs, not on
   the `RitualConfig` mechanism by itself.
2. Today's analysts run as a single inline synchronous deduction per turn, not a standing joined Ritual. A
   unified partner needs a real lifecycle decision: one long-lived Ritual per session (like the Ego's),
   with research/knowledge/task participants all joined once and available across every turn? Per user?
   Spun up fresh per turn, like today?
3. It isn't yet confirmed whether Cobbler's graph/node vocabulary can already express what these analysts
   do today (tri-state caching, tier escalation, committee debate rounds) or would need extending first.

## The other mechanical constraint: shared Prolog libraries are cross-repo

Every ruleset file is registered whole as one Dis `prolog_source_id` — there is no per-clause loading.
`use_module(library(X))` only works for libraries the Prolog engine already loaded at boot, from
clara-prolog's own `prolog-lib/` overlay (`the_coire`, `the_rabbit`, `the_cow`, `the_rat`, `the_leannan` —
a hardcoded array in `clara-prolog/src/backend/ffi/environment.rs`). Adding a new shared module means a
**clara-cerebellum deploy change**, not a lildaemon-only edit — confirmed by precedent: `the_leannan` was
added to that array on 2026-09-11. Whatever direction this takes, promoting anything into a real shared
library crosses a repo boundary.

## The actual duplication inventory

Six ruleset files: `deliberative_analyst.pl` (639 ln), `id_analyst.pl` (622 ln), `progressive_research.pl`
(248 ln), `superego_gate.pl` (171 ln), `terse_analyst.pl` (98 ln), `ego_analyst.pl` (80 ln).

- **`extract_hohi_response/2`, `research_step/8`, `answer_step/9`** — byte-identical, 3 copies each
  (`deliberative_analyst.pl`, `progressive_research.pl`, `terse_analyst.pl`), each explicitly commented
  "copied verbatim." Pure boilerplate, no behavioral risk in merging.
- **`extract_caws_response/2`** — 4 copies, **one silently diverged**: `progressive_research.pl`'s version
  omits the `strip_think/2` call every sibling makes. This is a real, live bug independent of any
  consolidation decision — a thinking model's `<think>...</think>` block leaks unstripped through this
  file's caws leg today. **Worth fixing on its own regardless of which direction this brainstorm lands
  on.**
- **`strip_think/2`** — 3 variants, not equivalent: `deliberative_analyst.pl`/`id_analyst.pl` are
  byte-identical (handles an unterminated `<think>` block with a placeholder); `ego_analyst.pl`'s copy is
  simplified (no placeholder — falls back to raw, unstripped text on that edge case); `superego_gate.pl`
  has a genuinely different implementation (single cut, no placeholder logic); `progressive_research.pl`
  has none at all. Four behaviors under one name — the riskiest "looks the same, isn't" case; a naive merge
  changes real behavior somewhere.
- **`caws_tristate/3` + `combine_tristate/3` + `research_step_tristate/9` + `committee_deadline_for/3`** —
  2-3 copies, already flagged in `hermes_agent_evaluator_plan_addendum_2026-09-20.md` (section 5): *"the
  trigger to promote `caws_tristate/3`... is the next time the Hermes/Ego plan touches it. Recommendation:
  do the promotion in the same change that writes `approve_action/4`."* A real, existing plan thread this
  brainstorm needs to reconcile with, not duplicate.
- **No duplication found** in judge/verdict-parsing or prompt-building helpers — every ruleset builds
  prompts inline; `superego_gate.pl`'s `judge_text/2`/`judge_verdict/3` have no analyst-side counterparts.
  Not a consolidation target.
- **Duplication as a fraction of file size**: `terse_analyst.pl` ~55%, `ego_analyst.pl` ~50%,
  `progressive_research.pl` ~30%, `deliberative_analyst.pl`/`id_analyst.pl` ~15% (the bulk of these two is
  genuinely unique debate/roll-call and brainstorm/spark logic — not good consolidation targets).

## Candidate directions (not a recommendation)

- **A. The unification vision itself** — one coherent `RitualConfig`-backed conversational partner with
  research/knowledge/gated-task capabilities (open question on deliberation/brainstorm). The biggest,
  most transformative option, and the one with the most open feasibility questions.
- **B. Promote shared Prolog helpers into clara-prolog's lib** (mirrors the `the_leannan` precedent) —
  eliminates today's text-level duplication directly, needs a clara-cerebellum change + redeploy. Could
  still matter even under A, if generated per-node Prolog wants shared predicates.
- **C. A lildaemon-side build/templating step** — ruleset *source* files stop being hand-copy-pasted (a
  script stitches shared fragments in before registration); no clara-cerebellum dependency, but new build
  machinery to design and trust. An alternative to B if a cross-repo change isn't wanted yet.
- **D. Narrow, deliberate fixes only** — leave the byte-identical boilerplate alone; fix the diverged bits
  on purpose (one canonical `strip_think/2`, the `progressive_research.pl` gap). Smallest scope, ships
  immediately, doesn't block or conflict with A.

These aren't mutually exclusive: D is worth doing regardless of what's decided about A; B and C are
tactics that could serve A's generated Prolog too, not just today's flat files.

## Open questions

- Does the unification vision (A) match what Clara and the team actually want next, or is it too big a
  lift right now relative to the tactical fixes (B/C/D)?
- Do deliberation and brainstorming fold into the same unified partner, or stay separate, explicitly-
  invoked Rituals?
- Does the Cobbler graph/transduction vocabulary already express what these analysts do, or would it need
  extending first? Worth a small spike on one analyst (`terse_analyst.pl`, the smallest) before committing
  either way.
- If analysts become `RitualConfig`s, what's the right Ritual lifecycle — per turn, per session, per user?
- Should `caws_tristate/3` promotion be bundled with the upcoming `approve_action/4` work as already
  recommended, or decoupled and done sooner?
- Does a canonical `strip_think/2` need to preserve the "unterminated think block → placeholder" behavior
  (the majority implementation), or is `superego_gate.pl`'s simpler cut an acceptable new standard?
- Is the `progressive_research.pl` bug urgent enough to ship as its own small fix immediately, independent
  of everything else here?

## Related

`hermes_agent_evaluator_plan_v3.md` (where this item was first gated), `hermes_agent_evaluator_plan_addendum_2026-09-20.md`
(the existing `caws_tristate` recommendation), `clara_shared_nfs_plan.md` and `ego_skill_tier_plan.md`
(the recently-landed Ego work that opened this gate).
