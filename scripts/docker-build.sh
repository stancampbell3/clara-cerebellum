#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<EOF
Usage: $0 [--compose] [--help]

Build Docker images for the project.

Options:
  --compose   Use docker-compose file at docker/docker-compose.yml to build images
  --help      Show this help

Examples:
  # Build using docker-compose
  $0 --compose

  # Build the top-level Dockerfile
  $0
EOF
}

USE_COMPOSE=false
for arg in "$@"; do
  case "$arg" in
    --compose) USE_COMPOSE=true ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown arg: $arg" >&2; usage; exit 2 ;;
  esac
done

if [ "$USE_COMPOSE" = true ]; then
  COMPOSE_FILE="docker/docker-compose.yml"
  if [ ! -f "$COMPOSE_FILE" ]; then
    echo "docker-compose file not found at $COMPOSE_FILE" >&2
    exit 3
  fi
  echo "Running: docker-compose -f $COMPOSE_FILE build"
  docker-compose -f "$COMPOSE_FILE" build
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
docker build -f "$DOCKERFILE_PATH" -t "$IMAGE_NAME" .
