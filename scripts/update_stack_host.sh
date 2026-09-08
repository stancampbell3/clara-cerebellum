#!/usr/bin/env bash
# update_stack_host.sh — pull latest code for this host's Clara-stack
# components and restart, without clobbering host-specific settings.
#
# Run ON the target host itself (e.g. pineal). Driven by metadata in
# docker/dis_domain_hosts.json rather than hardcoding which repos/compose
# file/env file a given host needs — add a new host there, not here.
# Today only the "remote-fierypit" role (pineal's role) is exercised;
# other roles can be added to the metadata as they come up.
#
# What this does NOT do: reset any data (see reset_clara_stack.sh for
# that, a separate, earlier step at a release/dev checkpoint), or
# provision a brand-new host from scratch (assumes the repos are already
# cloned in the usual Development/{lildaemon,clara-cerebellum} layout).
#
# Usage:
#   ./scripts/update_stack_host.sh [HOST] [--dry-run] [--yes]
#
#   HOST        Defaults to $(hostname). Looked up in dis_domain_hosts.json.
#   --dry-run   Show what would change; make no changes.
#   --yes       Non-interactive: abort (listing them) on any missing
#               required env var instead of prompting.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
DEV_ROOT="$(cd "$REPO_ROOT/.." && pwd)"
DOCKER_DIR="$REPO_ROOT/docker"
METADATA="$DOCKER_DIR/dis_domain_hosts.json"

HOST=""
DRY_RUN=false
ASSUME_YES=false
for arg in "$@"; do
  case "$arg" in
    --dry-run|-n) DRY_RUN=true ;;
    --yes|-y) ASSUME_YES=true ;;
    -*) echo "Unknown flag: $arg" >&2; exit 1 ;;
    *) HOST="$arg" ;;
  esac
done
HOST="${HOST:-$(hostname)}"
# Normalize case: `hostname` can return a capitalized form (confirmed live
# on pineal: "Pineal") while /etc/hosts, this metadata file, and every
# FieryPit's registered base_url all use lowercase — a case mismatch here
# would silently fail both the metadata lookup and the final
# check_remote_fierypit.sh substring match.
HOST="$(echo "$HOST" | tr '[:upper:]' '[:lower:]')"

if [[ ! -f "$METADATA" ]]; then
  echo "Error: metadata file not found: $METADATA" >&2
  exit 1
fi

ENTRY="$(jq -c --arg h "$HOST" '.hosts[$h] // empty' "$METADATA")"
if [[ -z "$ENTRY" ]]; then
  echo "Error: no entry for host '$HOST' in $METADATA." >&2
  echo "Add one (see the 'pineal' entry for the shape) or pass a" >&2
  echo "different host name as the first argument." >&2
  exit 1
fi

ROLE="$(jq -r '.role' <<<"$ENTRY")"
COMPOSE_FILE="$(jq -r '.compose_file' <<<"$ENTRY")"
ENV_FILE_NAME="$(jq -r '.env_file' <<<"$ENTRY")"
ENV_EXAMPLE_NAME="$(jq -r '.env_example_file' <<<"$ENTRY")"
DIS_BASE_URL_META="$(jq -r '.dis_base_url' <<<"$ENTRY")"
mapfile -t REPOS < <(jq -r '.repos[]' <<<"$ENTRY")

ENV_FILE="$DOCKER_DIR/$ENV_FILE_NAME"
ENV_EXAMPLE_FILE="$DOCKER_DIR/$ENV_EXAMPLE_NAME"

echo "=== update_stack_host.sh: $HOST (role: $ROLE) ==="
echo "Repos: ${REPOS[*]}"
echo "Compose: $COMPOSE_FILE   Env: $ENV_FILE_NAME"
echo

ANY_CHANGED=false

for repo in "${REPOS[@]}"; do
  REPO_DIR="$DEV_ROOT/$repo"
  if [[ ! -d "$REPO_DIR/.git" ]]; then
    echo "Error: $REPO_DIR is not a git checkout." >&2
    exit 1
  fi
  echo "--- $repo ---"

  git -C "$REPO_DIR" fetch --quiet

  DIRTY="$(git -C "$REPO_DIR" status --porcelain --untracked-files=no)"
  if [[ -n "$DIRTY" ]]; then
    echo "Error: $repo has local modifications to tracked files — refusing to touch it:" >&2
    echo "$DIRTY" >&2
    exit 1
  fi

  OLD="$(git -C "$REPO_DIR" rev-parse --short HEAD)"
  if ! UPSTREAM="$(git -C "$REPO_DIR" rev-parse --abbrev-ref --symbolic-full-name '@{u}' 2>/dev/null)"; then
    echo "Error: $repo's current branch has no upstream configured." >&2
    exit 1
  fi
  NEW="$(git -C "$REPO_DIR" rev-parse --short '@{u}')"

  if [[ "$OLD" == "$NEW" ]]; then
    echo "Already up to date ($OLD)."
    continue
  fi

  echo "$OLD -> $NEW (from $UPSTREAM)"
  if [[ "$DRY_RUN" == true ]]; then
    echo "[DRY RUN] Would fast-forward merge."
    ANY_CHANGED=true
    continue
  fi

  if ! git -C "$REPO_DIR" merge --ff-only '@{u}'; then
    echo "Error: $repo could not fast-forward — see git's message above." >&2
    echo "This is often an untracked file colliding with an incoming" >&2
    echo "tracked one, or genuinely diverged local history. Resolve by" >&2
    echo "hand (move/rename the conflicting file, or rebase) and re-run." >&2
    exit 1
  fi
  echo "Merged."
  ANY_CHANGED=true
