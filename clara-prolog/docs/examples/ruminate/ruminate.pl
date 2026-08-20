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

extract_status(JSON, Status) :-
    atom_json_dict(JSON, Dict, []),
    Status = Dict.status.

extract_hohi(JSON, Hohi) :-
  atom_json_dict(JSON, Dict, []),
  Hohi = Dict.hohi.

%% NOTE: this dot-notation form only works because this file is normally
%% `consult`ed (SWI expands Dict.key.key2 at clause-compile time). Copied
%% verbatim into a hand-authored `prolog_clauses` list (loaded via `assert`,
%% not `consult`), it silently mis-evaluates instead of throwing or failing —
%% use get_dict/3 there instead. Confirmed live 2026-08-19, see
%% clara-cerebellum/docs/ritual_rumination_answer_bugs_found.md, bug #4.
extract_hohi_response(JSON, Response) :-
    atom_json_dict(JSON, Dict, []),
    Response = Dict.hohi.response.content.

grounded_ponder(Query, EdgeAnswer) :- ruminate_opts(Query, _{llm_provider: ollama, llm_model: "gemma4:e4b"}, R1),
  ruminate_answer(R1, EdgeAnswer),
  string_concat("Given this query: ", Query, T1),
  string_concat(" does the following response answer the question?\nResponse:\n", T1, T2),
  string_concat(T2, EdgeAnswer, Qa),
  clara_fy(Qa, Tv),
  Tv == true.

considered_response(Query, Response) :- grounded_ponder(Query, Response).
considered_response(Query, Response) :- ponder_text(Query, R1),
  extract_hohi_response(R1, Response),
  string_concat("Given this query: ", Query, T1),
  string_concat(" does the following response answer the question?\nResponse:\n", T1, T2),
  string_concat(T2, Response, Qa),
  clara_fy(Qa, Tv),
  Tv == true.
