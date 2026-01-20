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