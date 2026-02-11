#!/usr/bin/bash
set -a
source .env
set +a
RUST_LOG=debug cargo run --bin clara-api
