#!/usr/bin/env bash
set -euo pipefail

# Common settings (can be overridden in environment)
RETRIES=${RETRIES:-3}
RETRY_DELAY=${RETRY_DELAY:-1}
CURL_MAX_TIME=${CURL_MAX_TIME:-15}

# http_request METHOD URL [DATA]
# Sends a request and retries on non-2xx responses. Prints prettified JSON body (if jq available) on success.
# Exits with non-zero on final failure.
http_request() {
  local method="$1"
  local url="$2"
  local data="${3:-}"
  local auth_header="${AUTH:-}"
  local attempt=1
  local tmp
  tmp=$(mktemp)
  trap 'rm -f "$tmp"' RETURN

  while [ $attempt -le $RETRIES ]; do
    if [ -n "$data" ]; then
      if [ -n "$auth_header" ]; then
        status=$(curl -sS -X "$method" -H "Content-Type: application/json" -H "Authorization: Bearer $auth_header" -d "$data" -w "%{http_code}" -o "$tmp" --max-time $CURL_MAX_TIME "$url" ) || status=000
      else
        status=$(curl -sS -X "$method" -H "Content-Type: application/json" -d "$data" -w "%{http_code}" -o "$tmp" --max-time $CURL_MAX_TIME "$url") || status=000
      fi
    else
      if [ -n "$auth_header" ]; then
        status=$(curl -sS -X "$method" -H "Authorization: Bearer $auth_header" -w "%{http_code}" -o "$tmp" --max-time $CURL_MAX_TIME "$url") || status=000
      else
        status=$(curl -sS -X "$method" -w "%{http_code}" -o "$tmp" --max-time $CURL_MAX_TIME "$url") || status=000
      fi
    fi

    body=$(cat "$tmp" || true)

    if [[ "$status" =~ ^2[0-9][0-9]$ ]]; then
      if command -v jq >/dev/null 2>&1; then
        if [ -n "$body" ]; then
          echo "$body" | jq . || echo "$body"
        else
          echo "{}"
        fi
      else
        echo "$body"
      fi
      return 0
    else
      echo "Request to $url failed with HTTP $status (attempt $attempt/$RETRIES)" >&2
      if [ -n "$body" ]; then
        echo "Response body:" >&2
        echo "$body" >&2
      fi
      if [ $attempt -lt $RETRIES ]; then
        sleep $RETRY_DELAY
        attempt=$((attempt+1))
        continue
      else
        return 4
      fi
    fi
  done
}

