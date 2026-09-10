# Reasoning-model upgrade: status & open issues

> **Handoff doc, written 2026-09-10, updated same day with the clean
> A/B re-run.** Session paused here for team review. Companion to
> `thinking_model_timeout_problem.md` (the original analysis) and
> `qwen_clara_27b_upgrade_plan.md` (the base eval) — this doc covers
> what happened *after* that analysis: the evaluator plumbing that got
> built, a second investigation into backing Dis's own deduction-loop
> predicates with an uncensored 27b, and a clean re-measurement of the
> base eval once ComfyUI was off the GPU.

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

## Prep implemented and committed (2026-09-10) — model swap held per Stan

Scope agreed: prep only, hold the model swap; promotion target is
**vanilla qwen3.8:27b** (not an uncensored variant).

- **`think`/`num_predict` plumbing committed** — lildaemon `eab9c4c`,
  pushed both remotes. Opt-in; not wired into `evaluators.yaml`.
- **`clara_system_prompt.txt` slimmed to persona only** — lildaemon
  `dfabdb6`. Dropped the hand-rolled "EXACT JSON format" tool-call block
  and the "After receiving tool results" paragraph (§5.2). Also fixed
  the character name: **Clara Oswald**, not Osborne — in the file and in
  `ClaraMindSplinter.SYSTEM_PROMPT` (which `GroqEvaluator` inherits).
- **`Modelfile.qwen-clara-27b` cleaned** — clara-cerebellum `6ef7ccd`.
  Dropped `presence_penalty 1.5` and `stop "<tool_res>"` (§5.3);
  `qwen-clara-27b:latest` rebuilt from it.

### Verification (§6 step 1) — clean GPU, slimmed prompt

| | 9b (regression check) | 27b default | 27b `think:false` |
|---|---|---|---|
| persona | intact, "Clara Oswald" | good voice, concise | good, 1.0s |
| single tool call | ✅ `get_datetime` | ✅ clean | ✅ clean |
| logic puzzle | solves (barrels through) | ✅ flags it under-specified | ✅ works |
| strict "one word" | still emits reasoning + sentence | exactly `Paris` | exactly `Paris` |
| **fake `{"name":"think"}` in text** | none | none | **none — §4.4 fixed** |
| `<think>` tag in content | still present (9b renderer) | none | none |

The slim fixes the fake-think-call on the 27b with thinking disabled and
does not regress the 9b. Everything sub-6s on the dedicated GPU.

### Deployed + regression-tested (2026-09-10)

- Slimmed prompt is **live** — `./clara up -d --build lildaemon` (local)
  + `update_stack_host.sh pineal`. Both containers healthy, "Clara
  Oswald" confirmed inside each.
- **Caught and fixed a self-inflicted regression:** `./clara up --build
  lildaemon` rebuilds `clara-api` too (dependency), from whatever branch
  is checked out — and `docs/qwen-clara-27b-eval` forked from `fb9b608`,
  *before* the actix `client_request_timeout` fix (`3c854f7`, master
  only). So the local `clara-api` briefly rebuilt without it. Fixed by
  merging master into the branch (`6565e72`) and rebuilding — the branch
  now carries the actix fix, so it survives the eventual PR.
- **Regression suites, both green:**
  - lildaemon: `1178 passed, 25 skipped` (skips all environmental — no
    Kafka broker, no pylint, FieryPit unreachable). Identical to
    pre-prep.
  - clara-cerebellum: `cargo test --workspace` — `521 passed, 0 failed,
    4 ignored` (the 4 ignores are doc-test examples marked `no_run`,
    pre-existing).

### Still open on this track

- Plumbing is committed but **not wired into `evaluators.yaml`** — no
  evaluator sets `think`/`num_predict` yet. That's part of the held
  model swap.
- `id_analyst.pl`'s documented 9b runaway-generation incident
  (2026-08-08) is still unguarded — a standalone `num_predict` on the
  current 9b seats would address it independent of any swap; not done.
- **Deploy hygiene:** any `./clara up --build <anything>` from a feature
  branch rebuilds `clara-api` from that branch. Keep feature branches
  merged up with master, or build `clara-api` explicitly from master.

---

_Earlier state, for history — this section is now superseded by the one
above:_

