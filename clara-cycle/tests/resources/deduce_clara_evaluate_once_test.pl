%% deduce_clara_evaluate_once_test : ensure that clara-prolog engine only executes a clara_evaluate once during a single 
%% evaluation for the same variables.  if we clara_evaluate(Json, Result) we want to table the result and not call the ffi
%% function again

:- use_module(library(the_rabbit)).
:- dynamic(clara_evaluate_result/2).

determinate(P, Q) :- clara_evaluate_result(P, Q), !.
determinate(P, Q) :- clara_evaluate(P, Q), !, assertz(clara_evaluate_result(P, Q)).

echo1(R1) :- Q = '{"tool":"echo","arguments":{"message":"startup test"}}',
    determinate('{"tool":"echo","arguments":{"message":"startup test"}}', R1).
echo2(R2) :- Q = '{"tool":"echo","arguments":{"message":"startup test"}}',
    determinate('{"tool":"echo","arguments":{"message":"startup test"}}', R2).

duh_dun :- echo1(_), echo2(_).
