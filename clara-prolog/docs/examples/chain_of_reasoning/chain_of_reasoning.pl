%% chain_of_reasoning.pl - Example 4 : chain of reasoning, refining deductions

% deduction_request(
% 	prolog_clauses([]),
%	clips_constructs([]),
%	initial_goal('reasoned_response(Response)'),
%	Max_cycles,
%	Trace_flag,
%	Persist_flag)

:- dynamic(prolog_clauses/1).
:- dynamic(clips_constructs/1).
:- dynamic(initial_goal/1).
:- dynamic(deduction_request/6).

deduction_request(prolog_clauses([]),clips_constructs([]),initial_goal('reasoned_response(Prompt, Context, Response)'),5,true,true).

