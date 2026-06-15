%% chain_of_reasoning.pl - Example 4 : chain of reasoning, reasonable generation

:- use_module(library(the_rabbit)).
:- use_module(library(http/json)).

:- dynamic(prompt/1).
:- dynamic(current_context/1).

reasoned_response(prompt(Prompt), Response) :- current_context(Context), 
	clara_fy(Prompt, Context, TruthValue)


