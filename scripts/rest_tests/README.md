# Clara Cerebrum REST API Test Scripts

A comprehensive suite of bash scripts for testing the Clara Cerebrum REST API endpoints.

**Last Updated:** October 23, 2025
**Status:** ✅ Fixed and ready to test

## Quick Start

### Prerequisites
- `bash` 4.0+
- `curl`
- `jq` (for JSON parsing)
- Clara API server running on `http://localhost:8080`

### Make Scripts Executable
```bash
chmod +x scripts/rest_tests/*.sh
```

### Run All Tests
```bash
cd scripts/rest_tests
BASE_URL=http://localhost:8080 ./all_tests.sh
```

---

## Running the Tests

### Full Orchestrator (Happy Path)

The `all_tests.sh` orchestrator runs the complete workflow:
1. ✅ Health check
2. ❌ Ephemeral eval (commented out - `/eval` endpoint not yet implemented)
3. ✅ Create persistent session
4. ✅ Execute CLIPS code in session
5. ✅ Save session checkpoint
6. ✅ Delete session

```bash
# From repo root
BASE_URL=http://localhost:8080 ./scripts/rest_tests/all_tests.sh

# With authentication
AUTH="your-token" BASE_URL=http://localhost:8080 ./scripts/rest_tests/all_tests.sh
```

**Expected Output:**
```
BASE_URL=http://localhost:8080

==> Health
{"status":"ok"}

==> Create persistent session
{
  "session_id": "sess-...",
  "user_id": "test-user",
  ...
}
Created session: sess-abc123

==> Eval against session
{
  "stdout": "session hello\n",
  "stderr": "",
  "exit_code": 0,
  "metrics": {"elapsed_ms": 23}
}

==> Save session
...

==> Delete session
...

All tests finished
```

---

## Individual Scripts

### Health Check

```bash
BASE_URL=http://localhost:8080 ./health.sh
```

Response: `{"status":"ok"}`

### Create Session

```bash
# Default user
BASE_URL=http://localhost:8080 ./create_session.sh

# Custom user
BASE_URL=http://localhost:8080 USER_ID="alice" ./create_session.sh

# With preload
BASE_URL=http://localhost:8080 PRELOAD="base_rules.clp" ./create_session.sh
```

**Payload:** Uses `user_id` (snake_case, not userId)
```json
{"user_id": "test-user"}
```

### Create Persistent Session (with ID extraction)

```bash
resp=$(BASE_URL=http://localhost:8080 ./create_persistent_session.sh)
SESSION_ID=$(echo "$resp" | jq -r '.session_id')
echo "Session created: $SESSION_ID"
```

### Evaluate in Session

```bash
BASE_URL=http://localhost:8080 SESSION_ID="sess-abc123" ./eval_session.sh

# Custom script and timeout
BASE_URL=http://localhost:8080 \
  SESSION_ID="sess-abc123" \
  SCRIPT='(defrule hello (initial-fact) => (printout t "Hello" crlf))' \
  TIMEOUT_MS=5000 \
  ./eval_session.sh
```

**Payload:** Uses `script` field (not `commands`)
```json
{
  "script": "(printout t \"Hello\" crlf)",
  "timeout_ms": 2000
}
```

### Save Session (Checkpoint)

```bash
BASE_URL=http://localhost:8080 \
  SESSION_ID="sess-abc123" \
  LABEL="checkpoint-1" \
  ./save_session.sh
```

### Load Rules into Session

```bash
BASE_URL=http://localhost:8080 \
  SESSION_ID="sess-abc123" \
  FILES="rules.clp,facts.clp" \
  ./load_session.sh
```

### Delete Session

```bash
BASE_URL=http://localhost:8080 SESSION_ID="sess-abc123" ./delete_session.sh
```

### Ephemeral Eval ⚠️

**Status:** Not yet implemented. The `/eval` endpoint does not exist yet. This test is commented out in `all_tests.sh`.

---

## Shared Helper: `_common.sh`

All scripts source `_common.sh` which provides robust HTTP handling with automatic retries.

### `http_request()` Function

**Features:**
- Automatic retries on non-2xx responses (default: 3 retries)
- Configurable retry delays (default: 1 second)
- JSON output pretty-printing
- Bearer token authentication support
- Configurable curl timeout (default: 15 seconds)

**Environment Variables:**
- `RETRIES` - Retry attempts (default: 3)
- `RETRY_DELAY` - Seconds between retries (default: 1)
- `CURL_MAX_TIME` - Curl timeout (default: 15)
- `AUTH` - Bearer token (optional)

---

## Environment Variables Reference

