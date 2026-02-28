use_module(library(the_rabbit)).
use_module(library(the_coire)).

man(stan).
plan(stan).
man_with_plan(Man) :- man(Man), plan(Man), coire_publish_assert(man_with_plan(Man)).
get_out_the_back(Dude) :- man_with_plan(_), man(Dude), coire_publish_assert(get_out_the_back(Dude)).
