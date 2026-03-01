; Transduced from: nonanimal(X) :- not(animal(X)).
(defrule transduced-nonanimal-on-not-0
    (not "animal(?X)")
    =>
    (coire-publish-goal (str-cat "nonanimal(" ?X ")")))

; Transduced from: vertebrata(X) :- has(X,backbone), animal(X).
(defrule transduced-vertebrata-on-has-1
    (has ?X backbone)
    =>
    (coire-publish-goal (str-cat "vertebrata(" ?X ")")))

; Transduced from: vertebrata(X) :- has(X,backbone), animal(X).
(defrule transduced-vertebrata-on-animal-2
    (animal ?X)
    =>
    (coire-publish-goal (str-cat "vertebrata(" ?X ")")))

; Transduced from: nonvertebrata(X) :- animal(X), not(has(X,backbone)).
(defrule transduced-nonvertebrata-on-animal-3
    (animal ?X)
    =>
    (coire-publish-goal (str-cat "nonvertebrata(" ?X ")")))

; Transduced from: nonvertebrata(X) :- animal(X), not(has(X,backbone)).
(defrule transduced-nonvertebrata-on-not-4
    (not "has(?X,backbone)")
    =>
    (coire-publish-goal (str-cat "nonvertebrata(" ?X ")")))

; Transduced from: reptiles(X) :- vertebrata(X), has(X,cold_blooded), has(X,scaly_skin).
(defrule transduced-reptiles-on-vertebrata-5
    (vertebrata ?X)
    =>
    (coire-publish-goal (str-cat "reptiles(" ?X ")")))

; Transduced from: reptiles(X) :- vertebrata(X), has(X,cold_blooded), has(X,scaly_skin).
(defrule transduced-reptiles-on-has-6
    (has ?X cold_blooded)
    =>
    (coire-publish-goal (str-cat "reptiles(" ?X ")")))

; Transduced from: reptiles(X) :- vertebrata(X), has(X,cold_blooded), has(X,scaly_skin).
(defrule transduced-reptiles-on-has-7
    (has ?X scaly_skin)
    =>
    (coire-publish-goal (str-cat "reptiles(" ?X ")")))

; Transduced from: fish(X) :- vertebrata(X), has(X,cold_blooded), has(X,gills), has(X,scaly_skin).
(defrule transduced-fish-on-vertebrata-8
    (vertebrata ?X)
    =>
    (coire-publish-goal (str-cat "fish(" ?X ")")))

; Transduced from: fish(X) :- vertebrata(X), has(X,cold_blooded), has(X,gills), has(X,scaly_skin).
(defrule transduced-fish-on-has-9
    (has ?X cold_blooded)
    =>
    (coire-publish-goal (str-cat "fish(" ?X ")")))

; Transduced from: fish(X) :- vertebrata(X), has(X,cold_blooded), has(X,gills), has(X,scaly_skin).
(defrule transduced-fish-on-has-10
    (has ?X gills)
    =>
    (coire-publish-goal (str-cat "fish(" ?X ")")))

; Transduced from: fish(X) :- vertebrata(X), has(X,cold_blooded), has(X,gills), has(X,scaly_skin).
(defrule transduced-fish-on-has-11
    (has ?X scaly_skin)
    =>
    (coire-publish-goal (str-cat "fish(" ?X ")")))

; Transduced from: amphibi(X) :- vertebrata(X), has(X,cold_blooded), has(X,slimy_skin).
(defrule transduced-amphibi-on-vertebrata-12
    (vertebrata ?X)
    =>
    (coire-publish-goal (str-cat "amphibi(" ?X ")")))

; Transduced from: amphibi(X) :- vertebrata(X), has(X,cold_blooded), has(X,slimy_skin).
(defrule transduced-amphibi-on-has-13
    (has ?X cold_blooded)
    =>
    (coire-publish-goal (str-cat "amphibi(" ?X ")")))

; Transduced from: amphibi(X) :- vertebrata(X), has(X,cold_blooded), has(X,slimy_skin).
(defrule transduced-amphibi-on-has-14
    (has ?X slimy_skin)
    =>
    (coire-publish-goal (str-cat "amphibi(" ?X ")")))

