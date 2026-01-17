#!/bin/bash

# Configuration
# Note: Documentation examples use port 8080 for the Prolog workflow
API_BASE="http://pineal:8080"
CONTENT_TYPE="Content-Type: application/json"

# Helper function for printing headers
print_step() {
    echo -e "\n\033[1;34m[STEP] $1\033[0m"
}

print_success() {
    echo -e "\033[1;32m[OK] $1\033[0m"
}

print_error() {
    echo -e "\033[1;31m[ERROR] $1\033[0m"
}

# Check for dependencies
if ! command -v jq &> /dev/null; then
    print_error "jq is not installed. Please install it to run this script."
    exit 1
fi

echo "==================================================="
echo "   LilDevils Prolog Session Exerciser"
echo "==================================================="

# ---------------------------------------------------------
# 1. Create a Session
# Endpoint: POST /devils/sessions
# ---------------------------------------------------------
print_step "Creating new Prolog session..."

CREATE_PAYLOAD='{
  "user_id": "script-runner",
  "config": {
    "max_facts": 1000,
    "max_rules": 500
  }
}'

RESPONSE=$(curl -s -X POST "$API_BASE/devils/sessions" \
  -H "$CONTENT_TYPE" \
  -d "$CREATE_PAYLOAD")

# Extract Session ID using jq
SESSION_ID=$(echo "$RESPONSE" | jq -r '.session_id')

if [ "$SESSION_ID" == "null" ] || [ -z "$SESSION_ID" ]; then
    print_error "Failed to create session. Response:"
    echo "$RESPONSE" | jq .
    exit 1
fi

print_success "Session created! ID: $SESSION_ID"
echo "Full Response:"
echo "$RESPONSE" | jq .


# ---------------------------------------------------------
# 2. Consult (Load Facts/Rules)
# Endpoint: POST /devils/sessions/{id}/consult
# ---------------------------------------------------------
print_step "Loading clauses (facts & rules)..."

# We will add some Star Wars facts and a rule
CONSULT_PAYLOAD='{
  "clauses": [
    "father(vader, luke)",
    "father(vader, leia)",
    "jedi(luke)",
    "sith(vader)",
    "sith_parent(X, Y) :- father(X, Y), sith(X)"
  ]
}'

CONSULT_RESPONSE=$(curl -s -X POST "$API_BASE/devils/sessions/$SESSION_ID/consult" \
  -H "$CONTENT_TYPE" \
  -d "$CONSULT_PAYLOAD")

STATUS=$(echo "$CONSULT_RESPONSE" | jq -r '.status')

if [ "$STATUS" != "clauses_loaded" ]; then
    print_error "Failed to load clauses."
    echo "$CONSULT_RESPONSE" | jq .
    exit 1
fi

print_success "Clauses loaded successfully."
echo "$CONSULT_RESPONSE" | jq .


# ---------------------------------------------------------
# 3. Query the Session
# Endpoint: POST /devils/sessions/{id}/query
# ---------------------------------------------------------
print_step "Executing Query: Who is the Sith parent of Luke?"

# Query: sith_parent(X, luke).
QUERY_PAYLOAD='{
  "goal": "sith_parent(X, luke)",
  "all_solutions": false
}'

QUERY_RESPONSE=$(curl -s -X POST "$API_BASE/devils/sessions/$SESSION_ID/query" \
  -H "$CONTENT_TYPE" \
  -d "$QUERY_PAYLOAD")

SUCCESS=$(echo "$QUERY_RESPONSE" | jq -r '.success')

if [ "$SUCCESS" != "true" ]; then
    print_error "Query failed."
    echo "$QUERY_RESPONSE" | jq .
else
    RESULT=$(echo "$QUERY_RESPONSE" | jq -r '.result')
    print_success "Query successful!"
    echo -e "Goal: sith_parent(X, luke)"
    echo -e "Result: \033[1;33m$RESULT\033[0m"
fi


# ---------------------------------------------------------
# 4. Query All Solutions (Optional Check)
# Endpoint: POST /devils/sessions/{id}/query
# ---------------------------------------------------------
print_step "Executing Query: Who are all children of Vader?"

# Query: father(vader, Child).
QUERY_ALL_PAYLOAD='{
  "goal": "father(vader, Child)",
  "all_solutions": true
}'

QUERY_ALL_RESPONSE=$(curl -s -X POST "$API_BASE/devils/sessions/$SESSION_ID/query" \
  -H "$CONTENT_TYPE" \
  -d "$QUERY_ALL_PAYLOAD")

print_success "Multi-solution query result:"
echo "$QUERY_ALL_RESPONSE" | jq .


# ---------------------------------------------------------
# 5. Terminate Session
# Endpoint: DELETE /devils/sessions/{id}
# ---------------------------------------------------------
print_step "Terminating session..."

DELETE_RESPONSE=$(curl -s -X DELETE "$API_BASE/devils/sessions/$SESSION_ID")

TERM_STATUS=$(echo "$DELETE_RESPONSE" | jq -r '.status')

if [ "$TERM_STATUS" == "terminated" ]; then
    print_success "Session $SESSION_ID terminated successfully."
else
    print_error "Failed to terminate session."
    echo "$DELETE_RESPONSE" | jq .
fi

echo "==================================================="
echo "   Test Complete"
echo "==================================================="