#!/usr/bin/bash
set -e

ENV_FILE="./docker/.env"

if [ ! -f "$ENV_FILE" ]; then
    echo "Error: $ENV_FILE not found."
    echo "Copy ./docker/.env.example to ./docker/.env and fill in real values."
    exit 1
fi

docker compose -f ./docker/docker-compose.yml --env-file "$ENV_FILE" up -d
