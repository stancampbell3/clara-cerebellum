%% animal_id.pl
%% ------------

use_module(library(the_rabbit)).
use_module(library(the_coire)).

nonanimal(X):-not(animal(X)).
vertebrata(X):-has(X,backbone),animal(X).
nonvertebrata(X):-animal(X),not(has(X,backbone)).
reptiles(X):-vertebrata(X),has(X,cold_blooded),has(X,scaly_skin).
fish(X):-vertebrata(X),has(X,cold_blooded),has(X,gills),has(X,scaly_skin).
amphibi(X):-vertebrata(X),has(X,cold_blooded),has(X,slimy_skin).
molluscs(X):-nonvertebrata(X),has(X,soft_body).
annelid(X):-nonvertebrata(X),has(X,segmented_body).
arthropods(X):-nonvertebrata(X),has(X,external_skeleton).
arachnid(X):-arthropods(X),has(X,leg_8).
insect(X):-arthropods(X),has(X,leg_6).
mammal(X):-vertebrata(X),has(X,warm_blooded),not(has(X,feather)).
bird(X):-vertebrata(X),has(X,warm_blooded),has(X,feather).

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
