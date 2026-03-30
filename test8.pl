:- use_module(library(the_rabbit)).
:- use_module(library(the_rat)).

:- dynamic(visitor/1).
:- dynamic(confused/1).

visitor_is_welcome(Visitor) :- visitor(Visitor).

visitor_has_context_json(Visitor, Json) :- visitor(Visitor), the_rabbit:deduce_context_json(Json).

detect_visitor_confused(Visitor) :- visitor(Visitor),
   the_rabbit:current_context(Context),
   clara_fy("Does the visitor seem confused or lost?", Context, R),
   R == true,
   assertz(confused(Visitor)).

suggestion(Visitor, 'Suggest sedation and taking a seat.') :-
   visitor(Visitor),
   confused(Visitor).

suggestion(Visitor, 'Summon the medical devils.') :-
   detect_visitor_confused(Visitor).
