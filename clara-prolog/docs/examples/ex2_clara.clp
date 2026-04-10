; Transduced from: it_rained :- wet_surface, not_sprinklers.
(defrule transduced-it_rained-on-wet_surface-0
    (wet_surface)
    =>
    (coire-publish-goal "it_rained"))

; Transduced from: it_rained :- wet_surface, not_sprinklers.
(defrule transduced-it_rained-on-not_sprinklers-1
    (not_sprinklers)
    =>
    (coire-publish-goal "it_rained"))

; Transduced from: wet_surface :- wet(ground).
(defrule transduced-wet_surface-on-wet-2
    (wet ground)
    =>
    (coire-publish-goal "wet_surface"))

; Transduced from: wet_surface :- wet(sidewalk).
(defrule transduced-wet_surface-on-wet-3
    (wet sidewalk)
    =>
    (coire-publish-goal "wet_surface"))

; Transduced from: not_sprinklers :- wet(_), day_of_week(saturday).
(defrule transduced-not_sprinklers-on-wet-4
    (wet ?)
    =>
    (coire-publish-goal "not_sprinklers"))

; Transduced from: not_sprinklers :- wet(_), day_of_week(saturday).
(defrule transduced-not_sprinklers-on-day_of_week-5
    (day_of_week saturday)
    =>
    (coire-publish-goal "not_sprinklers"))

; Transduced from: not_sprinklers :- wet(_), day_of_week(sunday).
(defrule transduced-not_sprinklers-on-wet-6
    (wet ?)
    =>
    (coire-publish-goal "not_sprinklers"))

; Transduced from: not_sprinklers :- wet(_), day_of_week(sunday).
(defrule transduced-not_sprinklers-on-day_of_week-7
    (day_of_week sunday)
    =>
    (coire-publish-goal "not_sprinklers"))

