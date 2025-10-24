#!/usr/bin/env bash
set -euo pipefail

BASE=${BASE_URL:-http://localhost:8080}
AUTH=${AUTH:-}
USER_ID=${USER_ID:-"test-user"}
PRELOAD=${PRELOAD:-}

# Build payload
if [ -n "$PRELOAD" ]; then
  payload=$(jq -n --arg userId "$USER_ID" --argjson preload $(jq -nc --arg p "$PRELOAD" '[$p]') '{userId: $userId, preload: $preload}')
else
  payload=$(jq -n --arg userId "$USER_ID" '{userId: $userId}')
fi

if [ -n "$AUTH" ]; then
  curl -sS -H "Content-Type: application/json" -H "Authorization: Bearer $AUTH" -d "$payload" "$BASE/sessions" | jq .
else
  curl -sS -H "Content-Type: application/json" -d "$payload" "$BASE/sessions" | jq .
fi

