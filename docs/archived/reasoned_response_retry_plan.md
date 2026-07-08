# reasoned_response retry strategy — planning doc

## Problem

`reasoned_response/2` generates an LLM response and validates it via `clara_fy`.
When validation returns `unresolved` or `false`, the predicate fails with no
recovery path. For factual or knowledge-heavy prompts this happens when the base
evaluator (gemma4 via Ollama) lacks sufficient context.

---

## Approach A — Web search in the Prolog/toolbox layer (deferred)

Add a `web_search/2` predicate to `the_rabbit` backed by a new tool in
`clara-toolbox` (SearXNG or Brave API). On validation failure:

1. Call `web_search(Prompt, SearchContext)` to retrieve relevant snippets
2. Format results as a context message list
3. Retry via `reasoned_response_with_context/3` with that context

```prolog
reasoned_response(Prompt, RR) :-
    ponder_text(Prompt, LLMRaw),
    extract_nested(LLMRaw, [hohi, response, response], Candidate),
    format(atom(ValidationQ), 'Does "~w" adequately answer the prompt: ~w?', [Candidate, Prompt]),
    ( clara_fy(ValidationQ, true) ->
        RR = Candidate
    ;
        web_search(Prompt, SearchContext),
        reasoned_response_with_context(Prompt, SearchContext, RR)
    ).
```

**Why deferred:** Requires a new Rust tool, a search backend (self-hosted or
API-keyed), and result formatting. More moving parts than necessary for the
initial reasoning strategy.

---

## Approach B — Offering to a tools-enabled evaluator (preferred for now)

Rather than handling search in the Prolog layer, delegate the retry to a
different evaluator that already has tools enabled (e.g. web search, RAG).
The Prolog side passes an **offering** — the original prompt plus an instruction
to use available tools — and receives an enriched response back.

Conceptually mirrors the ritual architecture where an offering is routed to a
capable evaluator rather than the base one.

```prolog
reasoned_response(Prompt, RR) :-
    ponder_text(Prompt, LLMRaw),
    extract_nested(LLMRaw, [hohi, response, response], Candidate),
    format(atom(ValidationQ), 'Does "~w" adequately answer the prompt: ~w?', [Candidate, Prompt]),
    ( clara_fy(ValidationQ, true) ->
        RR = Candidate
    ;
        make_offering(Prompt, Offering),
        ponder_offering(Offering, RR)     % routes to tools-enabled evaluator
    ).
```

This keeps the Prolog layer clean and puts search/tool capability in the
evaluator rather than the Prolog toolbox.

---

## Open questions before implementing Approach B

1. **Which evaluator supports tool calling in lildaemon?**
   gemma4 is the current base evaluator — unclear if it handles the tool calling
   mechanism correctly. Need to verify before routing offerings to it with tool
   instructions. Candidate alternatives: a larger Ollama model, a remote model
   via lildaemon, or a dedicated tool-calling endpoint.

2. **What does `enable_evaluator` need to look like for the tools-enabled path?**
   Currently `the_rat` initialises with `enable_evaluator('ollama', _)`. The
   offering path may need a second evaluator handle or a per-call override.

3. **Offering format:** What shape does an offering take? Is it a plain prompt
   atom, a context list with a system instruction, or a structured term that
   lildaemon knows how to route?

4. **Does the validation step also need to use the enriched evaluator?**
   After `ponder_offering` returns `RR`, we still call `clara_fy` on it. The
   classifier should be model-agnostic so this is probably fine, but worth
   confirming.

5. **Fallback behaviour:** If the tools-enabled evaluator also fails validation,
   should `reasoned_response` fail cleanly, or is there a third strategy
   (e.g. returning the best candidate with a `confidence` annotation)?

---

## Next steps

- Verify tool calling support for candidate Ollama models in lildaemon
- Settle the offering format and evaluator selection mechanism
- Implement `make_offering/2` and `ponder_offering/2` in `the_rabbit`
- Wire the retry into `reasoned_response/2`
- Add a test using a mock `ponder_offering` that verifies the fallback path
