# ClaraSplinteredMindTool - Prolog Usage Guide

## Overview

The `ClaraSplinteredMindTool` (tool name: `splinteredmind`) provides a bridge between Prolog predicates and the FieryPit REST API. This enables Prolog code to:

- Invoke LLM evaluations via FieryPit evaluators
- Manage and interact with remote CLIPS sessions
- Manage and interact with remote Prolog sessions
- Query FieryPit status and switch evaluators

This is particularly useful for building multi-agent reasoning systems where Prolog orchestrates interactions between different evaluation backends.

---

## Basic Invocation

From Prolog, use the `clara_evaluate/2` predicate to call the tool:

```prolog
?- clara_evaluate('{"tool":"splinteredmind","arguments":{"operation":"status"}}', Result).
```

The general JSON format is:

```json
{
  "tool": "splinteredmind",
  "arguments": {
    "operation": "<operation_name>",
    ... additional fields based on operation ...
  }
}
```

---

## Operations Reference

### General Operations

| Operation | Description | Required Fields |
|-----------|-------------|-----------------|
| `health` | Health check | - |
| `status` | Get FieryPit status | - |
| `info` | Get API metadata | - |
| `evaluate` | Evaluate via current evaluator | `data` |
| `list_evaluators` | List available evaluators | - |
| `get_evaluator` | Get evaluator details | `evaluator` |
| `set_evaluator` | Switch active evaluator | `evaluator` |
| `reset_evaluator` | Reset to default evaluator | - |

### CLIPS Session Operations

| Operation | Description | Required Fields |
|-----------|-------------|-----------------|
| `clips_create_session` | Create CLIPS session | `user_id` (optional) |
| `clips_list_sessions` | List all CLIPS sessions | - |
| `clips_get_session` | Get session details | `session_id` |
| `clips_terminate_session` | Terminate session | `session_id` |
| `clips_evaluate` | Execute CLIPS code | `session_id`, `script` |
| `clips_load_rules` | Load rules | `session_id`, `rules` |
| `clips_load_facts` | Assert facts | `session_id`, `facts` |
| `clips_query_facts` | Query facts | `session_id`, `pattern` (optional) |
| `clips_run` | Run rule engine | `session_id`, `max_iterations` (optional) |

### Prolog Session Operations

| Operation | Description | Required Fields |
|-----------|-------------|-----------------|
| `prolog_create_session` | Create Prolog session | `user_id` (optional) |
| `prolog_list_sessions` | List all Prolog sessions | - |
| `prolog_get_session` | Get session details | `session_id` |
| `prolog_terminate_session` | Terminate session | `session_id` |
| `prolog_query` | Execute Prolog goal | `session_id`, `goal` |
| `prolog_consult` | Load clauses | `session_id`, `clauses` |

---

## Scenario 1: LLM Evaluation

The most common use case is invoking an LLM through FieryPit's evaluation system.

**Important**: When using the Ollama evaluator, you must provide both `model` and `prompt` fields in the `data` object. Common models include `llama3.2`, `mistral`, `phi3`, etc.

### Check Available Evaluators

```prolog
% List what evaluators are available
list_evaluators(Evaluators) :-
    clara_evaluate(
        '{"tool":"splinteredmind","arguments":{"operation":"list_evaluators"}}',
        Evaluators
    ).
```

### Switch to an LLM Evaluator

```prolog
% Switch to the ollama evaluator for LLM queries
use_ollama :-
    clara_evaluate(
        '{"tool":"splinteredmind","arguments":{"operation":"set_evaluator","evaluator":"ollama"}}',
        _Result
    ).

% Switch to echo evaluator for testing
use_echo :-
    clara_evaluate(
        '{"tool":"splinteredmind","arguments":{"operation":"set_evaluator","evaluator":"echo"}}',
        _Result
    ).
```

### Ask an LLM a Question

```prolog
% LLM query with model specification (required for Ollama)
% Available models depend on what you have pulled in Ollama (e.g., llama3.2, mistral, phi3)
ask_llm(Model, Prompt, Response) :-
    format(atom(Json),
        '{"tool":"splinteredmind","arguments":{"operation":"evaluate","data":{"model":"~w","prompt":"~w"}}}',
        [Model, Prompt]),
    clara_evaluate(Json, Response).

% Convenience wrapper with default model
ask_llm(Prompt, Response) :-
    ask_llm('llama3.2', Prompt, Response).

% Example usage:
% ?- ask_llm('What is the capital of France?', R).
% ?- ask_llm('mistral', 'Explain quantum computing briefly.', R).
```

