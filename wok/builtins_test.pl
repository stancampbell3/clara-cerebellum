% Known models
known_model(clara_splinter_model, 'hf.co/bartowski/Qwen2.5-14B-Instruct-1M-GGUF:Q4_0').

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

% Example usage:
% ?- ask_llm('What is the capital of France?', R).
% ?- ask_llm('mistral', 'Explain quantum computing briefly.', R).

% -----------------------------------------------------------------
reset_clara_session :-
    clara_evaluate(
        '{"tool":"splinteredmind","arguments":{"operation":"reset_session"}}',
        _Result
    ).
