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

%% clarify_clara_asserted/1 - same call, via ruminate_and_assert_citations/3
%%   instead of ruminate_opts/3: asserts every citation as a citation/8 fact
%%   (deduped by id, thread_local — scoped to this one deduction call), links
%%   this specific conclusion to all of them via cite/2, and returns the
%%   linked citation ids. Downstream rules can then look up
%%   citation(CitationId, SourceType, DocumentId, FilePath, StartLine,
%%   EndLine, Score, Snippet) for any id in the list instead of re-parsing
%%   Result.sources themselves.
clarify_clara_asserted(CitationIds) :-
    ruminate_and_assert_citations("what is a clara?",
                                   _{llm_provider: ollama, llm_model: "gemma4:e4b"},
                                   Result),
    ruminate_citations(Result, Sources),
    findall(Id, (member(S, Sources), Id = S.id, cite(clarify_clara, Id)), CitationIds).

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
%%
%% ponder_text/2's envelope shape also isn't constant: sometimes the leaf
%% field is `hohi.response.content` (a reduced/tool-wrapped Hohi), other
%% times it's Ollama's native generate shape with `hohi.response.response`
%% and no `content` key at all — confirmed live 2026-08-20, same call
%% returning one shape or the other depending on evaluator routing, not the
%% query. get_dict/3 with a fallback handles both; plain dot-notation can't
%% (it throws on a missing key rather than trying an alternative).
%%
%% Root-caused 2026-08-21, not random: deterministic per evaluator class
%% (lildaemon's ToolifiedOllamaEvaluator always returns {content,
%% tool_call}; plain OllamaEvaluator always returns {response, ...}) —
%% ponder_text/2 rides whatever evaluator is currently focused on
%% lildaemon's shared GoatWrangler global, so the shape just reflects
%% which one was focused at call time. See lildaemon/goat/app/assistant/
%% rulesets/general_assistant.pl's copy of this comment for the full
%% explanation and how the original focus-clobbering mechanism was
%% removed the same day.
extract_hohi_response(JSON, Response) :-
    atom_json_dict(JSON, Dict, []),
    get_dict(hohi, Dict, D1),
    get_dict(response, D1, D2),
    (   get_dict(content, D2, Response)
    ->  true
    ;   get_dict(response, D2, Response)
    ).

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
