#!/bin/bash

# Create base directory and nested folders
mkdir -p clara-clips/src/backend/subprocess
mkdir -p clara-clips/src/backend/ffi
mkdir -p clara-clips/tests

# Create top-level files
touch clara-clips/Cargo.toml
touch clara-clips/build.rs

# Create src files
touch clara-clips/src/lib.rs
touch clara-clips/src/executor.rs
touch clara-clips/src/command.rs
touch clara-clips/src/output.rs
touch clara-clips/src/error.rs
touch clara-clips/src/timeout.rs

# Create backend files
touch clara-clips/src/backend/mod.rs

# Create subprocess files
touch clara-clips/src/backend/subprocess/mod.rs
touch clara-clips/src/backend/subprocess/process.rs
touch clara-clips/src/backend/subprocess/protocol.rs
touch clara-clips/src/backend/subprocess/io.rs

# Create ffi files
touch clara-clips/src/backend/ffi/mod.rs
touch clara-clips/src/backend/ffi/bindings.rs
touch clara-clips/src/backend/ffi/environment.rs
touch clara-clips/src/backend/ffi/conversion.rs

# Create tests files
touch clara-clips/tests/subprocess_tests.rs
touch clara-clips/tests/protocol_tests.rs

echo "clara-clips directory structure created successfully."

