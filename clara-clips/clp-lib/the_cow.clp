;;; the_cow.clp — Edgequake RAG bridge for CLIPS engines
;;;
;;; High-level wrappers over (clara-evaluate ...) for querying Edgequake's
;;; graphical RAG system via the `edgequake` tool registered in clara-toolbox.
;;; Mirrors the_rabbit.pl's Prolog predicates (ponder_text/ponder_text_with_context)
;;; for the CLIPS side.
;;;
;;; Unlike the_coire.clp, this library is NOT auto-loaded into every session.
;;; Sessions that want (ruminate ...) available load this file explicitly via
;;; the existing ClipsLoadRules operation.

;;; (ruminate ?query-str) → JSON response string from Edgequake (hybrid mode).
(deffunction ruminate (?query-str)
  (ruminate-mode ?query-str "hybrid"))

;;; (ruminate-with-context ?query-str ?context-str) → JSON response string.
(deffunction ruminate-with-context (?query-str ?context-str)
  (bind ?payload
    (str-cat "{\"tool\":\"edgequake\",\"arguments\":{\"operation\":\"query\",\"query\":\""
             ?query-str "\",\"context\":\"" ?context-str "\",\"mode\":\"hybrid\"}}"))
  (clara-evaluate ?payload))

;;; (ruminate-mode ?query-str ?mode-str) → JSON response string.
;;;   ?mode-str is one of: "naive", "local", "global", "hybrid", "mix".
(deffunction ruminate-mode (?query-str ?mode-str)
  (bind ?payload
    (str-cat "{\"tool\":\"edgequake\",\"arguments\":{\"operation\":\"query\",\"query\":\""
             ?query-str "\",\"mode\":\"" ?mode-str "\"}}"))
  (clara-evaluate ?payload))
