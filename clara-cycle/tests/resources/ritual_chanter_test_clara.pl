% ── Clara integration ────────────────────────────────────────────────────────
updated(Pred, Action, Context) :-
    clause(Head, _Body, Context),
    coire_publish_assert(Head),
    format('Updated ~w with action ~w in context ~p~n', [Pred, Action, Head]).
% ── End Clara integration ────────────────────────────────────────────────────

%% ritual_chanter_test_clara.pl
%%
%% Clara-augmented Prolog KB for the Phase 6 ritual e2e test.
%%
%% Goal: peer_answered(hello, Answer)
%% Succeeds once CLIPS relays answered/2 from the Hohi response.

:- use_module(library(the_coire)).

:- dynamic answered/2.

peer_answered(Prompt, Answer) :- answered(Prompt, Answer).
