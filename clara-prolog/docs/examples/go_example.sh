#!/usr/bin/bash
if [ -z "$1" ]; then
  echo "Usage: $0 <example_number>"
  exit 1
fi

N=$1
REQUEST="ex${N}_request.json"

# Transduce the source
./td_example.sh "$N" || exit 1

# Check for request JSON
if [ ! -f "$REQUEST" ]; then
  echo "No request file $REQUEST found — skipping deduction step."
  exit 0
fi

# Truncate existing log
echo "Truncating log..."
truncate -s 0 /tmp/dis.log

# Execute POST request and capture deduction_id
echo "Submitting $REQUEST..."
response=$(curl -s -X POST http://localhost:8080/deduce -H "Content-Type: application/json" -d @"$REQUEST")

deduction_id=$(echo "$response" | jq -r '.deduction_id')

if [ -z "$deduction_id" ] || [ "$deduction_id" = "null" ]; then
  echo "Failed to extract deduction_id. Response:"
  echo "$response" | jq .
  exit 1
fi

echo "deduction_id: $deduction_id"

# Polling loop
final_response=""
for attempt in {1..100}; do
  status_response=$(curl -s -X GET http://localhost:8080/deduce/$deduction_id)
  final_response=$status_response
  status=$(echo "$final_response" | jq -r '.status')
  echo "Attempt $attempt: status=$status"

  if [ "$status" != "running" ]; then
    break
  fi

  sleep 2
done

echo "Final status:"
echo "$final_response" | jq .
