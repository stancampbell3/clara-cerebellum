#!/usr/bin/env bash
# flush_coire.sh — wipe all rows from ./data/coire.duckdb without dropping schema.
# Safe to run between test runs; tables are recreated automatically on next API start.
#
# Usage:
#   ./scripts/flush_coire.sh [DB_PATH]
#
# DB_PATH defaults to ./data/coire.duckdb relative to the repo root.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

DB_PATH="${1:-$REPO_ROOT/data/coire.duckdb}"
SQL_FILE="$SCRIPT_DIR/drop_coire_data.sql"

if [[ ! -f "$DB_PATH" ]]; then
    echo "No database found at: $DB_PATH"
    echo "Nothing to flush."
    exit 0
fi

if ! command -v duckdb &>/dev/null; then
    echo "Error: 'duckdb' CLI not found in PATH." >&2
    exit 1
fi

echo "Flushing: $DB_PATH"
duckdb "$DB_PATH" < "$SQL_FILE"
echo "Done... 🦫"
