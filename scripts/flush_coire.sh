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

echo "Flushing: $DB_PATH"

if command -v duckdb &>/dev/null; then
    duckdb "$DB_PATH" < "$SQL_FILE"
else
    # Deployed Clara nodes won't always have the standalone `duckdb` CLI —
    # fall back to the Python `duckdb` binding, which lildaemon's own venv
    # (a sibling repo, needed for its own DuckDB access) already provides.
    PY=""
    if python3 -c "import duckdb" &>/dev/null; then
        PY="python3"
    elif [[ -x "$REPO_ROOT/../lildaemon/.venv/bin/python3" ]] \
         && "$REPO_ROOT/../lildaemon/.venv/bin/python3" -c "import duckdb" &>/dev/null; then
        PY="$REPO_ROOT/../lildaemon/.venv/bin/python3"
    fi
    if [[ -z "$PY" ]]; then
        echo "Error: neither the 'duckdb' CLI nor a Python with the 'duckdb'" >&2
        echo "package installed could be found." >&2
        exit 1
    fi
    "$PY" -c "
import duckdb, sys
con = duckdb.connect(sys.argv[1])
con.execute(open(sys.argv[2]).read())
" "$DB_PATH" "$SQL_FILE"
fi
echo "Done... 🦫"
