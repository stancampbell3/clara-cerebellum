#!/bin/bash

# Create base directory
mkdir -p clara-core/src/service clara-core/src/types clara-core/tests

# Create top-level file
touch clara-core/Cargo.toml

# Create src files
touch clara-core/src/lib.rs
touch clara-core/src/error.rs
touch clara-core/src/traits.rs

# Create service files
touch clara-core/src/service/mod.rs
touch clara-core/src/service/eval_service.rs
touch clara-core/src/service/session_service.rs
touch clara-core/src/service/load_service.rs

# Create types files
touch clara-core/src/types/mod.rs
touch clara-core/src/types/session.rs
touch clara-core/src/types/eval_result.rs
touch clara-core/src/types/resource_limits.rs

# Create tests file
touch clara-core/tests/service_tests.rs

echo "clara-core directory structure created successfully."

