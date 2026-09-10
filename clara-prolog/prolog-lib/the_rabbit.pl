%% the_rabbit.pl
%% -------------
%% Testing the extensive capabilities of our clever girl

:- module(the_rabbit, [
    dict_to_json/2,
    classify_text/2,
    classify_text_k/3,
    enable_evaluator/2,
    ponder_text/2,
    ponder_text/3,
    ponder_text_with_context/3,
    ponder_text_with_context/4,
    ponder_reason/2,
    ponder_reason_with_context/3,
    verdict_system_prompt/1,
    reasoning_system_prompt/1,
    extract_field/3,
    extract_response/3,
    extract_nested/3,
    descriminate/2,
    descriminate_k/3,
    descriminate_k_with_context/4,
    current_context/1
]).

:- use_module(library(http/json)).
% MUST be thread_local, not dynamic: dynamic predicates share one global
% clause store across every Prolog engine in the process, so an earlier
% deduction's seeded context would satisfy (or, if malformed, poison) a
% later, unrelated deduction's current_context/1 call — confirmed live
% 2026-09-07: a single context payload containing an embedded double
% quote broke JSON parsing for that call, and because the fact was never
% retracted, EVERY subsequent current_context/1 call in the process kept
% re-reading that same broken fact regardless of its own context, until
% the process restarted. thread_local scopes the fact to the engine,
% same pattern the_coire.pl's caws_result/2 etc. and edge_result/3 already
% use for exactly this reason (see that file's own comment on it).
:- thread_local(deduce_context_json/1).

%% dict_to_json/2 - Safely serialize a dict to a JSON atom
%%   Handles all escaping (newlines, quotes, unicode, control chars)
dict_to_json(Dict, Json) :-
    atom_json_dict(Json, Dict, []).

%% classify_text/2 - Classify text using the fastText classify tool
%% Fails cleanly if the classify tool is unavailable or returns an error.
classify_text(Text, Result) :-
    dict_to_json(_{tool: classify, arguments: _{text: Text}}, Json),
    clara_evaluate(Json, Raw),
    % value_string_as(atom): atom_json_dict/3 defaults to decoding JSON
    % strings as SWI strings, which never unify with the bare atom `error`
    % below (confirmed live: the unqualified [] form silently never detects
    % errors). Decoding as atoms makes the get_dict/3 comparison work.
    atom_json_dict(Raw, Dict, [value_string_as(atom)]),
    ( get_dict(status, Dict, error) ->
        format(user_error, "classify tool error: ~w~n", [Dict.message]),
        fail
    ;
        Result = Raw
    ).

%% classify_text_k/3 - Classify text returning top K predictions
%% Fails cleanly if the classify tool is unavailable or returns an error.
classify_text_k(Text, K, Result) :-
    dict_to_json(_{tool: classify, arguments: _{text: Text, k: K}}, Json),
    clara_evaluate(Json, Raw),
    atom_json_dict(Raw, Dict, [value_string_as(atom)]),
    ( get_dict(status, Dict, error) ->
        format(user_error, "classify tool error: ~w~n", [Dict.message]),
        fail
    ;
        Result = Raw
    ).

%% enable_evaluator
enable_evaluator(Evaluator, Result) :-
    dict_to_json(_{tool: splinteredmind,
                   arguments: _{operation: set_evaluator,
                                evaluator: Evaluator}}, Json),
    clara_evaluate(Json, Result).

%% --- System prompts for predicate-answering mode -----------------------
%%
%% ponder_text/2 and _with_context/3 send NO `system` override, so they
%% inherit whatever persona the focused evaluator carries
%% (config/prompts/clara_system_prompt.txt — the witty Clara Oswald
%% voice). That's right for user-facing answer *generation*, but wrong
%% for the two other things these predicates get used for inside a Dis
%% deduction:
%%
%%   1. descriminate/*  -> clara_fy/2,3 truth classification. The persona
%%      ("Oh, heavens no...", "Quite.") almost never leads with a bare
%%      yes/no, so response_shortcut/2 misses and the (currently weak)
%%      fastText classifier gets the call — and inverts clear answers.
%%      Verified 2026-09-10: a terse "reply with one word: yes/no/
%%      unresolved" system prompt lifts gemma4:e4b AND qwen-clara-27b
%%      from ~8/12 to ~11/12 correct on a fixed set, and makes the two
%%      models agree 11/12 (was 6/12) — response_shortcut carries 11/12,
%%      classifier off the critical path.
%%
%%   2. plain internal reasoning / musing steps that feed rule logic
%%      rather than the user. Internal monologue does not need a flowery
%%      voice; a plain, direct, truthful prompt is cheaper, faster, and
%%      leaves less room for out-of-band framing.
%%
%% These are plain facts so a caller (or a future config layer) can
%% override them without editing this file's logic.
verdict_system_prompt(
'You classify a statement or question into a truth value. Reply with EXACTLY ONE WORD and nothing else: "yes" if the statement is true or the answer is affirmative; "no" if it is false or the answer is negative; "unresolved" if it is genuinely contested, subjective, ambiguous, or cannot be determined from established knowledge. Output only that single lowercase word. No punctuation, no explanation.').

reasoning_system_prompt(
'You are a careful reasoning engine operating inside a larger inference system. Answer directly, concisely, and truthfully. State what is known, what is uncertain, and why. Do not adopt a persona, do not use a conversational or theatrical voice, and do not add pleasantries — your output is consumed by downstream logic, not shown to a person.').

%% ponder_text/2 - Evaluate a prompt using the LLM (persona default).
%%   Model: the Clara base tag (qwen-clara:latest). The `gemma4:e4b`
%%   hardcode this replaces was chosen when Dis ran on pineal's 12 GB
%%   5070, where the base Clara model and Edgequake's LLM couldn't both
%%   stay resident — matching them dodged Ollama hot-swaps mid-turn
%%   (answer_step/9 reconciles a ponder_text answer with an Edgequake
%%   ruminate answer in the same turn). On limbic's 32 GB 5090 that
%%   constraint is gone: qwen-clara-27b (~17.5 GB) + embeddinggemma
%%   (~0.7 GB) co-reside with room to spare (verified 2026-09-10), so
%%   the whole stack — ponder_text, descriminate, and Edgequake's own
%%   EDGEQUAKE_LLM_MODEL — should point at one model. NB: gemma4:e4b and
%%   the 27b do NOT co-reside (Ollama evicts the 27b), so retiring
%%   gemma4:e4b from the stack is the point, not running it alongside.
%%   Unchanged: still inherits the evaluator's persona. Use ponder_text/3
%%   (or ponder_reason/2) for non-user-facing calls.
ponder_text(Text, Result) :-
    dict_to_json(_{tool: splinteredmind,
                   arguments: _{operation: evaluate,
                                data: _{prompt: Text,
                                         model: 'qwen-clara:latest'}}}, Json),
    clara_evaluate(Json, Result).

%% ponder_text/3 - As ponder_text/2 but with an explicit system prompt,
%%   overriding the evaluator's persona for this one call.
ponder_text(Text, System, Result) :-
    dict_to_json(_{tool: splinteredmind,
                   arguments: _{operation: evaluate,
                                data: _{prompt: Text,
                                         system: System,
                                         model: 'qwen-clara:latest'}}}, Json),
    clara_evaluate(Json, Result).

%% ponder_reason/2 - plain, voiceless internal reasoning (reasoning_system_prompt/1).
ponder_reason(Text, Result) :-
    reasoning_system_prompt(Sys),
    ponder_text(Text, Sys, Result).

%% ponder_text_with_context/3 - Evaluate a prompt using the LLM with conversation context.
%%   Context is a list of message dicts, e.g. [_{role:user, content:"hello"}, ...].
%%   Unchanged: persona default.
ponder_text_with_context(Text, Context, Result) :-
    dict_to_json(_{tool: splinteredmind,
                   arguments: _{operation: evaluate,
                                data: _{prompt: Text,
                                        context: Context,
                                        model: 'qwen-clara:latest'}}}, Json),
    clara_evaluate(Json, Result).

%% ponder_text_with_context/4 - As /3 but with an explicit system prompt.
ponder_text_with_context(Text, Context, System, Result) :-
    dict_to_json(_{tool: splinteredmind,
                   arguments: _{operation: evaluate,
                                data: _{prompt: Text,
                                        context: Context,
                                        system: System,
                                        model: 'qwen-clara:latest'}}}, Json),
    clara_evaluate(Json, Result).

%% ponder_reason_with_context/3 - plain voiceless internal reasoning, grounded.
ponder_reason_with_context(Text, Context, Result) :-
    reasoning_system_prompt(Sys),
    ponder_text_with_context(Text, Context, Sys, Result).

%% current_context/1 - Retrieve the conversational context injected at deduce time.
%%   Returns a list of message dicts parsed from the deduce_context_json/1 fact.
%%   Falls back to an empty list when no context was provided.
current_context(Context) :-
    deduce_context_json(Json),
    atom_json_dict(Json, Context, []),
    !.
current_context([]).

%% extract_field/3 - Extract a field from a dict, converting key to atom if needed
extract_field(Dict, FieldName, Value) :-
    (   atom(FieldName) -> Key = FieldName
    ;   atom_string(Key, FieldName)
    ),
    get_dict(Key, Dict, Value).

%% extract_response/3 - Parse JSON and extract a top-level field by name
extract_response(RawJson, FieldName, Response) :-
    atom_json_dict(RawJson, Dict, []),
    extract_field(Dict, FieldName, Response),
    !.
extract_response(_, _, error(no_field)).

%% extract_nested/3 - Parse JSON and extract a value by path (list of keys)
%%   e.g. extract_nested(Json, [hohi, response], Value)
extract_nested(RawJson, Path, Value) :-
    atom_json_dict(RawJson, Dict, []),
    extract_path(Dict, Path, Value).

extract_path(Value, [], Value).
extract_path(Dict, [Key|Rest], Value) :-
    extract_field(Dict, Key, Sub),
    extract_path(Sub, Rest, Value).

%% --- Helpers for response normalization and shortcut detection ---

% shortcut_label/2 - map shortcut atom to the classifier label string
shortcut_label(true,       "__label____resolved_true__").
shortcut_label(false,      "__label____resolved_false__").
shortcut_label(unresolved, "__label____unresolved__").

% trim_leading_codes/2 - remove leading whitespace codes from a code list
trim_leading_codes([], []).
trim_leading_codes([C|Cs], Rest) :-
    ( C =:= 32 ; C =:= 9 ; C =:= 10 ; C =:= 13 ), !,
    trim_leading_codes(Cs, Rest).
trim_leading_codes(List, List).

% trim_leading/2 - remove leading whitespace from a string
trim_leading(Str, Trimmed) :-
    string_codes(Str, Codes),
    trim_leading_codes(Codes, Codes2),
    string_codes(Trimmed, Codes2).

% response_shortcut/2 - detect if the response begins with a recognizable token
%   and map it to one of the atoms: true, false, unresolved
response_shortcut(Response, Shortcut) :-
    % Accept atoms or strings
    ( atom(Response) -> atom_string(Response, RespStr) ; RespStr = Response ),
    string_lower(RespStr, Lower),
    trim_leading(Lower, Trim),
    ( sub_string(Trim, 0, _, _, "yes") -> Shortcut = true
    ; sub_string(Trim, 0, _, _, "true") -> Shortcut = true
    ; sub_string(Trim, 0, _, _, "no") -> Shortcut = false
    ; sub_string(Trim, 0, _, _, "false") -> Shortcut = false
    ; sub_string(Trim, 0, _, _, "unresolved") -> Shortcut = unresolved
    ; sub_string(Trim, 0, _, _, "that's correct") -> Shortcut = true
    ; sub_string(Trim, 0, _, _, "correct") -> Shortcut = true
    ; sub_string(Trim, 0, _, _, "that's incorrect") -> Shortcut = false
    ).

%% extract_llm_response/2 - Extract the LLM's text response from a
%%   ponder_text/2-family JSON result, handling both response envelope
%%   shapes a Hohi can carry (deterministic per currently-focused
%%   evaluator class, not random — confirmed live: a tool-calling
%%   evaluator, e.g. ClaraMindSplinter/GroqEvaluator, always nests the
%%   text under hohi.response.content; a plain, non-tool-calling
%%   OllamaEvaluator always nests it under hohi.response.response — see
%%   examples_ritual_rumination_answer.py's own extract_hohi_response/2
%%   for the original root-cause writeup). descriminate/2 and friends
%%   below previously only handled the plain-Ollama shape, so clara_fy
%%   failed outright whenever a tool-calling evaluator happened to be
%%   focused (confirmed live 2026-08-25 via
%%   examples_ritual_progressive_consult.py, the first caller to run
%%   clara_fy under such a focus).
extract_llm_response(RawJson, Response) :-
    extract_nested(RawJson, [hohi, response, content], Response), !.
extract_llm_response(RawJson, Response) :-
    extract_nested(RawJson, [hohi, response, response], Response).

%% descriminate - Extract the response from the LLM and classify it.
%%   Uses verdict_system_prompt/1: the LLM is asked for a bare
%%   yes/no/unresolved, so response_shortcut/2 carries the verdict and
%%   the fastText classifier is only a fallback (see the system-prompt
%%   comment above).
descriminate(Text, TruthValue) :-
    verdict_system_prompt(Sys),
    ponder_text(Text, Sys, LLMSez), % Get the JSON response from the LLM
    extract_llm_response(LLMSez, Response),
    !,
    % If the LLM response begins with an explicit token, shortcut and return
    % a single high-confidence result instead of calling the classifier.
    ( response_shortcut(Response, Shortcut) ->
        shortcut_label(Shortcut, LabelStr),
        atom_json_dict(TruthValue, _{predictions: [_{label: LabelStr, probability: 0.99}]}, [])
    ;
        atom_string(Text, TextStr),
        atom_string(Response, RespStr),
        string_concat(TextStr, " ", Tmp),
        string_concat(Tmp, RespStr, Pair),
        classify_text(Pair, TruthValue)
    ).
descriminate(_, _) :-
    format(user_error, "Error: Could not extract response from LLM output.~n", []),
    fail.

%% descriminate_k - Extract the response from the LLM and classify it with top K results
descriminate_k(Text, K, Results) :-
    verdict_system_prompt(Sys),
    ponder_text(Text, Sys, LLMSez), % Get the JSON response from the LLM
    extract_llm_response(LLMSez, Response),
    !,
    ( response_shortcut(Response, Shortcut) ->
        shortcut_label(Shortcut, LabelStr),
        atom_json_dict(Results, _{predictions: [_{label: LabelStr, probability: 0.99}]}, [])
    ;
        atom_string(Text, TextStr),
        atom_string(Response, RespStr),
        string_concat(TextStr, " ", Tmp),
        string_concat(Tmp, RespStr, Pair), % Combine the text and LLM's response
        classify_text_k(Pair, K, Results) % Classify with top K results
    ).
descriminate_k(_, _, _) :-
    format(user_error, "Error: Could not extract response from LLM output.~n", []),
    fail.

%% descriminate_k_with_context/4 - Like descriminate_k/3 but grounds the LLM
%%   call with a conversation context list.
descriminate_k_with_context(Text, K, Context, Results) :-
    verdict_system_prompt(Sys),
    ponder_text_with_context(Text, Context, Sys, LLMSez),
    extract_llm_response(LLMSez, Response),
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
descriminate_k_with_context(_, _, _, _) :-
    format(user_error, "Error: Could not extract response from LLM output.~n", []),
    fail.
