; Transduced from: echo1(R1) :- q1(Q), clara_evaluate(Q,R1).
(defrule transduced-echo1-on-q1-0
    (q1 ?Q)
    =>
    (coire-publish-goal "echo1(R1)"))

; Transduced from: echo1(R1) :- q1(Q), clara_evaluate(Q,R1).
(defrule transduced-echo1-on-clara_evaluate-1
    (clara_evaluate ?Q ?R1)
    =>
    (coire-publish-goal (str-cat "echo1(" ?R1 ")")))

; Transduced from: echo2(R2) :- q1(Q), clara_evaluate(Q,R2).
(defrule transduced-echo2-on-q1-2
    (q1 ?Q)
    =>
    (coire-publish-goal "echo2(R2)"))

; Transduced from: echo2(R2) :- q1(Q), clara_evaluate(Q,R2).
(defrule transduced-echo2-on-clara_evaluate-3
    (clara_evaluate ?Q ?R2)
    =>
    (coire-publish-goal (str-cat "echo2(" ?R2 ")")))

; Transduced from: duh_dun :- echo1(_), echo2(_).
(defrule transduced-duh_dun-on-echo1-4
    (echo1 ?)
    =>
    (coire-publish-goal "duh_dun"))

; Transduced from: duh_dun :- echo1(_), echo2(_).
(defrule transduced-duh_dun-on-echo2-5
    (echo2 ?)
    =>
    (coire-publish-goal "duh_dun"))

