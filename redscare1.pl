%% forward propagation testing

:- use_module(library(the_rabbit)).
:- dynamic(commie_state/1).
:- dynamic(suggestion/1).

bash_the_reds :- commie_state(visible), assertz(suggestion(bash_the_reds)).