| Variable | Default | Description |
|----------|---------|-------------|
| `BASE_URL` | `http://localhost:8080` | API server URL |
| `AUTH` | (empty) | Bearer token for Authorization |
| `USER_ID` | `test-user` | User for session creation |
| `SESSION_ID` | (required) | Session ID for operations |
| `SCRIPT` | Default script | CLIPS code to execute |
| `TIMEOUT_MS` | 2000 | Execution timeout (ms) |
| `LABEL` | (required) | Checkpoint label for save |
| `RETRIES` | 3 | HTTP retry attempts |
| `RETRY_DELAY` | 1 | Seconds between retries |

---

## Troubleshooting

### Connection Refused
```
curl: (7) Failed to connect to localhost port 8080: Connection refused
```

**Solution:** Start the API server:
```bash
# Terminal 1
RUST_LOG=info cargo run -p clara-api

# Terminal 2 (run tests)
BASE_URL=http://localhost:8080 ./all_tests.sh
```

### JSON Parse Error
```
parse error: Invalid numeric literal at line 1, column 1
```

**Solution:** Verify server is returning valid JSON:
```bash
curl http://localhost:8080/healthz
```

### 404 Not Found on Session Creation
```
Request to http://localhost:8080/sessions failed with HTTP 404
```

**Cause:** Payload field name mismatch (fixed Oct 23)
- ✅ Correct: `{"user_id": "..."}` (snake_case)
- ❌ Wrong: `{"userId": "..."}` (camelCase)

**Debug:**
```bash
curl -v -X POST -H "Content-Type: application/json" \
  -d '{"user_id":"test-user"}' \
  http://localhost:8080/sessions
```

### 404 on Session Eval
**Cause:** Response field extraction mismatch
- ✅ Correct: `.session_id` (snake_case)
- ❌ Wrong: `.sessionId` (camelCase)

All scripts have been fixed to use `session_id`.

### Timeout Errors
```
Request to http://localhost:8080/sessions/sess-abc123/eval failed with HTTP 504
```

**Solutions:**
1. Increase timeout: `TIMEOUT_MS=10000 ./eval_session.sh`
2. Check CLIPS subprocess is alive
3. Check for infinite loops in CLIPS script

---

## Advanced Usage

### Custom Workflow

Create `my_test.sh`:
```bash
#!/bin/bash
set -euo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$DIR/_common.sh"

BASE=${BASE_URL:-http://localhost:8080}

# Create session
resp=$(http_request POST "$BASE/sessions" \
  "$(jq -n --arg user_id "myuser" '{user_id: $user_id}')")
SESSION_ID=$(echo "$resp" | jq -r '.session_id')

# Run multiple evals
for i in {1..3}; do
  echo "Run $i..."
  http_request POST "$BASE/sessions/$SESSION_ID/eval" \
    "$(jq -n --arg script "(printout t \"Run $i\" crlf)" \
           --arg t 2000 '{script: $script, timeout_ms: $t}')" | jq .
done

# Cleanup
http_request DELETE "$BASE/sessions/$SESSION_ID" > /dev/null
echo "Done!"
```

### Parallel Testing

```bash
#!/bin/bash
BASE_URL=http://localhost:8080

for i in {1..5}; do
  (USER_ID="user-$i" BASE_URL="$BASE_URL" ./create_persistent_session.sh) &
done
wait
```

---

## Recent Changes (Oct 23, 2025)

- ✅ Fixed `userId` → `user_id` in payloads
- ✅ Fixed `commands` → `script` in eval payloads
- ✅ Fixed `sessionId` → `session_id` field extraction
- ✅ Commented out ephemeral eval (endpoint not yet implemented)
- ✅ Added comprehensive documentation

---

## Implementation Notes

### Known Issues
- [ ] Ephemeral eval endpoint (`POST /eval`) not yet implemented
- [ ] Session persistence not yet implemented
- [ ] No authentication/authorization layer yet
- [ ] Basic error detection in CLIPS output

### Next Steps
1. ✅ All session-based operations working
2. [ ] Implement `POST /eval` endpoint
3. [ ] Add persistence layer
4. [ ] Add authentication
5. [ ] Add load testing tools

---

## File Structure

```
scripts/rest_tests/
├── README.md (this file)
├── _common.sh (shared HTTP helper)
├── all_tests.sh (orchestrator)
├── health.sh
├── create_session.sh
├── create_persistent_session.sh
├── eval_session.sh
├── eval_ephemeral.sh (NOT YET WORKING)
├── delete_session.sh
├── save_session.sh
└── load_session.sh
```

---

## Additional Resources

- `docs/CLIPS_SERVICE_DESIGN.md` - Full API design
- `docs/DEVELOPMENT_STATUS.md` - Development status
- `docs/lipstick_on_collar.txt` - Previous session notes
