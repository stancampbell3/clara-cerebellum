#!/bin/bash
echo "Transduce test source..."
CWD=$(pwd)
cd $CWD/clara-frontdesk-poc/roost
./target/debug/transduction front_desk_poc_reprise.pl
echo "🐇"
cd $CWD
