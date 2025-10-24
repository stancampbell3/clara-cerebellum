#!/usr/bin/env bash
set -euo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$DIR/_common.sh"

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

resp=$(http_request POST "$BASE/sessions/$SESSION_ID/load" "$payload") || exit $?

echo "$resp" | jq . || echo "$resp"
