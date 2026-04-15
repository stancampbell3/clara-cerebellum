;;; ritual_chanter_test_clara.clp
;;;
;;; CLIPS KB for the Phase 6 ritual e2e test.
;;;
;;; Two rules drive the peer-evaluation round-trip:
;;;
;;;   ask-peer-chanter    — fires on (need-peer-eval ?prompt), emits an
;;;                         Offering to the Prolog evaluator/ Coire channel.
;;;                         Uses ?*prolog-session-id* (injected by Rust) to
;;;                         write directly to the Prolog mailbox.
;;;
;;;   receive-hohi-answer — fires when the peer's Hohi arrives as a
;;;                         (coire-event (origin "ritual/hohi")) fact
;;;                         (written by ingest_tephra dual-write), and
;;;                         publishes answered/2 back to Prolog via relay.
;;;
;;; The initial (need-peer-eval "hello") fact is asserted by the Rust test
;;; setup before CycleController::run() is called.

(defrule ask-peer-chanter
    ?f <- (need-peer-eval ?prompt)
    =>
    (retract ?f)
    (coire-emit ?*prolog-session-id*
                "evaluator/ask-chanter"
                (str-cat "{\"prompt\":\"" ?prompt "\"}")))

(defrule receive-hohi-answer
    ?ev <- (coire-event (origin "ritual/hohi"))
    =>
    (retract ?ev)
    (coire-publish-assert "answered(hello, chanter_responded)"))
