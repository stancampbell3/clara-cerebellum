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
  payload=$(jq -n --arg user_id "$USER_ID" --argjson preload "$(jq -nc --arg p "$PRELOAD" '[$p]')" '{user_id: $user_id, preload: $preload}')
else
  payload=$(jq -n --arg user_id "$USER_ID" '{user_id: $user_id}')
fi

echo "Creating session with payload:"
echo "$payload" | jq .

resp=$(http_request POST "$BASE/sessions" "$payload") || exit $?

echo "$resp" | jq . || echo "$resp"