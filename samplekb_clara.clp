; Transduced from: admit(Visitor,Reason) :- visitor(Visitor), greeted(Visitor), Reason.
(defrule transduced-admit-on-visitor-0
    (visitor ?Visitor)
    =>
    (coire-publish-goal (str-cat "admit(" ?Visitor ",Reason)")))

; Transduced from: admit(Visitor,Reason) :- visitor(Visitor), greeted(Visitor), Reason.
(defrule transduced-admit-on-greeted-1
    (greeted ?Visitor)
    =>
    (coire-publish-goal (str-cat "admit(" ?Visitor ",Reason)")))

