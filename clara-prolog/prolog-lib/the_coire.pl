:- module(the_coire, [
    coire_session/1,           % -SessionId
    coire_publish/2,           % +EventType, +DataTerm
    coire_publish_assert/1,    % +Fact
    coire_publish_retract/1,   % +Fact
    coire_publish_goal/1,      % +Goal
    coire_consume/0,
    coire_on_event/1,          % +EventDict — user hook
    caws_squawk/3,             % +TopicPath, +Tags, +Payload
    caws_offer/4,              % +TargetNodeId, +TopicPath, +Payload, -CorrelationId
    caws_await/2,              % +CorrelationId, -Result
    caws_consult/4,            % +TargetNodeId, +TopicPath, +Payload, -Result
    caws_pipe/4,               % +EdgeId, +TargetNodeId, +TopicPath, +IncomingCid
    caws_edge_reply/3,         % +EdgeId, +Kind, +CorrelationId
    caws_emit/4,               % +TargetNodeId, +TopicPath, +Kind, +Payload
    caws_tee/5,                % +EdgeId, +TargetNodeId, +TopicPath, +Kind, +IncomingCid
    caws_edge_message/3        % +EdgeId, +Kind, +CorrelationId
]).

:- use_module(library(http/json)).

% Thread-local fact — one clause per engine, set by Rust at creation.
% SWI-Prolog engines are independent threads for thread_local storage,
% so this is per-engine (not per-OS-thread).
:- thread_local coire_session_id/1.

% Per-engine caws state. Offers are memoized by (Target, Topic, Payload) so
% re-running a goal (the cycle re-queries the root goal when mailboxes drain)
% reuses the outstanding correlation id instead of publishing a duplicate
% Offering; results/failures are cached so a resolved consult stays resolved.
:- thread_local caws_offer_sent/2.   % Key, CorrelationId
:- thread_local caws_result/2.       % CorrelationId, PayloadDict
:- thread_local caws_failed/2.       % CorrelationId, PayloadDict
:- thread_local caws_offering/3.     % Cid, TopicPath, PayloadDict — cached incoming Offerings
:- thread_local caws_piped/2.        % EdgeId, IncomingCid — pipe memo
:- thread_local caws_edge_offer/2.   % EdgeId, OutgoingCid — for timeout attribution
:- thread_local caws_edge_replied/2. % EdgeId, ReplyCid — reply-dispatch memo
:- thread_local caws_emit_sent/2.    % Key, CorrelationId — manual-emit re-run memo
:- thread_local caws_emitted/1.      % CorrelationId — wire-level publish dedup
:- thread_local caws_message/3.      % Kind, CorrelationId, PayloadDict — event/hohi/tabu mirror
:- thread_local caws_teed/2.         % EdgeId, IncomingCid — auto-tee memo
:- thread_local caws_edge_msg_seen/2. % EdgeId, CorrelationId — receive-dispatch memo

% Edge results and user hooks live in user: so authored node source can match
% edge_result/3 and define on_edge_hohi/on_edge_tabu without declarations.
% edge_result/3 MUST be thread_local, not dynamic: dynamic predicates share
% one global clause store across every Prolog engine in the process, so a
% previous deduction's edge_result facts would satisfy a fresh deduction's
% root goal (stale replies leak across runs). thread_local scopes the facts
% to the engine, like the caws_* caches above. The hooks stay dynamic —
% they're authored definitions, not per-run state — declared only so calling
% them with no clause fails cleanly instead of raising existence errors.
:- thread_local user:edge_result/3.  % EdgeId, hohi|tabu, PayloadDict
:- dynamic user:on_edge_hohi/2.      % user-overridable hooks
:- dynamic user:on_edge_tabu/2.

% Same thread_local-vs-dynamic split as edge_result/3 above, for the
% event/hohi/tabu message edges (docs/ritual_edge_messages.md): edge_message/3
% is per-run state, on_edge_message/3 is an authored, process-wide hook.
:- thread_local user:edge_message/3. % EdgeId, event|hohi|tabu, PayloadDict
:- dynamic user:on_edge_message/3.   % user-overridable hook

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

