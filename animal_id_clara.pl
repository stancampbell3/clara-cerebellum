use_module(library(the_rabbit)).
use_module(library(the_coire)).
nonanimal(X) :- \+ animal(X), coire_publish_assert(nonanimal(X)).
vertebrata(X) :- has(X,backbone), animal(X), coire_publish_assert(vertebrata(X)).
nonvertebrata(X) :- animal(X), \+ has(X,backbone), coire_publish_assert(nonvertebrata(X)).
reptiles(X) :- vertebrata(X), has(X,cold_blooded), has(X,scaly_skin), coire_publish_assert(reptiles(X)).
fish(X) :- vertebrata(X), has(X,cold_blooded), has(X,gills), has(X,scaly_skin), coire_publish_assert(fish(X)).
amphibi(X) :- vertebrata(X), has(X,cold_blooded), has(X,slimy_skin), coire_publish_assert(amphibi(X)).
molluscs(X) :- nonvertebrata(X), has(X,soft_body), coire_publish_assert(molluscs(X)).
annelid(X) :- nonvertebrata(X), has(X,segmented_body), coire_publish_assert(annelid(X)).
arthropods(X) :- nonvertebrata(X), has(X,external_skeleton), coire_publish_assert(arthropods(X)).
arachnid(X) :- arthropods(X), has(X,leg_8), coire_publish_assert(arachnid(X)).
insect(X) :- arthropods(X), has(X,leg_6), coire_publish_assert(insect(X)).
mammal(X) :- vertebrata(X), has(X,warm_blooded), \+ has(X,feather), coire_publish_assert(mammal(X)).
bird(X) :- vertebrata(X), has(X,warm_blooded), has(X,feather), coire_publish_assert(bird(X)).
animal(cat).
animal(shark).
animal(tiger).
animal(eagle).
animal(snake).
animal(frog).
animal(spider).
animal(bee).
animal(snail).
animal(worm).
animal(scorpion).
has(cat,backbone).
has(cat,warm_blooded).
has(shark,backbone).
has(shark,gills).
has(shark,cold_blooded).
has(shark,scaly_skin).
has(tiger,backbone).
has(tiger,warm_blooded).
has(eagle,backbone).
has(eagle,feather).
has(eagle,warm_blooded).
has(snake,backbone).
has(snake,cold_blooded).
has(snake,scaly_skin).
has(frog,backbone).
has(frog,cold_blooded).
has(frog,slimy_skin).
has(spider,leg_8).
has(spider,external_skeleton).
has(scorpion,leg_8).
has(scorpion,external_skeleton).
has(bee,leg_6).
has(bee,external_skeleton).
has(snail,soft_body).
has(worm,segmented_body).
