#!/usr/bin/bash
curl -X POST http://localhost:8080/deduce/ -H "Content-Type: application/json" -d @deduce_request.json
