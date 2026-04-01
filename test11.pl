:- use_module(library(the_rabbit)).
:- use_module(library(the_rat)).
:- dynamic(suggestion/1).

somethings_fishy(Context) :-
  format('denmark?~n'), 
  clara_fy('can apples be green? please answer yes, no, or unresolved', Context, R1), R1 == true.

suggest(Suggestion) :- the_rabbit:current_context(Context),
  format('we have context~n'),
  somethings_fishy(Context),
  Suggestion = 'run for the hills!',
  assertz(suggestion(Suggestion)).
