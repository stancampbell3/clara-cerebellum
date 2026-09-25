% Non-ASCII text round trip through the Prolog <-> CLIPS relay (see utf8_deduce_test.rs).
:- prolog_listen(note/1, updated(note/1)).
:- prolog_listen(echoed/1, updated(echoed/1)).

updated(Pred, Action, Context) :-
    clause(Head, _Body, Context),
    coire_publish_assert(Head),
    format('Updated ~w with action ~w in context ~p~n', [Pred, Action, Head]).

:- use_module(library(the_rabbit)).
:- use_module(library(the_rat)).

:- dynamic(note/1).
:- dynamic(echoed/1).
