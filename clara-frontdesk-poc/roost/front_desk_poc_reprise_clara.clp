; Transduced from: suggestion(Visitor,'Greet the visitor.') :- visitor(Visitor), \+ greeted(Visitor).
(defrule transduced-suggestion-on-visitor-14
    (visitor ?Visitor)
    =>
    (coire-publish-goal (str-cat "suggestion(" ?Visitor ",'Greet the visitor.')")))

; Transduced from: suggestion(Visitor,'Greet the visitor.') :- visitor(Visitor), \+ greeted(Visitor).
(defrule transduced-suggestion-on-not_greeted-15
    (not_greeted ?Visitor)
    =>
    (coire-publish-goal (str-cat "suggestion(" ?Visitor ",'Greet the visitor.')")))

; Transduced from: suggestion(Visitor,'Direct the visitor to the nearest map kiosk.') :- visitor(Visitor), (lost_or_confused(Visitor) ; meets_condition(Visitor,"...")).
(defrule transduced-suggestion-on-visitor-16
    (visitor ?Visitor)
    =>
    (coire-publish-goal (str-cat "suggestion(" ?Visitor ",'Direct the visitor to the nearest map kiosk.')")))

; Transduced from: suggestion(Visitor,'Direct the visitor to the nearest map kiosk.') :- visitor(Visitor), (lost_or_confused(Visitor) ; meets_condition(Visitor,"...")).
(defrule transduced-suggestion-on-lost_or_confused-17
    (lost_or_confused ?Visitor)
    =>
    (coire-publish-goal (str-cat "suggestion(" ?Visitor ",'Direct the visitor to the nearest map kiosk.')")))

; Transduced from: suggestion(Visitor,'Direct the visitor to the nearest map kiosk.') :- visitor(Visitor), (lost_or_confused(Visitor) ; meets_condition(Visitor,"...")).
(defrule transduced-suggestion-on-meets_condition-18
    (meets_condition ?Visitor "Based on the conversation so far, does the visitor seem lost or confused?")
    =>
    (coire-publish-goal (str-cat "suggestion(" ?Visitor ",'Direct the visitor to the nearest map kiosk.')")))

; Transduced from: suggestion(Visitor,'Request the three required artifacts for summoned visitors.') :- visitor(Visitor), summoned_by(Visitor,_), findall(A,has_artifact(Visitor,A),Artifacts), length(Artifacts,N), N < 3.
(defrule transduced-suggestion-on-visitor-19
    (visitor ?Visitor)
    =>
    (coire-publish-goal (str-cat "suggestion(" ?Visitor ",'Request the three required artifacts for summoned visitors.')")))

; Transduced from: suggestion(Visitor,'Request the three required artifacts for summoned visitors.') :- visitor(Visitor), summoned_by(Visitor,_), findall(A,has_artifact(Visitor,A),Artifacts), length(Artifacts,N), N < 3.
(defrule transduced-suggestion-on-summoned_by-20
    (summoned_by ?Visitor ?)
    =>
    (coire-publish-goal (str-cat "suggestion(" ?Visitor ",'Request the three required artifacts for summoned visitors.')")))

; Transduced from: suggestion(Visitor,'Verify that the visitor made no prior stops before delivering their urgent message.') :- visitor(Visitor), urgent_message(Visitor), \+ stopped_elsewhere(Visitor).
(defrule transduced-suggestion-on-visitor-22
    (visitor ?Visitor)
    =>
    (coire-publish-goal (str-cat "suggestion(" ?Visitor ",'Verify that the visitor made no prior stops before delivering their urgent message.')")))

; Transduced from: suggestion(Visitor,'Verify that the visitor made no prior stops before delivering their urgent message.') :- visitor(Visitor), urgent_message(Visitor), \+ stopped_elsewhere(Visitor).
(defrule transduced-suggestion-on-urgent_message-23
    (urgent_message ?Visitor)
    =>
    (coire-publish-goal (str-cat "suggestion(" ?Visitor ",'Verify that the visitor made no prior stops before delivering their urgent message.')")))

; Transduced from: suggestion(Visitor,'Verify that the visitor made no prior stops before delivering their urgent message.') :- visitor(Visitor), urgent_message(Visitor), \+ stopped_elsewhere(Visitor).
(defrule transduced-suggestion-on-not_stopped_elsewhere-24
    (not_stopped_elsewhere ?Visitor)
    =>
    (coire-publish-goal (str-cat "suggestion(" ?Visitor ",'Verify that the visitor made no prior stops before delivering their urgent message.')")))

; Transduced from: suggestion(Visitor,'Ask the visitor to perform a simple reliability task.') :- visitor(Visitor), has_critical_info(Visitor), \+ performed_task(Visitor).
(defrule transduced-suggestion-on-visitor-25
    (visitor ?Visitor)
    =>
    (coire-publish-goal (str-cat "suggestion(" ?Visitor ",'Ask the visitor to perform a simple reliability task.')")))

; Transduced from: suggestion(Visitor,'Ask the visitor to perform a simple reliability task.') :- visitor(Visitor), has_critical_info(Visitor), \+ performed_task(Visitor).
(defrule transduced-suggestion-on-has_critical_info-26
    (has_critical_info ?Visitor)
    =>
    (coire-publish-goal (str-cat "suggestion(" ?Visitor ",'Ask the visitor to perform a simple reliability task.')")))

; Transduced from: suggestion(Visitor,'Ask the visitor to perform a simple reliability task.') :- visitor(Visitor), has_critical_info(Visitor), \+ performed_task(Visitor).
(defrule transduced-suggestion-on-not_performed_task-27
    (not_performed_task ?Visitor)
    =>
    (coire-publish-goal (str-cat "suggestion(" ?Visitor ",'Ask the visitor to perform a simple reliability task.')")))

; Transduced from: suggestion(Visitor,'Advise the visitor to wait until dawn before entry.') :- visitor(Visitor), carries_flamefruit(Visitor), after_sundown.
(defrule transduced-suggestion-on-visitor-28
    (visitor ?Visitor)
    =>
    (coire-publish-goal (str-cat "suggestion(" ?Visitor ",'Advise the visitor to wait until dawn before entry.')")))

; Transduced from: suggestion(Visitor,'Advise the visitor to wait until dawn before entry.') :- visitor(Visitor), carries_flamefruit(Visitor), after_sundown.
(defrule transduced-suggestion-on-carries_flamefruit-29
    (carries_flamefruit ?Visitor)
    =>
    (coire-publish-goal (str-cat "suggestion(" ?Visitor ",'Advise the visitor to wait until dawn before entry.')")))

; Transduced from: redirect(Visitor,'This visitor appears lost or confused and cannot proceed.','nearest map kiosk') :-
;     visitor(Visitor), (lost_or_confused(Visitor) ; meets_condition(Visitor,"...")).
(defrule transduced-redirect-on-visitor-30
    (visitor ?Visitor)
    =>
    (coire-publish-goal (str-cat "redirect(" ?Visitor ",_Reason,_Where)")))

; Transduced from: redirect(Visitor,'This visitor appears lost or confused and cannot proceed.','nearest map kiosk') :-
;     visitor(Visitor), (lost_or_confused(Visitor) ; meets_condition(Visitor,"...")).
(defrule transduced-redirect-on-lost_or_confused-31
    (lost_or_confused ?Visitor)
    =>
    (coire-publish-goal (str-cat "redirect(" ?Visitor ",_Reason,_Where)")))
