:- use_module(library(the_rabbit)).
:- use_module(library(the_rat)).

:- dynamic(visitor/1).

visitor_is_welcome(Visitor) :- visitor(Visitor).

visitor_has_context_json(Visitor, Json) :- visitor(Visitor), the_rabbit:deduce_context_json(Json).
