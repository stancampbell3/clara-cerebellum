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
    ponder_verdict/2,
    ponder_verdict_with_context/3,
    verdict_system_prompt/1,
    verdict_format/1,
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

%% --- Predicate-answering mode: system prompts + output constraint ------
%%
%% ponder_text/2 and _with_context/3 send NO `system` override, so they
%% inherit whatever persona the focused evaluator carries
%% (config/prompts/clara_system_prompt.txt — the witty Clara Oswald
%% voice). That's right for user-facing answer *generation*, but wrong
%% for the two other things these predicates get used for inside a Dis
%% deduction:
%%
%%   1. descriminate/*  -> clara_fy/2,3 truth classification. Handled by
%%      ponder_verdict/2 below: verdict_system_prompt/1 (terse "one word:
%%      yes/no/unresolved") + verdict_format/1 (an Ollama `format` enum
%%      that grammar-constrains the output to exactly those three
%%      tokens) + think:false. The model physically cannot emit anything
%%      else, so there is nothing to prefix-match — the old
%%      response_shortcut/2 heuristics and the dagda-0.2 fastText
%%      fallback are both gone from this path. Verified 2026-09-10: the
%%      terse prompt alone lifted gemma4:e4b and qwen-clara-27b from
%%      ~8/12 to ~11/12 on a fixed set; the format enum makes it
%%      deterministic. (See clara-cerebellum
%%      docs/reasoning_model_upgrade_status.md.)
%%
%%   2. plain internal reasoning / musing steps that feed rule logic
%%      rather than the user (ponder_reason/2). Internal monologue does
%%      not need a flowery voice; a plain, direct, truthful prompt is
%%      cheaper, faster, and leaves less room for out-of-band framing.
%%
%% These are plain facts so a caller (or a future config layer) can
%% override them without editing this file's logic.
verdict_system_prompt(
'You classify a statement or question into a truth value. Reply with EXACTLY ONE WORD and nothing else: "yes" if the statement is true or the answer is affirmative; "no" if it is false or the answer is negative; "unresolved" if it is genuinely contested, subjective, ambiguous, or cannot be determined from established knowledge. Output only that single lowercase word. No punctuation, no explanation.').

%% verdict_format/1 - Ollama structured-output constraint for the verdict
%%   path. Serialises to {"type":"string","enum":["yes","no","unresolved"]};
%%   the inference engine restricts generation to conform, so the reply
%%   is exactly one of the three (JSON-quoted).
verdict_format(_{type: string, enum: [yes, no, unresolved]}).

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

%% --- ponder_verdict/2, /3: constrained yes/no/unresolved -------------------
%%
%% Terse verdict system prompt + a `format` enum grammar constraint +
%% think:false. The reply can only be one of the three tokens, so
%% Verdict is the bare atom `true` / `false` / `unresolved` (mapped from
%% yes / no / unresolved) — no heuristics, no fastText fallback.

%% ponder_verdict/2
ponder_verdict(Text, Verdict) :-
    verdict_system_prompt(Sys),
    verdict_format(Fmt),
    dict_to_json(_{tool: splinteredmind,
                   arguments: _{operation: evaluate,
                                data: _{prompt: Text,
                                        system: Sys,
                                        format: Fmt,
                                        think: false,
                                        model: 'qwen-clara:latest'}}}, Json),
    clara_evaluate(Json, Raw),
    extract_llm_response(Raw, Resp0),
    verdict_atom(Resp0, Verdict).

%% ponder_verdict_with_context/3 - as /2 but grounded with a context list.
ponder_verdict_with_context(Text, Context, Verdict) :-
    verdict_system_prompt(Sys),
    verdict_format(Fmt),
    dict_to_json(_{tool: splinteredmind,
                   arguments: _{operation: evaluate,
                                data: _{prompt: Text,
                                        context: Context,
                                        system: Sys,
                                        format: Fmt,
                                        think: false,
                                        model: 'qwen-clara:latest'}}}, Json),
    clara_evaluate(Json, Raw),
    extract_llm_response(Raw, Resp0),
    verdict_atom(Resp0, Verdict).

%% verdict_atom/2 - normalise the (JSON-quoted, grammar-constrained)
%%   enum output to the bare atom true / false / unresolved. Strips
%%   quotes / whitespace / trailing punctuation defensively, then the
%%   token must be exactly one of the three. Fails otherwise (caller's
%%   error clause handles it).
verdict_strip_char('"').
verdict_strip_char('\'').
verdict_strip_char(' ').
verdict_strip_char('\t').
verdict_strip_char('\n').
verdict_strip_char('\r').
verdict_strip_char('.').

verdict_atom(Raw, Verdict) :-
    ( atom(Raw) -> atom_string(Raw, S) ; S = Raw ),
    string_lower(S, Low),
    string_chars(Low, Cs0),
    exclude(verdict_strip_char, Cs0, Cs),
    atom_chars(Tok, Cs),
    verdict_token(Tok, Verdict).

verdict_token(yes,        true).
verdict_token(no,         false).
verdict_token(unresolved, unresolved).

%% verdict_label/2 - verdict atom -> the classifier label string the
%%   the_rat.pl consumers expect (was shortcut_label/2, before
%%   response_shortcut/2 was retired).
verdict_label(true,       "__label____resolved_true__").
verdict_label(false,      "__label____resolved_false__").
verdict_label(unresolved, "__label____unresolved__").

%% verdict_result/2 - wrap a verdict atom in the predictions-JSON shape
%%   the_rat.pl's extract_top_k_labels/3 expects. Constrained decoding
%%   yields a single verdict, so: one prediction, probability 1.0.
%%   (A K>1 caller would get a 1-element list; there are none today —
%%   top_status/2,3 call with K=1.)
verdict_result(Verdict, ResultJson) :-
    verdict_label(Verdict, LabelStr),
    atom_json_dict(ResultJson, _{predictions: [_{label: LabelStr, probability: 1.0}]}, []).

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

%% descriminate/2 - classify Text into a truth value via ponder_verdict/2
%%   (constrained yes/no/unresolved). TruthValue is the predictions-JSON
%%   the_rat.pl's extract_top_k_labels/3 consumes.
descriminate(Text, TruthValue) :-
    ponder_verdict(Text, Verdict),
    !,
    verdict_result(Verdict, TruthValue).
descriminate(_, _) :-
    format(user_error, "Error: descriminate could not get a verdict from the LLM.~n", []),
    fail.

%% descriminate_k/3 - K is vestigial now (constrained decoding yields one
%%   verdict); returns the same one-element predictions list as
%%   descriminate/2. Kept for the_rat.pl's extract_top_k_labels/3, which
%%   only ever calls with K=1.
descriminate_k(Text, _K, Results) :-
    ponder_verdict(Text, Verdict),
    !,
    verdict_result(Verdict, Results).
descriminate_k(_, _, _) :-
    format(user_error, "Error: descriminate_k could not get a verdict from the LLM.~n", []),
    fail.

%% descriminate_k_with_context/4 - as descriminate_k/3 but grounds the
%%   LLM call with a conversation context list.
descriminate_k_with_context(Text, _K, Context, Results) :-
    ponder_verdict_with_context(Text, Context, Verdict),
    !,
    verdict_result(Verdict, Results).
descriminate_k_with_context(_, _, _, _) :-
    format(user_error, "Error: descriminate_k_with_context could not get a verdict from the LLM.~n", []),
    fail.