% Consume: poll inbound events for this session (origin "relay-*"), dispatch each.
% Self-emitted events (origin "prolog") are intentionally left in the mailbox
% so the Rust relay can forward them to the paired CLIPS engine.
coire_consume :-
    coire_session(Session),
    coire_poll_inbound(Session, Json),
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
% json_read_dict produces SWI-Prolog strings for JSON string values, but
% coire_dispatch_type clauses use atoms.  Normalise both fields here so that
% the dispatch pattern-matches correctly regardless of which JSON reader was used.
coire_builtin_handle(Payload) :-
    (get_dict(type, Payload, Type0), get_dict(data, Payload, Data0) ->
        (string(Type0) -> atom_string(Type, Type0) ; Type = Type0),
        (string(Data0) -> atom_string(Data, Data0) ; Data = Data0),
        coire_dispatch_type(Type, Data)
    ; true).

coire_dispatch_type(assert,  D) :- !, term_to_atom(Fact, D), assertz(user:Fact).
coire_dispatch_type(retract, D) :- !, term_to_atom(Fact, D), (retract(user:Fact) -> true ; true).
coire_dispatch_type(goal,    D) :- !, term_to_atom(Goal, D), (user:call(Goal) -> true ; true).
coire_dispatch_type(_, _).

% User-extensible hook. Define coire_on_event/1 clauses to intercept events
% before built-in dispatch. Succeeding skips built-in handling.
:- discontiguous coire_on_event/1.

% ── caws: typed edge messaging (docs/deduction_redux.md) ─────────────────────
%
% caws_offer/4 publishes an addressed, correlated Offering onto the Ritual's
% Coire channel; caws_await/2 resolves it against the correlated Hohi/Tabu
% (or the per-offer patience timeout, which fails the await — timeout to
% false). caws_consult/4 is the request/response pair generated for a graph
% edge. caws_squawk/3 is fire-and-forget on a logical topic path.
%
% The `_caws` payload block is lifted onto the TephraEnvelope routing fields
% by the cycle controller's publish_evaluator_events.

% Normalize a payload argument to a dict: accepts a dict or json([K=V,...]).
caws_payload_dict(Payload, Payload) :-
    is_dict(Payload), !.
caws_payload_dict(json(Pairs), Dict) :-
    !,
    maplist([K=V, K-V]>>true, Pairs, KVs),
    dict_pairs(Dict, json, KVs).

%!  caws_offer(+TargetNodeId, +TopicPath, +Payload, -CorrelationId)
%
%   Publish an Offering addressed to TargetNodeId on logical channel
%   TopicPath. Payload is a dict or json([K=V,...]) — e.g.
%   _{prompt: Question} for a plain evaluator, or
%   _{goal: Goal, context: Context} for a deduce-capable peer.
%   Idempotent per (TargetNodeId, TopicPath, Payload) within one engine.
caws_offer(Target, Topic, Payload, Cid) :-
    caws_payload_dict(Payload, Dict0),
    Key = offer(Target, Topic, Dict0),
    (   caws_offer_sent(Key, Cid0)
    ->  Cid = Cid0
    ;   caws_uuid(Cid),
        put_dict('_caws', Dict0,
                 _{correlation_id: Cid, target_node_id: Target, topic_path: Topic},
                 Dict),
        atom_json_dict(Json, Dict, []),
        coire_session(Session),
        coire_emit(Session, 'evaluator/offering', Json),
        assertz(caws_offer_sent(Key, Cid))
    ).

%!  caws_squawk(+TopicPath, +Tags, +Payload)
%
%   Fire-and-forget publish on a logical topic path with a list of tags.
%   Does not expect (or wait for) a reply and never blocks convergence.
caws_squawk(Topic, Tags, Payload) :-
    caws_payload_dict(Payload, Dict0),
    put_dict('_caws', Dict0,
             _{topic_path: Topic, tags: Tags, expects_reply: false},
             Dict),
    atom_json_dict(Json, Dict, []),
    coire_session(Session),
    coire_emit(Session, 'evaluator/squawk', Json).

