# The Rat - Clara System Prolog Library Summary

## Overview

`the_rat.pl` is a truth-value classification library for the Clara system. It provides predicates to query an LLM (Ember Devil model by default) and classify responses into **true/false/unresolved** categories based on top predictions from fastText-style labels.

---

## Core Functionality

### Truth Value Classification (`clara_fy/2`)
- Takes a text query and returns: `true`, `false`, or `unresolved`
- If the model is unsure (returns "unresolved"), it prompts for clarification with direct yes/no question before retrying

### Context-Aware Classification (`clara_fy/3`)
```prolog
clara_fy(Text, Context, TruthValue)
```
- Accepts a conversation context list of message dicts: `[{role:user, content:"hello"}, ...]`
- Useful for maintaining state across multi-turn conversations

---

## Available Predicates

| Predicate | Description |
|-----------|-------------|
| `extract_top_k_labels/3` | Extract top K simplified labels from text query |
| `top_status/2` | Get single best status prediction (K=1) |
| `clara_fy/2` | Classify a text into truth value using top prediction |
| `reasoned_response/2` | Generate response and validate it adequately answers the prompt |

---

## Label Normalization

The library maps fastText labels to Clara atoms:

```prolog
"__label____resolved_true___"  → true
"__label____resolved_false___" → false  
"__label____unresolved___"     → unresolved
Other                          → unknown (with warning)
```

---

## Reasoned Response Pattern (`reasoned_response/2`)

This predicate:
1. Generates an LLM response to a prompt using `ponder_text`
2. Extracts the nested JSON structure for the actual response text
3. Validates that the generated response adequately answers the original prompt via `clara_fy(ValidationQ, true)`
4. Fails if validation doesn't pass (leaving door open for retry/refinement strategies)

---

## Dependencies

- `library(the_rabbit)` - Underlying LLM interaction details
- `library(http/json)` - JSON handling and parsing

---

## Example Usage Pattern

```prolog
% Simple classification:
clara_fy('Is the visitor confused?', Truth).

% With conversation context:
current_context(Ctx), 
clara_fy('the guest seems agitated', Ctx, Status).

% Generate and validate response:
reasoned_response('Explain quantum entanglement simply', Response).
```

---

## Architecture Notes

- Uses `OllamaEvaluator` as the base evaluator (enabled via `enable_evaluator`)
- Employs `predsort/3` with custom comparison for sorting predictions by probability descending
- Implements backtracking-friendly logic using cut operators (`!`) in normalization rules
- Provides both context-aware and stateless variants of all major predicates

---

*Generated from the_rat.pl - Clara System Truth Classification Module*
