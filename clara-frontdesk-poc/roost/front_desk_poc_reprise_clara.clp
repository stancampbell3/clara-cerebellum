; Transduced from: suggestion(Visitor,'Greet the visitor.') :- visitor(Visitor), \+ greeted(Visitor).
(defrule transduced-suggestion-on-visitor-13
    (visitor ?Visitor)
    =>
    (coire-publish-goal (str-cat "suggestion(" ?Visitor ",'Greet the visitor.')")))

; Transduced from: suggestion(Visitor,'Greet the visitor.') :- visitor(Visitor), \+ greeted(Visitor).
(defrule transduced-suggestion-on-not_greeted-14
    (not_greeted ?Visitor)
    =>
    (coire-publish-goal (str-cat "suggestion(" ?Visitor ",'Greet the visitor.')")))

; Transduced from: suggestion(Visitor,'Direct the visitor to the nearest map kiosk.') :- visitor(Visitor), lost_or_confused(Visitor), meets_condition(Visitor,"Based on the conversation so far, does the visitor seem lost or confused?").
(defrule transduced-suggestion-on-visitor-15
    (visitor ?Visitor)
    =>
    (coire-publish-goal (str-cat "suggestion(" ?Visitor ",'Direct the visitor to the nearest map kiosk.')")))

; Transduced from: suggestion(Visitor,'Direct the visitor to the nearest map kiosk.') :- visitor(Visitor), lost_or_confused(Visitor), meets_condition(Visitor,"Based on the conversation so far, does the visitor seem lost or confused?").
(defrule transduced-suggestion-on-lost_or_confused-16
    (lost_or_confused ?Visitor)
    =>
    (coire-publish-goal (str-cat "suggestion(" ?Visitor ",'Direct the visitor to the nearest map kiosk.')")))

; Transduced from: suggestion(Visitor,'Direct the visitor to the nearest map kiosk.') :- visitor(Visitor), lost_or_confused(Visitor), meets_condition(Visitor,"Based on the conversation so far, does the visitor seem lost or confused?").
(defrule transduced-suggestion-on-meets_condition-17
    (meets_condition ?Visitor "Based on the conversation so far, does the visitor seem lost or confused?")
    =>
    (coire-publish-goal (str-cat "suggestion(" ?Visitor ",'Direct the visitor to the nearest map kiosk.')")))

; Transduced from: suggestion(Visitor,'Request the three required artifacts for summoned visitors.') :- visitor(Visitor), summoned_by(Visitor,_), findall(A,has_artifact(Visitor,A),Artifacts), length(Artifacts,N), N.
(defrule transduced-suggestion-on-visitor-18
    (visitor ?Visitor)
    =>
    (coire-publish-goal (str-cat "suggestion(" ?Visitor ",'Request the three required artifacts for summoned visitors.')")))

; Transduced from: suggestion(Visitor,'Request the three required artifacts for summoned visitors.') :- visitor(Visitor), summoned_by(Visitor,_), findall(A,has_artifact(Visitor,A),Artifacts), length(Artifacts,N), N.
(defrule transduced-suggestion-on-summoned_by-19
    (summoned_by ?Visitor ?)
    =>
    (coire-publish-goal (str-cat "suggestion(" ?Visitor ",'Request the three required artifacts for summoned visitors.')")))

; Transduced from: suggestion(Visitor,'Verify that the visitor made no prior stops before delivering their urgent message.') :- visitor(Visitor), urgent_message(Visitor), \+ stopped_elsewhere(Visitor).
(defrule transduced-suggestion-on-visitor-21
    (visitor ?Visitor)
    =>
    (coire-publish-goal (str-cat "suggestion(" ?Visitor ",'Verify that the visitor made no prior stops before delivering their urgent message.')")))

; Transduced from: suggestion(Visitor,'Verify that the visitor made no prior stops before delivering their urgent message.') :- visitor(Visitor), urgent_message(Visitor), \+ stopped_elsewhere(Visitor).
(defrule transduced-suggestion-on-urgent_message-22
    (urgent_message ?Visitor)
    =>
    (coire-publish-goal (str-cat "suggestion(" ?Visitor ",'Verify that the visitor made no prior stops before delivering their urgent message.')")))

; Transduced from: suggestion(Visitor,'Verify that the visitor made no prior stops before delivering their urgent message.') :- visitor(Visitor), urgent_message(Visitor), \+ stopped_elsewhere(Visitor).
(defrule transduced-suggestion-on-not_stopped_elsewhere-23
    (not_stopped_elsewhere ?Visitor)
    =>
    (coire-publish-goal (str-cat "suggestion(" ?Visitor ",'Verify that the visitor made no prior stops before delivering their urgent message.')")))

; Transduced from: suggestion(Visitor,'Ask the visitor to perform a simple reliability task.') :- visitor(Visitor), has_critical_info(Visitor), \+ performed_task(Visitor).
(defrule transduced-suggestion-on-visitor-24
    (visitor ?Visitor)
    =>
    (coire-publish-goal (str-cat "suggestion(" ?Visitor ",'Ask the visitor to perform a simple reliability task.')")))

; Transduced from: suggestion(Visitor,'Ask the visitor to perform a simple reliability task.') :- visitor(Visitor), has_critical_info(Visitor), \+ performed_task(Visitor).
(defrule transduced-suggestion-on-has_critical_info-25
    (has_critical_info ?Visitor)
    =>
    (coire-publish-goal (str-cat "suggestion(" ?Visitor ",'Ask the visitor to perform a simple reliability task.')")))

; Transduced from: suggestion(Visitor,'Ask the visitor to perform a simple reliability task.') :- visitor(Visitor), has_critical_info(Visitor), \+ performed_task(Visitor).
(defrule transduced-suggestion-on-not_performed_task-26
    (not_performed_task ?Visitor)
    =>
    (coire-publish-goal (str-cat "suggestion(" ?Visitor ",'Ask the visitor to perform a simple reliability task.')")))

; Transduced from: suggestion(Visitor,'Advise the visitor to wait until dawn before entry.') :- visitor(Visitor), carries_flamefruit(Visitor), after_sundown, assertz(where_to_go(pending)).
(defrule transduced-suggestion-on-visitor-27
    (visitor ?Visitor)
    =>
    (coire-publish-goal (str-cat "suggestion(" ?Visitor ",'Advise the visitor to wait until dawn before entry.')")))

; Transduced from: suggestion(Visitor,'Advise the visitor to wait until dawn before entry.') :- visitor(Visitor), carries_flamefruit(Visitor), after_sundown, assertz(where_to_go(pending)).
(defrule transduced-suggestion-on-carries_flamefruit-28
    (carries_flamefruit ?Visitor)
    =>
    (coire-publish-goal (str-cat "suggestion(" ?Visitor ",'Advise the visitor to wait until dawn before entry.')")))