%!  caws_await(+CorrelationId, -Result)
%
%   Resolve an outstanding caws_offer. Succeeds binding Result to the
%   correlated Hohi payload dict; fails on the correlated Tabu or the
%   patience timeout (timeout-to-false), or when no response has arrived
%   yet — the cycle re-runs the goal once the response lands.
caws_await(Cid, Result) :-
    (   caws_result(Cid, R)
    ->  Result = R
    ;   caws_failed(Cid, _)
    ->  fail
    ;   caws_drain_ritual_events,
        caws_result(Cid, R),
        Result = R
    ).

%!  caws_consult(+TargetNodeId, +TopicPath, +Payload, -Result)
%
%   Request/response round trip to a peer node: offer + await.
caws_consult(Target, Topic, Payload, Result) :-
    caws_offer(Target, Topic, Payload, Cid),
    caws_await(Cid, Result).

% Drain ritual/* mailbox events (correlated Hohi/Tabu/timeouts written by
% the cycle controller's ingest_tephra) into the per-engine caws cache.
% Only ritual/-prefixed origins are polled, so this can never starve the
% Prolog↔CLIPS relay or coire_consume.
caws_drain_ritual_events :-
    coire_session(Session),
    coire_poll_ritual(Session, Json),
    setup_call_cleanup(
        open_string(Json, Stream),
        (json_read_dict(Stream, Events, []),
         maplist(caws_cache_event, Events)),
        close(Stream)).

caws_cache_event(Event) :-
    (   get_dict(origin, Event, Origin0),
        get_dict(payload, Event, Payload),
        is_dict(Payload),
        get_dict('_routing', Payload, Routing),
        get_dict(correlation_id, Routing, Cid0)
    ->  (string(Origin0) -> atom_string(Origin, Origin0) ; Origin = Origin0),
        (string(Cid0)    -> atom_string(Cid, Cid0)       ; Cid = Cid0),
        caws_cache_by_origin(Origin, Cid, Payload)
    ;   true  % uncorrelated/foreign event — not caws traffic, ignore
    ).

caws_cache_by_origin('ritual/hohi', Cid, Payload) :- !,
    (caws_result(Cid, _) -> true ; assertz(caws_result(Cid, Payload))),
    (caws_message(hohi, Cid, _) -> true ; assertz(caws_message(hohi, Cid, Payload))).
caws_cache_by_origin('ritual/tabu', Cid, Payload) :- !,
    (caws_failed(Cid, _) -> true ; assertz(caws_failed(Cid, Payload))),
    (caws_message(tabu, Cid, _) -> true ; assertz(caws_message(tabu, Cid, Payload))).
caws_cache_by_origin('ritual/tabu-timeout', Cid, Payload) :- !,
    % Local-only wire label (never published) — feeds caws_failed for
    % caws_await/caws_tee(tabu) but intentionally NOT caws_message: the
    % tabu-tee's timeout trigger republishes via the ritual/tabu wire label
    % (see caws_tee/5), so there is no separate "tabu_timeout" message kind.
    (caws_failed(Cid, _) -> true ; assertz(caws_failed(Cid, Payload))).
% Manually-emitted or teed application messages on event/hohi/tabu edges
% (docs/ritual_edge_messages.md). Cached the same way incoming Offerings are:
% the drain is shared between caws_edge_message and the auto-tee path.
caws_cache_by_origin('ritual/event', Cid, Payload) :- !,
    (caws_message(event, Cid, _) -> true ; assertz(caws_message(event, Cid, Payload))).
% Incoming Offerings are cached too (not dropped): the drain is shared between
% caws_await and the auto-pipe path, so whichever drains first must not eat
% the payload the other needs.
caws_cache_by_origin('ritual/offering', Cid, Payload) :- !,
    (   caws_offering(Cid, _, _)
    ->  true
    ;   (   get_dict('_routing', Payload, R), get_dict(topic_path, R, T0)
        ->  (string(T0) -> atom_string(Topic, T0) ; Topic = T0)
        ;   Topic = ''
        ),
        assertz(caws_offering(Cid, Topic, Payload))
    ).
caws_cache_by_origin(_, _, _).

%!  caws_pipe(+EdgeId, +TargetNodeId, +TopicPath, +IncomingCid)
%
%   Auto-pipe: forward the cached incoming Offering IncomingCid along an
%   auto edge as a fresh addressed Offering to TargetNodeId. No await —
%   the controller's pending_offers entry blocks convergence until the
%   correlated Hohi/Tabu (or patience timeout) arrives; the reply is
%   dispatched by caws_edge_reply/3. Idempotent per (EdgeId, IncomingCid).
%   Always succeeds (a not-yet-cached payload is retried on the next event).
caws_pipe(EdgeId, _, _, Cid) :-
    caws_piped(EdgeId, Cid), !.
caws_pipe(EdgeId, Target, Topic, Cid) :-
    caws_drain_ritual_events,
    (   caws_offering(Cid, _, Payload0)
    ->  caws_strip_routing(Payload0, Payload),
        caws_offer(Target, Topic, Payload, OfferCid),
        assertz(caws_piped(EdgeId, Cid)),
        assertz(caws_edge_offer(EdgeId, OfferCid))
    ;   true
    ).

%!  caws_edge_reply(+EdgeId, +Kind, +Cid)
%
%   Reply dispatch for an edge's Hohi/Tabu (Kind = hohi | tabu |
%   tabu_timeout). Asserts user:edge_result(EdgeId, hohi|tabu, Payload)
%   exactly once per (EdgeId, Cid) and runs the user hook when defined.
%   tabu_timeout events carry no topic, so they are attributed only to
%   edges that piped the timed-out offer (caws_edge_offer/2).
caws_edge_reply(EdgeId, _, Cid) :-
    caws_edge_replied(EdgeId, Cid), !.
caws_edge_reply(EdgeId, hohi, Cid) :- !,
    caws_drain_ritual_events,
    (   caws_result(Cid, Payload0)
    ->  caws_dispatch_edge_result(EdgeId, hohi, Cid, Payload0)
    ;   true
    ).
caws_edge_reply(EdgeId, tabu, Cid) :- !,
    caws_drain_ritual_events,
    (   caws_failed(Cid, Payload0)
    ->  caws_dispatch_edge_result(EdgeId, tabu, Cid, Payload0)
    ;   true
    ).
caws_edge_reply(EdgeId, tabu_timeout, Cid) :- !,
    caws_drain_ritual_events,
    (   caws_edge_offer(EdgeId, Cid), caws_failed(Cid, Payload0)
    ->  caws_dispatch_edge_result(EdgeId, tabu, Cid, Payload0)
    ;   true
    ).
caws_edge_reply(_, _, _).

caws_dispatch_edge_result(EdgeId, Kind, Cid, Payload0) :-
    caws_strip_routing(Payload0, Payload),
    assertz(caws_edge_replied(EdgeId, Cid)),
    assertz(user:edge_result(EdgeId, Kind, Payload)),
    (   Kind == hohi
    ->  ignore(catch(user:on_edge_hohi(EdgeId, Payload), _, true))
    ;   ignore(catch(user:on_edge_tabu(EdgeId, Payload), _, true))
    ).

% Drop the controller-merged `_routing` block so handlers and edge_result/3
% consumers see the peer's clean payload.
caws_strip_routing(Payload0, Payload) :-
    (   is_dict(Payload0), del_dict('_routing', Payload0, _, P1)
    ->  Payload = P1
    ;   Payload = Payload0
    ).

% ── caws: event/hohi/tabu message edges (docs/ritual_edge_messages.md) ──────
%
% caws_emit/4 (and the manual `emit_*` helpers generated on message edges)
% publish a fire-and-forget, addressed, correlated message — like an
% Offering but with expects_reply:false, so it never registers a
% PendingOffer or blocks convergence. caws_tee/5 is the auto-mode
% forward-chaining counterpart: it republishes an already-cached inbound
% Hohi/Tabu/event payload onward to another node, preserving the incoming
% correlation id (so relay chains, e.g. A->B->C, stay traceable end to
% end). caws_edge_message/3 is the receive-side dispatch, generated on the
% target of every event/hohi/tabu edge.

%!  caws_emit(+TargetNodeId, +TopicPath, +Kind, +Payload)
%
%   Manual emit entry point (the generated `emit_*_<kind>/1` helpers).
%   Mints a fresh correlation id, memoized per (Target, Topic, Kind,
%   Payload) within one engine so a re-run of the goal does not re-publish.
caws_emit(Target, Topic, Kind, Payload) :-
    caws_payload_dict(Payload, Dict0),
    Key = emit(Target, Topic, Kind, Dict0),
    (   caws_emit_sent(Key, _)
    ->  true
    ;   caws_uuid(Cid),
        caws_emit_cid(Target, Topic, Kind, Dict0, Cid),
        assertz(caws_emit_sent(Key, Cid))
    ).

%!  caws_emit_cid(+TargetNodeId, +TopicPath, +Kind, +Payload, +Cid)
%
%   Core publisher shared by caws_emit/4 (fresh Cid) and caws_tee/5
%   (preserved incoming Cid). Idempotent per Cid (wire-level dedup) —
%   load-bearing for the tee path, where the same Cid may be offered to
%   more than one call site.
caws_emit_cid(Target, Topic, Kind, Payload, Cid) :-
    (   caws_emitted(Cid)
    ->  true
    ;   caws_payload_dict(Payload, Dict0),
        put_dict('_caws', Dict0,
                 _{correlation_id: Cid, target_node_id: Target, topic_path: Topic,
                   kind: Kind, expects_reply: false},
                 Dict),
        atom_json_dict(Json, Dict, []),
        coire_session(Session),
        coire_emit(Session, 'evaluator/emit', Json),
        assertz(caws_emitted(Cid))
    ).

%!  caws_tee(+EdgeId, +TargetNodeId, +TopicPath, +Kind, +IncomingCid)
%
%   Auto-tee: forward the cached inbound event/Hohi/Tabu payload for
%   IncomingCid onward to TargetNodeId on TopicPath, preserving the
%   correlation id. Idempotent per (EdgeId, IncomingCid). Always succeeds
%   (a not-yet-cached payload is retried on the next event) — same shape
%   as caws_pipe/4.
caws_tee(EdgeId, _, _, _, Cid) :-
    caws_teed(EdgeId, Cid), !.
caws_tee(EdgeId, Target, Topic, Kind, Cid) :-
    caws_drain_ritual_events,
    (   caws_tee_payload(Kind, Cid, Payload0)
    ->  caws_strip_routing(Payload0, Payload),
        assertz(caws_teed(EdgeId, Cid)),
        caws_emit_cid(Target, Topic, Kind, Payload, Cid)
    ;   true
    ).

% Payload lookup by kind for the tee: hohi/tabu reuse the existing
% caws_result/caws_failed caches (populated by caws_cache_by_origin,
% including the tabu-timeout->caws_failed path — the wire label the tee
% republishes with is always `tabu`, never a timeout-specific label, per
% the TephraEnvelope wire contract); event uses the new caws_message cache.
caws_tee_payload(hohi, Cid, Payload)  :- caws_result(Cid, Payload).
caws_tee_payload(tabu, Cid, Payload)  :- caws_failed(Cid, Payload).
caws_tee_payload(event, Cid, Payload) :- caws_message(event, Cid, Payload).

%!  caws_edge_message(+EdgeId, +Kind, +Cid)
%
%   Receive dispatch for an edge's event/Hohi/Tabu message. Asserts
%   user:edge_message(EdgeId, Kind, Payload) exactly once per (EdgeId, Cid)
%   and runs the user:on_edge_message/3 hook when defined. Always
%   succeeds (mirrors caws_edge_reply/3).
caws_edge_message(EdgeId, _, Cid) :-
    caws_edge_msg_seen(EdgeId, Cid), !.
caws_edge_message(EdgeId, Kind, Cid) :-
    caws_drain_ritual_events,
    (   caws_message(Kind, Cid, Payload0)
    ->  caws_strip_routing(Payload0, Payload),
        assertz(caws_edge_msg_seen(EdgeId, Cid)),
        assertz(user:edge_message(EdgeId, Kind, Payload)),
        ignore(catch(user:on_edge_message(EdgeId, Kind, Payload), _, true))
    ;   true
    ).
