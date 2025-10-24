#!/usr/bin/env bash
set -euo pipefail

echo "Bootstrapping developer environment for clara-cerebrum"

if ! command -v rustup >/dev/null 2>&1; then
  cat <<EOF
rustup not found. Please install rustup before continuing:
https://rustup.rs/
EOF
  exit 1
fi

# Ensure stable toolchain is present
echo "Ensuring Rust toolchain 'stable' is installed..."
rustup toolchain install stable --no-self-update || true
rustup default stable

# Build workspace
echo "Building the full workspace (this may take a while)..."
cargo build --workspace

cat <<EOF
Bootstrap finished.
Next steps you might want to run:
  - ./scripts/start_dev.sh   # to run the API server locally (uses `cargo run --bin clara-api`)
  - ./scripts/run-tests.sh  # run workspace tests, optionally with --rest
  - docker-compose -f docker/docker-compose.yml up --build   # optionally start services in docker
EOF
