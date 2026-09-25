; A note asserted in Prolog reaches CLIPS as a fact; this rule builds a Prolog goal out of its text and sends it back.
(defrule echo-note
    (note ?t)
    =>
    (coire-publish-goal (str-cat "assertz(echoed(\"" ?t "\"))")))
