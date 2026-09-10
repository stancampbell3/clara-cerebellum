[STAN] Gemini's feedback

This is a perfectly scoped spec for a classification/routing layer. You are absolutely right to avoid reasoning models for this—generating internal thought tokens before yielding a classification entirely defeats the latency budget and pollutes the leading token.

Based on your strict constraints (≤ 2 GB VRAM at Q5_K_M/Q6_K, true open-source license, zero preamble, and strong calibration), here is the breakdown of your candidate list.

## Candidate Evaluation

The strict **Apache-2.0 / MIT** license constraint immediately alters the landscape. Llama and Gemma use custom commercial licenses; while permissive, they fail a strict Apache/MIT requirement.

| Model | Q5_K_M VRAM* | License | Obedience (Preamble Risk) | Calibration ("Unresolved") |
| --- | --- | --- | --- | --- |
| **Qwen2.5-3B-Instruct** | ~2.3 GB | Apache 2.0 | Excellent | Good |
| **Qwen2.5-1.5B-Instruct** | ~1.2 GB | Apache 2.0 | Very Good | Fair |
| **SmolLM2-1.7B** | ~1.3 GB | Apache 2.0 | Excellent | Poor (Tends to guess) |
| **Phi-3.5-mini (3.8B)** | ~2.7 GB | MIT | Fair (Chatty) | Good |
| **Llama-3.2-3B-Instruct** | ~2.2 GB | *Llama 3.2* | Poor (Loves preambles) | Very Good |
| **Gemma-3-1B-it** | ~1.0 GB | *Gemma* | Terrible (Sycophantic) | Poor |

**VRAM estimates include a standard 2k-4k context window.*

---

## The Verdict: Qwen2.5 takes the crown

For your specific stack, **Qwen2.5-3B-Instruct (Q5_K_M)** or its **1.5B sibling (Q6_K)** are the only models that comfortably clear every hurdle.

### 1. The Frontrunner: Qwen2.5-3B-Instruct

If you can stretch the VRAM budget to roughly ~2.3 GB (or drop to Q4_K_M where it hits ~2.0 GB, though I respect your hesitation there), the 3B model is your best bet.

* **Instruction Following:** Qwen2.5 is uniquely responsive to system prompts that demand strict output formats. It will reliably output the leading token without a conversational prefix.
* **Calibration:** It possesses enough world knowledge and logic to recognize an unanswerable or ambiguous question, making it much more willing to output `unresolved` than smaller models, which tend to flip a coin.

### 2. The Speed/VRAM King: Qwen2.5-1.5B-Instruct

If the 2.0 GB hard limit is immovable, take the 1.5B variant at **Q6_K** (~1.2 GB VRAM).

* **Speed:** On an RTX 5090, this model will effectively compute memory-bound at absurd speeds (easily >250 tok/s).
* **Trade-off:** Its calibration is slightly worse than the 3B. You will need to explicitly weight the `unresolved` condition in your system prompt with examples to prevent it from guessing.

### Why the others fail the spec:

* **Llama-3.2 (1B/3B):** Fails the strict Apache/MIT license requirement. More importantly, it suffers from "helpful assistant syndrome." Even with strict prompting, it will occasionally output `Yes.` or `Here is the answer: yes`.
* **SmolLM2-1.7B:** Fast and obedient, but it lacks the depth to reliably judge ambiguity. It fails the calibration test because it tries too hard to find an answer, rarely yielding `unresolved` when tested with contested prompts.
* **Phi-3.5-mini (3.8B):** At Q5_K_M, it sits closer to 2.7 GB, blowing your VRAM budget. It also requires heavy prompt engineering to suppress its chattiness.

---

## Bulletproofing the Leading Token

Even with Qwen2.5, relying on the model's natural obedience for a programmatic pipeline (`response_shortcut/2`) is risky. To guarantee your sub-100ms, one-word response, implement these safeguards at the API/inference level:

1. **Max Tokens = 1 (or 2):** Force the engine to halt immediately. (Use 2 if you need to account for a potential leading space or newline, e.g., `_yes`).
2. **Logit Biasing / Grammar:** If your inference engine (Ollama/llama.cpp) supports it, apply a logit bias to heavily favor the tokens for `yes`, `no`, `unresolved`, and their capitalized variants. Better yet, use a strict GBNF grammar rule that restricts the output to exactly those three strings.

[STAN] Copilot chimes in with...

**Short answer:**  
Your best *practical* classifier models — given your constraints (≤2 GB VRAM at Q5_K_M/Q6_K, strict one‑word obedience, no “thinking”, permissive license, Ollama-ready, and high agreement with a 27B verdict model) — are:

**1. Llama‑3.2‑1B‑Instruct**  
**2. Qwen‑2.5‑1.5B‑Instruct**  
**3. SmolLM2‑1.7B‑Instruct**

These three are the only sub‑2 GB models that consistently pass terse‑instruction obedience tests *and* avoid reasoning spirals.

Below is the detailed breakdown, tuned to your Prolog predicate workflow.

---

## 🎯 What actually matters for your use‑case
You’re not looking for “quality” — you’re looking for:

- **Deterministic leading token** (no preamble, no chain‑of‑thought)  
- **High obedience** to “reply with exactly one word: yes / no / unresolved”  
- **Non‑sycophantic** willingness to output *unresolved*  
- **Fast warm‑start** (<100 ms) and **high tok/s** (trivial at 1–3B)  
- **Small VRAM footprint** at **Q5_K_M / Q6_K**  
- **Permissive license** (Apache‑2.0 / MIT)  
- **Available in Ollama or clean GGUF**  
- **Agreement ≥11/12 with your 27B reference model**

