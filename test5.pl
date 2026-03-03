% testing forward chaining

:- dynamic(murder/1).
:- dynamic(suspect/1).
:- dynamic(dislikes/2).
:- dynamic(was_rude_to/2).
:- dynamic(member_of/2).
:- dynamic(group/1).

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
prejudiced(Who, Whom, Group) :- group(Group), suspect(Who), murder(Whom), member_of(Who, Group), \+ member_of(Whom, Group), assertz(dislikes(Who, Whom)).

:- assert(group(hubology)).
:- assert(member_of(colonel_mustard, hubology)).