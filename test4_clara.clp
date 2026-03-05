; Transduced from: man_with_plan(Man) :- man(Man), plan(Man), coire_publish_assert(man_with_plan(Man)).
(defrule transduced-man_with_plan-on-man-0
    (man ?Man)
    =>
    (coire-publish-goal (str-cat "man_with_plan(" ?Man ")")))

; Transduced from: man_with_plan(Man) :- man(Man), plan(Man), coire_publish_assert(man_with_plan(Man)).
(defrule transduced-man_with_plan-on-plan-1
    (plan ?Man)
    =>
    (coire-publish-goal (str-cat "man_with_plan(" ?Man ")")))

; Transduced from: man_with_plan(Man) :- man(Man), plan(Man), coire_publish_assert(man_with_plan(Man)).
(defrule transduced-man_with_plan-on-coire_publish_assert-2
    (coire_publish_assert "man_with_plan(?Man)")
    =>
    (coire-publish-goal (str-cat "man_with_plan(" ?Man ")")))

; Transduced from: get_out_the_back(Dude) :- man_with_plan(_), man(Dude), coire_publish_assert(get_out_the_back(Dude)).
(defrule transduced-get_out_the_back-on-man_with_plan-3
    (man_with_plan ?_)
    =>
    (coire-publish-goal "get_out_the_back(Dude)"))

; Transduced from: get_out_the_back(Dude) :- man_with_plan(_), man(Dude), coire_publish_assert(get_out_the_back(Dude)).
(defrule transduced-get_out_the_back-on-man-4
    (man ?Dude)
    =>
    (coire-publish-goal (str-cat "get_out_the_back(" ?Dude ")")))

; Transduced from: get_out_the_back(Dude) :- man_with_plan(_), man(Dude), coire_publish_assert(get_out_the_back(Dude)).
(defrule transduced-get_out_the_back-on-coire_publish_assert-5
    (coire_publish_assert "get_out_the_back(?Dude)")
    =>
    (coire-publish-goal (str-cat "get_out_the_back(" ?Dude ")")))

