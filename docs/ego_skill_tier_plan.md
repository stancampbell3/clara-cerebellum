# Ego skill/action-classification tier

Design pass triggered by a concrete failure: the Ego tried to create an SVG diagram in its workspace and
the gate rejected it. This doc explains why that was actually correct behavior given today's design, and
proposes a minimal, backward-compatible extension so a defined class of "skills" can be trusted without
opening the gate up wholesale.

This is also the concrete follow-through on a previously-deferred item (`clara_shared_nfs_plan.md`'s
Deferred section): *"Global skill tier, and whether a graduating skill needs Superego review before every
Ego trusts it — open question."* This doc answers that question for the scope defined below.

## Why the SVG case was correctly rejected

`goat/mcp/ego_gate/handlers.py` registers exactly three `ActionSpec`s today: `EXPORT_DOCUMENT`,
`RECORD_NOTE`, `PUBLISH_DOCUMENT`. `goat/mcp/ego_gate/rulesets/superego_gate.pl`'s `decide/4` has exactly
two routes:

- **Deterministic** — the action matches a registered, allowlisted spec. Cheap: check the per-ritual usage
  limit, then the spec's own `reversible` flag decides approve vs. escalate. No LLM call.
- **Semantic** — anything else. A full LLM review via `caws_consult` to a joined reviewer participant,
  parsed with a strict three-line VERDICT/REVERSIBLE/CONTAINED grammar (`judge_verdict/3`, `field_values/3`
  — fail-closed: any missing or ambiguous field denies).

Critically, per the gate's own design: *an LLM verdict can never cause anything to run.* The caller only
executes actions that are both allowlisted **and** have a registered Python handler. So even a
hypothetical "approve" from the semantic route for "create an SVG" would never have produced a file —
there is no handler for it. The rejection wasn't a bug; there is currently no path for arbitrary file
creation to exist at all.

## Scope decisions for this phase

Confirmed by brainstorming before this doc was written:

