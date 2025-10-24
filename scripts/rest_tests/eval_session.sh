#!/usr/bin/env bash
set -euo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$DIR/_common.sh"

BASE=${BASE_URL:-http://localhost:8080}
AUTH=${AUTH:-}
SESSION_ID=${SESSION_ID:-}
SCRIPT=${SCRIPT:-"(printout t \"Hello from session eval\" crlf)"}
TIMEOUT_MS=2000

if [ -z "$SESSION_ID" ]; then
  echo "SESSION_ID must be provided (env var)" >&2
  exit 2
fi

# Build payload: script and timeout
payload=$(jq -n --arg script "$SCRIPT" --argjson t $TIMEOUT_MS '{script: $script, timeout_ms: $t}')
# echo to stderr for debugging
echo "target url: $BASE/sessions/$SESSION_ID/eval" >&2
echo "payload: $payload" >&2
resp=$(http_request POST "$BASE/sessions/$SESSION_ID/eval" "$payload") || exit $?

echo "$resp" | jq . || echo "$resp"
