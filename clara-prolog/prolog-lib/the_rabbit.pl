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

%% descriminate - Extract the response from the LLM and classify it
descriminate(Text, TruthValue) :-
    ponder_text(Text, LLMSez), % Get the JSON response from the LLM
    extract_nested(LLMSez, [hohi, response, response], Response),
    !,
    atom_string(Text, TextStr),
    atom_string(Response, RespStr),
    string_concat(TextStr, " ", Tmp),
    string_concat(Tmp, RespStr, Pair),
    classify_text(Pair, TruthValue).
descriminate(_, _) :-
    format(user_error, "Error: Could not extract response from LLM output.~n", []),
    fail.

%% descriminate_k - Extract the response from the LLM and classify it with top K results
descriminate_k(Text, K, Results) :-
    ponder_text(Text, LLMSez), % Get the JSON response from the LLM
    extract_nested(LLMSez, [hohi, response, response], Response),
    !,
    atom_string(Text, TextStr),
    atom_string(Response, RespStr),
    string_concat(TextStr, " ", Tmp),
    string_concat(Tmp, RespStr, Pair), % Combine the text and LLM's response
    classify_text_k(Pair, K, Results). % Classify with top K results
descriminate_k(_, _, _) :-
    format(user_error, "Error: Could not extract response from LLM output.~n", []),
    fail.
