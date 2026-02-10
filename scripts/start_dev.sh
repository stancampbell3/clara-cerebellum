#!/usr/bin/bash
source .env
RUST_LOG=debug JWT_SECRET=mysecretjwt cargo run --bin clara-api
