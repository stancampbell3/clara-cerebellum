#!/usr/bin/env bash
set -euo pipefail

BASE=${BASE_URL:-http://localhost:8080}
AUTH=${AUTH:-}

if [ -n "$AUTH" ]; then
  curl -sS -H "Authorization: Bearer $AUTH" "$BASE/healthz" | jq .
else
  curl -sS "$BASE/healthz" | jq .
fi

