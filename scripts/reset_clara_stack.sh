#!/usr/bin/env bash
# reset_clara_stack.sh — full RESET of a Clara stack's data back to empty.
#
# Built after a physical-hardware temperature limit made it unsafe to
# assume stale state (half-finished Rituals, orphaned research-task rows,
# leftover Edgequake documents, stale Kafka consumer groups) is safe to
# carry forward across restarts. This wipes every component's data:
#
#   1. Stops the stack (./clara down)
#   2. clara-api's CoireStore (data/coire.duckdb) — via flush_coire.sh
#   3. lildaemon's DuckDB (lildaemon/data/lildaemon.duc) — users, sessions,
#      ritual configs, research queue — via flush_lildaemon_db.sh
#   4. lildaemon's output/ and workspace/ directories on disk
#   5. Kafka — recreated with no volume, wiping every topic and every
#      Ritual's stale consumer-group offsets in one shot
#   6. Edgequake — ONLY the Clara stack's own workspace (documents,
#      chunks, vectors, AGE graph nodes, PDFs/assets), via Edgequake's
#      scoped bulk-delete API — plus a sweep of leftover adhoc.* demo
#      workspaces from crashed examples_ritual_*.py runs. Edgequake is a
#      separate, genuinely multi-tenant service — this deliberately never
#      touches any other tenant/workspace it might host.
#
# Cobbler (dagda) needs no step of its own: it owns no persistent data —
# its one write path (session bookmarks) lives inside lildaemon's own
# output/ tree, already covered by step 4.
#
# See docs/reset_clara_stack.md for the full rationale and what is
# deliberately left untouched (dagda's unrelated ingestion/training data,
# any other Edgequake tenant).
#
# Usage:
#   ./scripts/reset_clara_stack.sh [--dry-run] [--yes] [--start]
#
#   --dry-run   Show what would be wiped; make no changes.
#   --yes       Skip the interactive confirmation prompt.
#   --start     Bring the stack back up (./clara up -d) once reset.
#               Default: leave it down.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
DEV_ROOT="$(cd "$REPO_ROOT/.." && pwd)"
LILDAEMON_ROOT="$DEV_ROOT/lildaemon"

DRY_RUN=false
ASSUME_YES=false
START_AFTER=false

for arg in "$@"; do
  case "$arg" in
    --dry-run|-n) DRY_RUN=true ;;
    --yes|-y) ASSUME_YES=true ;;
    --start) START_AFTER=true ;;
    *) echo "Unknown argument: $arg" >&2; exit 1 ;;
  esac
done

clara() {
  (cd "$DEV_ROOT" && ./clara "$@")
}

# Row-count preview for --dry-run. Not every deployed node has the
# standalone `duckdb` CLI — falls back to Python's duckdb binding, same as
# flush_coire.sh / flush_lildaemon_db.sh.
duckdb_count_query() {
  local db_path="$1" sql="$2"
  if [[ ! -f "$db_path" ]]; then
    echo "  (no database found at $db_path)"
    return
  fi
  if command -v duckdb &>/dev/null; then
    duckdb "$db_path" "$sql"
    return
  fi
  local py=""
  if python3 -c "import duckdb" &>/dev/null; then
    py="python3"
  elif [[ -x "$LILDAEMON_ROOT/.venv/bin/python3" ]] \
       && "$LILDAEMON_ROOT/.venv/bin/python3" -c "import duckdb" &>/dev/null; then
    py="$LILDAEMON_ROOT/.venv/bin/python3"
  fi
  if [[ -z "$py" ]]; then
    echo "  (neither the duckdb CLI nor a Python with duckdb installed was found)"
    return
  fi
  "$py" -c "
import duckdb, sys
con = duckdb.connect(sys.argv[1], read_only=True)
for row in con.execute(sys.argv[2]).fetchall():
    print(' ', row[0], row[1])
" "$db_path" "$sql"
}

echo "=== Clara stack full RESET ==="
echo "This will wipe:"
echo "  - clara-api's CoireStore (rituals, deductions, coire events)"
echo "  - lildaemon's DuckDB (users, sessions, ritual configs, research queue)"
echo "  - lildaemon's output/ and workspace/ directories"
echo "  - Kafka (all topics + all Ritual consumer-group offsets)"
echo "  - Clara's own Edgequake workspace (documents, chunks, vectors, graph)"
echo
echo "Left untouched:"
echo "  - dagda's unrelated data-ingestion/training work"
echo "  - any other tenant/workspace Edgequake hosts"
echo

