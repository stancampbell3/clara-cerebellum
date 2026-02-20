:- module(the_coire, [
    coire_session/1,           % -SessionId
    coire_publish/2,           % +EventType, +DataTerm
    coire_publish_assert/1,    % +Fact
    coire_publish_retract/1,   % +Fact
    coire_publish_goal/1,      % +Goal
    coire_consume/0,
    coire_on_event/1           % +EventDict — user hook
]).

:- use_module(library(http/json)).

% Thread-local fact — one clause per engine, set by Rust at creation.
% SWI-Prolog engines are independent threads for thread_local storage,
% so this is per-engine (not per-OS-thread).
:- thread_local coire_session_id/1.

coire_session(Id) :- coire_session_id(Id).

% Publish: serialize DataTerm to atom, wrap in typed JSON, call coire_emit/3.
coire_publish(Type, DataTerm) :-
    coire_session(Session),
    term_to_atom(DataTerm, DataAtom),
    atom_json_dict(Json, _{type: Type, data: DataAtom}, []),
    coire_emit(Session, prolog, Json).

coire_publish_assert(Fact)  :- coire_publish(assert,  Fact).
coire_publish_retract(Fact) :- coire_publish(retract, Fact).
coire_publish_goal(Goal)    :- coire_publish(goal,    Goal).

% Consume: poll events for this session, dispatch each one.
coire_consume :-
    coire_session(Session),
    coire_poll(Session, Json),
    setup_call_cleanup(
        open_string(Json, Stream),
        (json_read_dict(Stream, Events, []),
         maplist(coire_dispatch_event, Events)),
        close(Stream)).

% Dispatch one ClaraEvent dict.
coire_dispatch_event(Event) :-
    (get_dict(payload, Event, Payload) ->
        (coire_on_event(Payload) -> true ; coire_builtin_handle(Payload))
    ; true).

% Built-in handlers keyed on payload.type.
coire_builtin_handle(Payload) :-
    (get_dict(type, Payload, Type), get_dict(data, Payload, Data) ->
        coire_dispatch_type(Type, Data)
    ; true).

coire_dispatch_type(assert,  D) :- !, term_to_atom(Fact, D), assertz(Fact).
coire_dispatch_type(retract, D) :- !, term_to_atom(Fact, D), (retract(Fact) -> true ; true).
coire_dispatch_type(goal,    D) :- !, term_to_atom(Goal, D), (call(Goal) -> true ; true).
coire_dispatch_type(_, _).

% User-extensible hook. Define coire_on_event/1 clauses to intercept events
% before built-in dispatch. Succeeding skips built-in handling.
:- discontiguous coire_on_event/1.
