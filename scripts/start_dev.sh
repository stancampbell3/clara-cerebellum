#!/bin/bash

# Function to log messages and errors.
log() {
    echo "$@" | tee -a /tmp/dis.log
}

set -a
source .env
set +a

echo "-- Dis tower erecting --" | tee -a /tmp/dis.log

if [ -f "Dis.pid" ]; then
    PID=$(cat Dis.pid)
    if ps -p $PID > /dev/null; then
        log "Existing process found with PID: ${PID}. Sending SIGTERM."
        kill -SIGTERM $PID
        sleep 2 # Wait for the existing process to terminate.
        if ps -p $PID > /dev/null; then
            log "Could not terminate process. Forcing termination with SIGKILL"
            kill -SIGKILL $PID
        fi
    else
        log "Found stale PID file: Dis.pid but no running process."
        rm Dis.pid
    fi
fi

# Start clara-api and pipe stdout/stderr to /tmp/dis.log.
RUST_LOG=debug cargo run --bin clara-api >> /tmp/dis.log 2>&1 &
CLARA_PID=$!
echo $CLARA_PID > Dis.pid
log "Started Clara API with PID: ${CLARA_PID}."
