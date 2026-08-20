#!/usr/bin/env bash
set -euo pipefail

# Path to your Kafka installation
KAFKA_DIR="/mnt/moonpool/tools/kafka_2.13-4.3.1"
BIN="$KAFKA_DIR/bin"
BOOTSTRAP="localhost:9094"

DRY_RUN=false

# Parse args
for arg in "$@"; do
  case "$arg" in
    --dry-run|-n)
      DRY_RUN=true
      ;;
  esac
done

echo "Listing topics..."
TOPICS=$("$BIN/kafka-topics.sh" --bootstrap-server "$BOOTSTRAP" --list)

if [[ -z "$TOPICS" ]]; then
  echo "No topics found."
  exit 0
fi

echo "Found topics:"
echo "$TOPICS"
echo

if [[ "$DRY_RUN" == true ]]; then
  echo "[DRY RUN] The following topics would be deleted:"
  for t in $TOPICS; do
    echo "  - $t"
  done
  echo
  echo "[DRY RUN] No changes were made."
  exit 0
fi

echo "Deleting topics..."
for t in $TOPICS; do
  echo "Deleting topic: $t"
  "$BIN/kafka-topics.sh" --bootstrap-server "$BOOTSTRAP" --delete --topic "$t"
done

echo "Waiting for deletion to propagate..."
sleep 3

echo "Verifying..."
"$BIN/kafka-topics.sh" --bootstrap-server "$BOOTSTRAP" --list

echo "Reset complete."
