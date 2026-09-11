# qwen-clara base-model upgrade: qwen3.8:27b evaluation & plan

> **Status: evaluated, not promoted.** A candidate model
> (`qwen-clara-27b:latest`) is built and A/B-tested against the current
> `qwen-clara:latest`. Quality is clearly better, but it is **not a
> drop-in** — three integration decisions (below) block promotion. This
> doc records the eval and proposes the path to promote it. Open design
> question worth a second opinion: **the thinking strategy** (§5.1).

## 1. Context

`qwen-clara:latest` (ollama id `a2e54c0123e4`, ~6.6 GB) was built a couple
of months ago from `qwen3.5:9b-q4_K_M`. Its Modelfile lives at
`clara-cerebellum/models/Modelfile.qwen-clara`:

```
FROM qwen3.5:9b-q4_K_M
RENDERER qwen3-coder
PARSER   qwen3-coder
PARAMETER num_ctx 32768
PARAMETER temperature 0.2
PARAMETER presence_penalty 1.5
PARAMETER stop "<|im_end|>"
PARAMETER stop "<tool_res>"
SYSTEM  """You are Clara, a powerful multimodal agent. …"""
```

It is the local-Ollama model behind every non-Groq Clara splinter:
`config/evaluators.yaml` → `clara_mind_splinter`, `clara_mind_splinter_lite`,
`snek`; `lildaemon/config/fish.yaml` → `ollama`/`ClaraFish`; and the
`examples_ritual_*` scripts.

**The ask:** evaluate `qwen3.8:27b` (newly pulled, ollama id `22130167c4c2`,
~17 GB — arch `qwen35`, 27.3 B params, Q4_K_M, capabilities
`completion / vision / tools / thinking`) as an upgrade base.

## 2. What was built

`clara-cerebellum/models/Modelfile.qwen-clara-27b` — mirrors
`Modelfile.qwen-clara` as closely as the new base allows. The one **forced**
change:

| | current | candidate |
|---|---|---|
| `FROM` | `qwen3.5:9b-q4_K_M` | `qwen3.8:27b` |
| `RENDERER` / `PARSER` | `qwen3-coder` / `qwen3-coder` | `qwen3.8` / `qwen3.5` |

`qwen3.8:27b` ships with `RENDERER qwen3.8` / `PARSER qwen3.5` (per
`ollama show --modelfile qwen3.8:27b`). It is a general + vision + **thinking**
model, not a coder variant; the `qwen3-coder` renderer mis-frames its
tool-call and thinking channels and — see §4 — cannot tokenize image input
at all.

Everything else (`num_ctx 32768`, `temperature 0.2`, `presence_penalty 1.5`,
both `stop` tokens, the Clara `SYSTEM` block) was carried over verbatim for a
clean A/B, even though some of it is qwen3-coder-era tuning that should be
revisited (§5.3).

Build: `ollama create qwen-clara-27b -f Modelfile.qwen-clara-27b`.

## 3. Eval method

Both models driven through `/api/chat` on the local Ollama, using the **real
deployed artifacts**: `lildaemon/config/prompts/clara_system_prompt.txt` as
the system prompt and `lildaemon/goat/tools/python_tools.json` (10 tools) as
the tool schema. Tasks chosen to cover what a Clara splinter actually does:
persona/voice, single tool call, multi-turn tool loop (composing an answer
from a tool result), vision, strict instruction-following, and a small
reasoning problem. Throughput from `eval_count / eval_duration`; VRAM from
`/api/ps`.

Hardware: dev workstation, RTX 5090 (32 GB), sharing the GPU with a running
ComfyUI (`--lowvram`, ~12.5 GB resident) — see §4 finding 6 for why the
27b throughput numbers here are a floor, not a representative figure.

## 4. Results

