# Ritual smoke test: adding "start a performance" coverage

## Context

`scripts/smoke_test.sh` currently exercises only part of the Ritual lifecycle on Dis
(clara-api, port 8080): create → join (x2, idempotency check) → status → terminate →
join-after-terminate (expect 409). We want the script to also exercise "starting a
performance" — i.e. actually attaching a deduction cycle to the ritual via `POST /deduce`
with `ritual_id` set, per `docs/rituals_101.md`. This is the piece of the API surface the
current smoke test never touches, even though it's central to what a Ritual is for.

`GET /ritual` listing and the `/cycle/coire/*` observability endpoints were considered as
additional coverage but are out of scope for this change.

The script also currently uses raw inline `curl` with no retry/backoff. A more robust
helper (`http_request METHOD URL [DATA]`) already exists in
`scripts/rest_tests/_common.sh:12-65` — it retries on non-2xx, prints a clear error with
the response body on final failure, and pretty-prints via `jq` on success. The updated
script should source and reuse this helper rather than reinventing it, while still
keeping the whole test in the single `scripts/smoke_test.sh` file.

## Approach

Edit `scripts/smoke_test.sh` in place (no new files). Structure:

1. **Source the shared helper.** At the top, resolve the script's own directory and
   source `rest_tests/_common.sh`:
   ```bash
   DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
   source "$DIR/rest_tests/_common.sh"
   BASE=${BASE_URL:-http://localhost:8080}
   ```
   This pulls in `http_request()`, `RETRIES`, `RETRY_DELAY`, `CURL_MAX_TIME` (all
   env-overridable, matching `scripts/rest_tests/all_tests.sh`'s convention).

2. **Replace the happy-path curl calls with `http_request`.** Create ritual, both joins,
   status, and terminate all expect 2xx — swap these to `http_request POST/GET/DELETE
   "$BASE/..." [payload]`, keeping the existing `jq -r` field extraction and the
   idempotency comparison (`PERFORMANCE_ID_1` vs `PERFORMANCE_ID_2`) unchanged.

3. **Leave the "expect 409" check as raw curl.** `http_request` treats non-2xx as failure
   and retries — that's wrong for the intentional negative check at the end (join after
   terminate). Keep that block exactly as-is (raw `curl -s` + `curl -s -o /dev/null -w
   "%{http_code}"`), just updated to use `$BASE` instead of the hardcoded
   `http://localhost:8080`.

4. **Add the new "start a performance" step**, inserted after the status check and before
   termination:
   - POST to `$BASE/deduce` with `ritual_id` set to `$RITUAL_ID`, using the canonical
     example clauses from `docs/deduce_endpoint.md:195,200` (`man(stan).` /
     `mortal(X) :- man(X).`, goal `mortal(X)`) — these don't call `coire_publish`, so the
     cycle converges on its own without needing a live FieryPit participant, even on
     `InMemoryBroker`. Build the JSON body with `jq -n` (matching
     `scripts/rest_tests/create_session.sh`'s pattern) to avoid manual string escaping:
     ```bash
     DEDUCE_PAYLOAD=$(jq -n --arg rid "$RITUAL_ID" \
       '{prolog_clauses: ["man(stan).", "mortal(X) :- man(X)."], initial_goal: "mortal(X)", ritual_id: $rid}')
     DEDUCE_RESPONSE=$(http_request POST "$BASE/deduce" "$DEDUCE_PAYLOAD")
     DEDUCTION_ID=$(echo "$DEDUCE_RESPONSE" | jq -r '.deduction_id')
     ```
   - Poll `GET /deduce/$DEDUCTION_ID` in a bounded loop (e.g. up to 20 attempts, 0.5s
     apart) until `.status` is no longer `"running"`.
   - Assert the final status is exactly `"converged"`; if it's `"error: ..."`,
     `"interrupted"`, or the loop times out while still `"running"`, print the last
     response and `exit 1`.

5. **Reorder if needed**: create → join x2 (idempotency) → status → **start performance
   (deduce, poll to convergence)** → terminate → join-after-terminate (409). Terminating
   only after the deduction converges keeps the test deterministic (per
   `docs/rituals_101.md:238-240`, a terminated ritual's existing handles keep working, but
   there's no reason to race it).

## Files touched

- `scripts/smoke_test.sh` — all changes above.
- `docs/rituals_101.md` lines ~244-282 (the "smoke test, end to end" section) — update
  the inline copy of the script and the surrounding prose to mention the new deduce/
  performance step, so the doc doesn't silently drift out of sync with the real script.

## Verification

- Run `cargo run --bin clara-api` (or `scripts/start_dev.sh`) locally without
  `KAFKA_BOOTSTRAP` set, so Dis starts with `InMemoryBroker` (no live Kafka/lildaemon
  needed — confirmed via `clara-api/src/main.rs:39-64`).
- Run `./scripts/smoke_test.sh` and confirm all steps print success, including the new
  "start a performance" step reaching `status: converged`.
- Sanity-check failure paths manually once: temporarily set `max_cycles: 0` or an
  unsatisfiable goal to confirm the script correctly detects and reports a non-converged
  result (then revert).
