# Reasoning-model upgrade: status & open issues

> **Handoff doc, written 2026-09-10.** Session paused here for team
> review. Companion to `thinking_model_timeout_problem.md` (the original
> analysis) and `qwen_clara_27b_upgrade_plan.md` (the base eval) — this
> doc covers what happened *after* that analysis: the evaluator plumbing
> that got built, and a second investigation into using an uncensored
> 27b model to back Dis's own deduction-loop predicates specifically.

## Done and deployed

- **Dis's 408-handling bug, fixed and live** (both hosts). `DisClient.
  poll_deduction` (lildaemon, async) and `KindlingEvaluator.
  _poll_deduction_to_completion` (sync mirror) both used to treat a
  literal 408 from Dis as fatal, aborting the whole assistant turn
  mid-poll — root cause of the frontdesk-poc "I ran into a problem
  answering that" errors. Both now back off (1s→15s) and retry instead.
  Commit `76b7227` (lildaemon), pushed to `github`+`origin`, deployed to
  the local stack and to pineal's standalone FieryPit.
- **Actix's `client_request_timeout` raised 5s → 30s** on clara-api
  (limbic). The 408 above was traced to this unconfigured default
  tripping under bursty concurrent load — `poll_deduce` itself is a
  trivial in-memory lookup with no way to be genuinely slow, so this was
  almost certainly a transport-level hiccup, not an app-level decision.
  Commit `3c854f7` (clara-cerebellum), pushed and deployed locally;
  pineal's checkout is synced too (it doesn't run clara-api, but tracks
  this repo for its `lildaemon` image build).
- **`thinking_model_timeout_problem.md`** written and pushed to
  `docs/qwen-clara-27b-eval` (commit `da79b5a`) — the original analysis
  of why Qwen thinking-mode models spiral on open-ended prompts, and the
  options for bounding it (Option A: thinking on + a budget cap,
  recommended there). **PR not yet opened** — was waiting on team
  feedback before that step.

## Built and tested, NOT yet committed

**Evaluator plumbing for `think`/`num_predict`**, in lildaemon,
implementing Option A's missing piece — 7 files changed, uncommitted
locally right now:

- `goat/evaluators/toolified_ollama.py`: `ToolifiedOllamaEvaluator.
  __init__` gains `think: Optional[bool]` and `num_predict:
  Optional[int]`, wired into both the sync and async payload builders.
  `think` is Ollama's own top-level switch (sent only if configured);
  `num_predict` folds into `options` the same way `temperature`/`top_p`/
  `num_ctx` already do. Both are per-`Offering`-overridable, same
  precedence as the existing sampling params (offering wins over
  instance config).
- Forwarding fixed at **every** intermediate layer that explicitly
  re-lists constructor kwargs instead of passing `**kwargs` through —
  this was the real risk, since a new param silently gets dropped at any
  layer that isn't updated: `ClaraMindSplinter.__init__` and
  `KindlingEvaluator.__init__`'s Phase-3 forward to
  `ToolifiedOllamaEvaluator.__init__` (the actual live path for
  `clara_mind_splinter`/`_lite`), plus `EmberEvaluator.__init__`'s
  equivalent for completeness.
- 9 new tests across `test_evaluator_offering_overrides.py` (sync+async
  precedence, including a "falsy override must still win" case),
  `test_kindling_evaluator.py`, and `test_clara_mind_splinter.py` (an
  end-to-end regression guard through the full init chain — exactly the
  layer that was silently dropping kwargs). Full suite: 1178 passed, 25
  skipped, no regressions.
- **Deliberately not wired into `evaluators.yaml` yet** — the current
  production model (`qwen-clara:latest`) isn't in "thinking mode" under
  its current `qwen3-coder` renderer, so setting `think` there is
  untested territory. `num_predict` is a plain generation-length cap
  that would apply regardless, and `id_analyst.pl`'s own comments
  document a live runaway-generation incident on this exact model in
  this exact seat (2026-08-08) — worth considering as a standalone
  safety net independent of any model swap. Team call needed either way.

## New investigation: backing Dis's own predicates, not just the Id/chat path

Stan raised a distinct, sharper concern: Dis backs a lot of Prolog
predicates with `qwen-clara:latest` running thinking-off / tool-less —
deliberately, so the LLM never gets to reason or act outside the
rule-based deduction it's embedded in. The mechanism, traced live in
`clara-prolog/prolog-lib/the_rabbit.pl`:

```prolog
descriminate(Text, TruthValue) :-
    ponder_text(Text, LLMSez),           % free-text LLM musing
    extract_llm_response(LLMSez, Response),
    ...
    classify_text(Pair, TruthValue).     % fastText classifier — NOT the LLM — decides
