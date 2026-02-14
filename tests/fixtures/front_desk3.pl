% Clara System for Front Desk Interaction Management
% --------------------------------------------------------------
% Known models
known_model(clara_splinter_model, 'hf.co/bartowski/Qwen2.5-14B-Instruct-1M-GGUF:Q4_0').

% dynamic current_messages_context/1.
current_messages_context('[]').

% Switch to the ollama evaluator for LLM queries
use_ollama :-
    clara_evaluate(
        '{"tool":"splinteredmind","arguments":{"operation":"set_evaluator","evaluator":"ollama"}}',
        _Result
    ).

% Switch to echo evaluator for testing
use_echo :-
    clara_evaluate(
        '{"tool":"splinteredmind","arguments":{"operation":"set_evaluator","evaluator":"echo"}}',
        _Result
    ).

% Switch to Clara evaluator for testing
use_clara :-
    clara_evaluate(
        '{"tool":"splinteredmind","arguments":{"operation":"set_evaluator","evaluator":"clara_mind_splinter"}}',
        _Result
    ).

% LLM query with model specification (required for Ollama)
% Available models depend on what you have pulled in Ollama (e.g., llama3.2, mistral, phi3)
ask_llm(Model, Prompt, Response) :-
    format(atom(Json),
        '{"tool":"splinteredmind","arguments":{"operation":"evaluate","data":{"model":"~w","prompt":"~w"}}}',
        [Model, Prompt]),
    clara_evaluate(Json, Response).

% Convenience wrapper with default model
ask_llm(Prompt, Response) :-
    ask_llm('llama3.2', Prompt, Response).

ask_clara_llm(Prompt, Response) :-
    known_model(clara_splinter_model, Model),
    ask_llm(Model, Prompt, Response).

% LLM query with system prompt and model specification
ask_llm_with_system_prompt(Model, SystemPrompt, UserPrompt, Response) :-
    format(atom(Json),
        '{"tool":"splinteredmind","arguments":{"operation":"evaluate","data":{"model":"~w","system":"~w","prompt":"~w"}}}',
        [Model, SystemPrompt, UserPrompt]),
    clara_evaluate(Json, Response).

% Convenience wrapper with default model
ask_llm_with_system_prompt(SystemPrompt, UserPrompt, Response) :-
    ask_llm_with_system_prompt('llama3.2', SystemPrompt, UserPrompt, Response).

% Convenience wrapper for Clara model with system prompt
ask_clara_with_system_prompt(SystemPrompt, UserPrompt, Response) :-
    known_model(clara_splinter_model, Model),
    ask_llm_with_system_prompt(Model, SystemPrompt, UserPrompt, Response).

% Example: Ask as a helpful assistant
% ?- ask_llm_with_system_prompt('You are a helpful coding assistant.',
%                         'How do I reverse a list in Prolog?', R).

% Example usage:
% ?- ask_llm('What is the capital of France?', R).
% ?- ask_llm('mistral', 'Explain quantum computing briefly.', R).

% Convenience wrapper with default model
ask_llm_with_context(Model, SystemPrompt, UserPrompt, MessagesContext, Response) :-
    known_model(_, Model),
    current_messages_context(MessagesContext),
    format(atom(Json),
        '{"tool":"splinteredmind","arguments":{"operation":"evaluate","data":{"model":"~w","system":"~w","user":"~w","messages":~w}}}',
        [Model, SystemPrompt, UserPrompt, MessagesContext]),
    clara_evaluate(Json, Response).

% Example: As front desk receptionist with message context, greeting the visitor.  Using the clara_splinter_model.
example_ask_front_desk(Response) :-
    known_model(clara_splinter_model, Model),
    SystemPrompt = 'You are a front desk receptionist at a tech company. Greet the visitor warmly and professionally.',
    UserPrompt = 'A visitor has just arrived at the front desk.',
    MessagesContext = '[{"role":"system","content":"You are a front desk receptionist at a tech company."},{"role":"user","content":"A visitor has just arrived at the front desk."}]',
    ask_llm_with_context(Model, SystemPrompt, UserPrompt, MessagesContext, Response).

