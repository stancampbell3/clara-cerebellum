%% ex2.pl - simple forward chaining

:- dynamic(wet/1).
:- dynamic(day_of_week/1).

it_rained :- wet_surface, not_sprinklers.

wet_surface :- wet(ground).
wet_surface :- wet(sidewalk).

not_sprinklers :- wet(_), day_of_week(saturday).
not_sprinklers :- wet(_), day_of_week(sunday).

