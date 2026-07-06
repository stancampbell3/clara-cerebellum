;;; the_coire.clp — Clara Coire semantic API for CLIPS engines
;;;
;;; Provides high-level wrappers over the low-level UDFs registered in
;;; userfunctions.c: (coire-emit ...), (coire-poll ...), (coire-mark ...),
;;; (coire-count ...).
;;;
;;; The session UUID is injected by Rust at ClipsEnvironment creation time via
;;;   (bind ?*coire-session-id* "uuid-string")
;;; Do not bind it manually.
;;;
;;; Incoming events (from Prolog or other engines) are dispatched by Rust via
;;; consume_coire_events(), which either evals them directly ("assert"/"goal"
;;; types) or asserts them as (coire-event ...) template facts so rules can fire.

;;; ── Session identity ────────────────────────────────────────────────────────

;;; Session UUID — set by Rust at ClipsEnvironment construction.
;;; Read-only from CLIPS code; use (coire-session) to access it.
(defglobal ?*coire-session-id* = "")

;;; Paired Prolog engine's session UUID — injected by Rust at construction.
;;; Use for cross-engine Coire writes (e.g. emitting evaluator/ events that
;;; publish_evaluator_events reads from the Prolog mailbox).
(defglobal ?*prolog-session-id* = "")

;;; (coire-session) → string: return this engine's session UUID.
(deffunction coire-session ()
  ?*coire-session-id*)

;;; ── Incoming event template ─────────────────────────────────────────────────

;;; Template for events dispatched by consume_coire_events() when the event
;;; type is not one of the built-in handled types ("assert" or "goal").
;;; Write defrules matching (coire-event (ev-type "...") (data "...")) to
;;; react to custom cross-engine events.
;;;
;;; Example:
;;;   (defrule handle-signal
;;;     (coire-event (ev-type "signal") (data ?d))
;;;     =>
;;;     (printout t "Got signal: " ?d crlf))
;;; Routing slots (topic / correlation / source-node) are populated from the
;;; payload's `_routing` block on ingested Ritual messages (Hohi/Tabu and
;;; addressed Offerings) so edge-generated defrules can match typed incoming
;;; messages, e.g.:
;;;   (coire-event (origin "ritual/hohi") (correlation ?cid) (data ?d))
(deftemplate coire-event
  (slot event-id    (type STRING) (default ""))
  (slot origin      (type STRING) (default ""))
  (slot ev-type     (type STRING) (default ""))
  (slot data        (type STRING) (default ""))
  (slot topic       (type STRING) (default ""))
  (slot correlation (type STRING) (default ""))
  (slot source-node (type STRING) (default "")))

;;; ── Publishing helpers ───────────────────────────────────────────────────────

;;; (coire-publish ?type ?data-str)
;;;   Emit a typed event to the Coire mailbox for this session.
;;;   ?type     — event type string: "assert", "retract", "goal", or any custom type
;;;   ?data-str — payload data string (must not contain unescaped double quotes)
;;;
;;; The event is stored as:
;;;   {"type": "assert", "data": "user_authenticated(alice)"}
(deffunction coire-publish (?type ?data-str)
  (bind ?payload
    (str-cat "{\"type\":\"" ?type "\",\"data\":\"" ?data-str "\"}"))
  (coire-emit ?*coire-session-id* "clips" ?payload))

;;; (coire-publish-assert ?fact-str)
;;;   Tell consuming engines to assert a fact.
;;;   For Prolog consumers: ?fact-str must be valid Prolog term syntax.
;;;     e.g. (coire-publish-assert "user_authenticated(alice)")
;;;   For CLIPS consumers: ?fact-str is eval'd as (assert <data>).
;;;     e.g. (coire-publish-assert "(main-ballast-valve closed)")
(deffunction coire-publish-assert (?fact-str)
  (coire-publish "assert" ?fact-str))

