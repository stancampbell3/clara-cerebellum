#!/usr/bin/env bash
set -euo pipefail

BASE=${BASE_URL:-http://localhost:8080}
AUTH=${AUTH:-}
SESSION_ID=${SESSION_ID:-}
FILES=${FILES:-}

if [ -z "$SESSION_ID" ]; then
  echo "SESSION_ID must be provided (env var)" >&2
  exit 2
fi
if [ -z "$FILES" ]; then
  echo "FILES must be provided (comma-separated)" >&2
  exit 2
fi

# split comma separated into JSON array
IFS=',' read -ra arr <<< "$FILES"
json_files=$(printf '%s
' "${arr[@]}" | jq -R . | jq -s .)
payload=$(jq -n --argjson files "$json_files" '{files: $files}')

if [ -n "$AUTH" ]; then
  curl -sS -H "Content-Type: application/json" -H "Authorization: Bearer $AUTH" -d "$payload" "$BASE/sessions/$SESSION_ID/load" | jq .
else
  curl -sS -H "Content-Type: application/json" -d "$payload" "$BASE/sessions/$SESSION_ID/load" | jq .
fi

