#!/usr/bin/env bash
set -euo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$DIR/_common.sh"

BASE=${BASE_URL:-http://localhost:8080}
AUTH=${AUTH:-}

echo "Checking health: $BASE/healthz"
resp=$(http_request GET "$BASE/healthz") || exit $?
echo "$resp" | jq . || echo "$resp"