done

echo
echo "--- Env file check: $ENV_FILE_NAME ---"
ENV_CHANGED=false
if [[ ! -f "$ENV_EXAMPLE_FILE" ]]; then
  echo "No $ENV_EXAMPLE_NAME found — skipping required-key check."
elif [[ ! -f "$ENV_FILE" ]]; then
  echo "Error: $ENV_FILE does not exist. This script assumes the host was" >&2
  echo "already bootstrapped once (cp $ENV_EXAMPLE_NAME $ENV_FILE_NAME and" >&2
  echo "fill in real values) — provisioning a brand-new host is out of" >&2
  echo "scope here." >&2
  exit 1
else
  MISSING=()
  while IFS='=' read -r key _; do
    [[ "$key" =~ ^[A-Z_][A-Z0-9_]*$ ]] || continue
    if ! grep -q "^${key}=.\+" "$ENV_FILE"; then
      MISSING+=("$key")
    fi
  done < <(grep -E '^[A-Z_][A-Z0-9_]*=' "$ENV_EXAMPLE_FILE")

  if [[ ${#MISSING[@]} -eq 0 ]]; then
    echo "All required keys present."
  elif [[ "$DRY_RUN" == true ]]; then
    echo "[DRY RUN] Missing required key(s): ${MISSING[*]}"
  elif [[ "$ASSUME_YES" == true ]]; then
    echo "Error: missing required key(s) in $ENV_FILE_NAME: ${MISSING[*]}" >&2
    exit 1
  else
    echo "New required setting(s) found — filling in $ENV_FILE_NAME:"
    for key in "${MISSING[@]}"; do
      read -r -p "  $key=" value
      echo "$key=$value" >> "$ENV_FILE"
    done
    ENV_CHANGED=true
  fi
fi

echo
echo "--- Front script: fierypit ---"
if [[ "$ROLE" != "remote-fierypit" ]]; then
  echo "Role '$ROLE' has no front script convention yet — skipping."
elif [[ "$DRY_RUN" == true ]]; then
  echo "[DRY RUN] Would ensure $DEV_ROOT/fierypit(.sh) symlinks to scripts/fierypit.sh."
else
  # Idempotent, and deliberately NOT gated on ANY_CHANGED below — a
  # freshly-provisioned host with repos already at HEAD should still get
  # these on its first run. Never creates clara/clara.sh: this host's own
  # differently-named front script is the whole point (see
  # docs/remote_fierypit_deployment.md's "Real bugs found" #5 — ./clara
  # on a remote-fierypit host silently stands up the wrong stack, since
  # docker-compose.yml and docker-compose.remote-fierypit.yml collide on
  # the same Compose project/service name).
  ln -sf "clara-cerebellum/scripts/fierypit.sh" "$DEV_ROOT/fierypit.sh"
  ln -sf "./fierypit.sh" "$DEV_ROOT/fierypit"
  echo "OK ($DEV_ROOT/fierypit -> fierypit.sh -> clara-cerebellum/scripts/fierypit.sh)"
fi

if [[ "$DRY_RUN" == true ]]; then
  echo
  echo "[DRY RUN] No changes were made."
  exit 0
fi

if [[ "$ANY_CHANGED" == false && "$ENV_CHANGED" == false ]]; then
  echo
  echo "=== Nothing changed — skipping rebuild/restart. ==="
  exit 0
fi

echo
echo "--- Rebuilding and restarting ($COMPOSE_FILE) ---"
( cd "$DOCKER_DIR" && docker compose -f "$COMPOSE_FILE" --env-file "$ENV_FILE_NAME" up -d --build )

echo
echo "--- Confirming Dis-domain rejoin ---"
set -a
# shellcheck disable=SC1090
source "$ENV_FILE"
set +a
DIS_URL="${DIS_BASE_URL:-$DIS_BASE_URL_META}"
"$DEV_ROOT/lildaemon/scripts/check_remote_fierypit.sh" "$HOST" "$DIS_URL"

echo
echo "=== Done ==="
