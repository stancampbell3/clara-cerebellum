#!/usr/bin/env bash
set -euo pipefail

BASE_URL="http://localhost:8080"

echo "== Creating session =="
SESSION_ID=$(curl -s -X POST "$BASE_URL/sessions" \
  -H "Content-Type: application/json" \
  -d '{"user_id":"raven_test","config":{"persistence":"memory"}}' \
  | jq -r '.session_id')

echo "Session created: $SESSION_ID"

echo "== Loading rule =="
RULE='(defrule english-has-sinister-collective-nouns-r1
  (str-index "sinister"
    (clara-evaluate "{ \"data\": { \"prompt\": \"What is the collective noun for a group of ravens?\", \"model\": \"hf.co/bartowski/Qwen2.5-14B-Instruct-1M-GGUF:Q4_0\" }}"))
  =>
  (assert (english-has-sinister-nouns))
)'

curl -s -X POST "$BASE_URL/sessions/$SESSION_ID/rules" \
  -H "Content-Type: application/json" \
  -d "$(jq -n --arg r "$RULE" '{rules:[$r]}')"

echo "Rule loaded."

echo "== Running rules =="
RUN_RESULT=$(curl -s -X POST "$BASE_URL/sessions/$SESSION_ID/run" \
  -H "Content-Type: application/json" \
  -d '{"max_iterations":100}')

echo "Run result:"
echo "$RUN_RESULT" | jq

echo "== Querying facts =="
FACTS=$(curl -s "$BASE_URL/sessions/$SESSION_ID/facts?pattern=(english-has-sinister-nouns)")

echo "$FACTS" | jq

echo "== Cleaning up session =="
curl -s -X DELETE "$BASE_URL/sessions/$SESSION_ID" > /dev/null
echo "Session deleted."
