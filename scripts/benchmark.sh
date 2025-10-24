#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<EOF
Usage: $0 [--bench <name>] [--help]

Runs cargo benches for the workspace or a specific benchmark.

Options:
  --bench NAME   Run a specific benchmark (passed as --bench <NAME> to cargo)
  --help         Show this help

Examples:
  # run all benches
  $0

  # run a single benchmark
  $0 --bench eval_throughput
EOF
}

BENCH_NAME=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --bench)
      shift
      BENCH_NAME="$1"
      shift
      ;;
    -h|--help)
      usage; exit 0
      ;;
    *) echo "Unknown arg: $1" >&2; usage; exit 2 ;;
  esac
done

if [ -n "$BENCH_NAME" ]; then
  echo "Running cargo bench --bench $BENCH_NAME"
  cargo bench --bench "$BENCH_NAME"
else
  echo "Running cargo bench --workspace"
  cargo bench --workspace
fi
