#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<EOF
Usage: $0 [--compose] [service...] [--help]

Build Docker images for the project.

Options:
  --compose   Use docker-compose file at docker/docker-compose.yml to build images
  --help      Show this help

Services (for use with --compose):
  clara-api       Main API server
  prolog-mcp      Prolog MCP server
  clips-mcp       CLIPS MCP server
  lildaemon       Lildaemon service
  cobbler         Cobbler service
  clara-frontdesk Front desk UI

Examples:
  # Build all images via docker-compose
  $0 --compose

  # Build a single service
  $0 --compose clara-api

  # Build multiple services
  $0 --compose lildaemon cobbler

  # Build the top-level Dockerfile
  $0
EOF
}

USE_COMPOSE=false
SERVICES=()
for arg in "$@"; do
  case "$arg" in
    --compose) USE_COMPOSE=true ;;
    -h|--help) usage; exit 0 ;;
    -*) echo "Unknown option: $arg" >&2; usage; exit 2 ;;
    *) SERVICES+=("$arg") ;;
  esac
done

if [ "${#SERVICES[@]}" -gt 0 ] && [ "$USE_COMPOSE" = false ]; then
  echo "Service names require --compose" >&2; usage; exit 2
fi

if [ "$USE_COMPOSE" = true ]; then
  COMPOSE_FILE="docker/docker-compose.yml"
  if [ ! -f "$COMPOSE_FILE" ]; then
    echo "docker-compose file not found at $COMPOSE_FILE" >&2
    exit 3
  fi
  echo "Running: docker-compose -f $COMPOSE_FILE build ${SERVICES[*]:-}"
  docker-compose -f "$COMPOSE_FILE" build "${SERVICES[@]}"
  exit $?
fi

# Fallback: try to build top-level Dockerfile
DOCKERFILE_PATH="docker/Dockerfile"
IMAGE_NAME=${IMAGE_NAME:-"clara-cerebrum:dev"}

if [ ! -f "$DOCKERFILE_PATH" ]; then
  echo "No docker/Dockerfile found. Please run with --compose or add docker/Dockerfile." >&2
  exit 4
fi

echo "Building image $IMAGE_NAME from $DOCKERFILE_PATH"
docker build -f "$DOCKERFILE_PATH" -t "$IMAGE_NAME" ..
