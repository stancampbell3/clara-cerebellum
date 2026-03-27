#!/usr/bin/bash

# Execute POST request and capture deduction_id
response=$(curl -X POST http://localhost:8080/deduce -H "Content-Type: application/json" -d @deduce_request.json)

# Extract the 'deduction_id' from the response JSON using jq (assuming you have jq installed)
deduction_id=$(echo $response | jq -r '.deduction_id')

if [ -z "$deduction_id" ]; then
  echo "Failed to extract deduction_id."
  exit 1
fi

# Variable to store the final response
final_response=""

# Polling loop with a maximum of 100 attempts
for attempt in {1..100}
do
  # Execute GET request using the deduction_id to check status
  status_response=$(curl -X GET http://localhost:8080/deduce/$deduction_id)

  # Store this response for pretty-printing later
  final_response=$status_response

  # Extract 'status' from the response JSON and print it for debugging purposes.
  status=$(echo $final_response | jq -r '.status')

  echo "Attempt $attempt: Status is $status"

  if [ "$status" != "running" ]; then
    break
  fi

  sleep 2  # Wait a few seconds between attempts to avoid overwhelming the server with requests
done

# Pretty-print the final response using jq
echo "Final status:"
echo $final_response | jq .
