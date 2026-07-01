#!/usr/bin/env bash
set -euo pipefail

# init-env.sh — bootstrap clara-cerebrum/docker/.env from .env.example
#
# Run once before first deploy, or after adding new variables to .env.example.
# Never commits secrets to git — .env is in .gitignore.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENV_FILE="$SCRIPT_DIR/.env"
EXAMPLE="$SCRIPT_DIR/.env.example"

if [[ ! -f "$EXAMPLE" ]]; then
  echo "ERROR: .env.example not found at $EXAMPLE" >&2
  exit 1
fi

if [[ -f "$ENV_FILE" ]]; then
  # Check for any keys in .env.example that are missing from .env
  missing=()
  while IFS= read -r line; do
    [[ "$line" =~ ^#.*$ || -z "$line" ]] && continue
    key="${line%%=*}"
    if ! grep -q "^${key}=" "$ENV_FILE"; then
      missing+=("$key")
    fi
  done < "$EXAMPLE"

  if [[ ${#missing[@]} -eq 0 ]]; then
    echo ".env is up to date at: $ENV_FILE"
  else
    echo "New variables found in .env.example not yet in .env:"
    for k in "${missing[@]}"; do
      echo "  - $k"
      # Append the line from .env.example
      grep "^${k}=" "$EXAMPLE" >> "$ENV_FILE"
    done
    echo ""
    echo "Added above keys to $ENV_FILE with placeholder values."
    echo "Please fill them in before deploying."
  fi
  exit 0
fi

cp "$EXAMPLE" "$ENV_FILE"
echo "Created $ENV_FILE from .env.example"
echo ""
echo "Please fill in the following values in $ENV_FILE before deploying:"
grep -E '^[A-Z_]+=.+$' "$ENV_FILE" | cut -d= -f1 | sed 's/^/  - /'
echo ""
echo "Tip: use 'openssl rand -hex 32' to generate strong secrets."