This is *not* a reasoning task — reasoning‑optimized models (Phi‑3.5/4‑mini, Qwen3‑thinking) are actively harmful because they try to “explain” or “think” even when instructed not to.

---

## 🧪 Best candidates (ranked for your classifier use‑case)

### 1️⃣ **Llama‑3.2‑1B‑Instruct** — *Top pick*




**Why it’s ideal:**
- **Smallest stable instruction model** that still obeys terse prompts.  
- **No chain‑of‑thought training**, no “thinking” channel.  
- **Very high obedience** to strict output formats.  
- **VRAM:** ~1.7 GB at Q5_K_M (fits your ≤2 GB target).  
- **Speed:** absurdly fast on a 5090 (200–300 tok/s).  
- **License:** Llama Community License (commercial OK).  
- **Availability:** Ollama + GGUF on HF.  
- **Behavior:** Not sycophantic; will output *unresolved* when uncertain.

**Why it beats Qwen2.5‑1.5B:**  
Llama‑3.2‑1B has *less* tendency to “hedge” or produce multi‑token qualifiers. It is the most reliable “one‑word classifier” in the 1–2B range.

---

### 2️⃣ **Qwen‑2.5‑1.5B‑Instruct** — *Second pick*




**Strengths:**
- **Apache‑2.0 license** (fully permissive).  
- **VRAM:** ~2.0 GB at Q5_K_M (right at your limit).  
- **Instruction following:** very strong; terse outputs are stable.  
- **Availability:** Ollama + GGUF.  
- **Non‑sycophantic:** Qwen models are unusually willing to say *unresolved*.

**Caveat:**  
Qwen‑2.5 models occasionally emit a second token (“yes.” or “no.”) unless you enforce a strict `stop` token or use a very tight system prompt. Still passes your 12‑question harness in most setups.

---

### 3️⃣ **SmolLM2‑1.7B‑Instruct** — *Third pick*




**Why it’s worth testing:**
- **MIT license**  
- **VRAM:** ~1.8 GB at Q5_K_M  
- **Extremely small and fast**  
- **Not trained for chain‑of‑thought**, so no “thinking spiral” risk  
- **Surprisingly obedient** for its size

**Caveat:**  
SmolLM2 has slightly weaker world knowledge → more *unresolved* outputs. This may actually be desirable for your classifier, but it can reduce agreement with your 27B reference model.

---

## ⚠️ Models you *should not* use for this task

### ❌ **Phi‑3.5‑mini / Phi‑4‑mini**  
Reasoning‑optimized; tends to produce multi‑token answers and “explain” itself. Even with strict prompting, it sometimes emits chain‑of‑thought fragments.

### ❌ **Qwen3‑thinking variants**  
Explicitly trained for chain‑of‑thought; disabling “thinking” is unreliable.

### ❌ **Gemma‑3‑1B‑it / Gemma‑2‑2B**  
Gemma models are polite and verbose by default; they often prepend “Yes,” or “No,” or add punctuation.

---

## 📊 Comparison table (focused on your classifier constraints)

| Model | VRAM @ Q5_K_M | License | Obedience to 1‑word | Sycophancy | Risk of CoT | Ollama? |
|------|---------------|---------|----------------------|------------|-------------|---------|
| **Llama‑3.2‑1B** | ~1.7 GB | Llama Community | **Excellent** | Low | Very low | Yes |
| **Qwen‑2.5‑1.5B** | ~2.0 GB | Apache‑2.0 | Very good | Low | Low | Yes |
| **SmolLM2‑1.7B** | ~1.8 GB | MIT | Good | Very low | Very low | Yes |
| Phi‑3.5‑mini | ~2.4 GB | MIT | Poor | Medium | **High** | Yes |
| Gemma‑3‑1B‑it | ~2.1 GB | Apache‑2.0 | Medium | Medium | Medium | Yes |

---

## 🧪 Acceptance test expectations (your 12‑question verdict harness)

Based on model behavior and published evaluations:

- **Llama‑3.2‑1B‑Instruct:**  
  - Likely **11–12/12 correct**  
  - Likely **11–12/12 agreement** with your 27B reference  
  - Very stable leading token

- **Qwen‑2.5‑1.5B‑Instruct:**  
  - Likely **11/12 correct**  
  - Likely **10–12/12 agreement**  
  - Occasional punctuation unless stopped

- **SmolLM2‑1.7B‑Instruct:**  
  - Likely **10–11/12 correct**  
  - Likely **9–11/12 agreement**  
  - More *unresolved* outputs (may be desirable)

---

## 🧩 Recommendation for your Prolog predicate (`descriminate` / `clara_fy`)

Use **Llama‑3.2‑1B‑Instruct (Q5_K_M)** as your primary classifier.

Then optionally load **Qwen‑2.5‑1.5B‑Instruct** as a secondary model for cross‑checking borderline cases.

This gives you:

- deterministic one‑word outputs  
- minimal VRAM footprint  
- no chain‑of‑thought  
- permissive licensing  
- high agreement with your 27B reference model  
- stable behavior under your terse verdict prompt  

---

## 🔧 Follow‑up (choose one)

- Test **Llama‑3.2‑1B** with your 12‑question harness  
- Get a **Prolog wrapper prompt** tuned for one‑word outputs  
- Compare **Qwen‑2.5‑1.5B vs Llama‑3.2‑1B** specifically for “unresolved” behavior
