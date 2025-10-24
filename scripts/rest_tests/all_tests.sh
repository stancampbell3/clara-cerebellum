#!/usr/bin/env bash
set -euo pipefail

# Runs a simple happy-path through the API: health -> ephemeral eval -> create session -> eval -> save -> delete
BASE=${BASE_URL:-http://localhost:8080}
AUTH=${AUTH:-}

echo "BASE_URL=$BASE"

echo "==> Health"
BASE_URL=$BASE AUTH=$AUTH scripts/rest_tests/health.sh

echo "==> Ephemeral eval"
BASE_URL=$BASE AUTH=$AUTH SCRIPT='(printout t \"ephemeral hello\" crlf)' scripts/rest_tests/eval_ephemeral.sh

echo "==> Create persistent session"
resp=$(BASE_URL=$BASE AUTH=$AUTH scripts/rest_tests/create_persistent_session.sh)
echo "$resp"
SESSION_ID=$(echo "$resp" | jq -r '.sessionId // empty')
if [ -z "$SESSION_ID" ]; then
  echo "failed to create session" >&2
  exit 3
fi
export SESSION_ID

echo "Created session: $SESSION_ID"

echo "==> Eval against session"
BASE_URL=$BASE AUTH=$AUTH SESSION_ID=$SESSION_ID SCRIPT='(printout t \"session hello\" crlf)' scripts/rest_tests/eval_session.sh

echo "==> Save session"
BASE_URL=$BASE AUTH=$AUTH SESSION_ID=$SESSION_ID LABEL='checkpoint-1' scripts/rest_tests/save_session.sh

echo "==> Delete session"
BASE_URL=$BASE AUTH=$AUTH SESSION_ID=$SESSION_ID scripts/rest_tests/delete_session.sh

echo "All tests finished"