### Structured LLM Query with System Prompt

```prolog
% LLM query with system context and model specification
ask_llm_with_context(Model, SystemPrompt, UserPrompt, Response) :-
    format(atom(Json),
        '{"tool":"splinteredmind","arguments":{"operation":"evaluate","data":{"model":"~w","system":"~w","prompt":"~w"}}}',
        [Model, SystemPrompt, UserPrompt]),
    clara_evaluate(Json, Response).

% Convenience wrapper with default model
ask_llm_with_context(SystemPrompt, UserPrompt, Response) :-
    ask_llm_with_context('llama3.2', SystemPrompt, UserPrompt, Response).

% Example: Ask as a helpful assistant
% ?- ask_llm_with_context('You are a helpful coding assistant.',
%                         'How do I reverse a list in Prolog?', R).
```

### Chain of Thought Reasoning

```prolog
% Use LLM for step-by-step reasoning
reason_about(Model, Problem, Analysis) :-
    format(atom(Json),
        '{"tool":"splinteredmind","arguments":{"operation":"evaluate","data":{"model":"~w","prompt":"Think step by step about: ~w","temperature":0.3}}}',
        [Model, Problem]),
    clara_evaluate(Json, Analysis).

% With default model
reason_about(Problem, Analysis) :-
    reason_about('llama3.2', Problem, Analysis).
```

---

## Scenario 2: CLIPS Interaction

Use FieryPit to manage CLIPS sessions for expert system reasoning.

### Create and Use a CLIPS Session

```prolog
% Create a new CLIPS session
create_clips_session(SessionId) :-
    clara_evaluate(
        '{"tool":"splinteredmind","arguments":{"operation":"clips_create_session","user_id":"prolog_agent"}}',
        Result
    ),
    % Extract session_id from result
    atom_json_dict(Result, Dict, []),
    SessionId = Dict.session_id.

% Load facts into a CLIPS session
clips_assert_facts(SessionId, Facts) :-
    % Facts should be a list like ["(person (name John))", "(person (name Jane))"]
    format(atom(FactsJson), '~w', [Facts]),
    format(atom(Json),
        '{"tool":"splinteredmind","arguments":{"operation":"clips_load_facts","session_id":"~w","facts":~w}}',
        [SessionId, FactsJson]),
    clara_evaluate(Json, _Result).

% Load rules into a CLIPS session
clips_load_rules(SessionId, Rules) :-
    format(atom(RulesJson), '~w', [Rules]),
    format(atom(Json),
        '{"tool":"splinteredmind","arguments":{"operation":"clips_load_rules","session_id":"~w","rules":~w}}',
        [SessionId, RulesJson]),
    clara_evaluate(Json, _Result).

% Run the CLIPS rule engine
clips_run(SessionId, RulesFired) :-
    format(atom(Json),
        '{"tool":"splinteredmind","arguments":{"operation":"clips_run","session_id":"~w"}}',
        [SessionId]),
    clara_evaluate(Json, Result),
    atom_json_dict(Result, Dict, []),
    RulesFired = Dict.rules_fired.

% Query facts from CLIPS session
clips_get_facts(SessionId, Facts) :-
    format(atom(Json),
        '{"tool":"splinteredmind","arguments":{"operation":"clips_query_facts","session_id":"~w"}}',
        [SessionId]),
    clara_evaluate(Json, Facts).

% Terminate a CLIPS session
clips_terminate(SessionId) :-
    format(atom(Json),
        '{"tool":"splinteredmind","arguments":{"operation":"clips_terminate_session","session_id":"~w"}}',
        [SessionId]),
    clara_evaluate(Json, _Result).
```

### Complete CLIPS Workflow Example

```prolog
% Run a complete expert system workflow
run_clips_expert_system :-
    % Create session
    create_clips_session(Session),
    format('Created CLIPS session: ~w~n', [Session]),

    % Load some facts
    clips_assert_facts(Session,
        '["(person (name John) (age 30))", "(person (name Jane) (age 17))"]'),

    % Load a rule
    clips_load_rules(Session,
        '["(defrule adult (person (name ?n) (age ?a&:(>= ?a 18))) => (assert (adult ?n)))"]'),

    % Run the engine
    clips_run(Session, Fired),
    format('Rules fired: ~w~n', [Fired]),

    % Query results
    clips_get_facts(Session, Facts),
    format('Facts: ~w~n', [Facts]),

    % Cleanup
    clips_terminate(Session).
```

---

## Scenario 3: Remote Prolog Interaction

Interact with other Prolog sessions running on FieryPit (useful for distributed reasoning).

