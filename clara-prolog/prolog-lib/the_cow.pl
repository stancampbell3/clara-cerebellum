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
    ruminate_mode/3
]).

:- use_module(library(http/json)).
:- use_module(library(the_rabbit), [dict_to_json/2]).

%% ruminate/2 - Query Edgequake's RAG API with default (hybrid) retrieval mode.
ruminate(Query, Result) :-
    ruminate_mode(Query, hybrid, Result).

%% ruminate_with_context/3 - Query Edgequake with conversational context.
%%   Context is a plain string; folded into the query the same way
%%   ponder_text_with_context/3 threads context through to the LLM.
ruminate_with_context(Query, Context, Result) :-
    dict_to_json(_{tool: edgequake,
                   arguments: _{operation: query,
                                query: Query,
                                context: Context,
                                mode: hybrid}}, Json),
    the_rabbit:clara_evaluate(Json, Raw),
    atom_json_dict(Raw, Dict, []),
    ( get_dict(status, Dict, error) ->
        format(user_error, "edgequake tool error: ~w~n", [Dict.message]),
        fail
    ;
        Result = Raw
    ).

%% ruminate_mode/3 - Query Edgequake's RAG API with an explicit retrieval mode.
%%   Mode is one of: naive, local, global, hybrid, mix.
ruminate_mode(Query, Mode, Result) :-
    dict_to_json(_{tool: edgequake,
                   arguments: _{operation: query,
                                query: Query,
                                mode: Mode}}, Json),
    the_rabbit:clara_evaluate(Json, Raw),
    atom_json_dict(Raw, Dict, []),
    ( get_dict(status, Dict, error) ->
        format(user_error, "edgequake tool error: ~w~n", [Dict.message]),
        fail
    ;
        Result = Raw
    ).
