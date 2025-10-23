#!/bin/bash

# Create base directory and subfolders
mkdir -p clara-config/src clara-config/tests

# Create top-level file
touch clara-config/Cargo.toml

# Create src files
touch clara-config/src/lib.rs
touch clara-config/src/loader.rs
touch clara-config/src/schema.rs
touch clara-config/src/validation.rs
touch clara-config/src/defaults.rs
touch clara-config/src/env.rs

# Create tests file
touch clara-config/tests/config_tests.rs

echo "clara-config directory structure created successfully."

