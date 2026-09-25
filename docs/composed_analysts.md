# The frontdesk analysts as a Ritual of Rituals

*Status 2026-09-25: built, live-verified, deployed on limbic, opt-in (`clara` ruleset; the default is unchanged). Routing only, by decision: the
background legs still run from `runtime.py`.*

## What it is

One partner instead of one analyst per session. A **parent Ritual** (`assistant:clara:<version>`) has a router and five **child Rituals**, one per existing
analyst (`terse`, `progressive`, `deliberative`, `id`, `ego`). Each turn the router picks a capability, consults that child with the conversation history, and the
child's own policy (`assistant_turn/5`) answers. The runtime then continues exactly as before, using the chosen analyst's ruleset for any background leg
(research, deliberation, brainstorm, Ego). Select it in the frontdesk as **"Clara (composed)"** (`ruleset_key = clara`).

## How it is built

- **The `.pl` files stay the source of truth.** `goat/app/assistant/composed.py` generates the configs from them: each child's entry node carries its ruleset file plus a small
  adapter that exposes the ruleset's `assistant_turn/5` through a RitualConfig Run's `reasoned_response/3` and returns `{action, reply, citations, citation_count}` as JSON.
- **Parent** = router node (`rulesets/clara_router.pl`) + five owned `ritual-group` children, with an offering edge to each so the router gets `consult_<child>/2`. `progressive`
  also declares a `groq-splinter` node, because a child cannot reach the parent's participants and its classify may ask for a Groq second opinion.
- **System-owned, persistent, versioned.** Owned by `system:assistant`, `persist=True`, named with a content hash (files + generator version). Created at startup in the
  background and resumed after a restart; when the hash changes the family is torn down and rebuilt; a family that is current but never goes live (a child's Dis ritual
  gone) is rebuilt after 90 s (verified live).
- **Routing** (lexical, ordered, first cut): decision cues -> `deliberative`; brainstorm cues -> `id`; explicit do-a-task cues -> `ego`; short question-free chit-chat -> `terse`;
  everything else -> `progressive`. The decision cues are a strict subset of the deliberative analyst's own (bare "or", "choose", "which one" are left out so "tea or coffee?"
  does not convene a committee); a test keeps them a subset. If the chosen child does not answer, `progressive` is asked; if that fails too the runtime falls back to the
  plain single-analyst path (logged), so the assistant never goes down because of the composition.

## What had to change underneath (found while building it)

- `perform()` forwards the conversation `context` and gives a peer-consulting entry a cycle budget above its patience (a parent waiting on a child needs more than 100 cycles);
  the `ritual` evaluator forwards the offering's `context` and now logs every refusal.
- **Evaluator slots are keyed by node id alone, across every ritual.** A child's entry node named like its parent's group node silently answered the parent's node with the
  child's `echo` evaluator (the router got "no answer"). Fixed twice: generated child entry nodes are `<analyst>-entry`, and joining a slot already held by a *different*
  evaluator is now refused (`RitualManager.join`, activation) instead of silently reused.
- **Test isolation:** the startup warm-up talks to a real Dis, and every test that starts the app built its own family there (a full suite left 64 stray Rituals). It is off
  under test (`ASSISTANT_COMPOSED_WARM=0` in `tests/conftest.py`).
- The ground-state tooling neither captures nor wipes system-owned configs (their Dis rituals are live), and Kafka verification treats a live Ritual's topics as expected.

## Measured

Live, warm: routing to terse, progressive, deliberative, id and ego all worked; a follow-up ("and double that?") was answered from the previous turn, so the history reaches the
child; roughly **+2 s** per turn over the direct path (5.3 s vs 3.2 s on a simple factual question). Cold first turns are slow or fail on both paths alike (see the model-warmth note
below), so that is not a composition effect.

## Known limits and follow-ups

- Two deductions and a Kafka round trip per turn; a second `groq-splinter` instance adds Groq rate-limit pressure.
- Voice differs per analyst (terse is blunt and pun-prone): harmonising system prompts across modes is an **idea** (Stan), not a decision.
- The synchronous progressive tier still reads Edgequake's Default Workspace until the workspace-slug change (ground_state.md, G1) is deployed; live tests triggered real background research
  that ingested extra documents into `assistant.general`, so the baseline was restored afterwards.
- Switching rulesets mid-session in the frontdesk demo seems to drop the context window (reported, not yet debugged); less relevant with one composed partner.
- Keeping the clara-api model hot (or serving it with vLLM) would remove most cold-start failures (idea).
- Not in this cut: background legs as child Performances, an LLM intent classifier, child self-nomination, making `clara` the default, Cobbler view of the generated configs.
- lildaemon leaves one `assistant-demo` standing Ritual in Dis per process start (older leak, observed again).
