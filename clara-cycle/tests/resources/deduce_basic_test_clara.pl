% ── Clara integration (auto-generated) ──────────────────────────────────────
:- prolog_listen(visitor/1, updated(visitor/1)).
:- prolog_listen(egg/1, updated(egg/1)).

updated(Pred, Action, Context) :-
    clause(Head, _Body, Context),
    coire_publish_assert(Head),
    format('Updated ~w with action ~w in context ~p~n', [Pred, Action, Head]).
% ── End Clara integration ───────────────────────────────────────────────────

%% test9 : verify a derived fact is passed through the entire deduction cycle back to the prolog engine

:- use_module(library(the_rabbit)).
:- use_module(library(the_rat)).

:- dynamic(visitor/1).
:- dynamic(egg/1).

break_some :- egg(unbroken),
  assertz(egg(broken)).

omelette(Visitor, Dish) :- visitor(Visitor),
  egg(broken),
  Dish = lovely_fluffy_goodness.


  
	