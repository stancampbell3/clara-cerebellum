#!/bin/bash

# Create base directory and subfolders
mkdir -p clara-security/src/auth
mkdir -p clara-security/src/filter
mkdir -p clara-security/src/sandbox
mkdir -p clara-security/tests

# Create top-level file
touch clara-security/Cargo.toml

# Create src files
touch clara-security/src/lib.rs
touch clara-security/src/audit.rs
touch clara-security/src/crypto.rs

# Create auth module files
touch clara-security/src/auth/mod.rs
touch clara-security/src/auth/jwt.rs
touch clara-security/src/auth/rbac.rs
touch clara-security/src/auth/scopes.rs

# Create filter module files
touch clara-security/src/filter/mod.rs
touch clara-security/src/filter/command_filter.rs
touch clara-security/src/filter/path_validator.rs

# Create sandbox module files
touch clara-security/src/sandbox/mod.rs
touch clara-security/src/sandbox/limits.rs
touch clara-security/src/sandbox/isolation.rs

# Create tests files
touch clara-security/tests/filter_tests.rs
touch clara-security/tests/auth_tests.rs

echo "clara-security directory structure created successfully."

