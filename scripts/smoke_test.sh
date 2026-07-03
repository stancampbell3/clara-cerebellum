#!/bin/bash

# Smoke Test Script for Dis Rituals
# Exercises the Ritual lifecycle end to end: create, join (idempotency), status,
# start a performance (deduction attached via ritual_id), terminate, and confirm
# joins are rejected after termination.

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$DIR/rest_tests/_common.sh"  # provides http_request() with retry/backoff

BASE=${BASE_URL:-http://localhost:8080}

# 1. Create a ritual
echo "Creating ritual..."
CREATE_RESPONSE=$(http_request POST "$BASE/ritual" '{"name":"smoke-test","participants":[]}')
echo "$CREATE_RESPONSE"

RITUAL_ID=$(echo "$CREATE_RESPONSE" | jq -r '.ritual_id')
echo "Ritual ID: $RITUAL_ID"

# 2. Join with participant (idempotent)
echo "Joining with participant..."
JOIN_RESPONSE=$(http_request GET "$BASE/ritual/$RITUAL_ID/join?participant=http://fiery-pit-1:8080")
echo "$JOIN_RESPONSE"

# Join again with same key — must return same performance_id
echo "Joining again with same participant..."
JOIN_RESPONSE_2=$(http_request GET "$BASE/ritual/$RITUAL_ID/join?participant=http://fiery-pit-1:8080")
echo "$JOIN_RESPONSE_2"

# Check that performance_id is the same
PERFORMANCE_ID_1=$(echo "$JOIN_RESPONSE" | jq -r '.performance_id')
PERFORMANCE_ID_2=$(echo "$JOIN_RESPONSE_2" | jq -r '.performance_id')

if [ "$PERFORMANCE_ID_1" != "$PERFORMANCE_ID_2" ]; then
    echo "ERROR: performance_id changed between joins!"
    exit 1
fi

echo "SUCCESS: performance_id is consistent"

# 3. Status
echo "Checking status..."
STATUS_RESPONSE=$(http_request GET "$BASE/ritual/$RITUAL_ID/status")
echo "$STATUS_RESPONSE"

# 4. Start a performance — attach a deduction cycle to the ritual via ritual_id.
# These clauses never call coire_publish(evaluator/...), so the cycle converges
# on its own without needing a live FieryPit participant (works against the
# InMemoryBroker used in dev/test mode).
echo "Starting a performance (deduction attached to ritual $RITUAL_ID)..."
DEDUCE_PAYLOAD=$(jq -n --arg rid "$RITUAL_ID" \
  '{prolog_clauses: ["man(stan).", "mortal(X) :- man(X)."], initial_goal: "mortal(X)", ritual_id: $rid}')
DEDUCE_RESPONSE=$(http_request POST "$BASE/deduce" "$DEDUCE_PAYLOAD")
echo "$DEDUCE_RESPONSE"

DEDUCTION_ID=$(echo "$DEDUCE_RESPONSE" | jq -r '.deduction_id')
echo "Deduction ID: $DEDUCTION_ID"

DEDUCE_STATUS="running"
ATTEMPTS=0
MAX_ATTEMPTS=20
while [ "$DEDUCE_STATUS" = "running" ] && [ "$ATTEMPTS" -lt "$MAX_ATTEMPTS" ]; do
    sleep 0.5
    DEDUCE_STATUS_RESPONSE=$(http_request GET "$BASE/deduce/$DEDUCTION_ID")
    DEDUCE_STATUS=$(echo "$DEDUCE_STATUS_RESPONSE" | jq -r '.status')
    ATTEMPTS=$((ATTEMPTS + 1))
done

echo "Final deduction status: $DEDUCE_STATUS"

if [ "$DEDUCE_STATUS" != "converged" ]; then
    echo "ERROR: Expected performance to converge, got status: $DEDUCE_STATUS"
    echo "$DEDUCE_STATUS_RESPONSE"
    exit 1
fi

echo "SUCCESS: performance converged"

# 5. Terminate
echo "Terminating ritual..."
TERMINATE_RESPONSE=$(http_request DELETE "$BASE/ritual/$RITUAL_ID")
echo "$TERMINATE_RESPONSE"

# 6. Join after terminate (should return 409) — expected-failure path, so this
# stays on raw curl rather than http_request (which retries and fails on non-2xx).
echo "Attempting to join after termination (expecting 409)..."
JOIN_AFTER_TERMINATE_BODY=$(curl -s "$BASE/ritual/$RITUAL_ID/join")
HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" "$BASE/ritual/$RITUAL_ID/join")

echo "$JOIN_AFTER_TERMINATE_BODY" | jq

if [ "$HTTP_CODE" != "409" ]; then
    echo "ERROR: Expected HTTP 409 Conflict, got $HTTP_CODE"
    exit 1
fi

echo "SUCCESS: Got expected 409 Conflict after termination"

echo "Smoke test completed successfully!"
