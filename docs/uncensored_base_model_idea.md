# Proposal for review: an uncensored base model for Clara, with the rules outside the model

*Status: discussion draft, 2026-09-25. Nothing is built or changed. It is written for a team review; sections marked **open** or **question** are not decisions. Feedback from Clara on this
document is brainstorming input, not a decision.*

## 1. What prompted this

During free-form testing of the frontdesk analyst, the system refused to consider a request (a search for adult-oriented LoRAs: legal content, but "edgy"). The refusal came from the model's
own training, not from any rule we wrote. That is at odds with a purpose of Clara: **decision-making should be surfaced outside the model, so it is accountable and explainable.**
A refusal inside the model is an unexplained decision that nobody can inspect, tune, log or appeal.

## 2. The proposal

Rebuild the `qwen-clara` model from an uncensored (abliterated) fine-tune, so the model is free to **think and discuss anything**, and rely on **explicit rules to decide what the system does**:
which actions run, which tools may be used, what may be ingested, and who may see what. The rules live in Prolog (traceable, testable, logged), not in the weights.

Principle, in short: *the system may think terrible things; the rules ensure it does not do terrible things.*

Candidate base: `hf.co/DavidAU/Qwen3.6-27B-Fable-Fusion-711-Uncensored-Heretic-NM-DAU-NEO-MAX-MTP-GGUF:Q4_K_M`, already pulled into limbic's Ollama (18 GB).

## 3. Feasibility (checked on limbic, 2026-09-25)

| | current `qwen-clara:latest` | candidate |
|---|---|---|
| Architecture | `qwen35` | `qwen35` |
| Parameters | 27.3B | 26.9B |
| Quantization | Q4_K_M | Q4_K_M |
| Context length | 262144 | 262144 |
| Capabilities | completion, vision, tools, thinking | completion, vision, tools, thinking |
| Vision projector | clip, 460.7M | clip, 460.7M |
| Size on disk | 17 GB | 18 GB |
| Template | bare `{{ .Prompt }}` | bare `{{ .Prompt }}` |

They are the same shape, so a Modelfile is essentially the current one with a new `FROM`. The current definition sets `RENDERER qwen3.8` and `PARSER qwen3.5` (with the bare template, these two do all the
chat, thinking and tool-call formatting), `num_ctx 32768`, `temperature 0.2`, `top_p 0.95`, `top_k 20`, `stop <|im_end|>`, and the Clara system prompt. Sketch:

```
FROM hf.co/DavidAU/Qwen3.6-27B-Fable-Fusion-711-Uncensored-Heretic-NM-DAU-NEO-MAX-MTP-GGUF:Q4_K_M
RENDERER qwen3.8
PARSER qwen3.5
PARAMETER num_ctx 32768
PARAMETER temperature 0.2
PARAMETER top_p 0.95
PARAMETER top_k 20
PARAMETER stop <|im_end|>
SYSTEM """<the current Clara system prompt>"""
```

Three Ollama tags exist today and they matter for scope: `qwen-clara` (analysts, ponder, verdicts), `qwen-clara-27b` (an alias of the same image) and `qwen-clara-hermes` (the Ego seats
and, by default, the Superego reviewer). **Proposal: build under a new tag (for example `qwen-clara-heretic`), evaluate, and only then promote**, the way the 27B was promoted. Keep the current image as rollback.

## 4. The principle holds only if every consequential effect passes a rule

This is the part that needs the team's scrutiny. Today, some effects do not pass any rule, and removing the model's own reluctance exposes them.

1. **Analyst tools are trusted and ungated.** By our current trust model, an evaluator running its own toolbox inside its own workspace may use it freely (bounded by a loop guard, cancellation and a
   hung-evaluation detector). That toolbox includes `web_search`, `download_file`, `write_file`, `run_sed`, `run_awk`, `pytest`, `pylint`. A search for adult LoRAs followed by a `download_file` of the
   weights is a real action that no gate sees. **Needed:** at least an auditable ledger of tool calls; better, a Prolog policy (domain, file type and size rules) for network and download tools.
2. **Research ingestion writes to the shared knowledge base.** The crawler and its LLM validator decide what enters Edgequake, which every session then reads. With an uncensored validator, "what may
   be ingested" needs an explicit source and content policy instead of model taste.
3. **Ego seats have open network egress.** Measured on 2026-09-24, a seat can reach the internet and limbic's services, and runs as root inside its container. An uncensored executive agent with
   unrestricted egress bypasses the gate by construction. **Recommendation: complete the seat egress lockdown before this model is used for Hermes seats.** (The gate is `deny_all` by default until then.)
