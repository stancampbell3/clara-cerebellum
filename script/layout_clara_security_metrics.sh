#!/bin/bash

# Create base directory and subfolders
mkdir -p clara-metrics/src/metrics
mkdir -p clara-metrics/src/tracing
mkdir -p clara-metrics/src/logging
mkdir -p clara-metrics/tests

# Create top-level file
touch clara-metrics/Cargo.toml

# Create src files
touch clara-metrics/src/lib.rs
touch clara-metrics/src/exporter.rs

# Create metrics module files
touch clara-metrics/src/metrics/mod.rs
touch clara-metrics/src/metrics/counters.rs
touch clara-metrics/src/metrics/histograms.rs
touch clara-metrics/src/metrics/gauges.rs

# Create tracing module files
touch clara-metrics/src/tracing/mod.rs
touch clara-metrics/src/tracing/setup.rs
touch clara-metrics/src/tracing/spans.rs

# Create logging module files
touch clara-metrics/src/logging/mod.rs
touch clara-metrics/src/logging/structured.rs
touch clara-metrics/src/logging/redaction.rs

# Create tests file
touch clara-metrics/tests/metrics_tests.rs

echo "clara-metrics directory structure created successfully."

