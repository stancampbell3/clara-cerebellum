#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<EOF
Usage: $0 [--rest] [--help]

Runs the Rust workspace tests. If --rest is provided, also runs the REST orchestrator
scripts/rest_tests/all_tests.sh (requires the service to be running at BASE_URL).

Environment:
  BASE_URL - base URL for REST tests (default: http://localhost:8080)
  AUTH     - optional bearer token forwarded to REST tests

Examples:
  # run unit/integration tests only
  $0

  # run tests and then the REST happy-path (service must be running)
  BASE_URL=http://localhost:8080 $0 --rest
EOF
}

RUN_REST=false
for arg in "$@"; do
  case "$arg" in
    --rest) RUN_REST=true ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown arg: $arg" >&2; usage; exit 2 ;;
  esac
done

echo "Running cargo test --workspace"
# Run workspace tests
cargo test --workspace

if [ "$RUN_REST" = true ]; then
  REST_SCRIPT="./scripts/rest_tests/all_tests.sh"
  if [ ! -x "$REST_SCRIPT" ]; then
    if [ -f "$REST_SCRIPT" ]; then
      chmod +x "$REST_SCRIPT"
    else
      echo "REST orchestrator not found at $REST_SCRIPT" >&2
      exit 3
    fi
  fi

  echo "Running REST orchestrator"
  BASE_URL=${BASE_URL:-http://localhost:8080}
  echo "Using BASE_URL=$BASE_URL"
  # Forward AUTH if set
  if [ -n "${AUTH:-}" ]; then
    AUTH="$AUTH" BASE_URL="$BASE_URL" "$REST_SCRIPT"
  else
    BASE_URL="$BASE_URL" "$REST_SCRIPT"
  fi
fi
