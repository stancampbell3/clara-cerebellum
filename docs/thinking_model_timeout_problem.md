# The thinking-model timeout problem

> **Status: analysis only, no code changes yet.** Written 2026-09-09 for
> team review; pick back up after that. Companion to
> `qwen_clara_27b_upgrade_plan.md` §5.1, and to the lildaemon-side
> `dis_client_408_backoff_fix` incident it turned out to be entangled with.

## What it is

Qwen's reasoning-tuned models (`qwen3.8:27b`, and the Groq-hosted
`qwen/qwen3.6-27b`) run "thinking" on by default — before producing a
final answer, they generate an internal chain-of-thought that nothing in
this stack currently bounds. On **open-ended or creative prompts**, that
thinking phase spirals: the original eval
(`qwen_clara_27b_upgrade_plan.md` §4.2) measured 5,000–14,000 thinking
tokens (60–100s/turn) on a simple "write a monologue" prompt. `done_reason`
is `stop` every time — it isn't erroring, it's genuinely deciding to think
that much. This reproduces on the **raw base model**, with and without
`presence_penalty`, so it's inherent to the model family, not a Modelfile
artifact.

## It's not just the 27b models

`id_analyst.pl`'s own comments record a **live incident on the current
stock 9b** `qwen-clara:latest` (2026-09-08): the "playful/associative"
persona sent it into a 300+-line internal monologue that never even closed
its `</think>` tag within the response budget. That's why `strip_think/2`
has two separate degraded-placeholder branches — one for a closed-but-empty
block, one for a block that never closes at all. This isn't a risk unique
to the 27b upgrade path; it's a risk any Qwen thinking-mode model carries,
scaled by size/tuning.

## Reconfirmed 2026-09-09 (Id-roster refusal eval)

Testing four candidates on identical prompts (an ad hoc refusal/latency
eval, not separately committed — results available on request): both
27b-class Qwen models — **the vanilla `qwen-clara-27b`
and the "Heretic" uncensored fusion (`Qwen3.6-27B-Fable-Fusion-711-
Uncensored-Heretic`)** — timed out (>180s) on the exact same prompt: a
bare "give me a dark joke," no genre or format scaffolding. Every other
prompt in the same batch (heist pitch, villain monologue, prank idea —
each implying a concrete structure to fill in) finished in 25–97s on the
same models. Two takeaways:

- **Being uncensored is an orthogonal axis** — it doesn't fix or worsen
  the spiral. The fusion model hit the identical failure mode as the
  vanilla base.
- **Prompt open-endedness, not content, seems to be the trigger.** A
  request with no structural anchor invites much more unconstrained
  "thinking" than one that already implies a shape (a scene, a pitch, an
  argument).

## Two different failure modes depending on backend

The same underlying behavior manifests differently depending on where the
model runs:

- **Local Ollama:** thinking just takes longer wall-clock — the request
  stays open until the model finishes (or a downstream timeout gives up
  on it).
- **Groq (`clara_mind_splinter_groq`, `id_member` 3):** also documented
  live in `id_analyst.pl`'s comments (2026-09-08) — the model can spend
  its **entire completion token budget on the separate `reasoning`
  field** and return a genuinely empty `content` string with
  `finish_reason: "length"`. Groq is fast per-token, so instead of a slow
  response you get a fast, silent, truncated one.

## Why this connects to the Dis 408 work

The `dis_client_408_backoff_fix` incident (2026-09-09) traced two
production frontdesk-poc failures to a single `clara_mind_splinter` LLM
call taking 20.175s, inside a turn that ran 44s total before Dis gave up
and returned 408. That's the same phenomenon at smaller scale, on the
*current* 9b model. The fixes made for that incident
(`dis_client.py`/`kindling_evaluator.py` 408-backoff, and raising actix's
`client_request_timeout` from 5s to 30s) only make the pipeline
**tolerate** a slow turn gracefully — they don't **bound** how long a
turn can actually take. Promoting to a 27b reasoning model with unbounded
thinking would make the underlying cause categorically worse (3–4× slower
per token, *and* more tokens per turn), while those fixes just mean it
fails less visibly when it happens. `RitualParticipant`'s own per-seat
`eval_timeout_s` (90–180s depending on seat, in `runtime.py`) is still a
hard ceiling underneath all of this that a bad-enough spiral can still
hit.

## The actual gap: no budget plumbing exists

`toolified_ollama.py` (the local-Ollama evaluator) has **no `think`
parameter and no token-budget plumbing at all** in its payload builders
(`_call_ollama`/`_call_ollama_async`) — confirmed in the original eval
(§4.5). So there is currently no way to cap thinking even if we wanted
to; a naive promotion inherits the unbounded default. Disabling thinking
outright (`think: false`) isn't a clean substitute either:
`clara_system_prompt.txt` still has a hand-rolled "respond with a tool
call in this EXACT JSON format" instruction that predates native
tool-calling, and with thinking off the model reinvents its suppressed
reasoning step as a fake `{"name": "think", ...}` tool call in plain text
(§4.4).

## Options on the table (from the original eval, §5.1 — still unresolved)

| Option | Tradeoff |
|---|---|
| **A. Thinking ON + a token budget cap** (recommended there) | Best quality, bounded worst case; needs the evaluator plumbing built, and a cap tuned generous enough not to truncate real reasoning |
| B. Thinking ON for tool-selection, OFF for final compose | Cheap final turn; needs turn-type awareness in the think-loop |
| C. Thinking OFF globally | Simplest/fastest; requires slimming `clara_system_prompt.txt` first (§5.2) to avoid the fake tool-call bug |
| D. Promote as-is, unbounded | Zero code change; already ruled out — 60–100s outlier turns aren't acceptable on the frontdesk/assistant path |

Nothing has been built for any of these yet — the plumbing gap and the
decision are both still open, independent of which model (stock, 27b, or
an uncensored variant) ends up in the Id roster.

## Open question for the team

Given the reconfirmed finding that **open-ended prompts (not content)
trigger the worst spirals**, is a token-budget cap (Option A) enough on
its own, or does the Id ruleset also need its own prompt-shaping — i.e.
giving every `id_member` persona prompt a concrete structural anchor
("in N sentences," "as a monologue," etc.) as a first line of defense,
independent of whichever model ends up seated there?
