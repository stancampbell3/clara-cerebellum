% ── Clara integration (auto-generated) ──────────────────────────────────────
:- prolog_listen(visitor/1, updated(visitor/1)).
:- prolog_listen(reason/1, updated(reason/1)).
:- prolog_listen(greeted/1, updated(greeted/1)).

updated(Pred, Action, Context) :-
    clause(Head, _Body, Context),
    coire_publish_assert(Head),
    format('Updated ~w with action ~w in context ~p~n', [Pred, Action, Head]).
% ── End Clara integration ───────────────────────────────────────────────────

%% forward propagation testing

:- use_module(library(the_rabbit)).
:- dynamic(visitor/1).
:- dynamic(reason/1).
:- dynamic(greeted/1).

admit(Visitor, Reason) :- visitor(Visitor), greeted(Visitor), Reason = 'The visitor has been greeted, please come in.'.

suggestion(Visitor, Suggestion) :- visitor(Visitor), \+ greeted(Visitor), Suggestion = 'Greet the visitor'.
