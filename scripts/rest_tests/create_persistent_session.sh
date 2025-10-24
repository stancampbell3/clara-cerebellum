#!/usr/bin/env bash
set -euo pipefail

# Wrapper: creates a persistent session and prints the sessionId
BASE=${BASE_URL:-http://localhost:8080}
AUTH=${AUTH:-}
USER_ID=${USER_ID:-"test-user"}
PRELOAD=${PRELOAD:-}

if [ -n "$PRELOAD" ]; then
  payload=$(jq -n --arg userId "$USER_ID" --argjson preload $(jq -nc --arg p "$PRELOAD" '[$p]') '{userId: $userId, preload: $preload}')
else
  payload=$(jq -n --arg userId "$USER_ID" '{userId: $userId}')
fi

resp=$(if [ -n "$AUTH" ]; then
  curl -sS -H "Content-Type: application/json" -H "Authorization: Bearer $AUTH" -d "$payload" "$BASE/sessions"
else
  curl -sS -H "Content-Type: application/json" -d "$payload" "$BASE/sessions"
fi)

echo "$resp" | jq .
# extract sessionId if present
echo
sessionId=$(echo "$resp" | jq -r '.sessionId // empty')
if [ -n "$sessionId" ]; then
  echo "SESSION_ID=$sessionId"
else
  echo "No sessionId in response" >&2
  exit 1
fi