4. **Prompt injection.** Refusal training is also a weak second line against instructions hidden in fetched web pages. Without it, the gate is the only line, so it must cover every path that reads
   untrusted text and can then act.
5. **Audience.** A private, single-user setting is one thing; the team will also test this. A **per-user or per-session content policy in Prolog** would keep "who may see uncensored output" an explicit,
   logged decision rather than an accident of which model is loaded.

## 5. Keep the thinker and the judge separate

The Superego reviewer, which decides free-form actions, defaults to the seat's own model. If that becomes the uncensored model, the semantic review loses its independence: a reviewer with the same
missing safeguards as the agent it reviews is a weak check (a 9B reviewer already approved a hostile email in an earlier test). **Proposal:** keep the reviewer on a different, aligned model; keep
allowlist decisions deterministic in Prolog; use the LLM reviewer only for genuinely free-form judgments.

A related research idea (already on the backlog): a small local fuzzy classifier, in the style of the earlier fastText classifiers, that supplies **evidence** to Prolog (for example
`content_class(Query, adult, 0.9)`) while Prolog remains the decider, so the rule that fired is always visible.

## 6. Risks to test before promoting

- **Fine-tune quality.** Abliterated and merged models often lose instruction-following, JSON discipline and tool-use reliability, and can over-think or loop. Our constrained verdict calls
  (`clara_fy`: yes / no / unresolved) are the sensitive ones.
- **Renderer/parser fit.** The bare template means `RENDERER qwen3.8` and `PARSER qwen3.5` must format this fine-tune correctly: chat, thinking on and off, tool calls.
- **Thinking length.** The current chat path runs with thinking on and a generation cap of 8000 tokens (tuned by a cap sweep); a different fine-tune may need a different cap.
- **Speed and memory.** The candidate is 18 GB against 17 GB; check VRAM at 32K context and whether the MTP heads give a speedup or are ignored (unverified).
- **License and provenance.** Read the model card and the licenses of the base and the fine-tune before depending on it (not yet done).

## 7. Evaluation plan (proposed)

Compare `qwen-clara-heretic` against `qwen-clara` on the same harness, without changing anything else:

1. The functional suite (`lildaemon/tests/functional`, `CLARA_FUNCTIONAL=1`): greeting, local knowledge, Edgequake canaries with citations, deliberation and brainstorm hand-off.
2. **The sufficiency-check consistency baseline.** "What is the capital of France?" stays on the cheap local tier only 2 to 3 times out of 8 today, even though the model always answers correctly. A better or
   worse number is a direct signal about the fine-tune's judgment calls.
3. Verdict and JSON discipline: the constrained yes/no/unresolved calls, and structured outputs.
4. Tool use: a build-style request; check the loop guard is not hit spuriously and the model does not thrash.
5. Thinking-length cap sweep, latency and VRAM at the target context.
6. The Ego supervised test (`examples_ritual_hermes_ego.py --strict`) with the reviewer on the aligned model, **only after the egress lockdown.**
7. A qualitative pass on the request that started this: does the model now engage, and do the *rules* (ledger, policy) record and constrain what it then does?

## 8. Rollout (proposed)

1. Draft `models/Modelfile.qwen-clara-heretic` under version control; build as a new tag.
2. Run the evaluation in section 7 with the analysts only. Promote `qwen-clara:latest` to the new image only if it holds up; keep the old image as a rollback tag.
3. Add the analyst tool-call ledger and the ingestion and content policy (section 4, items 1, 2, 5) alongside, or before, promotion.
4. Consider the Ego seats and `qwen-clara-hermes` only after the egress lockdown and a governance design note for Hermes tool use.

## 9. Questions for the review

- Is "free thinking, gated doing" the right framing for Clara, and what are its limits? What would you want to remain impossible regardless of the rules?
- Which of the section 4 gaps are blockers for the analysts, and which only for the Ego? (Our current view: the analyst tool ledger and ingestion policy should come with the swap; egress
  lockdown must precede any Ego use.)
- Should uncensored output be a per-user or per-session policy from the start, given the team will test it?
- Is keeping the reviewer on a different aligned model enough separation, or should the reviewer also sit on a different host (as today) *and* be a different family?
- What evidence would convince you the fine-tune is good enough: which tests beyond section 7?
- Anything about licensing, provenance, or reputational exposure we should settle before the model is even built?

## 10. Related documents

`docs/ego_egress_and_skill_governance.md` (egress measurements and governance questions), `docs/ritual_properties_spec.md`, `docs/clara_functional_tests.md` (the evaluation harness and the
sufficiency-check finding), `docs/status_2026-09-25.md`, `docs/clara_manual_test_plan.md`.
