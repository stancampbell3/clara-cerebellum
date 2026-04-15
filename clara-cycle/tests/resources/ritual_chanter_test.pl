%% ritual_chanter_test.pl
%%
%% Prolog KB for the Phase 6 ritual e2e test.
%%
%% Goal: peer_answered(hello, Answer)
%% This succeeds only after CLIPS relays the Hohi response from the peer
%% evaluator back to Prolog by asserting answered/2.

:- use_module(library(the_coire)).

:- dynamic answered/2.

%% peer_answered(+Prompt, -Answer) — the root deduction goal.
%% Succeeds once answered/2 is asserted (via CLIPS relay from Hohi response).
peer_answered(Prompt, Answer) :- answered(Prompt, Answer).
