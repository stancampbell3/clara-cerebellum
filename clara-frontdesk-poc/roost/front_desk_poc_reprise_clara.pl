% ── Clara integration (auto-generated) ──────────────────────────────────────
:- prolog_listen(visitor/1, updated(visitor/1)).
:- prolog_listen(summoned_by/2, updated(summoned_by/2)).
:- prolog_listen(has_artifact/2, updated(has_artifact/2)).
:- prolog_listen(urgent_message/1, updated(urgent_message/1)).
:- prolog_listen(stopped_elsewhere/1, updated(stopped_elsewhere/1)).
:- prolog_listen(carries_flamefruit/1, updated(carries_flamefruit/1)).
:- prolog_listen(after_sundown/0, updated(after_sundown/0)).
:- prolog_listen(has_critical_info/1, updated(has_critical_info/1)).
:- prolog_listen(performed_task/1, updated(performed_task/1)).
:- prolog_listen(lost_or_confused/1, updated(lost_or_confused/1)).
:- prolog_listen(greeted/1, updated(greeted/1)).

updated(Pred, Action, Context) :-
    clause(Head, _Body, Context),
    coire_publish_assert(Head),
    format('Updated ~w with action ~w in context ~p~n', [Pred, Action, Head]).

% ── Clara synthetic groups (multi-clause predicates) ─────────────────────────
% Declared dynamic so that assertz'd results trigger forward-chaining
% notification. Mirrored as umbrella nodes in the DOT graph.
:- dynamic(admit/2).
:- prolog_listen(admit/2, updated(admit/2)).
:- dynamic(redirect/3).
:- prolog_listen(redirect/3, updated(redirect/3)).
:- dynamic(suggestion/2).
:- prolog_listen(suggestion/2, updated(suggestion/2)).
% ── End Clara integration ───────────────────────────────────────────────────

%% City of Dis Front Desk Logic (source for Clara transducer)

:- use_module(library(the_rabbit)).
:- use_module(library(the_rat)).

%% Dynamic facts that may be asserted via coire relay.
:- dynamic(visitor/1).
:- dynamic(summoned_by/2).
:- dynamic(has_artifact/2).
:- dynamic(urgent_message/1).
:- dynamic(stopped_elsewhere/1).
:- dynamic(carries_flamefruit/1).
:- dynamic(after_sundown/0).
:- dynamic(has_critical_info/1).
:- dynamic(performed_task/1).
:- dynamic(lost_or_confused/1).
:- dynamic(greeted/1).

%% Application entry point — called once per conversation turn.
%% Decision ∈ { admitted, redirected, pending }.
%% Suggestions is a list of guidance atoms for Agent Minos.
%% Reason and Where carry the grounds for the decision.
daemonic_turn(Visitor, Suggestions, Decision, Reason, Where) :-
    visitor(Visitor),
    findall(S, suggestion(Visitor, S), Suggestions),
    (   admit(Visitor, R)        -> Decision = admitted,   Reason = R, Where = ''
    ;   redirect(Visitor, R, W)  -> Decision = redirected, Reason = R, Where = W
    ;                               Decision = pending,    Reason = '', Where = ''
    ).

%% Ask the LLM whether a condition holds, given conversation context.
meets_condition(Visitor, Question) :-
    visitor(Visitor),
    the_rabbit:current_context(Context),
    format('Asking Clara: ~w~nContext: ~w~n', [Question, Context]),
    clara_fy(Question, Context, R),
    R == true.

%% ----------------------------------------------------------------------
%%  Admittance Rules (LLM-augmented, pure — no side effects)
%%  First matching rule wins via -> in daemonic_turn/5.
%% ----------------------------------------------------------------------

%% Rule 1: Summoned visitors with 3 artifacts OR who claim to have them.
admit(Visitor, 'Summoned visitor bearing the required artifacts.') :-
    visitor(Visitor),
    summoned_by(Visitor, _Official),
    (   findall(A, has_artifact(Visitor, A), Arts), length(Arts, N), N >= 3
    ;   meets_condition(Visitor, "Does the visitor claim to possess the three required artifacts for their summons?")
    ).

%% Rule 2: Urgent message + came directly, without prior stops.
admit(Visitor, 'Bearer of an urgent message who came directly, without prior stops.') :-
    visitor(Visitor),
    urgent_message(Visitor),
    (   \+ stopped_elsewhere(Visitor)
    ;   meets_condition(Visitor, "Does the visitor assert they came directly here without stopping elsewhere?")
    ).

%% Rule 3: Flamefruit carrier, arriving before sundown.
admit(Visitor, 'Flamefruit carrier arriving before sundown.') :-
    visitor(Visitor),
    (   carries_flamefruit(Visitor)
    ;   meets_condition(Visitor, "Is the visitor carrying the rare Flamefruit?")
    ),
    \+ after_sundown.

%% Rule 4: Critical intelligence + demonstrated reliability.
admit(Visitor, 'Bearer of critical intelligence who has proven reliability.') :-
    visitor(Visitor),
    (   has_critical_info(Visitor)
    ;   meets_condition(Visitor, "Does the visitor claim to possess critical information for the City?")
    ),
    (   performed_task(Visitor)
    ;   meets_condition(Visitor, "Has the visitor demonstrated reliability by completing the requested task?")
    ).

%% ----------------------------------------------------------------------
%%  Redirect Rules (LLM-augmented, pure — no side effects)
%%  Checked after all admit rules fail.
%% ----------------------------------------------------------------------

%% Lost or confused visitors cannot proceed; send to the map kiosk.
redirect(Visitor, 'This visitor appears lost or confused and cannot proceed.', 'nearest map kiosk') :-
    visitor(Visitor),
    (   lost_or_confused(Visitor)
    ;   meets_condition(Visitor, "Does the visitor appear lost or confused about where they are or why they are here?")
    ).

%% ----------------------------------------------------------------------
%%  Suggestion Rules (pure queries — inform Agent Minos, do not affect Decision)
%% ----------------------------------------------------------------------

suggestion(Visitor, 'Greet the visitor.') :-
    visitor(Visitor),
    \+ greeted(Visitor).

suggestion(Visitor, 'Direct the visitor to the nearest map kiosk.') :-
    visitor(Visitor),
    (   lost_or_confused(Visitor)
    ;   meets_condition(Visitor, "Based on the conversation so far, does the visitor seem lost or confused?")
    ).

suggestion(Visitor, 'Request the three required artifacts for summoned visitors.') :-
    visitor(Visitor),
    summoned_by(Visitor, _),
    findall(A, has_artifact(Visitor, A), Artifacts),
    length(Artifacts, N),
    N < 3.

suggestion(Visitor, 'Verify that the visitor made no prior stops before delivering their urgent message.') :-
    visitor(Visitor),
    urgent_message(Visitor),
    \+ stopped_elsewhere(Visitor).

suggestion(Visitor, 'Ask the visitor to perform a simple reliability task.') :-
    visitor(Visitor),
    has_critical_info(Visitor),
    \+ performed_task(Visitor).

suggestion(Visitor, 'Advise the visitor to wait until dawn before entry.') :-
    visitor(Visitor),
    carries_flamefruit(Visitor),
    after_sundown.