% -----------------------------------------------------------------
reset_clara_session :-
    clara_evaluate(
        '{"tool":"splinteredmind","arguments":{"operation":"reset_session"}}',
        _Result
    ).

% DFA for Front Desk Interaction Management
% -----------------------------------------------------------------
% This DFA manages the states and transitions for a front desk interaction
% using Clara for handling complex questions and user interaction.
% -----------------------------------------------------------------



% Q: states of the machine
% Σ: events that trigger transitions
% δ: transition function
% q0: initial state
% F: set of accepting states
% -----------------------------------------------------------------

% States (Q):
% front_desk
% - greeting
% - question_and_answer
% - collecting_contact_information
% - saying_goodbye
% - next_visitor_please
% -----------------------------------------------------------------
front_desk_state(greeting).
front_desk_state(question_and_answer).
front_desk_state(collecting_contact_information).
front_desk_state(saying_goodbye).

% Events (Σ):
% - visitor_shows_interest_in_services
% - conversation_unclear_or_off_topic
% - visitor_interested_and_wants_to_proceed
% - visitor_is_not_interested
% - contact_info_collected_successfully
% - contact_info_declined_or_failure
% - visitor_leaves
% -----------------------------------------------------------------
event(visitor_shows_interest_in_services).
event(conversation_unclear_or_off_topic).
event(visitor_interested_and_wants_to_proceed).
event(visitor_is_not_interested).
event(contact_info_collected_successfully).
event(contact_info_declined_or_failure).
event(visitor_leaves).
% -----------------------------------------------------------------

% Initial state (q0):
:- dynamic current_state/1.
current_state(greeting).

% Define the action items per state
% Each entry is state, [action items]
action_item(A,_) :- front_desk_state(A).
action_items(greeting, [introduce_company_services, ask_visitor_reason]).
action_items(question_and_answer, [engage_conversation, determine_interest_level]).                                                
action_items(collecting_contact_information, [request_contact_details]).                                                           
action_items(saying_goodbye, [provide_courteous_farewell]).

% An action item is pending if it is in the list of action items for the current state and has not been completed yet
:- dynamic completed_action_item/1.
pending_action_item(Item) :-
    current_state(State),
    action_items(State, Items),
    member(Item, Items),
    \+ completed_action_item(Item). % negation by failure (not completed)

% Mark an action item as completed
complete_action_item(Item) :-
    assertz(completed_action_item(Item)).

% Clear completed action items (e.g., when transitioning to a new state)
clear_completed_action_items :-
    retractall(completed_action_item(_)).
                                                                                                                                   
% Transitions (δ):
% Each transition must be between two states on an event
transition(A,B,C) :- front_desk_state(A),
    event(B),
    front_desk_state(C).
transition(greeting, visitor_shows_interest_in_services, question_and_answer).                                                     
transition(greeting, conversation_unclear_or_off_topic, greeting).                                                                 
transition(question_and_answer, visitor_interested_and_wants_to_proceed, collecting_contact_information).                          
transition(question_and_answer, visitor_is_not_interested, greeting).                                                              
transition(collecting_contact_information, contact_info_collected_successfully, saying_goodbye).                                   
transition(collecting_contact_information, contact_info_declined_or_failure, greeting).
transition(greeting, visitor_leaves, greeting).
transition(question_and_answer, visitor_leaves,  greeting).
transition(collecting_contact_information, visitor_leaves, greeting).
transition(saying_goodbye, visitor_leaves, greeting).

% Final states (F):
% In this DFA, the greeting state is considered an accepting state as it is the starting point for
% new interactions accepting_state(greeting).
final_state(greeting).

% If the system is not idle, then we are in the middle of a conversation (convenience predicate)
system_busy :- \+ system_idle.
system_idle :- final_state(S), current_state(S).

% Transition to the next state based on the current state and event
next_state(Event, NextState) :-
    current_state(CurrentState),
    event(Event),
    transition(CurrentState, Event, NextState).

% -----------------------------------------------------------------
% We keep track of completed and pending action items internally, but we can pre load their values
% from interactions before consulting the evaluator.  additionally, clara-evaluate callbacks can resolve them during rule evaluation.
% we will need to handle making this asynchronous.  Perhaps by returning early, saving a future of the clara-evaluate call in the session.