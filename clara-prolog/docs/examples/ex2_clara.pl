% ── Clara integration (auto-generated) ──────────────────────────────────────
:- prolog_listen(wet/1, updated(wet/1)).
:- prolog_listen(day_of_week/1, updated(day_of_week/1)).

updated(Pred, Action, Context) :-
    clause(Head, _Body, Context),
    coire_publish_assert(Head),
    format('Updated ~w with action ~w in context ~p~n', [Pred, Action, Head]).

% ── Clara synthetic groups (multi-clause predicates) ─────────────────────────
% Declared dynamic so that assertz'd results trigger forward-chaining
% notification. Mirrored as umbrella nodes in the DOT graph.
:- dynamic(not_sprinklers/0).
:- prolog_listen(not_sprinklers/0, updated(not_sprinklers/0)).
:- dynamic(wet_surface/0).
:- prolog_listen(wet_surface/0, updated(wet_surface/0)).
% ── End Clara integration ───────────────────────────────────────────────────

%% ex2.pl - simple forward chaining

:- dynamic(wet/1).
:- dynamic(day_of_week/1).

it_rained :- wet_surface, not_sprinklers.

wet_surface :- wet(ground).
wet_surface :- wet(sidewalk).

not_sprinklers :- wet(_), day_of_week(saturday).
not_sprinklers :- wet(_), day_of_week(sunday).