**Evaluator plumbing for `think`/`num_predict`** was built and tested
before being committed — 7 files:

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
   stack, and Clara has GPU priority.** Stan has shut it down and is
   relocating it off this box. The GPU is now dedicated to Clara, which
   enabled the clean re-run in the next section.

## Clean re-run of the section-3 A/B (2026-09-10, GPU dedicated)

Repeated `qwen_clara_27b_upgrade_plan.md`'s eval with the real deployed
artifacts (`clara_system_prompt.txt` + `python_tools.json`), 9b vs 27b,
across persona / creative-open-ended / single-tool-call / logic-puzzle /
strict-instruction tasks. Two passes: thinking default, then a
`num_predict` cap.

### Throughput — the doc's numbers were badly contention-poisoned

| | doc (contended) | clean, now |
|---|---|---|
| 9b (`qwen-clara:latest`) | ~195 tok/s | ~195–202 tok/s (unchanged — it always fit) |
| **27b (`qwen-clara-27b`)** | 45–62 "GPU-resident" / 15–28 contended | **106–187 tok/s** |

The 27b is ~1.5–2× slower per token than the 9b, **not 3–4×**. Cold-load
is ~150s, one-time (stays warm on `keep_alive`).

### Warm per-task latency (27b, clean)

| task | 27b warm | notes |
|---|---|---|
| persona, 2 sentences | 1.5s | |
| single tool call | 0.9s | correct `get_datetime` call, clean |
| logic puzzle | 5–7s | 27b correctly flags it as under-specified; 9b just guesses |
| strict "one word only" | 0.6s | 27b returns exactly `Paris`; 9b emits reasoning + a sentence |
| **creative 120-word monologue** | **31s, ~5,800 tokens (~3,200 thinking)** | the spiral — faster GPU helped (was 60–100s) but did **not** fix it |

### The `num_predict` cap floor (creative prompt, 3 runs per cap)

| cap | outcome |
|---|---|
| 2,048 | ❌ empty content — entire budget spent thinking |
| 4,000 | ❌ 2 of 3 hit the cap; one empty, one truncated to 352 chars |
| **6,000** | ✅ 3 of 3 fine (used 2,485–3,556 tokens) |
| 8,000 / 12,000 | ✅ 3 of 3 fine; one 12k run still took 35s |

Thinking for this single prompt varied **~1,400–4,000 tokens across
runs** (non-deterministic at temp 0.2); the answer itself is ~170 tokens
every time. **A cap below ~6,000 risks deleting the answer, not
shortening it.** A well-chosen cap (6–8k) prevents the empty-answer
failure and bounds the tail, but does **not** make creative prompts fast
— they're 15–35s on the 27b regardless.

### `<think>` separation, confirmed clean

Zero `<think>`-tag leakage into `content` on the 27b across every task
(`thinking` field populated instead). The 9b pollutes **every** response
with `<think>…</think>` markers in content via its `qwen3-coder`
renderer.

### Implication: split the decision

The spiral is triggered by **open-ended / creative** prompts. Dis's
deduction predicates (`descriminate` / `ponder_text`) don't send those —
they send narrow judgment tasks, which ran **0.6–0.9s warm on the 27b**
and would use `think: false` anyway per the design. So:

- **Dis-backing role:** a 27b works cleanly and fast *now* — `think:
  false` + a small `num_predict` (few hundred tokens). Spiral doesn't
  apply. Only open item is the `classify_text` compatibility check
  (#3 below).
- **Frontdesk / assistant chat path:** separate call. This is where the
  spiral lives and where Option A's 6–8k cap — or Option B (thinking on
  for tool-selection, off for the compose turn) — actually matters.
  Creative turns will be 15–35s even done right.

## Open issues / decisions needed

1. ~~Commit the evaluator plumbing~~ — **done** (lildaemon `eab9c4c`).
   Still needs wiring into `evaluators.yaml` as part of the held swap.
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
4. **The `num_predict` cap for the chat path needs to be ~6,000–8,000**,
   not a small number — a cap below the thinking-phase size deletes the
   answer entirely rather than truncating it (see the cap-floor table
   above). Even a good cap leaves creative turns at 15–35s; if that's
   unacceptable, Option B (thinking off for the compose turn) is the
   real lever, not a tighter cap.
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
