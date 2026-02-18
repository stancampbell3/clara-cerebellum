#!/bin/bash

set -a
source .env
set +a

echo "-- Dis tower erecting --"
RUST_LOG=debug cargo run --bin clara-api 
echo "~~~~~~~~~~~~~~~"	
