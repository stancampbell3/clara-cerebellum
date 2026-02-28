#!/usr/bin/bash
set -a
source .env
set +a

echo "-- Dis tower DESTRUCTION * --" 

if [ -f "Dis.pid" ]; then
    PID=$(cat Dis.pid)
    if ps -p $PID > /dev/null; then
        echo "Existing process found with PID: ${PID}. Sending SIGTERM."
        kill -SIGTERM $PID
        sleep 2 # Wait for the existing process to terminate.
        if ps -p $PID > /dev/null; then
            echo "Could not terminate process. Forcing termination with SIGKILL"
            kill -SIGKILL $PID
        fi
    else
        echo "Found stale PID file: Dis.pid but no running process."
        rm Dis.pid
    fi
fi
