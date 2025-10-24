#!/usr/bin/env bash
set -euo pipefail

BASE=${BASE_URL:-http://localhost:8080}
AUTH=${AUTH:-}
SESSION_ID=${SESSION_ID:-}

if [ -z "$SESSION_ID" ]; then
  echo "SESSION_ID must be provided (env var)" >&2
  exit 2
fi

if [ -n "$AUTH" ]; then
  curl -sS -X DELETE -H "Authorization: Bearer $AUTH" "$BASE/sessions/$SESSION_ID" | jq .
else
  curl -sS -X DELETE "$BASE/sessions/$SESSION_ID" | jq .
fi

