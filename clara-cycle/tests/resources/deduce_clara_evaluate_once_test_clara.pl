% ── Clara integration (auto-generated) ──────────────────────────────────────
updated(Pred, Action, Context) :-
    clause(Head, _Body, Context),
    coire_publish_assert(Head),
    format('Updated ~w with action ~w in context ~p~n', [Pred, Action, Head]).
% ── End Clara integration ───────────────────────────────────────────────────

%% deduce_clara_evaluate_once_test : ensure that clara-prolog engine only executes a clara_evaluate once during a single 
%% evaluation for the same variables.  if we clara_evaluate(Json, Result) we want to table the result and not call the ffi
%% function again

:- use_module(library(the_rabbit)).
table(clara_evaluate).

q1('{"tool":"echo","arguments":{"message":"startup test"}}').

echo1(R1) :- q1(Q), the_rabbit:clara_evaluate(Q, R1).
echo2(R2) :- q1(Q), the_rabbit:clara_evaluate(Q, R2).

duh_dun :- echo1(_), echo2(_).
