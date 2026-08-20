%% the_cow.pl
%% ----------
%% Bridge to the Edgequake graphical RAG system, via the `edgequake` tool
%% registered in clara-toolbox. Mirrors the_rabbit.pl's shape: build a
%% {tool, arguments} dict, dispatch through clara_evaluate/2, and check the
%% response status before returning.
%%
%% clara_evaluate/2 is registered as a foreign predicate inside the
%% `the_rabbit` module (see clara-prolog/src/backend/ffi/callbacks.rs), so
%% callers outside that module must qualify the call as
%% the_rabbit:clara_evaluate/2.

:- module(the_cow, [
    ruminate/2,
    ruminate_with_context/3,
    ruminate_mode/3,
    ruminate_opts/3,
    ruminate_status/2,
    ruminate_answer/2,
    ruminate_citations/2,
    ruminate_and_assert_citations/3,
    cite/2,
    citation/8,
    cites/2
]).

:- use_module(library(http/json)).
:- use_module(library(the_rabbit), [dict_to_json/2]).

% citation/8 and cites/2 hold per-deduction state (populated by
% ruminate_and_assert_citations/3), so they need the same thread_local
% isolation the_coire.pl's mutable predicates already rely on: each /deduce
% call runs on its own SWI engine, but engines share one global dynamic-
% predicate database unless declared thread_local (see consult_string's doc
% comment in clara-prolog/src/backend/ffi/environment.rs for the full
% mechanism). This also settles citation lifecycle for free: with
% thread_local, asserted citations are naturally scoped to the one
% deduction call that asserted them and never leak into the next.
:- thread_local citation/8.
:- thread_local cites/2.

%% ruminate/2 - Query Edgequake's RAG API with default (hybrid) retrieval mode.
ruminate(Query, Result) :-
    ruminate_mode(Query, hybrid, Result).

