%% ex1.pl - Example 1 : simple backward chaining

:- dynamic(pick_position/1).
:- dynamic(turn/1).
:- dynamic(tumbler/2).

unlocked :- tumbler(1,set), tumbler(2,set), tumbler(3,set).

tumbler_1 :- pick_position(3), turn(left), assert(tumbler(1,set)).
tumbler_2 :- pick_position(1), turn(right), assert(tumbler(2,set)).
tumbler_3 :- pick_position(2), turn(left), assert(tumbler(3,set)).
