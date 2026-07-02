#!/bin/bash

# Smoke Test Script for Dis Rituals 
# This script performs a basic smoke tests exercising ritual behavior

set -e  # Exit on error

# Create a ritual
echo "Creating ritual..."
CREATE_RESPONSE=$(curl -s -X POST http://localhost:8080/ritual \
  -H 'Content-Type: application/json' \
  -d '{"name":"smoke-test","participants":[]}')

echo "$CREATE_RESPONSE" | jq

# Extract ritual ID from response
RITUAL_ID=$(echo "$CREATE_RESPONSE" | jq -r '.ritual_id')

echo "Ritual ID: $RITUAL_ID"

# Join with participant (idempotent)
echo "Joining with participant..."
JOIN_RESPONSE=$(curl -s "http://localhost:8080/ritual/$RITUAL_ID/join?participant=http://fiery-pit-1:8080")

echo "$JOIN_RESPONSE" | jq

# Join again with same key — must return same performance_id
echo "Joining again with same participant..."
JOIN_RESPONSE_2=$(curl -s "http://localhost:8080/ritual/$RITUAL_ID/join?participant=http://fiery-pit-1:8080")

echo "$JOIN_RESPONSE_2" | jq

# Check that performance_id is the same
PERFORMANCE_ID_1=$(echo "$JOIN_RESPONSE" | jq -r '.performance_id')
PERFORMANCE_ID_2=$(echo "$JOIN_RESPONSE_2" | jq -r '.performance_id')

if [ "$PERFORMANCE_ID_1" != "$PERFORMANCE_ID_2" ]; then
    echo "ERROR: performance_id changed between joins!"
    exit 1
fi

echo "SUCCESS: performance_id is consistent"

# Status
echo "Checking status..."
STATUS_RESPONSE=$(curl -s http://localhost:8080/ritual/$RITUAL_ID/status)

echo "$STATUS_RESPONSE" | jq

# Terminate
echo "Terminating ritual..."
TERMINATE_RESPONSE=$(curl -s -X DELETE http://localhost:8080/ritual/$RITUAL_ID)

echo "$TERMINATE_RESPONSE" | jq

# Join after terminate (should return 409)
echo "Attempting to join after termination (expecting 409)..."
JOIN_AFTER_TERMINATE_BODY=$(curl -s "http://localhost:8080/ritual/$RITUAL_ID/join")
HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" "http://localhost:8080/ritual/$RITUAL_ID/join")

echo "$JOIN_AFTER_TERMINATE_BODY" | jq

if [ "$HTTP_CODE" != "409" ]; then
    echo "ERROR: Expected HTTP 409 Conflict, got $HTTP_CODE"
    exit 1
fi

echo "SUCCESS: Got expected 409 Conflict after termination"

echo "Smoke test completed successfully!"
