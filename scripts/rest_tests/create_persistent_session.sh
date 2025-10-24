#!/usr/bin/env bash
set -euo pipefail

# Wrapper: creates a persistent session and prints the sessionId
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$DIR/_common.sh"

BASE=${BASE_URL:-http://localhost:8080}
AUTH=${AUTH:-}
USER_ID=${USER_ID:-"test-user"}
PRELOAD=${PRELOAD:-}

if [ -n "$PRELOAD" ]; then
  payload=$(jq -n --arg user_id "$USER_ID" --argjson preload $(jq -nc --arg p "$PRELOAD" '[$p]') '{user_id: $user_id, preload: $preload}')
else
  payload=$(jq -n --arg user_id "$USER_ID" '{user_id: $user_id}')
fi

echo "$payload" | jq .
resp=$(http_request POST "$BASE/sessions" "$payload") || exit $?
# extract session_id if present
echo
session_id=$(echo "$resp" | jq -r '.session_id // empty')
if [ -n "$session_id" ]; then
  export SESSION_ID=$session_id
else
  echo "No session_id in response" >&2
  exit 1
fi

# echo our JSON payload containing the session_id
echo "$resp"