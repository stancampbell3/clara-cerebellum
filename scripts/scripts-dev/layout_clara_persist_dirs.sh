#!/bin/bash

# Create base directory and subfolders
mkdir -p clara-persistence/src/format
mkdir -p clara-persistence/src/storage
mkdir -p clara-persistence/tests

# Create top-level file
touch clara-persistence/Cargo.toml

# Create src files
touch clara-persistence/src/lib.rs
touch clara-persistence/src/codec.rs
touch clara-persistence/src/integrity.rs
touch clara-persistence/src/migration.rs

# Create format module files
touch clara-persistence/src/format/mod.rs
touch clara-persistence/src/format/json.rs
touch clara-persistence/src/format/binary.rs

# Create storage module files
touch clara-persistence/src/storage/mod.rs
touch clara-persistence/src/storage/filesystem.rs
touch clara-persistence/src/storage/s3.rs

# Create tests file
touch clara-persistence/tests/roundtrip_tests.rs

echo "clara-persistence directory structure created successfully."

