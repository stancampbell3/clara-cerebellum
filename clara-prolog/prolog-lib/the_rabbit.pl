%% the_rabbit.pl
%% -------------
%% Testing the extensive capabilities of our clever girl

:- module(the_rabbit, [
    dict_to_json/2,
    classify_text/2,
    classify_text_k/3,
    enable_evaluator/2,
    ponder_text/2,
    extract_field/3,
    extract_response/3,
    extract_nested/3,
    descriminate/2,
    descriminate_k/3
]).

:- use_module(library(http/json)).

%% dict_to_json/2 - Safely serialize a dict to a JSON atom
%%   Handles all escaping (newlines, quotes, unicode, control chars)
dict_to_json(Dict, Json) :-
    atom_json_dict(Json, Dict, []).

%% classify_text/2 - Classify text using the fastText classify tool
classify_text(Text, Result) :-
    dict_to_json(_{tool: classify, arguments: _{text: Text}}, Json),
    clara_evaluate(Json, Result).

%% classify_text_k/3 - Classify text returning top K predictions
classify_text_k(Text, K, Result) :-
    dict_to_json(_{tool: classify, arguments: _{text: Text, k: K}}, Json),
    clara_evaluate(Json, Result).

%% enable_evaluator
enable_evaluator(Evaluator, Result) :-
    dict_to_json(_{tool: splinteredmind,
                   arguments: _{operation: set_evaluator,
                                evaluator: Evaluator}}, Json),
    clara_evaluate(Json, Result).

%% ponder_text - Evaluate a prompt using the LLM
ponder_text(Text, Result) :-
    dict_to_json(_{tool: splinteredmind,
                   arguments: _{operation: evaluate,
                                data: _{prompt: Text,
                                         model: 'qwen2.5:7b'}}}, Json),
    clara_evaluate(Json, Result).

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

%% descriminate - Extract the response from the LLM and classify it
descriminate(Text, TruthValue) :-
    ponder_text(Text, LLMSez), % Get the JSON response from the LLM
    extract_nested(LLMSez, [hohi, response, response], Response),
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
    ponder_text(Text, LLMSez), % Get the JSON response from the LLM
    extract_nested(LLMSez, [hohi, response, response], Response),
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
