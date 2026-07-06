:- use_module(library(the_rabbit)).
:- use_module(library(the_rat)).

reasoned_response(Query, Context, Response) :-
    ponder_text_with_context(Query, Context, Response).