### Create and Query a Remote Prolog Session

```prolog
% Create a remote Prolog session
create_prolog_session(SessionId) :-
    clara_evaluate(
        '{"tool":"splinteredmind","arguments":{"operation":"prolog_create_session","user_id":"local_prolog"}}',
        Result
    ),
    atom_json_dict(Result, Dict, []),
    SessionId = Dict.session_id.

% Load clauses into remote session
prolog_consult_remote(SessionId, Clauses) :-
    format(atom(ClausesJson), '~w', [Clauses]),
    format(atom(Json),
        '{"tool":"splinteredmind","arguments":{"operation":"prolog_consult","session_id":"~w","clauses":~w}}',
        [SessionId, ClausesJson]),
    clara_evaluate(Json, _Result).

% Query the remote Prolog session
prolog_query_remote(SessionId, Goal, Result) :-
    format(atom(Json),
        '{"tool":"splinteredmind","arguments":{"operation":"prolog_query","session_id":"~w","goal":"~w","all_solutions":true}}',
        [SessionId, Goal]),
    clara_evaluate(Json, Result).

% Terminate remote session
prolog_terminate_remote(SessionId) :-
    format(atom(Json),
        '{"tool":"splinteredmind","arguments":{"operation":"prolog_terminate_session","session_id":"~w"}}',
        [SessionId]),
    clara_evaluate(Json, _Result).
```

### Distributed Reasoning Example

```prolog
% Use a remote Prolog for specialized reasoning
distributed_ancestor_query(Person, Ancestors) :-
    % Create remote session with genealogy KB
    create_prolog_session(Session),

    % Load family tree
    prolog_consult_remote(Session,
        '["parent(tom, mary).", "parent(mary, ann).", "parent(ann, bob).",
          "ancestor(X, Y) :- parent(X, Y).",
          "ancestor(X, Y) :- parent(X, Z), ancestor(Z, Y)."]'),

    % Query ancestors
    format(atom(Goal), 'ancestor(~w, Who)', [Person]),
    prolog_query_remote(Session, Goal, Ancestors),

    % Cleanup
    prolog_terminate_remote(Session).

% Example:
% ?- distributed_ancestor_query(tom, A).
```

---

## Scenario 4: Hybrid Reasoning (Prolog + CLIPS + LLM)

Combine multiple reasoning systems for sophisticated workflows.

### LLM-Assisted Rule Generation

```prolog
% Ask LLM to generate CLIPS rules, then execute them
llm_generate_and_run_rules(Domain, Facts, Results) :-
    % Ask LLM to generate rules
    format(atom(Prompt),
        'Generate CLIPS rules for ~w domain. Return only valid CLIPS defrule syntax.',
        [Domain]),
    ask_llm(Prompt, RulesResponse),

    % Create CLIPS session and load generated rules
    create_clips_session(Session),
    clips_assert_facts(Session, Facts),
    % Note: In practice, you'd parse the LLM response to extract rules
    clips_load_rules(Session, RulesResponse),
    clips_run(Session, _),
    clips_get_facts(Session, Results),
    clips_terminate(Session).
```

### Multi-Stage Reasoning Pipeline

```prolog
% Stage 1: Use CLIPS for initial classification
% Stage 2: Use LLM for natural language explanation
% Stage 3: Use Prolog for logical verification

hybrid_reasoning(Input, FinalResult) :-
    % Stage 1: CLIPS classification
    create_clips_session(ClipsSession),
    format(atom(Fact), '["(input-data ~w)"]', [Input]),
    clips_assert_facts(ClipsSession, Fact),
    clips_load_rules(ClipsSession, '["(defrule classify ...)"]'),
    clips_run(ClipsSession, _),
    clips_get_facts(ClipsSession, Classification),
    clips_terminate(ClipsSession),

    % Stage 2: LLM explanation
    format(atom(ExplainPrompt),
        'Explain this classification in simple terms: ~w',
        [Classification]),
    ask_llm(ExplainPrompt, Explanation),

    % Stage 3: Prolog verification
    verify_classification(Classification, Verified),

    % Combine results
    FinalResult = result{
        classification: Classification,
        explanation: Explanation,
        verified: Verified
    }.

verify_classification(Classification, true) :-
    % Add your verification logic here
    ground(Classification).
verify_classification(_, false).
```

---

## Error Handling

### Checking for Errors