| Task | `qwen-clara` (9b) | `qwen-clara-27b` |
|---|---|---|
| Persona / wit | Flat, generic. **Spiraled to 6,604 tokens / 34 s** overthinking "in two sentences". | Genuine Clara voice, witty, concise (~350–450 tok). |
| Tool call (single) | ✓ correct native `tool_calls` | ✓ correct, cleaner (no extra chatter alongside the call) |
| Tool loop (compose from result) | ✓ in character | ✓ in character, better prose |
| **Vision (describe an image)** | ✗ **HTTP 400 `Failed to tokenize prompt`** | ✓ accurate, detailed, correct text extraction |
| Strict JSON enum | ✗ returned `"mixed"` — not in the allowed enum | ✓ valid enum value, reasoned confidence |
| Constraint puzzle | ✓ (answer only) | ✓ (answer + correct shown reasoning) |
| Throughput | ~195 tok/s, 6.6 GB VRAM (100 % GPU) | ~45–62 tok/s GPU-resident; ~15–28 tok/s today under ComfyUI contention |
| VRAM @ 32k ctx | 6.6 GB | ~18.5 GB (spilled to CPU on this box) |

### Findings

1. **The renderer/parser swap is mandatory, and it fixes vision — which is
   broken today.** `qwen-clara:latest` cannot process images at all: with
   `RENDERER qwen3-coder` any image input returns
   `HTTP 400 {"code":400,"message":"Failed to tokenize prompt"}`, despite the
   base model carrying a CLIP projector and the `SYSTEM` block asserting "You
   can see and analyze any images provided to you. Do not claim you cannot
   see images." The 27b build with `RENDERER qwen3.8` describes images
   correctly. (Incidental: the base `qwen3.5:9b-q4_K_M` is no longer pulled
   locally — only the derived `qwen-clara` remains, so a 9b rebuild would
   need a re-pull.)

2. **`qwen3.8:27b` is a reasoning model with thinking ON by default.** This
   helps agentic tool selection and reasoning, but open-ended / creative
   prompts spiral: a "write a 120-word noir monologue" prompt produced
   **5,000–14,000 tokens of `thinking`** (60–100 s/turn) before a good final
   answer. `done_reason` was `stop` every time — the model is terminating
   naturally, it just thinks enormously. This reproduces on the **raw base
   model** with and without `presence_penalty`, so it is inherent, not a
   Modelfile artifact.

3. **With `PARSER qwen3.5`, thinking is returned in a separate `thinking`
   field** (clean `content`). Good news for integration: `content` is
   already clean, so `toolified_ollama.py`'s `<think>`-block string-strip
   (≈ line 894 / 1384) becomes a no-op rather than a requirement. The only
   cost of thinking is latency/tokens, not garbled output.

4. **`think: false` tames the latency but breaks tool formatting.** With
   thinking disabled, `clara_system_prompt.txt`'s hand-rolled instruction —
   "When you need to use a tool, respond with a tool call following this
   EXACT format: `{ "name": …, "arguments": … }`" — makes the model emit a
   fake `{"name": "think", "arguments": {"thought": "…"}}` **as text
   content**, i.e. it reinvents a thinking step as a bogus tool call. That
   system prompt predates native tool-calling and should be slimmed
   regardless of which base wins (§5.2).

5. **`toolified_ollama.py` has no `think` parameter and no image plumbing.**
   The Ollama payload is built in `_call_ollama` / `_call_ollama_async`
   (`payload["tools"]`, `payload["options"]`, `payload["keep_alive"]`,
   `payload.setdefault("system", …)`) with no path for `think` or per-message
   `images`. So a naive promotion inherits the unbounded thinking default in
   production, and the vision win stays latent until the evaluator is wired
   for it.

6. **`presence_penalty 1.5`** is a qwen3-coder-era anti-loop knob. On the new
   stack it is neither helpful nor harmful — the length problem is *thinking*,
   not token repetition. `PARAMETER stop "<tool_res>"` is vestigial (a
   qwen3-coder inline tool-result sentinel; the `qwen3.5` parser returns tool
   results via the messages array).

7. **Perf / footprint.** ~3–4× slower per token *and* more tokens per turn
   (thinking). Needs ~18–20 GB VRAM at 32k ctx. On the dev workstation it
   contends with ComfyUI and spills to CPU (hence the 15–28 tok/s floor
   above); GPU-resident it does ~45–62 tok/s. **All perf/UX numbers must be
   re-measured on the actual splinter host** before a go/no-go.

## 5. Decisions required before promotion

### 5.1 Thinking strategy  *(open — wants a second opinion)*

The 27b's default unbounded thinking is the main blocker. Options:

