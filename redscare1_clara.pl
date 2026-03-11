% ── Clara integration (auto-generated) ──────────────────────────────────────
:- prolog_listen(commie_state/1, updated(commie_state/1)).
:- prolog_listen(suggestion/1, updated(suggestion/1)).

updated(Pred, Action, Context) :-
    clause(Head, _Body, Context),
    coire_publish_assert(Head),
    format('Updated ~w with action ~w in context ~p~n', [Pred, Action, Head]).
% ── End Clara integration ───────────────────────────────────────────────────

%% forward propagation testing

:- use_module(library(the_rabbit)).
:- dynamic(commie_state/1).
:- dynamic(suggestion/1).

bash_the_reds :- commie_state(visible), assertz(suggestion(bash_the_reds)).

advice(Suggestion) :- suggestion(Suggestion).

