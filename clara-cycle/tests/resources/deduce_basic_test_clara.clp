; Transduced from: break_some :- egg(unbroken), assertz(egg(broken)).
(defrule transduced-break_some-on-egg-0
    (egg unbroken)
    =>
    (coire-publish-goal "break_some"))

; Transduced from: omelette(Visitor,Dish) :- visitor(Visitor), egg(broken), Dish.
(defrule transduced-omelette-on-visitor-1
    (visitor ?Visitor)
    =>
    (coire-publish-goal (str-cat "omelette(" ?Visitor ",Dish)")))

; Transduced from: omelette(Visitor,Dish) :- visitor(Visitor), egg(broken), Dish.
(defrule transduced-omelette-on-egg-2
    (egg broken)
    =>
    (coire-publish-goal "omelette(Visitor,Dish)"))

