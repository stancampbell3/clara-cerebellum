#!/usr/bin/env bash
set -euo pipefail

BASE=${BASE_URL:-http://localhost:8080}
AUTH=${AUTH:-}
SESSION_ID=${SESSION_ID:-}
SCRIPT=${SCRIPT:-"(printout t \"Hello from session eval\" crlf)"}
TIMEOUT_MS=${TIMEOUT_MS:-2000}

if [ -z "$SESSION_ID" ]; then
  echo "SESSION_ID must be provided (env var)" >&2
  exit 2
fi

payload=$(jq -n --argjson commands "[$SCRIPT]" --argjson t $TIMEOUT_MS '{commands: [$commands[0]], timeout_ms: $t}')
# More robust: build commands array properly
payload=$(jq -n --arg cmd "$SCRIPT" --argjson t $TIMEOUT_MS '{commands: [$cmd], timeout_ms: $t}')

if [ -n "$AUTH" ]; then
  curl -sS -H "Content-Type: application/json" -H "Authorization: Bearer $AUTH" -d "$payload" "$BASE/sessions/$SESSION_ID/eval" | jq .
else
  curl -sS -H "Content-Type: application/json" -d "$payload" "$BASE/sessions/$SESSION_ID/eval" | jq .
fi

