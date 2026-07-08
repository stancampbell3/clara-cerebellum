#!/bin/bash
curl -s http://localhost:6666/ritual \
  -H "Authorization: Bearer $FIERYPIT_SERVICE_KEY" | jq .
