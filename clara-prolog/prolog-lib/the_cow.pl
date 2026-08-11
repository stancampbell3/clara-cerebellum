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
    ruminate_citations/2
]).

:- use_module(library(http/json)).
:- use_module(library(the_rabbit), [dict_to_json/2]).

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
