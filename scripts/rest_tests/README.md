# REST test scripts for clara-cerebrum

Collection of simple bash scripts to exercise the clara-cerebrum REST API.

Defaults:
- BASE_URL: http://localhost:8080
- AUTH: (optional) bearer token placed in $AUTH

Requirements:
- bash
- curl
- jq (used for payload building and pretty-printing JSON)

Make scripts executable:

```bash
chmod +x scripts/rest_tests/*.sh
```

Orchestrator (run the full happy-path)

This runs: health -> ephemeral eval -> create persistent session -> eval -> save -> delete

```bash
# from repo root
scripts/rest_tests/all_tests.sh
# or explicitly
./scripts/rest_tests/all_tests.sh
```

If your service requires authentication, include an AUTH env var (Bearer token):

```bash
AUTH="your-token" BASE_URL=http://localhost:8080 ./scripts/rest_tests/all_tests.sh
```

Run individual scripts

Health check:

```bash
BASE_URL=http://localhost:8080 scripts/rest_tests/health.sh
```

Ephemeral eval:

```bash
BASE_URL=http://localhost:8080 SCRIPT='(printout t "hello ephemeral" crlf)' scripts/rest_tests/eval_ephemeral.sh
```

Create a persistent session (prints JSON response including sessionId):

```bash
BASE_URL=http://localhost:8080 scripts/rest_tests/create_persistent_session.sh
```

Eval against an existing session (requires SESSION_ID env var):

```bash
BASE_URL=http://localhost:8080 SESSION_ID=<id> SCRIPT='(printout t "hello session" crlf)' scripts/rest_tests/eval_session.sh
```

Save a session:

```bash
BASE_URL=http://localhost:8080 SESSION_ID=<id> LABEL='checkpoint-1' scripts/rest_tests/save_session.sh
```

Load files into a session (comma-separated FILES):

```bash
BASE_URL=http://localhost:8080 SESSION_ID=<id> FILES="rules.clp,more.clp" scripts/rest_tests/load_session.sh
```

Delete a session:

```bash
BASE_URL=http://localhost:8080 SESSION_ID=<id> scripts/rest_tests/delete_session.sh
```

Troubleshooting

- If you see errors about `jq` not found, install it (e.g., `sudo apt install jq`).
- If the orchestrator fails while creating a session, run the `create_persistent_session.sh` script directly and inspect the JSON it returns — the orchestrator extracts `sessionId` from that response.
- If your API uses different endpoint paths or response shapes, edit the scripts in `scripts/rest_tests/` to match your server.

If you want, I can:
- Add retries/timeouts and better failure detection to the orchestrator.
- Add a CI-friendly wrapper that runs the suite against a running docker-compose stack and fails on any non-2xx responses.