if [[ "$DRY_RUN" == false && "$ASSUME_YES" == false ]]; then
  read -r -p "Type RESET to confirm: " CONFIRM
  if [[ "$CONFIRM" != "RESET" ]]; then
    echo "Aborted — no changes made."
    exit 1
  fi
fi

if [[ "$DRY_RUN" == true ]]; then
  echo "--- [DRY RUN] CoireStore (data/coire.duckdb) ---"
  duckdb_count_query "$REPO_ROOT/data/coire.duckdb" \
    "select 'coire_events', count(*) from coire_events
     union all select 'deduction_snapshots', count(*) from deduction_snapshots
     union all select 'tableau_changes', count(*) from tableau_changes
     union all select 'source_registry', count(*) from source_registry
     union all select 'source_artifacts', count(*) from source_artifacts
     union all select 'rituals', count(*) from rituals;"

  echo "--- [DRY RUN] lildaemon DuckDB (lildaemon/data/lildaemon.duc) ---"
  duckdb_count_query "$REPO_ROOT/lildaemon/data/lildaemon.duc" \
    "select 'users', count(*) from users
     union all select 'repl_sessions', count(*) from repl_sessions
     union all select 'assistant_sessions', count(*) from assistant_sessions
     union all select 'assistant_topics', count(*) from assistant_topics
     union all select 'assistant_document_tags', count(*) from assistant_document_tags
     union all select 'assistant_pending_research', count(*) from assistant_pending_research
     union all select 'ritual_configs', count(*) from ritual_configs
     union all select 'ritual_participants', count(*) from ritual_participants;"

  echo "--- [DRY RUN] lildaemon output/ and workspace/ ---"
  du -sh "$REPO_ROOT/lildaemon/output" "$REPO_ROOT/lildaemon/workspace" 2>/dev/null \
    || echo "  (nothing on disk yet)"

  echo "--- [DRY RUN] Kafka ---"
  echo "  Would recreate the kafka container (no volume — wipes every topic"
  echo "  and every Ritual's consumer-group offsets)."

  echo "--- [DRY RUN] Edgequake ---"
  "$SCRIPT_DIR/reset_edgequake_clara_workspace.sh" --dry-run || true

  echo
  echo "[DRY RUN] No changes were made."
  exit 0
fi

echo "--- Stopping the stack ---"
clara down

echo "--- Flushing CoireStore ---"
"$SCRIPT_DIR/flush_coire.sh" "$REPO_ROOT/data/coire.duckdb"

echo "--- Flushing lildaemon's DuckDB ---"
"$LILDAEMON_ROOT/scripts/flush_lildaemon_db.sh" "$REPO_ROOT/lildaemon/data/lildaemon.duc"

echo "--- Purging lildaemon output/ and workspace/ ---"
(cd "$REPO_ROOT/lildaemon" && "$LILDAEMON_ROOT/scripts/purge_output.sh")
(cd "$REPO_ROOT/lildaemon" && "$LILDAEMON_ROOT/scripts/purge_workspace.sh")

echo "--- Recreating Kafka (wipes all topics + consumer-group offsets) ---"
clara up -d --no-deps kafka
clara stop kafka
clara rm -f kafka

echo "--- Resetting Clara's Edgequake workspace ---"
"$SCRIPT_DIR/reset_edgequake_clara_workspace.sh"

echo "--- Sweeping leftover adhoc demo workspaces ---"
if [[ -f "$REPO_ROOT/docker/.env" ]]; then
  # shellcheck disable=SC1091
  set -a; source "$REPO_ROOT/docker/.env"; set +a
fi
if [[ -n "${EDGEQUAKE_DEFAULT_TENANT:-}" ]]; then
  "$SCRIPT_DIR/reset_adhoc_workspaces.sh" --tenant "$EDGEQUAKE_DEFAULT_TENANT" --pattern 'adhoc.*' || true
else
  echo "  (EDGEQUAKE_DEFAULT_TENANT not set — skipping adhoc-workspace sweep)"
fi

echo
echo "=== Reset complete ==="
echo "Wiped: CoireStore, lildaemon DuckDB, lildaemon output/workspace, Kafka, Clara's Edgequake workspace."
echo "Left untouched: dagda's unrelated ingestion/training data, any other Edgequake tenant."

if [[ "$START_AFTER" == true ]]; then
  echo "--- Starting the stack back up ---"
  clara up -d
else
  echo "Stack left down. Run './clara up -d' when ready."
fi