```prolog
% Wrapper that handles errors
safe_splinteredmind_call(Operation, Args, Result) :-
    format(atom(Json),
        '{"tool":"splinteredmind","arguments":{"operation":"~w"~w}}',
        [Operation, Args]),
    clara_evaluate(Json, RawResult),
    (   atom_json_dict(RawResult, Dict, []),
        (   get_dict(status, Dict, error)
        ->  throw(splinteredmind_error(Dict.message))
        ;   Result = Dict
        )
    ;   Result = RawResult
    ).

% Example with error handling
safe_clips_evaluate(SessionId, Script, Result) :-
    catch(
        (   format(atom(Json),
                '{"tool":"splinteredmind","arguments":{"operation":"clips_evaluate","session_id":"~w","script":"~w"}}',
                [SessionId, Script]),
            clara_evaluate(Json, Result)
        ),
        Error,
        (   format('CLIPS evaluation failed: ~w~n', [Error]),
            Result = error(Error)
        )
    ).
```

### Common Error Scenarios

| Error | Likely Cause | Solution |
|-------|--------------|----------|
| `Missing required field: 'model'` | Ollama evaluator needs model name | Add `"model":"llama3.2"` to data |
| `Missing required field: 'prompt'` | Ollama evaluator needs prompt | Add `"prompt":"..."` to data |
| `Model 'X' not found on Ollama` | Model not pulled locally | Run `ollama pull X` |
| `session_id required` | Missing session ID | Ensure you created a session first |
| `Session not found` | Invalid/expired session | Create a new session |
| `Connection refused` | FieryPit not running | Start lildaemon server on port 6666 |
| `Evaluator not found` | Invalid evaluator name | Check `list_evaluators` |

---

## Configuration

### FieryPit URL

The `ClaraSplinteredMindTool` connects to FieryPit at the URL configured during registration. The default is `http://localhost:6666`. You can override this with the `FIERYPIT_URL` environment variable:

```bash
# Use default (http://localhost:6666)
./target/debug/prolog-repl

# Use custom URL
FIERYPIT_URL=http://my-server:9000 ./target/debug/prolog-repl
```

In Rust code, registration looks like:

```rust
let tool = ClaraSplinteredMindTool::with_url("http://localhost:6666");
manager.register_tool(Arc::new(tool));
```

### Default User ID

When creating sessions without specifying `user_id`, the default is `"clara"`.

### Ollama Model Requirements

When using the Ollama evaluator, you must specify a `model` field. Available models depend on what you have pulled locally:

```bash
# List available models
ollama list

# Pull a model if needed
ollama pull llama3.2
ollama pull mistral
```

---

## Performance Tips

1. **Reuse Sessions**: Create sessions once and reuse them rather than creating/destroying for each query.

2. **Batch Operations**: Load multiple facts/rules in a single call rather than one at a time.

3. **Use Appropriate Evaluators**: Use `echo` evaluator for testing to avoid LLM latency.

4. **Session Cleanup**: Always terminate sessions when done to free resources.

```prolog
% Session pool pattern
:- dynamic active_session/2.

get_or_create_session(Type, SessionId) :-
    active_session(Type, SessionId), !.
get_or_create_session(clips, SessionId) :-
    create_clips_session(SessionId),
    assertz(active_session(clips, SessionId)).
get_or_create_session(prolog, SessionId) :-
    create_prolog_session(SessionId),
    assertz(active_session(prolog, SessionId)).

cleanup_all_sessions :-
    forall(
        active_session(Type, Session),
        (   (Type = clips -> clips_terminate(Session) ; prolog_terminate_remote(Session)),
            retract(active_session(Type, Session))
        )
    ).
```

---

## Quick Reference

### Minimal Examples

```prolog
% Check FieryPit status
?- clara_evaluate('{"tool":"splinteredmind","arguments":{"operation":"status"}}', R).

% List evaluators
?- clara_evaluate('{"tool":"splinteredmind","arguments":{"operation":"list_evaluators"}}', R).

% LLM call with Ollama (requires model and prompt)
?- clara_evaluate('{"tool":"splinteredmind","arguments":{"operation":"evaluate","data":{"model":"llama3.2","prompt":"Hello!"}}}', R).

% Create CLIPS session
?- clara_evaluate('{"tool":"splinteredmind","arguments":{"operation":"clips_create_session"}}', R).

% Create Prolog session
?- clara_evaluate('{"tool":"splinteredmind","arguments":{"operation":"prolog_create_session"}}', R).
```

---

## See Also

- `TOOLBOX_SYSTEM.md` - General toolbox architecture
- `CLIPS_CALLBACKS.md` - FFI callback system
- `fiery_pit_endpoints.md` - Complete FieryPit API reference
- `DEMONIC_VOICE_PROTOCOL.md` - Related lil-daemon protocol
