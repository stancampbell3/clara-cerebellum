; Transduced from: unlocked :- tumbler(1,set), tumbler(2,set), tumbler(3,set).
(defrule transduced-unlocked-on-tumbler-0
    (tumbler 1 set)
    =>
    (coire-publish-goal "unlocked"))

; Transduced from: unlocked :- tumbler(1,set), tumbler(2,set), tumbler(3,set).
(defrule transduced-unlocked-on-tumbler-1
    (tumbler 2 set)
    =>
    (coire-publish-goal "unlocked"))

; Transduced from: unlocked :- tumbler(1,set), tumbler(2,set), tumbler(3,set).
(defrule transduced-unlocked-on-tumbler-2
    (tumbler 3 set)
    =>
    (coire-publish-goal "unlocked"))

; Transduced from: tumbler_1 :- pick_position(3), turn(left), assert(tumbler(1,set)).
(defrule transduced-tumbler_1-on-pick_position-3
    (pick_position 3)
    =>
    (coire-publish-goal "tumbler_1"))

; Transduced from: tumbler_1 :- pick_position(3), turn(left), assert(tumbler(1,set)).
(defrule transduced-tumbler_1-on-turn-4
    (turn left)
    =>
    (coire-publish-goal "tumbler_1"))

; Transduced from: tumbler_2 :- pick_position(1), turn(right), assert(tumbler(2,set)).
(defrule transduced-tumbler_2-on-pick_position-5
    (pick_position 1)
    =>
    (coire-publish-goal "tumbler_2"))

; Transduced from: tumbler_2 :- pick_position(1), turn(right), assert(tumbler(2,set)).
(defrule transduced-tumbler_2-on-turn-6
    (turn right)
    =>
    (coire-publish-goal "tumbler_2"))

; Transduced from: tumbler_3 :- pick_position(2), turn(left), assert(tumbler(3,set)).
(defrule transduced-tumbler_3-on-pick_position-7
    (pick_position 2)
    =>
    (coire-publish-goal "tumbler_3"))

; Transduced from: tumbler_3 :- pick_position(2), turn(left), assert(tumbler(3,set)).
(defrule transduced-tumbler_3-on-turn-8
    (turn left)
    =>
    (coire-publish-goal "tumbler_3"))

