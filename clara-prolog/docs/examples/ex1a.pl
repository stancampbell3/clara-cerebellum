%% ex1a.pl - Example 1a : backward chaining with forward chaining, unlock it

:- dynamic(pick_position/1).
:- dynamic(turn/1).
:- dynamic(tumbler/2).
:- dynamic(suggestion/1).

unlocked :- tumbler(1,set), tumbler(2,set), tumbler(3,set).

tumbler_1 :- pick_position(3), turn(left), assert(tumbler(1,set)).
tumbler_2 :- pick_position(1), turn(right), assert(tumbler(2,set)).
tumbler_3 :- pick_position(2), turn(left), assert(tumbler(3,set)).

suggest(Suggestion) :- tumbler_1, \+ tumbler_2, Suggestion = 'try attach formation alpha', assert(pick_position(1)), assert(turn(right)).
suggest(Suggestion) :- tumbler_2, Suggestion = 'try attach formation beta', assert(pick_position(2)), assert(turn(left)).

