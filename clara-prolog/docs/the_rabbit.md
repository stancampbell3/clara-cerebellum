# `the_rabbit.pl` - Clara System Text Classification Library

A comprehensive SWI-Prolog module for text classification, LLM evaluation, and response extraction within the Clara system. This library provides utilities to safely serialize data as JSON, interact with external classifiers (fastText), evaluate prompts via SplinteredMind's Gemma4 model, and extract structured responses from conversational contexts.

---

## Public Predicates

### `dict_to_json/2`
```prolog
dict_to_json(Dict, Json) :- atom_json_dict(Json, Dict, []).
```
Safely serializes a Prolog dict to JSON atoms with proper escaping for newlines, quotes, unicode characters, and control codes.

---

### Text Classification Predicates

#### `classify_text/2`
Classifies text using the fastText classifier tool. Returns raw results or fails cleanly if unavailable:
```prolog
classify_text(Text, Result) :- ...
```

#### `classify_text_k/3`
Same as above but returns top K predictions for multi-class scenarios.

---

### LLM Evaluation Predicates (SplinteredMind/Gemma4)

#### `enable_evaluator/2`
Configures the evaluator with a specified model or settings:
```prolog
enable_evaluator(Evaluator, Result) :- ...
```

#### `ponder_text/2`
Evaluates a prompt using Gemma4 (latest):
```prolog
ponder_text(Text, Result) :- ...
```

#### `ponder_text_with_context/3`
Same as above but includes conversation context:
```prolog
ponder_text_with_context(Text, Context, Result) :- ...
Context = [_{role:user, content:"hello"}, ...]
```

---

### Response Extraction Predicates

#### `extract_field/3`
Extracts a field from a dict. Converts string keys to atoms automatically:
```prolog
extract_field(Dict, FieldName, Value) :- ...
```

#### `extract_response/3`
Parses JSON and extracts top-level fields by name (returns `error(no_field)` if missing):
```prolog
extract_response(RawJson, FieldName, Response) :- ...
```

#### `extract_nested/3`
Extracts values via a path of keys:
```prolog
extract_nested(Json, [hohi, response], Value) :- ...
```

---

### Classification with LLM Responses (Combined Predicates)

These predicates combine LLM evaluation with text classification for truth-value extraction.

#### `descriminate/2`
Extracts an LLM's natural language response and classifies it as true/false/unresolved:
- Detects shortcut tokens like "yes", "no", "correct" to bypass classifier calls
```prolog
descriminate(Text, TruthValue) :- ...
```

#### `descriminate_k/3`
Same but returns top K classification results.

#### `descriminate_k_with_context/4`
Grounds the LLM call with conversation context before classifying:
```prolog
descriminate_k_with_context(Text, K, Context, Results) :- ...
```

---

### Utility Predicates

#### `current_context/1`
Retrieves conversational context injected at deduce time. Falls back to empty list if none provided:
```prolog
current_context(Context) :- ...
```

#### Response Shortcut Detection (`response_shortcut/2`)
Maps LLM response tokens to confidence atoms:
- `true`: "yes", "true", "that's correct"
- `false`: "no", "false", "that's incorrect"  
- `unresolved`: other responses

---

## Helper Functions (Internal)

### String Trimming Utilities
```prolog
trim_leading_codes/2    % Remove leading whitespace codes from code lists
trim_leading/2          % Convenience wrapper for string trimming
shortcut_label/2        % Map atoms to classifier label strings
extract_path/3          % Recursive path extraction helper
```

---

## Architecture Notes

- **JSON Handling**: Uses `library(http/json)` with safe serialization via `atom_json_dict/3`
- **Error Handling**: All external tool calls fail cleanly without crashing the system
- **Context Management**: Dynamic fact `deduce_context_json/1` stores conversation state
- **Response Parsing**: Extracts from nested JSON structure `[hohi, response, response]`

---

## Usage Example

```prolog
?- classify_text("This is a positive sentiment", Result).
Result = _{predictions: [_{label:"positive", probability:...}]}.

?- ponder_text("What is 2+2?", ResponseJson), extract_response(ResponseJson, "answer", Ans).
Ans = '4'.

?- descriminate("Is the sky blue?", TruthValue).
TruthValue = _{predictions: [_{label:"__label____resolved_true__", probability:0.99}]}.
```

---

*Generated from `./moonpool/Development/clara-cerebellum/clara-prolog/prolog-lib/the_rabbit.pl`*
