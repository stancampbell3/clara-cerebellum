#!/usr/bin/env bash
set -euo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$DIR/_common.sh"

# Runs a simple happy-path through the API: health -> ephemeral eval -> create session -> eval -> save -> delete
BASE=${BASE_URL:-http://localhost:8080}
AUTH=${AUTH:-}

export BASE_URL="$BASE"
export AUTH="$AUTH"

echo "BASE_URL=$BASE"

echo "==> Health"
"$DIR/health.sh" || { echo "Health check failed" >&2; exit 2; }

# NOTE: Ephemeral eval (/eval endpoint) not yet implemented
# echo "==> Ephemeral eval"
# SCRIPT='(printout t "ephemeral hello" crlf)'
# BASE_URL="$BASE" AUTH="$AUTH" SCRIPT="$SCRIPT" "$DIR/eval_ephemeral.sh" || { echo "Ephemeral eval failed" >&2; exit 3; }

echo "==> Create persistent session"
resp=$(BASE_URL="$BASE" AUTH="$AUTH" "$DIR/create_persistent_session.sh") || { echo "Create session failed" >&2; echo "$resp" >&2; exit 4; }
echo "Create session response:"
echo "$resp"
SESSION_ID=$(echo "$resp" | jq -r '.session_id // empty')
if [ -z "$SESSION_ID" ]; then
  echo "failed to create session" >&2
  exit 3
fi
export SESSION_ID

echo "Created session: $SESSION_ID"

echo "Give the session a moment to initialize..."
sleep 30

echo "==> Eval against session"
BASE_URL="$BASE" AUTH="$AUTH" SESSION_ID="$SESSION_ID" SCRIPT='(printout t "session hello" crlf)' "$DIR/eval_session.sh" || { echo "Session eval failed" >&2; exit 5; }

echo "==> Save session"
BASE_URL="$BASE" AUTH="$AUTH" SESSION_ID="$SESSION_ID" LABEL='checkpoint-1' "$DIR/save_session.sh" || { echo "Save session failed" >&2; exit 6; }

echo "==> Delete session"
BASE_URL="$BASE" AUTH="$AUTH" SESSION_ID="$SESSION_ID" "$DIR/delete_session.sh" || { echo "Delete session failed" >&2; exit 7; }

echo "All tests finished"
