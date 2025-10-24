#!/usr/bin/env bash
set -euo pipefail

BASE=${BASE_URL:-http://localhost:8080}
AUTH=${AUTH:-}
SESSION_ID=${SESSION_ID:-}
LABEL=${LABEL:-"checkpoint-$(date -u +%s)"}

if [ -z "$SESSION_ID" ]; then
  echo "SESSION_ID must be provided (env var)" >&2
  exit 2
fi

payload=$(jq -n --arg label "$LABEL" '{label: $label}')

if [ -n "$AUTH" ]; then
  curl -sS -H "Content-Type: application/json" -H "Authorization: Bearer $AUTH" -d "$payload" "$BASE/sessions/$SESSION_ID/save" | jq .
else
  curl -sS -H "Content-Type: application/json" -d "$payload" "$BASE/sessions/$SESSION_ID/save" | jq .
fi

