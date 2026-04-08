#!/usr/bin/bash

PIDFILE=/tmp/clara-demo.pid
LOGFILE=/tmp/demo.log

# Kill any previously running demo process
if [ -f "$PIDFILE" ]; then
    OLD_PID=$(cat "$PIDFILE")
    if kill -0 "$OLD_PID" 2>/dev/null; then
        echo "Stopping existing demo process (PID $OLD_PID)..."
        kill "$OLD_PID"
        wait "$OLD_PID" 2>/dev/null
    fi
    rm -f "$PIDFILE"
fi

# Graceful shutdown on exit
cleanup() {
    if [ -f "$PIDFILE" ]; then
        PID=$(cat "$PIDFILE")
        if kill -0 "$PID" 2>/dev/null; then
            echo "Shutting down demo (PID $PID)..."
            kill "$PID"
            wait "$PID" 2>/dev/null
        fi
        rm -f "$PIDFILE"
    fi
}
trap cleanup EXIT INT TERM

# Start demo, save PID, append to log
FRONTDESK_CONFIG=clara-frontdesk-poc/config/localnet_dis.toml RUST_LOG=clara_frontdesk=debug \
    cargo run -p clara-frontdesk-poc >> "$LOGFILE" 2>&1 &
DEMO_PID=$!
echo "$DEMO_PID" > "$PIDFILE"
echo "Demo started (PID $DEMO_PID). Logging to $LOGFILE"

tail -f "$LOGFILE"
