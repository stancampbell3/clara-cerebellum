%% test9 : verify a derived fact is passed through the entire deduction cycle back to the prolog engine

:- use_module(library(the_rabbit)).
:- use_module(library(the_rat)).

:- dynamic(visitor/1).
:- dynamic(egg/1).

break_some :- egg(unbroken),
  assertz(egg(broken)).

omelette(Visitor, Dish) :- visitor(Visitor),
  egg(broken),
  Dish = lovely_fluffy_goodness.


  
	