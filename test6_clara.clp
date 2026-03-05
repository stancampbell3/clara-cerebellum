; Transduced from: test1(A) :- like(donna,davey), like(davey,A), assert(like(donna,A)).
(defrule transduced-test1-on-like-0
    (like donna davey)
    =>
    (coire-publish-goal "test1(A)"))

; Transduced from: test1(A) :- like(donna,davey), like(davey,A), assert(like(donna,A)).
(defrule transduced-test1-on-like-1
    (like davey ?A)
    =>
    (coire-publish-goal (str-cat "test1(" ?A ")")))

; Transduced from: test1(A) :- like(donna,davey), like(davey,A), assert(like(donna,A)).
(defrule transduced-test1-on-like-2
    (like donna ?A)
    =>
    (coire-publish-goal (str-cat "test1(" ?A ")")))

