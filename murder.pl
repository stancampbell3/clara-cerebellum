% murder case


:- dynamic(murder/1).
:- dynamic(murderer_of/2).
:- dynamic(present_at/3).
:- dynamic(capable_of_using/2).
:- dynamic(hates/2).
:- dynamic(cheated_on/2).
:- dynamic(killed_father/2).
:- dynamic(owes_money_to/2).
:- dynamic(is_blackmailing/2).
:- dynamic(core_value/2).
:- dynamic(is_zealot/1).
:- dynamic(is_target_of_group/2).
:- dynamic(is_possessed_of_great_expectations/1).
:- dynamic(is_opposed_to/2).
:- dynamic(is_tabu_for/2).
:- dynamic(member_of/2).
:- dynamic(violates_tabu/2).
:- dynamic(loves/2).
:- dynamic(is_responsible/2).
:- dynamic(fear_of_exposure/2).
:- dynamic(is_obstacle/1).
:- dynamic(looming_deadline/2).
:- dynamic(place/1).
:- dynamic(financially_strained/1).

suspect(dr_bookish).
suspect(lady_pantsuit).
suspect(mr_house).
suspect(lilly_shrinks).

hates(mittens, A) :- suspect(A).
hates(mr_house, lady_pantsuit).
hates(lady_pantsuit, mr_house).
hates(lady_pantsuit, mittens).
hates(lilly, mittens).

place(great_hall).
place(library).
place(kitchen).
place(boathouse).

weapon(knife).
weapon(lead_pipe).
weapon(revolver).

location(knife, place(library)).
location(lead_pipe, place(kitchen)).
location(revolver, place(boathose)).

murderer_of(Suspect, murder(Victim)) :- motive(Suspect, Victim), opportunity(Suspect, Victim, Weapon), capable_of_using(Suspect, Weapon).

opportunity(Suspect, Victim, Weapon) :- murder(Victim),
	suspect(Suspect),
	present_at(Suspect, Where, When), 
	present_at(Victim, Where, When), 
	weapon(Weapon),

capable_of_using(knife, lady_pantsuit).
capable_of_using(lead_pipe, lady_pantsuit).
capable_of_using(mr_house, revolver).
capable_of_using(dr_bookish, revolver).
capable_of_using(dr_bookish, knife).
capable_of_using(lilly_shrinks, Weapon) :- weapon(Weapon).

% motive can be emotional, financial, ideological, or situational
motive(Who, Whom) :- emotional_motive(Who, Whom).
motive(Who, Whom) :- financial_motive(Who, Whom).
motive(Who, Whom) :- ideological_motive(Who, Whom).
motive(Who, Whom) :- situational_motive(Who, Whom).

emotional_motive(Who, Whom) :- hates(Who, Whom).
emotional_motive(Who, Whom) :- cheated_on(Whom, Who).
emotional_motive(Who, Whom) :- killed_father(Whom, Who).

financial_motive(Who, Whom) :- owes_money_to(Who, Whom).
financial_motive(Who, Whom) :- is_blackmailing(Whom, Who).
financial_motive(_, Whom) :- is_possessed_of_great_expectations(Whom).

% principled opposition
ideological_motive(Who, Whom) :- core_value(Who, Something), is_opposed_to(Whom, Something).

% reformist zeal
ideological_motive(Who, Whom) :- is_zealot(Who), is_target_of_group(_, Whom).

% tradition or loyalty
ideological_motive(Who, Whom) :- is_tabu_for(Tabu, Group), 
	violates_tabu(Whom, Tabu), member_of(Who, Group).

% tabus
violates_tabu(Who, Tabu) :- is_tabu_for(Tabu, _), loves(Who, Tabu).

% situational motive
situational_motive(Who, Whom) :- financially_strained(Who), is_obstacle(Whom).
situational_motive(Who, Whom) :- looming_deadline(Who, Deadline), is_responsible(Whom, Deadline).
situational_motive(Who, Whom) :- fear_of_exposure(Who, Dirt), is_responsible(Whom, Dirt).
