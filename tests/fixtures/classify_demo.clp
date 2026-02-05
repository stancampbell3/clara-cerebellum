;;;; classify_demo.clp - Demonstrates text classification via clara-evaluate
;;;;
;;;; Usage: Load this file into a CLIPS session with the toolbox initialized
;;;; and DAGDA_MODEL_PATH set, then (reset) and (run).

(deftemplate text-to-classify
    (slot text)
    (slot source))

(deftemplate classification-result
    (slot text)
    (slot label)
    (slot source))

(defrule classify-text
    "Classify pending text using the fastText classify tool"
    (text-to-classify (text ?text) (source ?source))
    =>
    (bind ?json (str-cat "{\"tool\":\"classify\",\"arguments\":{\"text\":\"" ?text "\"}}"))
    (bind ?result (clara-evaluate ?json))
    (printout t "Classification of [" ?source "]: " ?result crlf))

;;; Sample facts to classify
(deffacts sample-texts
    (text-to-classify
        (text "Water boils at 100C at sea level. Yes, that is correct.")
        (source "resolved-example"))
    (text-to-classify
        (text "Cats can teleport through quantum tunneling. The cosmic energy says so!")
        (source "unresolved-example")))
