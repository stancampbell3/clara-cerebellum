#!/bin/bash

if [ -f /tmp/dis.log ]; then
  echo "Flushing old logfiles from /tmp"
  rm /tmp/dis.log
fi

# Function to log messages and errors.
log() {
    echo "$@" | tee -a /tmp/dis.log
}

stop_pid_file() {
    local pid_file="$1"
    local label="$2"
    if [ -f "$pid_file" ]; then
        local PID
        PID=$(cat "$pid_file")
        if ps -p "$PID" > /dev/null 2>&1; then
            log "Stopping ${label} (PID: ${PID})..."
            kill -SIGTERM "$PID"
            sleep 2
            if ps -p "$PID" > /dev/null 2>&1; then
                log "Could not terminate ${label}. Forcing with SIGKILL."
                kill -SIGKILL "$PID"
            fi
        else
            log "Found stale PID file for ${label}."
        fi
        rm -f "$pid_file"
    fi
}

set -a
source .env
set +a

echo "-- ♖ Dis tower erecting --" | tee -a /tmp/dis.log

ensure_kafka() {
    if bash -c "echo > /dev/tcp/localhost/9094" 2>/dev/null; then
        log "Kafka already listening on :9094."
        return 0
    fi

    log "Kafka not reachable — starting docker-kafka-1..."
    if ! docker start docker-kafka-1 >> /tmp/dis.log 2>&1; then
        log "docker start failed; attempting docker compose up..."
        docker compose -f docker/docker-compose.yml up -d kafka >> /tmp/dis.log 2>&1
    fi

    log "Waiting for Kafka on :9094..."
    for i in $(seq 1 30); do
        if bash -c "echo > /dev/tcp/localhost/9094" 2>/dev/null; then
            log "Kafka is up. ♟"
            return 0
        fi
        sleep 2
    done

    log "Kafka did not come up in time. Aborting."
    exit 1
}

ensure_kafka

# Stop any running instances.
stop_pid_file "Dis.pid"          "Clara API"
stop_pid_file "first_gate.pid"   "First Gate (clips-mcp-adapter)"
stop_pid_file "second_gate.pid"  "Second Gate (prolog-mcp-adapter)"

# Start clara-api.
RUST_LOG=debug cargo run --bin clara-api >> /tmp/dis.log 2>&1 &
CLARA_PID=$!
echo $CLARA_PID > Dis.pid
log "Started Clara API with PID: ${CLARA_PID}."

# Wait for clara-api to be ready before starting the gates.
log "Waiting for Clara API on port 8080..."
for i in $(seq 1 30); do
    if curl -sf http://localhost:8080/ > /dev/null 2>&1 || \
       curl -sf http://localhost:8080/healthz > /dev/null 2>&1; then
        log "Clara API is up. 開"
        break
    fi
    sleep 1
    if [ "$i" -gt 29 ]; then
	log "Clara slept in.  Get the bucket."
	exit
    fi
done

# Start first gate (clips-mcp-adapter).
HTTP_PORT=1951 RUST_LOG=debug cargo run --bin clips-mcp-adapter >> /tmp/first_gate.log 2>&1 &
FIRST_PID=$!
echo $FIRST_PID > first_gate.pid
log "聞 Started First Gate (clips-mcp-adapter) with PID: ${FIRST_PID} → /tmp/first_gate.log"

# Start second gate (prolog-mcp-adapter).
HTTP_PORT=1968 RUST_LOG=debug cargo run --bin prolog-mcp-adapter >> /tmp/second_gate.log 2>&1 &
SECOND_PID=$!
echo $SECOND_PID > second_gate.pid
log "聞 Started Second Gate (prolog-mcp-adapter) with PID: ${SECOND_PID} → /tmp/second_gate.log"

log "間 City of Dis 間"
shutdown() {
    log ""
    log "Shutting down all services..."
    stop_pid_file "Dis.pid"          "Clara API"
    stop_pid_file "first_gate.pid"   "First Gate (clips-mcp-adapter)"
    stop_pid_file "second_gate.pid"  "Second Gate (prolog-mcp-adapter)"

    # Verify all gone
    local all_clear=true
    for pid_file in Dis.pid first_gate.pid second_gate.pid; do
        if [ -f "$pid_file" ]; then
            all_clear=false
            log "WARNING: ${pid_file} still exists after shutdown attempt."
        fi
    done
    if $all_clear; then
        log "All processes stopped."
    fi

    log "閉 The City and towers crumble into Avernus..."
    exit 0
}

trap shutdown INT TERM

log "All services started. Tailing /tmp/dis.log (Ctrl-C to stop all)."
multitail /tmp/dis.log /tmp/first_gate.log /tmp/second_gate.log

# If multitail exits on its own (e.g. window closed), also shut down.
shutdown
