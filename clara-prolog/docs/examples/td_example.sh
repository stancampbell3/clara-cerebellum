#!/bin/bash
if [ -z "$1" ]; then
  echo "Usage: $0 <example_number>"
  exit 1
fi

N=$1
SRC="ex${N}.pl"

if [ ! -f "$SRC" ]; then
  echo "Source file $SRC not found."
  exit 1
fi

TD_CMD=/home/stanc/Desktop/Development/clara-cerebrum/target/debug/transduction
echo "Transducing $SRC..."
eval "$TD_CMD $SRC"
echo "Done."
