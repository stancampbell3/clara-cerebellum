#!/bin/bash
curl -sS -X POST http://127.0.0.1:8000/evaluate \
  -H "Content-Type: application/json" \
  -d '{"data": {"text": "test offering", "mode": "echo"}}' | jq .

