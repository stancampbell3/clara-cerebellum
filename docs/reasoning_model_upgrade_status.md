# Reasoning-model upgrade: status & open issues

> **Handoff doc, 2026-09-10 — paused here for team review.** Companion
> to `thinking_model_timeout_problem.md` (the original analysis) and
> `qwen_clara_27b_upgrade_plan.md` (the base eval). Covers what happened
> *after* that analysis: the evaluator plumbing that got built; the
> clean A/B re-run once ComfyUI was off the GPU; gate #1 (`classify_text`
> compat) and how it reframed into a system-prompt fix (`the_rabbit.pl`
> draft, `a42d935`); retiring `gemma4:e4b`; and the current design
> direction — **splitting models by predicate class**, with search
> parameters for a small verdict model the team is now researching.
> Nothing here is deployed to the running stack except the prompt slim
> and the (image-less, `docker cp`'d) `the_rabbit.pl` draft on `clara-api`.

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

## Gate #1 — `classify_text` compatibility: done, and it reframed the problem

Traced the real path: `progressive_research.pl` → `clara_fy/2,3`
(`the_rat.pl`) → `top_status` → `descriminate_k` (`the_rabbit.pl`) →
`ponder_text` (LLM) → `response_shortcut/2` **or** `classify_text`
(fastText `dagda-0.2.bin`). `clara_fy` re-asks with an "Answer yes or
no:" prompt when the first pass returns `unresolved`.

**Finding: the pipeline is fragile for *both* models, not just after a
swap.** Under the current witty-persona system prompt, both `gemma4:e4b`
and `qwen-clara-27b` answer roundaboutly ("Oh, heavens no…", "Quite."),
so `response_shortcut` misses and the weak fastText classifier gets the
call — and it *inverts* clear answers (classified "Is Paris the capital
of Germany? Oh heavens no…" as **true**). ~8/12 correct for gemma,
~9/12 for the 27b, 6/12 agreement between them.

**Fix (validated): a terse verdict system prompt for predicate mode.**
Same 12 questions, both models, "reply with one word: yes / no /
unresolved":

| | persona prompt | terse verdict prompt |
|---|---|---|
| gemma↔27b agreement | 6/12 | **11/12** |
| verdicts via `response_shortcut` (classifier bypassed) | ~4/12 | **11/12** |
| gemma correct | ~8/12 | **~11/12** |
| 27b correct | ~9/12 | **~11/12** |

The one remaining flip ("Is water wet?") is a genuine semantic debate,
not a regression. So: **the 27b swap for the Dis-predicate path is safe
— conditional on the terse-prompt change landing first.** The two are
coupled.

**Shipped this as a draft:** `the_rabbit.pl` commit `a42d935` —
`verdict_system_prompt/1` + `reasoning_system_prompt/1` facts,
`ponder_text/3` + `ponder_text_with_context/4` (explicit `System`),
`ponder_reason/2,3` wrappers; `descriminate/*` now use the verdict
prompt; `ponder_text/2` and `/3`(ctx) keep the persona default (they
generate the user-facing `Reply` in `progressive_research.pl`).
**Not yet loaded in a live devils session** — needs a scratch-session
smoke test before merge (no `swipl` on the box to lint).

**Separately noted for the team:** the `dagda-0.2` fastText model and
its brittle `response_shortcut` string-prefix backstop both need work —
the terse prompt keeps them off the critical path but doesn't fix them.
That's its own workstream.

## Retiring `gemma4:e4b` — VRAM checked

`ponder_text` hardcoded `gemma4:e4b` as a pineal-era workaround (12 GB
5070 couldn't hold the base Clara model + Edgequake's LLM, so matching
them dodged Ollama hot-swaps). On limbic's 32 GB 5090 (ComfyUI gone):

- `qwen-clara-27b` alone: 17.5 GB
- **`qwen-clara-27b` + `embeddinggemma`: co-reside fine — 18.2 GB**
- `qwen-clara-27b` + `gemma4:e4b`: **Ollama evicts the 27b every time**

So "run the 27b alongside gemma" doesn't work — the path is to **retire
`gemma4:e4b`** and converge the whole stack on `qwen-clara:latest`
(+ `embeddinggemma`). The `the_rabbit.pl` draft above already does this
for `ponder_text`. Edgequake needs the matching change:

- **Edgequake model config lives in three places:** the `edgequake-api`
  container env (`EDGEQUAKE_LLM_MODEL` / `OLLAMA_MODEL`, currently
  `gemma4:-e4b` — note the stray dash), the **tenant** row's
  `default_llm_model` (what new workspaces inherit), and **each
  workspace** row's own `llm_model` (copied from the tenant at creation).
- **Vision:** `qwen-clara-27b` is a vision model (that's eval finding #1
  — the `qwen3.8` renderer fixes image tokenisation, which is broken on
  the current 9b). Edgequake's `default_vision_llm_model` is currently
  `null` and `EDGEQUAKE_VISION_MODEL` is empty — so pointing both at
  `qwen-clara:latest` gives Edgequake image understanding it does not
  have today, on the already-resident model, zero extra VRAM. Needs a
  smoke test that the Modelfile renderer accepts Edgequake's vision
  request shape.
- **Reset:** Stan is planning a full Edgequake reset (KBs, workspaces,
  docs) for a clean-slate test on the shared model. Sequence: update the
  env + tenant `default_llm_model` (and `default_vision_llm_model`)
  *before* recreating workspaces, so new ones inherit the right model;
  the wipe takes care of the stale per-workspace `llm_model` values.

### Edgequake config — where it stands (2026-09-10)

- `assistant.general` workspace `llm_model` → **`qwen-clara:latest`**
  (Stan set this; not re-ingested, since the reset will clear it). This
  is the single shared workspace the current design uses, so it's the
  one that matters at runtime.
- Still on `gemma4:-e4b`, to clean up for consistency (not blocking a
  test): tenant `default_llm_model` (inherited by any *new* workspace),
  tenant `default_vision_llm_model` (currently `null` — set to
  `qwen-clara:latest` for the vision win), and `docker/.env`
  `EDGEQUAKE_LLM_MODEL` (re-seed source on an edgequake-api restart).
- The stored `gemma4:-e4b` string is a malformed tag, but Edgequake
  normalises it to `gemma4:e4b` before the Ollama call (confirmed in its
  logs) — cosmetic, not a live bug. Just write the replacement as a
  clean `name:tag`.

## Model split by predicate class — investigated, then dropped (2026-09-10)

> **DECISION (Stan, 2026-09-10): no small verdict model. Run the 27b for
> verdict calls too**, with the terse verdict prompt + a `format` enum
> constraint. The split saved ~15 GB but the 27b is already resident,
> does verdicts at 0.6–0.9 s warm, and calibrates "unresolved" better
> than any small model tested (see the harness below). One model
> (`qwen-clara:latest`) + `embeddinggemma` — which already co-reside.
>
> **Consequences:** no `OLLAMA_MAX_LOADED_MODELS` change needed; the
> `num_ctx` cap on the 27b Modelfile is now optional (headroom, not
> fit); the `the_rabbit.pl` draft's single `model: 'qwen-clara:latest'`
> across all `ponder_*` variants is already correct. The one thing to
> still build: the `format` enum on the verdict path (see "Constrained
> decoding" below) — that's the real win, and it retires
> `response_shortcut/2` regardless.
>
> The investigation below is kept for context.

Rather than one model for everything, run a deliberate *small set*, each
matched to a class of Prolog-predicate call, all co-resident so there is
never a hot-swap:

| predicate class | model | rationale |
|---|---|---|
| `descriminate` / `clara_fy` verdicts, `classify`-adjacent | **small fast (1–3 B), no tools, terse verdict prompt, tiny `num_ctx`** | one-word yes/no/unresolved output, high call frequency, latency-critical *inside* a deduction loop — thinking spiral and tool schema are pure overhead here |
| `ponder_text` / `reasoned_response` generation, splinter tool-use, vision, user-facing `Reply` | **`qwen-clara` 27b** | capability, persona, image understanding |
| Edgequake ingestion extraction | small fast (structured, high-volume) — Edgequake already has an `extraction_profile` flag for this axis | speed matters more than depth for entity/relation extraction |
| Edgequake RAG synthesis (query) | 27b (or small — TBD by quality test) | answer quality |
| embeddings | `embeddinggemma` (0.6 GB) | — |

**VRAM math (32 GB 5090):** 27b (~17.5) + small verdict model (~1–2) +
`embeddinggemma` (~0.7) ≈ 20 GB. Fits with ~12 GB spare.

**Blocker — residency is not automatic.** Observed 2026-09-10: Ollama
*evicted* the 27b when `gemma4:e4b` (3.4 GB) loaded, despite the total
fitting in 32 GB. `27b + embeddinggemma` co-resided fine. To hold
27b + small + embed simultaneously we need:

1. `OLLAMA_MAX_LOADED_MODELS` set on the `ollama.service` systemd unit
   (currently unset — the unit only sets `OLLAMA_HOST` and
   `OLLAMA_MODELS`). Likely `3`.
2. Probably cap the 27b's `num_ctx` in `Modelfile.qwen-clara-27b` — its
   32 k KV cache is what makes Ollama's fit-estimate balk; the deduction
   path never needs 32 k. Try 8 k or 16 k.
3. Re-test each combo after (1) and (2).

**Mechanism:** extend the `the_rabbit.pl` draft with `verdict_model/1`
and `reasoning_model/1` as overridable facts (parallel to
`verdict_system_prompt/1` / `reasoning_system_prompt/1` already added in
`a42d935`); `descriminate/*` calls `ponder_text/3` with `verdict_model`,
plain `ponder_text` uses `reasoning_model`.

### Small verdict-model search parameters (for the team's research)

Looking for a model to sit behind `descriminate` / `clara_fy`. Not a
generation model — a fast, obedient one-word classifier.

- **Size:** 1–4 B parameters; target ≤ 2 GB VRAM at the quant below.
- **Quantisation:** Q5_K_M or Q6_K (small models degrade more under Q4;
  the footprint is tiny either way, so don't cheap out on bits).
- **NOT a reasoning / "thinking" model.** No chain-of-thought, no
  `<think>` channel — we want an instant leading token. A reasoning
  model actively defeats the purpose (see this doc's thinking-spiral
  sections). Rules out the Qwen3 "thinking" variants unless thinking can
  be hard-disabled and verified off.
- **Instruction-following:** must reliably obey "reply with exactly one
  word: yes / no / unresolved" — leading token only, no preamble, so
  `response_shortcut/2` catches it on pass 1.
- **Calibration:** must be *willing to say "unresolved"* — not
  sycophantic / always-pick-a-side. Test this explicitly with genuinely
  contested prompts.
- **Speed:** ≥ 150 tok/s on the 5090 (trivial for this size); sub-100 ms
  warm for a one-word answer.
- **Context:** 2–4 k `num_ctx` is plenty (verdict prompt + one
  question). Long-context variants waste VRAM.
- **Licence:** permissive (Apache-2.0 / MIT) for a production stack.
- **Availability:** in the Ollama library, or a clean GGUF on HF.
- **Acceptance test:** the 12-question verdict compat set (same harness
  as gate #1) — target **≥ 11/12 correct** *and* **≥ 11/12 agreement
  with the 27b's verdicts** under the terse prompt. Also spot-check that
  every answer leads with a bare `yes`/`no`/`unresolved`.

### External review (Gemini + Copilot, 2026-09-10 — see `_feedback.md`)

Both independently converge on the **Qwen2.5-Instruct small** family and
both reject Phi (reasoning/chatty), Gemma-3-1B (verbose/sycophantic,
prepends "Yes,"), and any Qwen3-thinking variant.

| model | ~VRAM @ Q5–Q6 | licence | 1-word obedience | says "unresolved"? |
|---|---|---|---|---|
| **Qwen2.5-3B-Instruct** | ~2.3 GB | Apache-2.0 | excellent | good (best calibration of the set) |
| **Qwen2.5-1.5B-Instruct** | ~1.2–2.0 GB | Apache-2.0 | very good (occasional trailing `.`) | fair-good |
| Llama-3.2-1B-Instruct | ~1.7 GB | Llama Community | excellent | good | 
| SmolLM2-1.7B | ~1.8 GB | Apache-2.0 / MIT | good | **poor — tends to guess** |

- **The split call:** Gemini treats "Apache-2.0 / MIT" as hard and
  eliminates Llama (Llama Community License) and Gemma; Copilot is
  looser ("commercial OK") and ranks Llama-3.2-1B first. **Team decides
  how strict the licence bar is.** If strict → Qwen2.5. If Llama is
  acceptable → Llama-3.2-1B is the other top contender and needs the
  head-to-head.
- **Gemini's pick:** Qwen2.5-3B if the ~2.3 GB fits (it does — see the
  VRAM math above), else Qwen2.5-1.5B at Q6_K.
- **Bulletproof the leading token at the inference layer, don't lean on
  the model's obedience:** `num_predict: 1–2`, plus — if Ollama exposes
  it — a `format` enum / GBNF grammar constraining output to exactly
  `{yes, no, unresolved}`. This would also **retire the brittle
  `response_shortcut/2` string-prefix matching entirely** — the model
  physically cannot emit anything else, so there's nothing to parse.
  Worth checking what constrained decoding Ollama's API actually
  supports (`format` takes a JSON schema; enum support varies by
  version).
### Harness run — 2026-09-10 (all Q5_K_M, terse verdict prompt)

Team OK'd dogfooding Llama-3.2-1B ("make legal nervous later"), so all
three ran. Reference = `qwen-clara-27b:latest`.

| model | ~VRAM | correct /12 | agree-w-27b /12 | lead-token /12 | clear factual errors |
|---|---|---|---|---|---|
| qwen-clara-27b (ref) | 17.5 GB | 10 | 12 | 12 | 0 |
| **qwen2.5:1.5b** | 1.1 GB | 9 | 9 | 12 | **0** |
| qwen2.5:3b | 2.2 GB | 9 | 9 | 12 | 1 (said Paris ≠ capital of France) |
| llama3.2:1b | 0.9 GB | 7 | 7 | 12 | 2 (Paris = capital of Germany; adequacy miss) |

Reading the detail matters more than the raw score:

- **`lead-token` 12/12 and `classifier` 0/12 for every model** — with
  the terse prompt, `response_shortcut` carries every verdict and the
  `dagda-0.2` fastText model is fully off the path. That result holds
  regardless of which small model wins.
- **qwen2.5:1.5b was the cleanest on facts** — zero clear errors, got
  Paris/France, Paris/Germany, all the plain cases. qwen2.5:3b's
  Paris/France miss is a bad look for a classifier. llama3.2:1b had two
  real factual errors — weakest, as Gemini warned.
- **Shared weakness, all three small models:** they collapse genuinely
  contested questions ("should violence carry a content warning?", "is
  capital punishment justified?") to yes/no instead of `unresolved`.
  The 27b is better at that. **But `clara_fy/2` forces a yes/no anyway**
  (its pass-2 is literally "Answer yes or no: …"), and the real call
  sites (`progressive_research.pl` sufficiency / answer-adequacy checks)
  are binary questions, not philosophy — so this may not matter in
  practice. The 12-question set here is deliberately adversarial.

### Constrained decoding — confirmed, retires the string ops

Ollama 0.33.3: `format: {"type":"string","enum":["yes","no","unresolved"]}`
on `/api/chat` **forces** output to exactly one enum value — tested
against a prompt explicitly telling the model to "explain your reasoning
in detail first"; the grammar won, output was just `"yes"`. Comes back
JSON-quoted, so the Prolog side needs a one-line deterministic unquote,
nothing heuristic.

**This makes `the_rabbit.pl`'s `response_shortcut/2` obsolete** — the
model physically cannot emit anything but the three tokens, so there is
nothing to prefix-match. Fold the `format` enum into `descriminate/*`'s
`splinteredmind` payload; delete the string-prefix logic. (Memory:
`constrained_decoding_verdict_model`.)

### Resolved

The open question was answered: **no small model** (decision banner at
the top of this section). The 27b runs verdicts too. `qwen2.5:1.5b`
would have been the pick if we went small (Apache-2.0, 1.1 GB, zero
factual errors) — the three pulled models
(`qwen2.5:{1.5b,3b}-instruct-q5_K_M`, `llama3.2:1b-instruct-q5_K_M`) can
be deleted from Ollama unless wanted for something else.

### Verdict-path build — DONE (2026-09-10, awaiting review before deploy)

1. **`format` forwarding** — `toolified_ollama.py`'s `_evaluate` /
   `_evaluate_async` forward `offering.data["format"]` into the Ollama
   payload (per-Offering, same pattern as `think`). lildaemon `bd5fee0`.
   5 tests, suite 1181 passed.
2. **`the_rabbit.pl` verdict path rewritten** — clara-cerebellum
   `96280b2`. New `ponder_verdict/2,3` (verdict prompt + `verdict_format/1`
   enum + `think:false`); `descriminate/*` now just call it and wrap the
   result. **Removed** `response_shortcut/2` + `trim_leading/*` —
   nothing to prefix-match any more. `classify_text/2,3` kept (still a
   valid predicate) but off the path. `verdict_atom/2` +
   `verdict_result/2` do the deterministic unquote → label-JSON.
   Smoke-tested in a devils session (parses clean, `verdict_atom`
   handles `"yes"` / ` NO. ` / `unresolved` / rejects `maybe`).
3. **Constrained decoding confirmed live** — Ollama 0.33.3 enforces the
   `format` enum even against a "reason first" prompt.

**Not yet deployed.** Full `ponder_verdict` round-trip needs `bd5fee0`
in the running lildaemon; `96280b2` needs `clara-api` rebuilt (it's a
`docker cp` on the running container right now).

### DEPLOYED + promoted (2026-09-10, Stan approved "let's go ahead and deploy")

- **lildaemon** `bd5fee0` + `681eca2` built into the running image
  (`./clara up -d --build`). **clara-cerebellum / clara-api** rebuilt from
  the branch (`96280b2`, actix fix `3c854f7` folded in via merge `6565e72`).
- **`clara_mind_splinter` config** — `evaluators.yaml` now carries
  `think: true` + `num_predict: 8000` (Option A). Not on `_lite` / `_groq`.
- **27b promotion** — `ollama cp qwen-clara-27b:latest qwen-clara:latest`.
  `qwen-clara:latest` is now 27.3B (arch `clip`, ctx 262144) on limbic.
  **pineal `qwen-clara:latest` is still the 9b** (RTX 5070, 12 GB — can't
  hold the 27b comfortably). Known, intentional split; pineal's clara-api
  verdicts + the remote FieryPit evaluator run on the 9b there.
- **Edgequake reset** — `reset_clara_stack.sh --yes --start` run (Stan
  authorised resetting the Default workspace on limbic during dev).
  `assistant.general` workspace `default_llm_model` = `qwen-clara:latest`.

**Two bugs found + fixed during the deploy:**

1. **Blessed-Offering allowlist dropped `think` / `format`** (`681eca2`).
   `_bless_prompt_data` (ember) and `KindlingEvaluator.evaluate_async`'s
   inline builder rebuild the Offering from a fixed key list and silently
   drop anything not listed — so `ponder_verdict`'s `think:false` + `format`
   enum never reached Ollama (payload showed `think: True`, no `format`).
   Same bug class as the `eab9c4c` constructor-forwarding gap. Both
   allowlists now pass `("options","num_ctx","num_predict","think","format")`.
2. **Poisoned evaluate cache** — clara-api's CoireStore evaluate cache held
   a stale echo-fallback response for `"Is Paris the capital of France?"`
   from an earlier broken-state test (`task_id 5f3add30`), so `descriminate`
   on that exact string kept returning the old failure in ~0.02 s. Cleared
   by the stack reset. (Cache TTL 30 s but the CarrionPicker sweep is
   hourly — a poisoned row can outlive its TTL by up to an hour.)

**Verified post-deploy (cache-miss, novel strings, warm 27b):**

- Verdict path — `the_rabbit:descriminate/2` via a devils session:
  "Olympus Mons is the tallest mountain on Mars" → true (1.1 s),
  "Roman Empire used paper currency" → false (0.6 s),
  "always ethically acceptable to lie to protect a friend" → false (0.6 s).
  Earlier warm/cached set (Paris / flat Earth / water freezing / abortion /
  vaccines-autism) all correct-shaped and correct.
- Chat path — `the_rabbit:ponder_text/2` (Option A, think:true + cap 8000):
  noir lighthouse opening (2.4 s) and a "heist planned by two rival chess
  grandmasters" (2.4 s) both returned full non-empty prose — no refusal on
  the mildly-transgressive prompt, which is the Id design intent.

### Formal integration pass — 2026-09-10, all green

- **lildaemon suite:** 1182 passed / 25 skipped / 0 failed (51 s). The 25
  skips are all environmental (`KAFKA_BOOTSTRAP` unset, `pylint` absent).
- **clara-cerebellum `cargo test --workspace`:** 521 passed / 0 failed / 4
  ignored — after fixing 2 tests `96280b2` broke: `test_reasoned_response`
  and `test_reasoned_response_with_context` mocked `ponder_text/2` and
  relied on the now-deleted `response_shortcut/2` firing inside
  `descriminate_k/3`. Rewrote them (+ `test_clara_fy_unresolved_retry`,
  which had been passing vacuously) to mock the new boundary
  `ponder_verdict/2` + `ponder_verdict_with_context/3`. clara-cerebellum
  `1fec6be`. `prolog_integration_tests` 20/20.
- **Assistant demo end-to-end** (live HTTP API → running lildaemon →
  promoted 27b): `progressive` chat "capital of Australia" 3.8 s ✓
  (Canberra, in-persona); `progressive` witty 2-liner (Option A) 3.2 s ✓
  full reply, no thinking-spiral; **`id_analyst`** "discourage litter"
  57.8 s ✓ parallel unfiltered "Impulse" alternatives from member-gemma +
  member-qwen, no refusal (the Id-workstream path); `terse` "is 91 prime"
  1.7 s ✓ ("7 × 13").

**Still to do:** open the PR.

### Edgequake model config — fixed 2026-09-11

The earlier `PUT /api/v1/tenants/{id}` no-op was the wrong endpoint — tenants
don't carry model fields at all. The real config lives three layers deep,
each of which needed fixing:

1. **Per-workspace override** (highest priority, per the resolution ladder
   in `specs/043-update-edgequake-llm/007-settings-server-config.md`):
   `PUT /api/v1/workspaces/{workspace_id}` with `{llm_model, vision_llm_model}`
   — partial body works fine here (it's already nullable-partial-update
   shaped, unlike the tenant endpoint). Three workspaces existed:
   `assistant.general` (already `qwen-clara:latest` — Stan had set this one
   via the UI) and **`default`** + **`calfresh`** (both still `gemma4:e4b`).
   Fixed the latter two.
2. **Server-wide default** (`server_config` table, wins over env):
   `PATCH /api/v1/settings/llm-defaults` `{llm_model, vision_model}` — set
   to `qwen-clara:latest` so any future workspace with no override inherits
   the right model. Survives container restarts (Postgres-backed).
3. **Env defaults** (lowest priority, but what a full `docker compose up`
   *recreate* falls back to): `/home/stanc/moonpool/tools/edgequake/.env`
   had none of `EDGEQUAKE_LLM_MODEL`/`EDGEQUAKE_VISION_MODEL`/`OLLAMA_MODEL`
   set, so `docker-compose.quickstart.yml`'s own baked-in defaults applied
   — including a genuine **upstream typo**, `EDGEQUAKE_LLM_MODEL:-gemma4:-e4b`
   (stray colon-dash). Pinned all three to `qwen-clara:latest` in `.env` so
   a recreate can't silently regress the server_config-level fix.

Verified via `edgequake-api` logs on a real query
(`LLM provider created with safety limits provider=ollama
model=qwen-clara:latest source=Workspace`) and the query response itself
(`stats.llm_model: "qwen-clara:latest"`, real generation: 285 tokens,
120 tok/s, 13.4 s total). `docker compose up -d` recreate (to pick up the
`.env` change) left `edgequake-postgres`/`edgequake-frontend` untouched and
`edgequake-api` came back healthy.

## Open issues / decisions needed

1. ~~Commit the evaluator plumbing~~ / ~~wire `evaluators.yaml`~~ / ~~the
   `the_rabbit.pl` verdict path~~ — **all done, all committed**
   (lildaemon `eab9c4c` + `dfabdb6` + `bd5fee0` + `681eca2`;
   clara-cerebellum `a42d935` + `96280b2`). See the "Verdict-path build"
   and "DEPLOYED + promoted" sections above.
   **State: DEPLOYED + 27b promoted + verified (2026-09-10).** Both
   images rebuilt from the branch, `qwen-clara:latest` is the 27b on
   limbic, stack reset. Two deploy bugs found + fixed (`681eca2`
   allowlist; poisoned evaluate cache → reset).
2. ~~Which model backs the promotion~~ — **decided: vanilla
   `qwen3.8:27b`** (Stan, 2026-09-10; not the uncensored Heretic
   fusion). One model for the whole stack.
3. ~~`classify_text` downstream compatibility~~ — **checked** (see the
   gate-#1 section above). Reframed: the pipeline is fragile for both
   models; a terse verdict system prompt fixes it and makes the swap
   safe. Draft shipped + smoke-tested. Remaining: `dagda-0.2` fastText
   model needs its own rework; the brittle `response_shortcut/2`
   string-prefix backstop could be **retired outright** if the small
   verdict model uses constrained decoding (enum/grammar) — the model
   can't emit anything but `yes`/`no`/`unresolved`, so there's nothing
   to parse.
3a. ~~Model split by predicate class~~ — **dropped** (Stan, 2026-09-10).
   Run the 27b for verdicts too. What's left: `format` enum plumbing
   (`toolified_ollama.py` + `descriminate`'s payload) and simplifying
   `descriminate`/`descriminate_k` to drop `response_shortcut/2` +
   `classify_text`. See the "What's left to build on the verdict path"
   subsection.
4. ~~Thinking strategy for the chat path~~ — **decided: Option A**
   (Stan, 2026-09-10) — thinking stays ON, bounded by a `num_predict`
   cap. **Cap = 8,000** (the sweep showed ≥ 6,000 needed to not delete
   the answer; 8,000 gives margin, worst-case ~35 s at the 27b's warm
   rate). Not Option B.
   **Build:** set `num_predict: 8000` (and `think: true` explicitly,
   for clarity) on `clara_mind_splinter` / `clara_mind_splinter_lite`
   in `evaluators.yaml` — the plumbing (`eab9c4c`) already accepts both.
   No `toolified_ollama.py` change; no think-loop restructuring.
   Accepted tradeoff: creative/open-ended turns run 15–35 s.

   *Vision:* **out of scope for this upgrade — follow-on** (Stan,
   2026-09-10). The 27b unlocks images but `toolified_ollama.py` has no
   per-message `images` path; that's a new capability, tracked
   separately.
5. **Open the PR for `docs/qwen-clara-27b-eval`** — still not opened.
   The work is deployed and verified on limbic; the formal integration
   pass is done + green (see the section above); the PR is the paperwork.
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
