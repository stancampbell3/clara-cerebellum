#!/bin/bash

set -a
source .env
set +a

HTTP_PORT=1968
echo "-- South Tower erecting --"
RUST_LOG=debug cargo run --bin prolog-mcp-adapter 
echo "~~~~~~~~~~~~~~~"
