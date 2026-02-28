#!/bin/bash

set -a
source .env
set +a
HTTP_PORT=1951
echo "-- North Tower erecting --"
RUST_LOG=debug cargo run --bin clips-mcp-adapter 
echo "~~~~~~~~~~~~~~~"	
