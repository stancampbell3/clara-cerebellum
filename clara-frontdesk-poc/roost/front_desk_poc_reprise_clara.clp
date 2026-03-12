; Transduced from: meets_condition(Visitor,Question) :- visitor(Visitor), current_context(Context), clara_fy(Question,Context,R), R.
(defrule transduced-meets_condition-on-visitor-0
    (visitor ?Visitor)
    =>
    (coire-publish-goal (str-cat "meets_condition(" ?Visitor ",Question)")))

; Transduced from: meets_condition(Visitor,Question) :- visitor(Visitor), current_context(Context), clara_fy(Question,Context,R), R.
(defrule transduced-meets_condition-on-current_context-1
    (current_context ?Context)
    =>
    (coire-publish-goal "meets_condition(Visitor,Question)"))

; Transduced from: meets_condition(Visitor,Question) :- visitor(Visitor), current_context(Context), clara_fy(Question,Context,R), R.
(defrule transduced-meets_condition-on-clara_fy-2
    (clara_fy ?Question ?Context ?R)
    =>
    (coire-publish-goal (str-cat "meets_condition(Visitor," ?Question ")")))

