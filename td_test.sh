#!/bin/bash
echo "Transduce test source..."
TD_CMD=/home/stanc/Desktop/Development/clara-cerebrum/target/debug/transduction
CWD=$(pwd)
cd $CWD/clara-frontdesk-poc/roost
eval "$TD_CMD front_desk_poc_reprise.pl"
echo "🐇"
cd $CWD
