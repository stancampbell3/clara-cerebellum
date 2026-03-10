%% forward propagation testing

:- use_module(library(the_rabbit)).
:- dynamic(visitor/1).
:- dynamic(reason/1).
:- dynamic(greeted/1).

admit(Visitor, Reason) :- visitor(Visitor), greeted(Visitor), Reason = 'The visitor has been greeted, please come in.'.

suggestion(Visitor, Suggestion) :- visitor(Visitor), \+ greeted(Visitor), Suggestion = 'Greet the visitor'.