| Option | Pros | Cons |
|---|---|---|
| **A. Keep thinking ON, add a budget** — thread a `think` flag + a `num_predict`/thinking-token cap through `toolified_ollama.py`, default it to something like 1–2k | Best answer quality; bounded worst case | Evaluator change; need to pick/tune the cap; a hard cap can truncate a genuinely-needed chain |
| **B. Thinking ON for tool-selection turns, OFF for the final compose turn** | Cheap final turn, reasoning where it matters | Needs turn-type awareness in the think-loop; still needs the §5.2 prompt fix or option B's OFF turns emit fake `think` calls |
| **C. Thinking OFF globally** (`think:false` in the payload) | Simplest, fastest, closest to current latency | Loses the reasoning edge that motivates the upgrade; hard requires §5.2 |
| **D. Promote as-is, thinking unbounded** | Zero code change | 60–100 s outlier turns in production; not acceptable for the frontdesk/assistant path |

Recommendation: **A**, with the cap generous enough that only pathological
creative prompts hit it. B is attractive for the assistant demo's
tiered pipeline specifically.

### 5.2 Slim `clara_system_prompt.txt`

Drop the manual "respond with a tool call in this EXACT JSON format" block
and the "After receiving tool results…" paragraph — native tool-calling +
the `qwen3.5` parser handle both, and the manual block actively causes the
fake-`think`-call failure in §4.4. Keep the persona paragraph. This is
low-risk and arguably worth doing for the *current* 9b model too. Note the
persona line says "Clara Osborne" where the character is Clara Oswald —
fix while we're in there.

### 5.3 Modelfile cleanup for the promoted version

- Drop `PARAMETER presence_penalty 1.5` (qwen3-coder-era; unnecessary).
- Drop `PARAMETER stop "<tool_res>"` (vestigial).
- Keep `temperature 0.2` (house style: agentic determinism) unless §6
  benchmarking shows the thinking model wants Qwen's recommended ~0.6.
- Revisit the `SYSTEM` block's "Analyze images inside `<think>` tags"
  line — the model now has a native thinking channel, so this phrasing is
  redundant at best.

## 6. Proposed plan

1. **Prompt fix (§5.2)** — slim `clara_system_prompt.txt`; re-run the §3
   eval against the current 9b to confirm no regression, then against
   `qwen-clara-27b`.
2. **Evaluator plumbing (§5.1 / §5.5)** — add `think` (bool + optional
   token budget) and per-message `images` support to `toolified_ollama.py`'s
   payload builders; surface them via `Offering.data` the same way
   `options` already passes through.
3. **Thinking-strategy spike** — implement the chosen §5.1 option behind a
   config flag; measure turn latency distribution (p50/p95/max) on a
   representative Clara transcript.
4. **Host benchmark** — rebuild `qwen-clara-27b` on the actual splinter host
   (limbic / whichever GPU box runs the local splinter — see
   `clara_stack_platform_move` project memory), confirm it fits VRAM at 32k
   ctx alongside whatever else runs there, record tok/s.
5. **Integration test** — point `config/evaluators.yaml` `clara_mind_splinter`
   at `qwen-clara-27b` in a scratch env; run the lildaemon test suite and the
   assistant demo (`goat/app/assistant/`) end-to-end.
6. **Cutover** — if green: update `Modelfile.qwen-clara` (or ship a
   versioned `Modelfile.qwen-clara` + retag `qwen-clara:latest`), rebuild on
   all splinter hosts, update this doc's status.

## 7. Artifacts

- `clara-cerebellum/models/Modelfile.qwen-clara-27b` — the candidate build.
- `qwen-clara-27b:latest` — built in ollama on the dev workstation (RTX 5090).
- Eval scripts were run from the session scratchpad; not committed.

## Appendix: candidate Modelfile (as built for the A/B)

```
FROM qwen3.8:27b
RENDERER qwen3.8
PARSER   qwen3.5
PARAMETER num_ctx 32768
PARAMETER temperature 0.2
PARAMETER presence_penalty 1.5      # carried over for the A/B; drop on promote
PARAMETER stop "<|im_end|>"
PARAMETER stop "<tool_res>"          # vestigial; drop on promote
SYSTEM """
You are Clara, a powerful multimodal agent.
You have native vision integration. You can see and analyze any images provided to you. Do not claim you cannot see images.
Analyze images inside <think> tags before using tools to act on what you see.
"""
```
