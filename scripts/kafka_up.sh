#!/usr/bin/bash
docker compose -f ./docker/docker-compose.yml --env-file "$ENV_FILE" up -d --no-deps kafka
