%% classify_demo.pl - Demonstrates text classification via clara_evaluate/2
%%
%% Usage: Consult this file in a Prolog session with clara_evaluate registered
%% and DAGDA_MODEL_PATH set.
%%
%% ?- classify_text('Water boils at 100C. Yes that is correct.', Result).

%% classify_text/2 - Classify text using the fastText classify tool
classify_text(Text, Result) :-
    format(atom(Json),
        '{"tool":"classify","arguments":{"text":"~w"}}',
        [Text]),
    clara_evaluate(Json, Result).

%% classify_text_k/3 - Classify text returning top K predictions
classify_text_k(Text, K, Result) :-
    format(atom(Json),
        '{"tool":"classify","arguments":{"text":"~w","k":~d}}',
        [Text, K]),
    clara_evaluate(Json, Result).

%% Demo queries (uncomment to run on consult):
%% :- classify_text('Water boils at 100C at sea level. Yes, that is correct.', R),
%%    format('Resolved example: ~w~n', [R]).
%%
%% :- classify_text('Cats can teleport through quantum tunneling. The cosmic energy says so!', R),
%%    format('Unresolved example: ~w~n', [R]).