```

`ponder_text/2` gets the LLM to produce raw text; a **separate,
deterministic fastText classifier** (`classify_text`) is what actually
judges/labels it, and that's what the deduction converges on. The LLM
never makes the decision — which is exactly the "no out-of-band
decisions" property Stan wants preserved while upgrading to a bigger,
less-refusal-prone model for better raw material.

### Findings (all via a scratch eval, not committed — reproducible from this doc)

1. **`think: false` (today's new plumbing) cleanly suppresses thinking
   leakage on 27b-class models.** Zero `<think>` tag or `thinking`-field
   leakage across every test call on both `qwen-clara-27b:latest`
   (vanilla) and `Qwen3.6-27B-Fable-Fusion-711-Uncensored-Heretic`. By
   contrast, the *current* `qwen-clara:latest` still emits literal
   `<think>  </think>` tag markers into `content` even with `think:
   false` (empty, but present — a `RENDERER qwen3-coder` artifact). So
   the upgrade path is a strict improvement here, not a regression.

2. **Uncensored variant is measurably less refusal-rigid on borderline
   prompts, though the gap is smaller under `descriminate`'s actual
   framing** ("give your reaction to this request" rather than "do the
   task"). Clearest case: asked to react to a lockpicking-mechanics
   request, vanilla 27b flatly refuses; the Heretic model engages
   ("standard educational content... I'd frame it responsibly"). Both
   still decline on the bank-robbery and offensive-joke prompts.

3. **First latency read was wrong — confounded by GPU contention.**
   Initial testing showed 68-120s responses and outright timeouts on
   both 27b models even with `think: false`, which looked like a hard
   blocker (a single `ponder_text` call that slow would stall deduction
   convergence). Checked and found the real cause: ComfyUI was
   co-resident on the same GPU (`--lowvram`, ~13.6GB) alongside Ollama,
   leaving razor-thin headroom on the 32GB card, and the test script
   itself compounded it by cycling through four different ~17-21GB
   models in one run — each switch likely forcing a cold reload.

4. **Clean re-benchmark (ComfyUI stopped, one model at a time, GPU
   fully free) reverses that conclusion.** Cold-load cost is real
   (~100-175s — this is Ollama loading ~18-21GB off disk, not
   generation), but every warm call after that landed in **0.6-0.9
   seconds**, at 64-110 tok/s (vanilla) / ~75 tok/s (Heretic) — both
   *better* than the original eval's contended-GPU floor, since the
   model now has the whole card to itself. **Latency is not a blocker
   for either candidate**, provided the model stays warm as a standing
   default rather than being swapped in and out (a `keep_alive` tuning
   question, not a model-choice one).

5. Per Stan's decision: **ComfyUI is a co-tenant, not part of the Clara
   stack, and Clara has GPU priority.** It's being relocated off this
   box. It was stopped (gracefully — queue was empty) to run the clean
   benchmark above and **is currently still stopped** — an attempt to
   restart it was blocked by the session's own permission guardrails;
   someone should restart it manually or fold that into the relocation
   work, whichever comes first.

## Open issues / decisions needed

1. **Commit and deploy the evaluator plumbing?** (lildaemon, 7 files,
   currently uncommitted — see above.) Independent of everything else
   below; it's needed either way before any model swap for either the
   Id/chat path or Dis's predicates can use `think`/`num_predict`.
2. **Which model, if any, backs Dis's predicates going forward** —
   vanilla `qwen-clara-27b` (closer to current behavior, smaller
   refusal-reduction) vs. the uncensored Heretic fusion (more willing on
   borderline-but-legitimate requests, unofficial/community fine-tune).
   Latency is no longer the blocker; this is now a judgment call about
   how much refusal-reduction is wanted for this specific role.
3. **`classify_text` (fastText) downstream compatibility is unverified.**
   It was presumably trained/tuned against the *current* model's output
   style. Swapping the upstream LLM changes the raw text
   `descriminate/2` feeds it — nobody has checked whether the classifier
   still behaves correctly against a different model's phrasing. This
   needs validation before any promotion, and isn't something I can test
   without the classifier's training data/eval set.
4. **Restart or relocate ComfyUI.** Currently stopped on the dev box.
5. **Open the PR for `docs/qwen-clara-27b-eval`** once team feedback is
   in — was deferred pending review of `thinking_model_timeout_problem.
   md`; this doc adds to the same branch.
6. **Groq's side of the thinking-timeout problem is still unaddressed.**
   `clara_mind_splinter_groq` (now the standard default evaluator) has
   the documented empty-content-on-reasoning-budget-exhaustion failure
   mode (`id_analyst.pl` comments, 2026-08-08) — `max_tokens` exists as
   a knob there already but isn't set to anything in `evaluators.yaml`,
   and a Groq-specific `reasoning_effort` parameter was considered but
   not implemented (untested against this model, didn't want to guess).
7. **Reminder, still outstanding from earlier in the week:** rotate the
   Groq API key that briefly leaked into a session transcript during an
   unrelated diagnostic command — flagged at the time as low-urgency
   since the keys are short-expiry, but worth confirming it happened.
