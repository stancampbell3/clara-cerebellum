#!/usr/bin/env bash
set -euo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$DIR/_common.sh"

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

resp=$(http_request POST "$BASE/sessions" "$payload") || exit $?

echo "$resp" | jq . || echo "$resp"
