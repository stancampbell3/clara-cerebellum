; Transduced from: accuse(Suspect,Victim) :- murder(Victim), suspect(Suspect), motive(Suspect,Victim), opportunity(Suspect,Victim), capable(Suspect).
(defrule transduced-accuse-on-murder-0
    (murder ?Victim)
    =>
    (coire-publish-goal (str-cat "accuse(Suspect," ?Victim ")")))

; Transduced from: accuse(Suspect,Victim) :- murder(Victim), suspect(Suspect), motive(Suspect,Victim), opportunity(Suspect,Victim), capable(Suspect).
(defrule transduced-accuse-on-suspect-1
    (suspect ?Suspect)
    =>
    (coire-publish-goal (str-cat "accuse(" ?Suspect ",Victim)")))

; Transduced from: accuse(Suspect,Victim) :- murder(Victim), suspect(Suspect), motive(Suspect,Victim), opportunity(Suspect,Victim), capable(Suspect).
(defrule transduced-accuse-on-motive-2
    (motive ?Suspect ?Victim)
    =>
    (coire-publish-goal (str-cat "accuse(" ?Suspect "," ?Victim ")")))

; Transduced from: accuse(Suspect,Victim) :- murder(Victim), suspect(Suspect), motive(Suspect,Victim), opportunity(Suspect,Victim), capable(Suspect).
(defrule transduced-accuse-on-opportunity-3
    (opportunity ?Suspect ?Victim)
    =>
    (coire-publish-goal (str-cat "accuse(" ?Suspect "," ?Victim ")")))

; Transduced from: accuse(Suspect,Victim) :- murder(Victim), suspect(Suspect), motive(Suspect,Victim), opportunity(Suspect,Victim), capable(Suspect).
(defrule transduced-accuse-on-capable-4
    (capable ?Suspect)
    =>
    (coire-publish-goal (str-cat "accuse(" ?Suspect ",Victim)")))

; Transduced from: motive(Suspect,Victim) :- suspect(Suspect), murder(Victim), dislikes(Suspect,Victim).
(defrule transduced-motive-on-suspect-5
    (suspect ?Suspect)
    =>
    (coire-publish-goal (str-cat "motive(" ?Suspect ",Victim)")))

; Transduced from: motive(Suspect,Victim) :- suspect(Suspect), murder(Victim), dislikes(Suspect,Victim).
(defrule transduced-motive-on-murder-6
    (murder ?Victim)
    =>
    (coire-publish-goal (str-cat "motive(Suspect," ?Victim ")")))

; Transduced from: motive(Suspect,Victim) :- suspect(Suspect), murder(Victim), dislikes(Suspect,Victim).
(defrule transduced-motive-on-dislikes-7
    (dislikes ?Suspect ?Victim)
    =>
    (coire-publish-goal (str-cat "motive(" ?Suspect "," ?Victim ")")))

; Transduced from: capable(A) :- suspect(A).
(defrule transduced-capable-on-suspect-8
    (suspect ?A)
    =>
    (coire-publish-goal (str-cat "capable(" ?A ")")))

; Transduced from: opportunity(A,B) :- suspect(A), murder(B).
(defrule transduced-opportunity-on-suspect-9
    (suspect ?A)
    =>
    (coire-publish-goal (str-cat "opportunity(" ?A ",B)")))

; Transduced from: opportunity(A,B) :- suspect(A), murder(B).
(defrule transduced-opportunity-on-murder-10
    (murder ?B)
    =>
    (coire-publish-goal (str-cat "opportunity(A," ?B ")")))

; Transduced from: prejudiced(Who,Whom,Group) :- dislikes(Who,Whom), group(Group), member_of(Whom,Group).
(defrule transduced-prejudiced-on-dislikes-11
    (dislikes ?Who ?Whom)
    =>
    (coire-publish-goal (str-cat "prejudiced(" ?Who "," ?Whom ",Group)")))

; Transduced from: prejudiced(Who,Whom,Group) :- dislikes(Who,Whom), group(Group), member_of(Whom,Group).
(defrule transduced-prejudiced-on-group-12
    (group ?Group)
    =>
    (coire-publish-goal (str-cat "prejudiced(Who,Whom," ?Group ")")))

; Transduced from: prejudiced(Who,Whom,Group) :- dislikes(Who,Whom), group(Group), member_of(Whom,Group).
(defrule transduced-prejudiced-on-member_of-13
    (member_of ?Whom ?Group)
    =>
    (coire-publish-goal (str-cat "prejudiced(Who," ?Whom "," ?Group ")")))

; Transduced from: prejudiced(Who,Whom,Group) :- group(Group), suspect(Who), murder(Whom), member_of(Who,Group), \+ member_of(Whom,Group), assertz(dislikes(Who,Whom)).
(defrule transduced-prejudiced-on-group-14
    (group ?Group)
    =>
    (coire-publish-goal (str-cat "prejudiced(Who,Whom," ?Group ")")))

; Transduced from: prejudiced(Who,Whom,Group) :- group(Group), suspect(Who), murder(Whom), member_of(Who,Group), \+ member_of(Whom,Group), assertz(dislikes(Who,Whom)).
(defrule transduced-prejudiced-on-suspect-15
    (suspect ?Who)
    =>
    (coire-publish-goal (str-cat "prejudiced(" ?Who ",Whom,Group)")))

; Transduced from: prejudiced(Who,Whom,Group) :- group(Group), suspect(Who), murder(Whom), member_of(Who,Group), \+ member_of(Whom,Group), assertz(dislikes(Who,Whom)).
(defrule transduced-prejudiced-on-murder-16
    (murder ?Whom)
    =>
    (coire-publish-goal (str-cat "prejudiced(Who," ?Whom ",Group)")))

; Transduced from: prejudiced(Who,Whom,Group) :- group(Group), suspect(Who), murder(Whom), member_of(Who,Group), \+ member_of(Whom,Group), assertz(dislikes(Who,Whom)).
(defrule transduced-prejudiced-on-member_of-17
    (member_of ?Who ?Group)
    =>
    (coire-publish-goal (str-cat "prejudiced(" ?Who ",Whom," ?Group ")")))

; Transduced from: prejudiced(Who,Whom,Group) :- group(Group), suspect(Who), murder(Whom), member_of(Who,Group), \+ member_of(Whom,Group), assertz(dislikes(Who,Whom)).
(defrule transduced-prejudiced-on-not_member_of-18
    (not_member_of ?Whom ?Group)
    =>
    (coire-publish-goal (str-cat "prejudiced(Who," ?Whom "," ?Group ")")))

; Transduced from: prejudiced(Who,Whom,Group) :- group(Group), suspect(Who), murder(Whom), member_of(Who,Group), \+ member_of(Whom,Group), assertz(dislikes(Who,Whom)).
(defrule transduced-prejudiced-on-dislikes-19
    (dislikes ?Who ?Whom)
    =>
    (coire-publish-goal (str-cat "prejudiced(" ?Who "," ?Whom ",Group)")))

