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

%% Helper: ask Clara whether a condition is satisfied in context.
meets_condition(Visitor, Question) :-
    visitor(Visitor),
    current_context(Context),
    format('Current context: ~w~n', Context),
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
    Reason = 'Summoned visitor with required artifacts (verified or claimed). Grant entry.'.

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
    Reason = 'Visitor carries an urgent message and came directly. Grant entry.'.

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
    Reason = 'Flamefruit carrier before sundown. Grant entry.'.

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
    Reason = 'Visitor proved reliability and carries critical information. Grant entry.'.

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
    Reason = 'Visitor appears lost or confused. Do not admit; direct to map kiosk.'.

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

