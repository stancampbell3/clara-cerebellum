%% The Rat
%% -------
%% Provides clara_fy predicate allowing us to query an LLM and classify its response to a truth value
%% (true/false/unresolved) based on the top prediction from the Ember Devil model (by default).
%% See the_rabbit.pl for underlying LLM interaction details.

:- module(the_rat, [
    clara_fy/2,
    clara_fy/3,
    extract_top_k_labels/3,
    extract_top_k_labels_with_context/4,
    top_status/2,
    top_status_with_context/3
]).

:- use_module(library(the_rabbit)).
:- use_module(library(http/json)).

% We will be dealing with the base OllamaEvaluator
:- enable_evaluator('ollama', _).

% Main entry point: Get Top K simplified labels
extract_top_k_labels(Text, K, SimpleLabels) :-
    descriminate_k(Text, K, RawJson),
    atom_json_dict(RawJson, Dict, []),
    % Sort predictions by probability descending
    predsort(compare_probs, Dict.predictions, Sorted),
    % Take the first K
    length(TopK, K),
    append(TopK, _, Sorted),
    % Map the raw dicts to simplified labels (true/false/unresolved)
    maplist(get_simple_label, TopK, SimpleLabels).

% Helper: Extract and normalize
get_simple_label(Dict, Simple) :-
    normalize_label(Dict.label, Simple).

% Normalize the fastText labels to atoms
normalize_label("__label____resolved_true__",  true) :- !.
normalize_label("__label____resolved_false__", false) :- !.
normalize_label("__label____unresolved__",     unresolved) :- !.
normalize_label(Other, unknown) :-
    format(user_error, "Warning: Unexpected label format: ~w~n", [Other]).

% Comparison for predsort (Descending)
compare_probs(Delta, P1, P2) :-
    ( P1.probability >= P2.probability -> Delta = (<) ; Delta = (>) ).

% Get the single best status
top_status(Text, Status) :-
    extract_top_k_labels(Text, 1, [Status]).

%% -----------------------------------------------------------------------------------
%% clara_fy/2 : classify a text query into a truth value using the top prediction
%% from the Ember Devil model.
%% -----------------------------------------------------------------------------------
clara_fy(Text, TruthValue) :- top_status(Text, B1),
    format('B1: ~w~n', [B1]),
    TruthValue = B1.

%% -----------------------------------------------------------------------------------
%% clara_fy/3 : like clara_fy/2 but grounds the LLM call with a conversation context.
%%   Context is a list of message dicts, e.g. [_{role:user, content:"hello"}, ...].
%%   Typical use:
%%     current_context(Ctx), clara_fy('the visitor seems confused', Ctx, R).
%% -----------------------------------------------------------------------------------
clara_fy(Text, Context, TruthValue) :-
    top_status_with_context(Text, Context, B1),
    format('B1: ~w~n', [B1]),
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