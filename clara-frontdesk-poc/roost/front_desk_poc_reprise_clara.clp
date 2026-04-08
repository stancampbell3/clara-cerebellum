; Transduced from: daemonic_turn(Visitor,Suggestions,Decision,Reason,Where) :- visitor(Visitor), findall(S,suggestion(Visitor,S),Suggestions), admit(Visitor,Reason).
(defrule transduced-daemonic_turn-on-visitor-0
    (visitor ?Visitor)
    =>
    (coire-publish-goal (str-cat "daemonic_turn(" ?Visitor ",Suggestions,Decision,Reason,Where)")))

; Transduced from: daemonic_turn(Visitor,Suggestions,Decision,Reason,Where) :- visitor(Visitor), findall(S,suggestion(Visitor,S),Suggestions), admit(Visitor,Reason).
(defrule transduced-daemonic_turn-on-admit-1
    (admit ?Visitor ?Reason)
    =>
    (coire-publish-goal (str-cat "daemonic_turn(" ?Visitor ",Suggestions,Decision," ?Reason ",Where)")))

; Transduced from: meets_condition(Visitor,Question) :- visitor(Visitor), the_rabbit.
(defrule transduced-meets_condition-on-visitor-2
    (visitor ?Visitor)
    =>
    (coire-publish-goal (str-cat "meets_condition(" ?Visitor ",Question)")))

; Transduced from: meets_condition(Visitor,Question) :- visitor(Visitor), the_rabbit.
(defrule transduced-meets_condition-on-the_rabbit-3
    (the_rabbit)
    =>
    (coire-publish-goal "meets_condition(Visitor,Question)"))