;;; (coire-publish-retract ?fact-str)
;;;   Tell consuming Prolog engines to retract a fact.
;;;   ?fact-str must be valid Prolog term syntax.
;;;     e.g. (coire-publish-retract "session_open(old_session)")
;;; Note: CLIPS consumers receive this as a (coire-event (ev-type "retract") ...)
;;; template fact. Write a defrule to handle it.
(deffunction coire-publish-retract (?fact-str)
  (coire-publish "retract" ?fact-str))

;;; (coire-publish-goal ?goal-str)
;;;   Tell consuming engines to execute a goal or expression.
;;;   For Prolog consumers: ?goal-str is a Prolog goal (called via call/1).
;;;     e.g. (coire-publish-goal "run_diagnostics")
;;;   For CLIPS consumers: ?goal-str is eval'd directly as a CLIPS expression.
;;;     e.g. (coire-publish-goal "(run)")
(deffunction coire-publish-goal (?goal-str)
  (coire-publish "goal" ?goal-str))

;;; ── caws: typed edge messaging (docs/deduction_redux.md) ────────────────────
;;;
;;; CLIPS mirrors of the Prolog caws_offer/caws_squawk predicates. The
;;; `_caws` block is lifted onto the outgoing TephraEnvelope's routing fields
;;; by the cycle controller. Emitted into the *Prolog* mailbox (via
;;; ?*prolog-session-id*) because publish_evaluator_events drains evaluator/
;;; events from there. Awaiting/synthesis is a Prolog-side concern
;;; (caws_await/2); CLIPS rules react to replies via
;;;   (coire-event (origin "ritual/hohi") (correlation ?cid) ...)

;;; (caws-offer ?target-node-id ?topic-path ?correlation-id ?payload-json)
;;;   Publish an addressed, correlated Offering. ?payload-json is the JSON
;;;   object the target evaluator expects, without the outer braces' _caws
;;;   block — e.g. "\"prompt\":\"hello\"" fragments are NOT accepted; pass a
;;;   full object like "{\"prompt\":\"hello\"}".
(deffunction caws-offer (?target ?topic ?cid ?payload-json)
  (bind ?caws
    (str-cat "{\"correlation_id\":\"" ?cid
             "\",\"target_node_id\":\"" ?target
             "\",\"topic_path\":\"" ?topic "\"}"))
  ;; splice _caws into the payload object: {..., "_caws": {...}}
  (bind ?len (str-length ?payload-json))
  (bind ?body (sub-string 1 (- ?len 1) ?payload-json))
  (bind ?sep (if (eq ?body "{") then "" else ","))
  (coire-emit ?*prolog-session-id*
              "evaluator/offering"
              (str-cat ?body ?sep "\"_caws\":" ?caws "}")))

;;; (caws-squawk ?topic-path ?tags-json ?payload-json)
;;;   Fire-and-forget publish on a logical topic path. ?tags-json is a JSON
;;;   array string, e.g. "[\"urgent\"]".
(deffunction caws-squawk (?topic ?tags-json ?payload-json)
  (bind ?caws
    (str-cat "{\"topic_path\":\"" ?topic
             "\",\"tags\":" ?tags-json
             ",\"expects_reply\":false}"))
  (bind ?len (str-length ?payload-json))
  (bind ?body (sub-string 1 (- ?len 1) ?payload-json))
  (bind ?sep (if (eq ?body "{") then "" else ","))
  (coire-emit ?*prolog-session-id*
              "evaluator/squawk"
              (str-cat ?body ?sep "\"_caws\":" ?caws "}")))

;;; ── Notes on consumption ─────────────────────────────────────────────────────
;;;
;;; Event consumption from the Coire mailbox is driven by Rust:
;;;
;;;   let n = env.consume_coire_events()?;
;;;
;;; For each pending event:
;;;   "assert" → (assert <data>) — CLIPS fact string is asserted directly
;;;   "goal"   → <data> is eval'd as a CLIPS expression
;;;   other    → asserted as (coire-event ...) template fact + (run)
;;;
;;; There is no CLIPS-side (coire-consume) function because CLIPS cannot parse
;;; the JSON array returned by (coire-poll ...) natively. Use the Rust API.
