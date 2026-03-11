%% forward propagation testing

:- use_module(library(the_rabbit)).
:- use_module(library(the_rat)).
:- dynamic(visitor/1).
:- dynamic(reason/1).
:- dynamic(greeted/1).

admit(Visitor, Reason) :- visitor(Visitor), greeted(Visitor), Reason = 'The visitor has been greeted, please come in.'.

suggestion(Visitor, Suggestion) :-
    visitor(Visitor),
    greeted(Visitor),
    current_context(Context),
    clara_fy('Based upon the conversation so far, does the visitor seem lost or confused?', Context, R),
    !, 
    R = true,
    Suggestion = 'Direct the visitor to the help kiosk.'.

suggestion(Visitor, Suggestion) :- visitor(Visitor), \+ greeted(Visitor), Suggestion = 'Greet the visitor'.
