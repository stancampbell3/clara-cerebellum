#!/usr/bin/env bash
# reset_edgequake_clara_workspace.sh — wipe only the Clara stack's own
# Edgequake workspace (documents, chunks, vectors, AGE graph nodes, PDFs/
# assets) via Edgequake's scoped bulk-delete endpoint. Edgequake is a
# separate, genuinely multi-tenant service (its own docker-compose project,
# not part of this repo's stack) — DO NOT use its own `make db-reset` /
# `db-clean` / `db-clean-force` targets for this, they wipe every tenant
# and workspace the instance hosts, not just Clara's.
#
# Reads EDGEQUAKE_BASE_URL / EDGEQUAKE_DEFAULT_TENANT / EDGEQUAKE_DEFAULT_
# WORKSPACE from clara-cerebellum/docker/.env (the same file `./clara`
# hands to docker compose) so this always targets whatever tenant/
# workspace the running stack actually uses, rather than a hardcoded guess.
#
# Usage:
#   ./scripts/reset_edgequake_clara_workspace.sh [--dry-run]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
ENV_FILE="$REPO_ROOT/docker/.env"

DRY_RUN=false
for arg in "$@"; do
  case "$arg" in
    --dry-run|-n) DRY_RUN=true ;;
  esac
done

if [[ -f "$ENV_FILE" ]]; then
  set -a
  # shellcheck disable=SC1090
  source "$ENV_FILE"
  set +a
fi

EDGEQUAKE_BASE_URL="${EDGEQUAKE_BASE_URL:-http://10.0.0.192:8082}"
TENANT="${EDGEQUAKE_DEFAULT_TENANT:-}"
WORKSPACE="${EDGEQUAKE_DEFAULT_WORKSPACE:-}"
SLUG="${ASSISTANT_WORKSPACE_SLUG:-assistant.general}"
API="${EDGEQUAKE_BASE_URL%/}/api/v1"

if [[ -z "$TENANT" ]]; then
  echo "Error: EDGEQUAKE_DEFAULT_TENANT is not set in $ENV_FILE." >&2
  echo "Refusing to guess a tenant — pass it explicitly in docker/.env." >&2
  exit 1
fi

# Not -f: any HTTP response (even 404) proves the service is reachable —
# only a connection-level failure (refused/timeout/DNS) means it's down.
if ! curl -s -o /dev/null -m 5 "$EDGEQUAKE_BASE_URL"; then
  echo "Edgequake unreachable at $EDGEQUAKE_BASE_URL — skipping (nothing to reset)."
  exit 0
fi

HEADERS=(-H "X-Tenant-ID: $TENANT")
if [[ -n "${EDGEQUAKE_API_KEY:-}" ]]; then
  HEADERS+=(-H "X-API-Key: $EDGEQUAKE_API_KEY")
fi

# Fall back to resolving the workspace by slug if the fixed UUID isn't
# configured (matches EdgequakeClient._lookup_workspace's own endpoint).
if [[ -z "$WORKSPACE" ]]; then
  echo "EDGEQUAKE_DEFAULT_WORKSPACE not set; resolving slug '$SLUG'..."
  WORKSPACE=$(curl -sf "${HEADERS[@]}" "$API/tenants/$TENANT/workspaces/by-slug/$SLUG" | jq -r '.id // empty')
  if [[ -z "$WORKSPACE" ]]; then
    echo "No workspace found for slug '$SLUG' under tenant '$TENANT'." >&2
    echo "Nothing to reset."
    exit 0
  fi
fi

echo "Edgequake:  $EDGEQUAKE_BASE_URL"
echo "Tenant:     $TENANT"
echo "Workspace:  $WORKSPACE (slug: $SLUG)"

COUNT=$(curl -sf "${HEADERS[@]}" -H "X-Workspace-ID: $WORKSPACE" \
  "$API/documents?page=1&page_size=1" | jq -r '.total // 0')
echo "Documents currently in workspace: $COUNT"

if [[ "$COUNT" == "0" ]]; then
  echo "Nothing to delete."
  exit 0
fi

if [[ "$DRY_RUN" == true ]]; then
  echo "[DRY RUN] Would call:"
  echo "  curl -X DELETE ${HEADERS[*]} -H \"X-Workspace-ID: $WORKSPACE\" \\"
  echo "       -H \"X-EdgeQuake-Confirm: delete-all-documents\" $API/documents"
  echo "[DRY RUN] No changes were made."
  exit 0
fi

echo "Deleting all $COUNT document(s) in workspace $WORKSPACE..."
curl -sf -X DELETE "${HEADERS[@]}" -H "X-Workspace-ID: $WORKSPACE" \
  -H "X-EdgeQuake-Confirm: delete-all-documents" "$API/documents"

echo
echo "Clara's Edgequake workspace cleared. Other tenants/workspaces on this"
echo "Edgequake instance (if any) were not touched."
