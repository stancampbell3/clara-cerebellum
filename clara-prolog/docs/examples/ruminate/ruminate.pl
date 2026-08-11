:- use_module(library(the_cow)).

%% clarify_clara/1 - ruminate_opts/3 returns the parsed response dict
%%   directly; ruminate_status/2 pulls the status atom (`success`/`error`)
%%   out of it.
clarify_clara(Status) :-
    ruminate_opts("what is a clara?", _{llm_provider: ollama, llm_model: "gemma4:e4b"}, Result),
    ruminate_status(Result, Status).

%% clarify_clara_answer/1 - same call, pulling the generated answer text.
clarify_clara_answer(Answer) :-
    ruminate_opts("what is a clara?", _{llm_provider: ollama, llm_model: "gemma4:e4b"}, Result),
    ruminate_answer(Result, Answer).

%% clarify_clara_citations/1 - same call, pulling the citation list.
clarify_clara_citations(Citations) :-
    ruminate_opts("what is a clara?", _{llm_provider: ollama, llm_model: "gemma4:e4b"}, Result),
    ruminate_citations(Result, Citations).
