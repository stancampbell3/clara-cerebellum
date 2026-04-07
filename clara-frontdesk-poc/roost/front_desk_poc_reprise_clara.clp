; Transduced from: meets_condition(Visitor,Question) :- visitor(Visitor), the_rabbit.
(defrule transduced-meets_condition-on-visitor-0
    (visitor ?Visitor)
    =>
    (coire-publish-goal (str-cat "meets_condition(" ?Visitor ",Question)")))

; Transduced from: meets_condition(Visitor,Question) :- visitor(Visitor), the_rabbit.
(defrule transduced-meets_condition-on-the_rabbit-1
    (the_rabbit)
    =>
    (coire-publish-goal "meets_condition(Visitor,Question)"))