; Transduced from: molluscs(X) :- nonvertebrata(X), has(X,soft_body).
(defrule transduced-molluscs-on-nonvertebrata-15
    (nonvertebrata ?X)
    =>
    (coire-publish-goal (str-cat "molluscs(" ?X ")")))

; Transduced from: molluscs(X) :- nonvertebrata(X), has(X,soft_body).
(defrule transduced-molluscs-on-has-16
    (has ?X soft_body)
    =>
    (coire-publish-goal (str-cat "molluscs(" ?X ")")))

; Transduced from: annelid(X) :- nonvertebrata(X), has(X,segmented_body).
(defrule transduced-annelid-on-nonvertebrata-17
    (nonvertebrata ?X)
    =>
    (coire-publish-goal (str-cat "annelid(" ?X ")")))

; Transduced from: annelid(X) :- nonvertebrata(X), has(X,segmented_body).
(defrule transduced-annelid-on-has-18
    (has ?X segmented_body)
    =>
    (coire-publish-goal (str-cat "annelid(" ?X ")")))

; Transduced from: arthropods(X) :- nonvertebrata(X), has(X,external_skeleton).
(defrule transduced-arthropods-on-nonvertebrata-19
    (nonvertebrata ?X)
    =>
    (coire-publish-goal (str-cat "arthropods(" ?X ")")))

; Transduced from: arthropods(X) :- nonvertebrata(X), has(X,external_skeleton).
(defrule transduced-arthropods-on-has-20
    (has ?X external_skeleton)
    =>
    (coire-publish-goal (str-cat "arthropods(" ?X ")")))

; Transduced from: arachnid(X) :- arthropods(X), has(X,leg_8).
(defrule transduced-arachnid-on-arthropods-21
    (arthropods ?X)
    =>
    (coire-publish-goal (str-cat "arachnid(" ?X ")")))

; Transduced from: arachnid(X) :- arthropods(X), has(X,leg_8).
(defrule transduced-arachnid-on-has-22
    (has ?X leg_8)
    =>
    (coire-publish-goal (str-cat "arachnid(" ?X ")")))

; Transduced from: insect(X) :- arthropods(X), has(X,leg_6).
(defrule transduced-insect-on-arthropods-23
    (arthropods ?X)
    =>
    (coire-publish-goal (str-cat "insect(" ?X ")")))

; Transduced from: insect(X) :- arthropods(X), has(X,leg_6).
(defrule transduced-insect-on-has-24
    (has ?X leg_6)
    =>
    (coire-publish-goal (str-cat "insect(" ?X ")")))

; Transduced from: mammal(X) :- vertebrata(X), has(X,warm_blooded), not(has(X,feather)).
(defrule transduced-mammal-on-vertebrata-25
    (vertebrata ?X)
    =>
    (coire-publish-goal (str-cat "mammal(" ?X ")")))

; Transduced from: mammal(X) :- vertebrata(X), has(X,warm_blooded), not(has(X,feather)).
(defrule transduced-mammal-on-has-26
    (has ?X warm_blooded)
    =>
    (coire-publish-goal (str-cat "mammal(" ?X ")")))

; Transduced from: mammal(X) :- vertebrata(X), has(X,warm_blooded), not(has(X,feather)).
(defrule transduced-mammal-on-not-27
    (not "has(?X,feather)")
    =>
    (coire-publish-goal (str-cat "mammal(" ?X ")")))

; Transduced from: bird(X) :- vertebrata(X), has(X,warm_blooded), has(X,feather).
(defrule transduced-bird-on-vertebrata-28
    (vertebrata ?X)
    =>
    (coire-publish-goal (str-cat "bird(" ?X ")")))

; Transduced from: bird(X) :- vertebrata(X), has(X,warm_blooded), has(X,feather).
(defrule transduced-bird-on-has-29
    (has ?X warm_blooded)
    =>
    (coire-publish-goal (str-cat "bird(" ?X ")")))

; Transduced from: bird(X) :- vertebrata(X), has(X,warm_blooded), has(X,feather).
(defrule transduced-bird-on-has-30
    (has ?X feather)
    =>
    (coire-publish-goal (str-cat "bird(" ?X ")")))

