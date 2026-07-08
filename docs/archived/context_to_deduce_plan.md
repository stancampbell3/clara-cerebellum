# Plan: Conversational Context in `/deduce`

## Overview

The context is a list of `{role, content}` messages (external conversation history). It needs to
flow from the HTTP request into both Prolog and CLIPS engines so rules can reason over it, and into
`clara_evaluate` JSON payloads so the LLM can use it as grounding.

---

## Layer 1 — Data model (`clara-api/src/models/request.rs`)

Add a `ConversationMessage` struct and a `context` field to `DeduceRequest`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMessage {
    pub role: String,
    pub content: String,
}

// In DeduceRequest:
#[serde(default)]
pub context: Vec<ConversationMessage>,
```

`DeduceResumeRequest` gets the same field so resumed sessions can replay with the original context.
`DeductionSnapshot` also needs to persist it.

---

## Layer 2 — Session seeding (`clara-cycle/src/session.rs`)

Add `seed_context(&mut self, messages: &[ConversationMessage])`:

```rust
pub fn seed_context(&mut self, messages: &[ConversationMessage]) -> Result<(), CycleError> {
    // Serialize to JSON string, assert as Prolog fact
    let json = serde_json::to_string(messages)?;
    let escaped = json.replace('\'', "\\'");
    self.prolog.assertz(&format!("deduce_context_json('{escaped}')"))?;

    // Assert into CLIPS as a string fact
    self.clips.build(&format!(
        "(deffacts deduce-context (deduce-context (json \"{}\")))",
        json.replace('"', "\\\"")
    ))?;
    Ok(())
}
```

The Prolog fact stores the raw JSON atom; CLIPS gets a template fact. Both are injected before
`CycleController::run()`.

---

## Layer 3 — Handler wiring (`clara-api/src/handlers/deduce_handler.rs`)

In `start_deduce`, after `session.seed_prolog(...)` and `session.seed_clips(...)`:

```rust
session.seed_context(&context_bg)?;
```

Context is cloned from `req.context` just like the other fields. In the snapshot save path,
context is included in `DeductionSnapshot` so resume works.

---

## Layer 4 — Prolog library extensions (`the_rabbit.pl`)

Add context-aware variants of `ponder_text` and `descriminate_k`:

```prolog
%% ponder_text_with_context/3 — LLM evaluate with conversation context
ponder_text_with_context(Text, Context, Result) :-
    dict_to_json(_{tool: splinteredmind,
                   arguments: _{operation: evaluate,
                                data: _{prompt: Text,
                                        context: Context,
                                        model: 'qwen2.5:7b'}}}, Json),
    clara_evaluate(Json, Result).

%% descriminate_k_with_context/4
descriminate_k_with_context(Text, K, Context, Results) :-
    ponder_text_with_context(Text, Context, LLMSez),
    extract_nested(LLMSez, [hohi, response, response], Response),
    !,
    ( response_shortcut(Response, Shortcut) ->
        shortcut_label(Shortcut, LabelStr),
        atom_json_dict(Results, _{predictions: [_{label: LabelStr, probability: 0.99}]}, [])
    ;
        atom_string(Text, TextStr),
        atom_string(Response, RespStr),
        string_concat(TextStr, " ", Tmp),
        string_concat(Tmp, RespStr, Pair),
        classify_text_k(Pair, K, Results)
    ).
```

Also add a `current_context/1` helper that retrieves the asserted context and parses it back to a
list of dicts:

```prolog
%% current_context/1 — retrieve the injected conversation context
current_context(Context) :-
    deduce_context_json(Json),
    atom_json_dict(Json, Context, []).
current_context([]).  % fallback when no context was injected
```

Location TBD (see open questions below).

---

## Layer 5 — `the_rat.pl` extension

Add `clara_fy/3` and its supporting predicates:

```prolog
clara_fy(Text, Context, TruthValue) :-
    top_status_with_context(Text, Context, B1),
    TruthValue = B1.

top_status_with_context(Text, Context, Status) :-
    extract_top_k_labels_with_context(Text, 1, Context, [Status]).

extract_top_k_labels_with_context(Text, K, Context, SimpleLabels) :-
    descriminate_k_with_context(Text, K, Context, RawJson),
    atom_json_dict(RawJson, Dict, []),
    predsort(compare_probs, Dict.predictions, Sorted),
    length(TopK, K),
    append(TopK, _, Sorted),
    maplist(get_simple_label, TopK, SimpleLabels).
```

User-land rule example:

```prolog
redirect_the_visitor(Visitor, help_kiosk) :-
    visitor(Visitor, _),
    current_context(Ctx),
    clara_fy('the visitor seems confused or lost', Ctx, true).
```

---

## Dependency note — FieryPit `/evaluate`

The `context` field passes through:

```
clara_evaluate/2  →  evaluate_json_string  →  ToolboxManager  →  splinteredmind tool  →  FieryPit /evaluate
```

FieryPit's LLM handler will need to accept and use the `context` array (pass it as the message
history to the LLM). This is outside the clara-cerebrum workspace and should be confirmed before
implementing the Prolog layer.

---

## Implementation order

1. `ConversationMessage` struct + `DeduceRequest.context` field
2. `DeductionSnapshot` persistence field
3. `DeductionSession::seed_context`
4. Handler wiring in `start_deduce` / `resume_deduce`
5. `the_rabbit.pl` — `ponder_text_with_context`, `descriminate_k_with_context`
6. `the_rat.pl` — `clara_fy/3`, `top_status_with_context`, `extract_top_k_labels_with_context`
7. `current_context/1` helper

---

## Open questions

1. **Context format**: Should `context` be `Vec<{role, content}>`, or a flexible
   `serde_json::Value` array to avoid coupling to a fixed schema?

[STAN] Let's use serde_json::Value as we'll need to support various LLM calling interfaces and it may change over time or target.

2. **CLIPS context**: Is CLIPS context injection needed now, or can it be a follow-up?

[STAN] To simplify development and testing, let's do a two step implementation of the context.  We'll treat Prolog first, then follow up with CLIPS.  The Prolog rules contain most of the "business logic" while CLIPS is used mostly for forward chaining and side effects.

3. **FieryPit `context` field**: Is the LLM backend already capable of accepting a `context` array
   in `/evaluate`, or does that need coordinating first?

[STAN] We've extended the Ollama evaluator (upon which the EmberEvaluator and KindlingEvaluator are based) to support context in its evaluate payload, so this is already present.

4. **`current_context/1` location**: Should it live in `the_rat.pl` (alongside `clara_fy`) or in
   a shared utility module like `the_rabbit.pl`?

[STAN] the context is closer to the clara_evaluate than the_rat's clara_fy, so it should live in the_rabbit. By the way, we're using Irish mythology as a theme in Clara, so that's where the odd names come from :0).