1. **Superego mediates almost every action.** The one exception already shipped and is out of scope here:
   per-user memory (`USER.md`, via Hermes' own native memory tool) is evaluator-owned and unsupervised,
   because its blast radius is a single capped file with no external effect. Skills are actions with real
   effects (they create files other participants/users can see), so they stay Superego-mediated.
2. **Skills are a hand-authored, code-reviewed catalog for this phase** — the same trust model as today's
   three actions, just more of them and with a richer decision cascade. A dynamic lifecycle where the Ego
   itself proposes and grows new skills (graduating from ritual-private to global, tracked with a success
   rate — sketched earlier in `brainstorm_clara_nfs_hermes_layout.md`) is a real, bigger idea, deliberately
   deferred to a later phase once the hand-authored catalog has real mileage.
3. **No new fastText classifier for this phase.** See below — the codebase has already moved away from
   fastText for this exact kind of decision.
4. **Hermes' own native skill-acquisition tool stays closed.** `hermes_agent_evaluator_plan_v3.md` already
   lists `skills` among the runtime surfaces the single-tool gate must close (alongside cronjob, delegation,
   webhook, a2a, peer, kanban) — same posture memory/`MEMORY.md` was in before this repo's per-user memory
   tier deliberately and narrowly turned it on. This doc is entirely about *our own* Superego-mediated
   action catalog, not about opening that door.

## Why not fastText

fastText is real infrastructure in this stack (clara-cerebellum's `ClassifyTool`, Rust `fasttext` crate,
model `dagda-0.2.bin`, called from Prolog via `the_rabbit.pl`'s `classify_text/2`). But the exact same use
case this doc is solving for — judging whether something is safe/sufficient — already migrated **away**
from fastText inside this codebase. `descriminate/2`, which used to shortcut on fastText labels plus a
brittle prefix-matching heuristic (`response_shortcut/2`, retired in commit `c64b87e`), now calls
`clara_fy`'s `ponder_verdict/2`: a grammar/enum-constrained LLM call locked to `yes`/`no`/`unresolved` —
"the model physically cannot emit anything else, so there is nothing to prefix-match." fastText has no
training pipeline in this repo today and is flagged elsewhere as tech debt.

There's a real, different future argument for fastText — not judgment quality but raw speed: a plain
forward-pass classifier can run inside a single Prolog cycle with no LLM round trip at all
(`docs/assistant_demo.md` sketches this for a different lexical gate, `decision_pattern/1`). That's worth
revisiting if the design below's LLM round trip ever becomes a real bottleneck at volume. Not needed to
solve the concrete problem in front of us.

## Design: a third route, added without touching the two that already work

**Route 0 — deterministic (unchanged).** A skill whose safety envelope is fully captured by its JSON
schema alone (closed enums, bounded lengths — like all three of today's actions) skips straight to the
existing reversible + per-ritual-limit check. Zero behavior change for existing skills.

**Route 1 — quick content review (new).** For a skill whose params can't be fully vetted by schema alone
— an arbitrary SVG or text body, for example — `ActionSpec` gains one new field:
`needs_content_review: bool`. When true, the context handed to the Prolog deduction
(`SuperegoGate.build_context()`) also includes a size-capped `content_preview`, as plain data exactly like
`justification` already is (never built into a Prolog goal — the ruleset's own long-standing discipline).
A new predicate asks the *same already-joined reviewer participant* one short, grammar-constrained
question via `caws_consult`, reusing `ponder_verdict`'s enum-locked prompt convention (ported to a
`caws_consult`-reachable form, since the Superego's own deduction is pure Prolog policy with no directly
attached model of its own — the reviewer is deliberately always a separate participant, never the actor).
Parse is fail-closed, same discipline as `judge_verdict/3`: an unresolved answer escalates, it never
silently approves. Outcome is approve / deny / escalate directly, with no automatic chaining into Route 2
— this keeps the added cost to exactly one extra round trip, not two.

*Correction to an earlier assumption while researching this:* `clara_fy`/`ponder_verdict` are cheaper than
`caws_consult` when the caller's own deduction has an LLM directly attached (no Ritual/Kafka hop needed).
The Superego's deduction has no such attachment, so Route 1 still pays for a `caws_consult` round trip —
what it actually saves is prompt size, and, later, optionally, a smaller/cheaper dedicated judge model.
Worth stating precisely rather than overselling it.

**Route 2 — full semantic review (unchanged).** The fallback for any action name that isn't a registered
skill at all. Exactly today's `caws_consult` and three-line parse.

## Worked example: the actual SVG case

A new `ActionSpec`, `CREATE_DIAGRAM`, registered in `handlers.py` alongside the existing three:
`reversible=True` (creates a new file — same reversibility class as `EXPORT_DOCUMENT`), a capped
`max_per_ritual`, schema `{name, content (size-capped SVG/text), content_type: enum}`,
`needs_content_review=True`. The handler writes through the **existing** outbox mechanism
(the same `_ritual_outbox`/atomic-write path `EXPORT_DOCUMENT`/`RECORD_NOTE` already use) — no new
writable surface, no new safety story for where files land. Opening the seat's own ephemeral `workspace/`
directory as a second writable area is a real option for later, deliberately deferred pending this design
holding up in practice.

## Governance question, now resolved for this phase

*"Does a graduating skill need Superego review before every Ego trusts it?"* — yes, but that review
happens **once, at code-review time**: a developer ships a new `ActionSpec` via a normal PR, the same trust
model as today's three actions. Per-invocation, a skill still goes through Route 0 or Route 1 above; it is
never unconditionally trusted just because it's in the registry.

## Explicitly out of scope for this phase

- Dynamic skill proposal/graduation (Ego suggesting new skills that get promoted into the registry).
- A new fastText classifier.
- Opening Hermes' own native skill-acquisition tool.
- Per-user/per-ritual skill *permission* scoping beyond "in the registry = usable by anyone" (the
  agent-private/ritual-shared/user-shared/global scope table sketched in
  `brainstorm_clara_nfs_hermes_layout.md`) — worth revisiting once there's more than one or two skills to
  actually scope.

## Files expected to change, if this design is approved for a build pass

1. `goat/mcp/ego_gate/actions.py` — `ActionSpec` gains `needs_content_review: bool = False`.
2. `goat/mcp/ego_gate/superego.py` — `build_context()` includes a size-capped `content_preview` only when
   the matched spec has `needs_content_review=True`.
3. `goat/mcp/ego_gate/rulesets/superego_gate.pl` — extend `decide/4`'s allowlisted clause to branch on
   `needs_content_review`; add the new quick-check predicate, reusing `review_prompt/2`'s prompt style and
   `judge_verdict/3`'s fail-closed parsing discipline rather than inventing a new parser.
4. `goat/mcp/ego_gate/handlers.py` — the new `CREATE_DIAGRAM` `ActionSpec` + handler.
5. `goat/mcp/ego_gate/wiring.py` — only if Route 1 needs its own model/timeout override; default is to
   reuse the existing reviewer config as-is.

## Verification, once a build pass is approved

1. Unit tests: the new `ActionSpec` field; `build_context()`'s content-preview inclusion (present only
   when `needs_content_review=True`, unchanged otherwise); the ledger recording the new action like the
   existing three.
2. Prolog-level tests for the new quick-check predicate: approve / deny / unresolved-escalate cases, plus a
   regression test that the existing three actions' deterministic route is byte-for-byte unaffected.
3. Live proof: have the Ego actually request `create_diagram` with a real SVG body and confirm
   approve→executed with the file landing in the outbox; separately confirm a deliberately suspicious body
   (e.g. an embedded `<script>` tag) gets denied or escalated, never silently approved.
4. Full lildaemon test suite plus black/mypy on touched files.

## Related

`clara_shared_nfs_plan.md` (the deferred item this resolves), `hermes_agent_evaluator_plan_v3.md` (the
single-tool-gate invariant this respects), `brainstorm_clara_nfs_hermes_layout.md` (the bigger skill-tier
vision this deliberately defers), `docs/assistant_demo.md` (the fastText-for-speed idea this defers).
