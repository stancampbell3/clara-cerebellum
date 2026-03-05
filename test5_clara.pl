% ── Clara integration (auto-generated) ──────────────────────────────────────
:- prolog_listen(murder/1, updated(murder/1)).
:- prolog_listen(suspect/1, updated(suspect/1)).
:- prolog_listen(dislikes/2, updated(dislikes/2)).
:- prolog_listen(was_rude_to/2, updated(was_rude_to/2)).
:- prolog_listen(member_of/2, updated(member_of/2)).
:- prolog_listen(group/1, updated(group/1)).

updated(Pred, Action, Context) :-
    clause(Head, _Body, Context),
    coire_publish_assert(Head),
    format('Updated ~w with action ~w in context ~p~n', [Pred, Action, Head]).
% ── End Clara integration ───────────────────────────────────────────────────

% testing forward chaining
:- use_module(library(the_rabbit)).
:- use_module(library(the_coire)).

:- dynamic(murder/1).
:- prolog_listen(murder/1, updated(murder/1)).
:- dynamic(suspect/1).
:- prolog_listen(suspect/1, updated(suspect/1)).
:- dynamic(dislikes/2).
:- prolog_listen(dislikes/2, updated(dislikes/2)).
:- dynamic(was_rude_to/2).
:- prolog_listen(was_rude_to/2, updated(was_rude_to/2)).
:- dynamic(member_of/2).
:- prolog_listen(member_of/2, updated(member_of/2)).
:- dynamic(group/1).
:- prolog_listen(group/1, updated(group/1)).

updated(Pred, Action, Context) :-
    clause(Head, _Body, Context),
    coire_publish_assert(Head),
    format('Updated ~w with action ~w in context ~p~n', [Pred, Action, Head]).

murder(mittens).

suspect(lady_pantsuit).
suspect(colonel_mustard).

accuse(Suspect, Victim) :- murder(Victim), suspect(Suspect), motive(Suspect, Victim), opportunity(Suspect, Victim), capable(Suspect).

motive(Suspect, Victim) :- suspect(Suspect), murder(Victim), dislikes(Suspect, Victim).


% testing all capable
capable(A) :- suspect(A).

% testing all have opportunity
opportunity(A, B) :- suspect(A), murder(B).

dislikes(lady_pantsuit, mittens).

% testing 
% outmembers are disliked
prejudiced(Who, Whom, Group) :- dislikes(Who, Whom), group(Group), member_of(Whom, Group).
prejudiced(Who, Whom, Group) :- group(Group), suspect(Who), murder(Whom), member_of(Who, Group), \+ member_of(Whom, Group), assertz(dislikes(Who, Whom)).

:- assert(group(hubology)).
:- assert(member_of(colonel_mustard, hubology)).