%% ruminate_with_context/3 - Query Edgequake with conversational context.
%%   Context is a plain string; folded into the query the same way
%%   ponder_text_with_context/3 threads context through to the LLM.
%%   Result is the parsed response dict (see ruminate_status/2,
%%   ruminate_answer/2, ruminate_citations/2 for common fields).
ruminate_with_context(Query, Context, Result) :-
    dict_to_json(_{tool: edgequake,
                   arguments: _{operation: query,
                                query: Query,
                                context: Context,
                                mode: hybrid}}, Json),
    the_rabbit:clara_evaluate(Json, Raw),
    % value_string_as(atom): without this, atom_json_dict/3 decodes JSON
    % strings as SWI strings, which never unify with the bare atom `error`
    % below — confirmed live that the unqualified [] form silently never
    % detects errors (see the_rabbit.pl's classify_text/2 for the same fix).
    atom_json_dict(Raw, Dict, [value_string_as(atom)]),
    ( get_dict(status, Dict, error) ->
        format(user_error, "edgequake tool error: ~w~n", [Dict.message]),
        fail
    ;
        Result = Dict
    ).

%% ruminate_mode/3 - Query Edgequake's RAG API with an explicit retrieval mode.
%%   Mode is one of: naive, local, global, hybrid, mix.
%%   Result is the parsed response dict (see ruminate_status/2,
%%   ruminate_answer/2, ruminate_citations/2 for common fields).
ruminate_mode(Query, Mode, Result) :-
    dict_to_json(_{tool: edgequake,
                   arguments: _{operation: query,
                                query: Query,
                                mode: Mode}}, Json),
    the_rabbit:clara_evaluate(Json, Raw),
    % value_string_as(atom): without this, atom_json_dict/3 decodes JSON
    % strings as SWI strings, which never unify with the bare atom `error`
    % below — confirmed live that the unqualified [] form silently never
    % detects errors (see the_rabbit.pl's classify_text/2 for the same fix).
    atom_json_dict(Raw, Dict, [value_string_as(atom)]),
    ( get_dict(status, Dict, error) ->
        format(user_error, "edgequake tool error: ~w~n", [Dict.message]),
        fail
    ;
        Result = Dict
    ).

%% ruminate_opts/3 - Query Edgequake with explicit overrides.
%%   Opts is a dict of any of: tenant, workspace, llm_provider, llm_model,
%%   mode, context, max_results — merged over the base {operation, query}
%%   dict via .put/1, so unset keys fall through to the edgequake tool's
%%   configured defaults (EDGEQUAKE_DEFAULT_TENANT/WORKSPACE env vars,
%%   Edgequake's own tenant/workspace default provider+model). Result is
%%   the parsed response dict (see ruminate_status/2, ruminate_answer/2,
%%   ruminate_citations/2 for common fields). Example:
%%     ruminate_opts("what is a qubit?",
%%                   _{llm_provider: ollama, llm_model: "gemma4:e4b"},
%%                   Result)
ruminate_opts(Query, Opts, Result) :-
    Args = _{operation: query, query: Query}.put(Opts),
    dict_to_json(_{tool: edgequake, arguments: Args}, Json),
    the_rabbit:clara_evaluate(Json, Raw),
    atom_json_dict(Raw, Dict, [value_string_as(atom)]),
    ( get_dict(status, Dict, error) ->
        format(user_error, "edgequake tool error: ~w~n", [Dict.message]),
        fail
    ;
        Result = Dict
    ).

%% ruminate_status/2 - Extract the status atom (`success`/`error`) from a
%%   ruminate*/N result dict.
ruminate_status(Result, Status) :-
    Status = Result.status.

%% ruminate_answer/2 - Extract the generated answer string from a
%%   ruminate*/N result dict.
ruminate_answer(Result, Answer) :-
    Answer = Result.answer.

%% ruminate_citations/2 - Extract the list of source/citation dicts from a
%%   ruminate*/N result dict. Each entry carries (per Edgequake's
%%   SourceReference): source_type, id, score, snippet, and, when present,
%%   document_id, file_path, start_line/end_line, chunk_index, etc.
%%   Defaults to [] rather than failing when sources is absent.
ruminate_citations(Result, Citations) :-
    Citations = Result.get(sources, []).

%% ruminate_and_assert_citations/3 - Like ruminate_opts/3, but also asserts
%%   each citation in Result's `sources` list as a citation/8 fact, so
%%   downstream rules/conclusions can point at a citation by its stable id
%%   (cite/2) instead of each embedding its own copy. A separate predicate
%%   from ruminate_opts/3 rather than a side effect of every call: plain
%%   ruminate/2 callers who only want text shouldn't be forced into
%%   mutating the fact base. Dedups by id (Edgequake's own SourceReference.id
%%   doc comment guarantees it's stable/unique) — calling this repeatedly
%%   with overlapping citations across a session does not accumulate
%%   duplicate facts.
%%   citation(Id, SourceType, DocumentId, FilePath, StartLine, EndLine,
%%            Score, Snippet) - fields Edgequake omits (e.g. entity-type
%%   sources have no file/line) are bound to the atom `none`.
ruminate_and_assert_citations(Query, Opts, Result) :-
    ruminate_opts(Query, Opts, Result),
    ruminate_citations(Result, Sources),
    maplist(assert_citation, Sources).

assert_citation(Source) :-
    Id = Source.id,
    ( citation(Id, _, _, _, _, _, _, _)
    -> true
    ;  SourceType = Source.get(source_type, none),
       DocumentId = Source.get(document_id, none),
       FilePath = Source.get(file_path, none),
       StartLine = Source.get(start_line, none),
       EndLine = Source.get(end_line, none),
       Score = Source.get(score, none),
       Snippet = Source.get(snippet, none),
       assertz(citation(Id, SourceType, DocumentId, FilePath, StartLine,
                         EndLine, Score, Snippet))
    ).

%% cite/2 - Record that a conclusion (any caller-chosen id — the rule/
%%   session context knows what conclusion it's drawing, ruminate_opts/3
%%   itself has no notion of "conclusion") rests on a given citation id.
%%   Idempotent: calling cite/2 twice with the same pair asserts one fact,
%%   not two.
cite(ConclusionId, CitationId) :-
    ( cites(ConclusionId, CitationId)
    -> true
    ;  assertz(cites(ConclusionId, CitationId))
    ).
