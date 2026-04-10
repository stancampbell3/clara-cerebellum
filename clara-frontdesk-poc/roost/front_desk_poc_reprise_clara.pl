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
:- prolog_listen(where_to_go/1, updated(where_to_go/1)).

updated(Pred, Action, Context) :-
    clause(Head, _Body, Context),
    coire_publish_assert(Head),
    format('Updated ~w with action ~w in context ~p~n', [Pred, Action, Head]).

% ── Clara synthetic groups (multi-clause predicates) ─────────────────────────
% Declared dynamic so that assertz'd results trigger forward-chaining
% notification. Mirrored as umbrella nodes in the DOT graph.
:- dynamic(admit/2).
:- prolog_listen(admit/2, updated(admit/2)).
:- dynamic(suggestion/2).
:- prolog_listen(suggestion/2, updated(suggestion/2)).
% ── End Clara integration ───────────────────────────────────────────────────

%% City of Dis Front Desk Logic (source for Clara transducer)

:- use_module(library(the_rabbit)).
:- use_module(library(the_rat)).

%% Facts that must be visible to Coire
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
:- dynamic(where_to_go/1).

%% POC entry point
daemonic_turn(Visitor, Suggestions, Decision, Reason, Where) :-
    visitor(Visitor),
    findall(S, suggestion(Visitor, S), Suggestions),
    (   admit(Visitor, Reason)
    ->  true
    ;   Reason = 'Entry denied.'
    ),
    effective_decision(Decision, Where).

%% Priority order: admitted/redirected/denied beat pending.
%% 'redirected' maps to a human-readable destination for the outcome screen.
effective_decision(Decision, Where) :-
    (   where_to_go(admitted)
    ->  Decision = admitted,   Where = ''
    ;   where_to_go(redirected)
    ->  Decision = redirected, Where = 'Nearest Map Kiosk'
    ;   where_to_go(denied)
    ->  Decision = denied,     Where = ''
    ;   Decision = pending,    Where = ''
    ).

%% Helper: ask Clara whether a condition is satisfied in context.
meets_condition(Visitor, Question) :-
    visitor(Visitor),
    the_rabbit:current_context(Context),
    format('Asking Clara: ~w~nContext: ~w~n', [Question, Context]),
    clara_fy(Question, Context, R),
    R == true.

%% ----------------------------------------------------------------------
%%  Admittance Rules (LLM‑augmented)
%% ----------------------------------------------------------------------

%% Rule 1: Summoned visitors with 3 artifacts OR who *claim* they have them.
admit(Visitor, Reason) :-
    visitor(Visitor),
    summoned_by(Visitor, _Official),
    (
        % symbolic check
        findall(A, has_artifact(Visitor, A), Artifacts),
        length(Artifacts, N),
        N >= 3
    ;
        % LLM‑mediated check
        meets_condition(
            Visitor,
            "Does the visitor claim to possess the three required artifacts for their summons?"
        )
    ),
    Reason = 'Summoned visitor with required artifacts (verified or claimed). Grant entry.',
    assertz(where_to_go('admitted')).

%% Rule 2: Urgent message + no prior stops (symbolic or LLM‑verified).
admit(Visitor, Reason) :-
    visitor(Visitor),
    urgent_message(Visitor),
    (
        \+ stopped_elsewhere(Visitor)
    ;
        meets_condition(
            Visitor,
            "Does the visitor assert that they came directly here without stopping elsewhere?"
        )
    ),
    Reason = 'Visitor carries an urgent message and came directly. Grant entry.',
    assertz(where_to_go('admitted')).

%% Rule 3: Flamefruit carriers before sundown (symbolic or LLM‑verified).
admit(Visitor, Reason) :-
    visitor(Visitor),
    (
        carries_flamefruit(Visitor)
    ;
        meets_condition(
            Visitor,
            "Is the visitor carrying the rare Flamefruit?"
        )
    ),
    \+ after_sundown,
    Reason = 'Flamefruit carrier before sundown. Grant entry.',
    assertz(where_to_go('admitted')).

%% Rule 4: Critical info + reliability task (symbolic or LLM‑verified).
admit(Visitor, Reason) :-
    visitor(Visitor),
    (
        has_critical_info(Visitor)
    ;
        meets_condition(
            Visitor,
            "Does the visitor claim to possess critical information for the City?"
        )
    ),
    (
        performed_task(Visitor)
    ;
        meets_condition(
            Visitor,
            "Has the visitor demonstrated reliability by completing the requested task?"
        )
    ),
    Reason = 'Visitor proved reliability and carries critical information. Grant entry.',
    assertz(where_to_go('admitted')).

%% Rule 5: Lost or confused visitors are never admitted.
admit(Visitor, Reason) :-
    visitor(Visitor),
    (
        lost_or_confused(Visitor)
    ;
        meets_condition(
            Visitor,
            "Does the visitor appear lost or confused about where they are?"
        )
    ),
    Reason = 'Visitor appears lost or confused. Do not admit; direct to map kiosk.',
    assertz(where_to_go('redirected')).

%% ----------------------------------------------------------------------
%%  Suggestions (LLM‑augmented)
%% ----------------------------------------------------------------------

suggestion(Visitor, 'Greet the visitor.') :-
    visitor(Visitor),
    \+ greeted(Visitor).

suggestion(Visitor, 'Direct the visitor to the nearest map kiosk.') :-
    visitor(Visitor),
    (
        lost_or_confused(Visitor)
    ;
        meets_condition(
            Visitor,
            "Based on the conversation so far, does the visitor seem lost or confused?"
        )
    ),
    assertz(where_to_go('redirected')).

suggestion(Visitor, 'Request the three required artifacts for summoned visitors.') :-
    visitor(Visitor),
    summoned_by(Visitor, _),
    findall(A, has_artifact(Visitor, A), Artifacts),
    length(Artifacts, N),
    N < 3,
    assertz(where_to_go('pending')).

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
    after_sundown,
    assertz(where_to_go('pending')).

