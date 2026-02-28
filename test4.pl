use_module(library(the_rabbit)).
use_module(library(the_coire)).

man(stan).
plan(stan).
man_with_plan(Man) :- man(Man), plan(Man), coire_publish_assert(man_with_plan(Man)).
