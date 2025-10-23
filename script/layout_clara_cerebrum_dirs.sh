#!/bin/bash

# Create base directory
mkdir -p clara-cerebrum

# Create top-level files
touch clara-cerebrum/Cargo.toml
touch clara-cerebrum/Cargo.lock
touch clara-cerebrum/README.md
touch clara-cerebrum/LICENSE
touch clara-cerebrum/.gitignore
touch clara-cerebrum/.env.example
touch clara-cerebrum/rust-toolchain.toml
touch clara-cerebrum/deny.toml
touch clara-cerebrum/clippy.toml
touch clara-cerebrum/rustfmt.toml

# Create docs and ADR files
mkdir -p clara-cerebrum/docs/ADR
touch clara-cerebrum/docs/API.md
touch clara-cerebrum/docs/DEPLOYMENT.md
touch clara-cerebrum/docs/SECURITY.md
touch clara-cerebrum/docs/DEVELOPMENT.md
touch clara-cerebrum/docs/ADR/001-backend-selection.md
touch clara-cerebrum/docs/ADR/002-persistence-format.md
touch clara-cerebrum/docs/ADR/003-security-model.md

# Create config files
mkdir -p clara-cerebrum/config
touch clara-cerebrum/config/default.toml
touch clara-cerebrum/config/development.toml
touch clara-cerebrum/config/production.toml
touch clara-cerebrum/config/schema.json

# Create scripts
mkdir -p clara-cerebrum/scripts
touch clara-cerebrum/scripts/setup-dev.sh
touch clara-cerebrum/scripts/run-tests.sh
touch clara-cerebrum/scripts/benchmark.sh
touch clara-cerebrum/scripts/security-audit.sh
touch clara-cerebrum/scripts/docker-build.sh

# Create migrations
mkdir -p clara-cerebrum/migrations
touch clara-cerebrum/migrations/001_initial_schema.sql

# Create clips structure
mkdir -p clara-cerebrum/clips/binaries
mkdir -p clara-cerebrum/clips/rules/examples
mkdir -p clara-cerebrum/clips/tests
touch clara-cerebrum/clips/binaries/.gitkeep
touch clara-cerebrum/clips/rules/base_rules.clp
touch clara-cerebrum/clips/tests/basic.clp
touch clara-cerebrum/clips/tests/persistence.clp
touch clara-cerebrum/clips/tests/stress.clp

# Create crates
mkdir -p clara-cerebrum/crates/clara-api
mkdir -p clara-cerebrum/crates/clara-core
mkdir -p clara-cerebrum/crates/clara-clips
mkdir -p clara-cerebrum/crates/clara-session
mkdir -p clara-cerebrum/crates/clara-persistence
mkdir -p clara-cerebrum/crates/clara-security
mkdir -p clara-cerebrum/crates/clara-metrics
mkdir -p clara-cerebrum/crates/clara-config

# Create tests structure
mkdir -p clara-cerebrum/tests/integration
mkdir -p clara-cerebrum/tests/load
mkdir -p clara-cerebrum/tests/chaos
mkdir -p clara-cerebrum/tests/fixtures
touch clara-cerebrum/tests/integration/eval_tests.rs
touch clara-cerebrum/tests/integration/session_tests.rs
touch clara-cerebrum/tests/integration/persistence_tests.rs
touch clara-cerebrum/tests/integration/security_tests.rs
touch clara-cerebrum/tests/load/concurrent_eval.rs
touch clara-cerebrum/tests/load/session_churn.rs
touch clara-cerebrum/tests/chaos/timeout_storms.rs
touch clara-cerebrum/tests/chaos/process_kill.rs
touch clara-cerebrum/tests/fixtures/test_rules.clp
touch clara-cerebrum/tests/fixtures/test_data.json

# Create benches
mkdir -p clara-cerebrum/benches
touch clara-cerebrum/benches/eval_throughput.rs
touch clara-cerebrum/benches/session_lifecycle.rs

# Create docker files
mkdir -p clara-cerebrum/docker
touch clara-cerebrum/docker/Dockerfile
touch clara-cerebrum/docker/Dockerfile.dev
touch clara-cerebrum/docker/docker-compose.yml

# Create GitHub workflows
mkdir -p clara-cerebrum/.github/workflows
touch clara-cerebrum/.github/workflows/ci.yml
touch clara-cerebrum/.github/workflows/security.yml
touch clara-cerebrum/.github/workflows/release.yml

echo "clara-cerebrum directory structure created successfully."

