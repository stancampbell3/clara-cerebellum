#!/bin/bash
echo "Transduce test source..."
CWD=$(pwd)
TD_CMD=$CWD/target/debug/transduction
cd $CWD/clara-frontdesk-poc/roost
eval "$TD_CMD front_desk_poc_reprise.pl"
echo "🐇"
cd $CWD
