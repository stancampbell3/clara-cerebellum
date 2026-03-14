#!/bin/bash
# Clara PoC — Deployment script
#
# Syncs clara-cerebrum, lildaemon, and edgequake to the EC2 instance,
# then builds and starts the Docker stack.
#
# Usage:
#   ./scripts/deploy-clara.sh              # sync + build + start
#   ./scripts/deploy-clara.sh --sync-only  # rsync only, no build
#   ./scripts/deploy-clara.sh --no-cache   # force full Docker rebuild

set -e

EC2_HOST="ec2-54-177-89-105.us-west-1.compute.amazonaws.com"
EC2_USER="ec2-user"
SSH_KEY="$HOME/vastness/.ssh/SeashellAnalytics_220325.pem"
REMOTE_DIR="/opt/clara"

# Resolve paths relative to this script's location
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CEREBRUM_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"       # clara-cerebrum/
PARENT_DIR="$(cd "$CEREBRUM_DIR/.." && pwd)"        # Development/

SYNC_ONLY=false
BUILD_FLAGS=""

for arg in "$@"; do
    case $arg in
        --sync-only) SYNC_ONLY=true ;;
        --no-cache)  BUILD_FLAGS="--no-cache" ;;
    esac
done

SSH="ssh -i $SSH_KEY"

echo "================================"
echo "  Clara PoC Deployment"
echo "================================"
echo "  Target : $EC2_USER@$EC2_HOST"
echo "  Remote : $REMOTE_DIR"
echo ""

# Verify SSH key
if [ ! -f "$SSH_KEY" ]; then
    echo "ERROR: SSH key not found: $SSH_KEY"
    exit 1
fi

# Test SSH connection
echo "Testing SSH connection..."
$SSH -o ConnectTimeout=10 "$EC2_USER@$EC2_HOST" "echo '  SSH OK'" 2>/dev/null

# Ensure remote directory exists
$SSH "$EC2_USER@$EC2_HOST" "mkdir -p $REMOTE_DIR"

echo ""
echo "Step 1: Syncing clara-cerebrum..."
rsync -avz --delete \
    --exclude='target/' \
    --exclude='.git/' \
    --exclude='*.tmp' \
    -e "ssh -i $SSH_KEY" \
    "$CEREBRUM_DIR/" \
    "$EC2_USER@$EC2_HOST:$REMOTE_DIR/clara-cerebrum/"

echo ""
echo "Step 2: Syncing lildaemon..."
rsync -avz --delete \
    --exclude='__pycache__/' \
    --exclude='*.pyc' \
    --exclude='.git/' \
    --exclude='.venv/' \
    --exclude='venv/' \
    --exclude='logs/' \
    --exclude='output/' \
    -e "ssh -i $SSH_KEY" \
    "$PARENT_DIR/lildaemon/" \
    "$EC2_USER@$EC2_HOST:$REMOTE_DIR/lildaemon/"

echo ""
echo "Step 3: Syncing edgequake..."
rsync -avz --delete \
    --exclude='.git/' \
    --exclude='target/' \
    --exclude='node_modules/' \
    -e "ssh -i $SSH_KEY" \
    "$PARENT_DIR/edgequake/" \
    "$EC2_USER@$EC2_HOST:$REMOTE_DIR/edgequake/"

echo ""
echo "Step 4: Syncing models (checksum — slow first time)..."
rsync -avz --checksum \
    -e "ssh -i $SSH_KEY" \
    "$CEREBRUM_DIR/models/" \
    "$EC2_USER@$EC2_HOST:$REMOTE_DIR/clara-cerebrum/models/"

echo ""
echo "Step 5: Placing .dockerignore at build context root..."
rsync -avz \
    -e "ssh -i $SSH_KEY" \
    "$CEREBRUM_DIR/docker/.dockerignore" \
    "$EC2_USER@$EC2_HOST:$REMOTE_DIR/.dockerignore"

if [ "$SYNC_ONLY" = true ]; then
    echo ""
    echo "Sync complete (--sync-only, skipping build)."
    exit 0
fi

echo ""
echo "Step 6: Building and starting Clara stack on EC2..."
$SSH "$EC2_USER@$EC2_HOST" bash << ENDSSH
set -e
cd $REMOTE_DIR

# Ensure Docker is running
sudo systemctl start docker 2>/dev/null || true

# Build (runs from Development/ so context includes both repos)
docker compose \
    -f clara-cerebrum/docker/docker-compose.yml \
    --env-file clara-cerebrum/docker/.env \
    build $BUILD_FLAGS

# Start
docker compose \
    -f clara-cerebrum/docker/docker-compose.yml \
    --env-file clara-cerebrum/docker/.env \
    up -d

echo "Clara stack status:"
docker compose \
    -f clara-cerebrum/docker/docker-compose.yml \
    ps
ENDSSH

echo ""
echo "Done. Frontdesk accessible at: http://$EC2_HOST/frontdesk/"
echo "EdgeQuake accessible at:       http://$EC2_HOST/edgequake/  (requires auth)"
