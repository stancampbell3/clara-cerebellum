#!/bin/bash

# Create base directory and subfolders
mkdir -p clara-session/src clara-session/tests

# Create top-level file
touch clara-session/Cargo.toml

# Create src files
touch clara-session/src/lib.rs
touch clara-session/src/manager.rs
touch clara-session/src/store.rs
touch clara-session/src/lifecycle.rs
touch clara-session/src/eviction.rs
touch clara-session/src/queue.rs
touch clara-session/src/metadata.rs

# Create tests files
touch clara-session/tests/manager_tests.rs
touch clara-session/tests/eviction_tests.rs

echo "clara-session directory structure created successfully."

