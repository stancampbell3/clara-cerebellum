#!/usr/bin/env bash
set -euo pipefail

BASE=${BASE_URL:-http://localhost:8080}
AUTH=${AUTH:-}
SCRIPT=${SCRIPT:-"(printout t \"Hello from ephemeral eval\" crlf)"}
TIMEOUT_MS=${TIMEOUT_MS:-2000}

payload=$(jq -n --arg script "$SCRIPT" --argjson t $TIMEOUT_MS '{script: $script, timeout_ms: $t}')

if [ -n "$AUTH" ]; then
  curl -sS -H "Content-Type: application/json" -H "Authorization: Bearer $AUTH" -d "$payload" "$BASE/eval" | jq .
else
  curl -sS -H "Content-Type: application/json" -d "$payload" "$BASE/eval" | jq .
fi

