#!/usr/bin/env bash
set -euo pipefail

# --- CONFIG ---
API="http://localhost:8082/api/v1"
TENANT=""
PATTERN=""
DRY_RUN=false

usage() {
  echo "Usage: $0 --tenant TENANT_NAME_OR_ID --pattern GLOB [--dry-run]"
  echo "  PATTERN is a shell glob matched against workspace names, e.g. 'adhoc.*' or 'adhoc*'"
  echo "  -----------------------------------------------------------------------"
  echo "  example: ./scripts/reset_adhoc_workspaces.sh --tenant Default --pattern 'adhoc.*"
  exit 1
}

# --- Parse args ---
while [[ $# -gt 0 ]]; do
  case "$1" in
    --tenant)
      TENANT="$2"
      shift 2
      ;;
    --pattern)
      PATTERN="$2"
      shift 2
      ;;
    --dry-run|-n)
      DRY_RUN=true
      shift
      ;;
    *)
      echo "Unknown argument: $1"
      usage
      ;;
  esac
done

[[ -z "$TENANT" || -z "$PATTERN" ]] && usage

# --- Resolve tenant name/slug to a tenant UUID ---
UUID_RE='^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$'
if [[ "$TENANT" =~ $UUID_RE ]]; then
  TENANT_ID="$TENANT"
else
  echo "Resolving tenant '$TENANT'..."
  TENANT_ID=$(curl -sf "$API/tenants?limit=100" \
    | jq -r --arg t "$TENANT" '.items[] | select(.name == $t or .slug == $t) | .id')
  if [[ -z "$TENANT_ID" ]]; then
    echo "No tenant found matching '$TENANT'." >&2
    exit 1
  fi
  if [[ $(wc -l <<<"$TENANT_ID") -gt 1 ]]; then
    echo "Multiple tenants matched '$TENANT'; pass the tenant UUID instead:" >&2
    echo "$TENANT_ID" >&2
    exit 1
  fi
fi
echo "Using tenant ID: $TENANT_ID"

echo "Fetching workspace list..."
WORKSPACES_JSON=$(curl -sf "$API/tenants/$TENANT_ID/workspaces?limit=100")

echo "Filtering workspaces for pattern '$PATTERN'..."
MATCHED_IDS=()
MATCHED_NAMES=()

while IFS=$'\t' read -r id name; do
  if [[ "$name" == $PATTERN ]]; then
    MATCHED_IDS+=("$id")
    MATCHED_NAMES+=("$name")
  fi
done < <(jq -r '.items[] | [.id, .name] | @tsv' <<<"$WORKSPACES_JSON")

if [[ ${#MATCHED_IDS[@]} -eq 0 ]]; then
  echo "No matching workspaces found."
  exit 0
fi

echo "Matched workspaces:"
for i in "${!MATCHED_IDS[@]}"; do
  echo "  - ${MATCHED_NAMES[$i]} (${MATCHED_IDS[$i]})"
done
echo

if [[ "$DRY_RUN" == true ]]; then
  echo "[DRY RUN] The following DELETE calls would be made:"
  for id in "${MATCHED_IDS[@]}"; do
    echo "  curl -X DELETE -H \"X-Tenant-ID: $TENANT_ID\" $API/workspaces/$id"
  done
  echo
  echo "[DRY RUN] No changes were made."
  exit 0
fi

echo "Deleting workspaces..."
for i in "${!MATCHED_IDS[@]}"; do
  echo "Deleting: ${MATCHED_NAMES[$i]} (${MATCHED_IDS[$i]})"
  curl -sf -X DELETE -H "X-Tenant-ID: $TENANT_ID" "$API/workspaces/${MATCHED_IDS[$i]}"
done

echo "Workspace cleanup complete."
