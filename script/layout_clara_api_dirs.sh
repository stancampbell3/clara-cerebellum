#!/bin/bash

# Create base directory
mkdir -p clara-api/src/{middleware,routes,handlers,models,validation} clara-api/tests

# Create top-level file
touch clara-api/Cargo.toml

# Create src files
touch clara-api/src/main.rs
touch clara-api/src/lib.rs
touch clara-api/src/server.rs

# Create middleware files
touch clara-api/src/middleware/mod.rs
touch clara-api/src/middleware/auth.rs
touch clara-api/src/middleware/rate_limit.rs
touch clara-api/src/middleware/cors.rs
touch clara-api/src/middleware/tracing.rs

# Create routes files
touch clara-api/src/routes/mod.rs
touch clara-api/src/routes/eval.rs
touch clara-api/src/routes/sessions.rs
touch clara-api/src/routes/health.rs
touch clara-api/src/routes/metrics.rs

# Create handlers files
touch clara-api/src/handlers/mod.rs
touch clara-api/src/handlers/eval_handler.rs
touch clara-api/src/handlers/session_handler.rs
touch clara-api/src/handlers/error_handler.rs

# Create models files
touch clara-api/src/models/mod.rs
touch clara-api/src/models/request.rs
touch clara-api/src/models/response.rs
touch clara-api/src/models/error.rs

# Create validation files
touch clara-api/src/validation/mod.rs
touch clara-api/src/validation/input.rs

# Create tests file
touch clara-api/tests/api_tests.rs

echo "Directory and file structure created successfully."

