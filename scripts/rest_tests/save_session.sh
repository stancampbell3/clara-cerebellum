#!/usr/bin/env bash
set -euo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$DIR/_common.sh"

BASE=${BASE_URL:-http://localhost:8080}
AUTH=${AUTH:-}
SESSION_ID=${SESSION_ID:-}
LABEL=${LABEL:-"checkpoint-$(date -u +%s)"}

if [ -z "$SESSION_ID" ]; then
  echo "SESSION_ID must be provided (env var)" >&2
  exit 2
fi

payload=$(jq -n --arg label "$LABEL" '{label: $label}')

resp=$(http_request POST "$BASE/sessions/$SESSION_ID/save" "$payload") || exit $?

echo "$resp" | jq . || echo "$resp"
