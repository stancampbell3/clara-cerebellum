# Clara Deduce — Reasoning Cycle Controller

## What is `/deduce`?

`/deduce` is Clara's **neurosymbolic reasoning engine endpoint**. A single POST
launches a self-contained deduction run that cycles between a SWI-Prolog engine
and a CLIPS inference engine until the two reach a stable, convergent state.

The endpoint is asynchronous: the call returns immediately with a `deduction_id`
and the cycle runs in the background on a dedicated blocking thread. You poll for
the result, or cancel it, using that ID.

---

## How the cycle works

```
POST /deduce
     │
     ▼
 DeductionSession::new()          ← fresh isolated Prolog + CLIPS pair
 session.seed_prolog(clauses)     ← assert facts/rules into Prolog
 session.seed_clips(constructs)   ← build defrules/deftemplates into CLIPS
     │
     ▼
 ┌──────────────────────────────────────────────────────────┐
 │                   CYCLE  (repeats up to max_cycles)      │
 │                                                          │
 │  1. Prolog pass                                          │
 │     • consume_coire_events() — dispatch any Coire        │
 │       events waiting in Prolog's mailbox                 │
 │     • query_once(initial_goal | "true")                  │
 │       (initial_goal runs on cycle 0 only)                │
 │                                                          │
 │  2. Relay Prolog → CLIPS                                 │
 │     • drain Prolog's Coire mailbox, re-emit each event   │
 │       into CLIPS's Coire mailbox with a fresh event_id   │
 │                                                          │
 │  3. Evaluator pass  [stub — LilDaemon/FieryPit future]  │
 │                                                          │
 │  4. CLIPS pass                                           │
 │     • consume_coire_events() — dispatch relayed events   │
 │       as facts / (coire-event …) template asserts        │
 │     • (run) — fire inference engine to saturation        │
 │                                                          │
 │  5. Relay CLIPS → Prolog                                 │
 │     • same mechanism in reverse                          │
 │                                                          │
 │  6. Convergence check                                    │
 │     • Prolog's Coire mailbox has zero pending events     │
 │     • CLIPS's Coire mailbox has zero pending events      │
 │     • CLIPS agenda empty (no rules ready to fire)        │
 │     • pending-event snapshot unchanged from last cycle   │
 │     → if all four true: CONVERGED, exit loop             │
 │                                                          │
 │  7. Interrupt check                                      │
 │     • if DELETE /deduce/{id} was called: INTERRUPTED     │
 │                                                          │
 └──────────────────────────────────────────────────────────┘
     │
     ▼
 DeductionResult { status, cycles, prolog_session_id, clips_session_id }
```

### Engine isolation

Each `/deduce` call creates a **completely fresh** Prolog engine and CLIPS
environment. They share no state with sessions created through the `/sessions`
or `/devils/sessions` endpoints. This means:

- Concurrent deductions never interfere.
- Seeded knowledge is ephemeral — it only lives for that run.
- Coire session UUIDs are auto-assigned; the relay uses them to route events
  between engines without either engine knowing about the other.

### Coire as the sole inter-engine channel

Prolog and CLIPS communicate **exclusively** through the
[Coire](https://github.com/anthropics/clara-cerebrum) event mailbox. Neither
engine holds a reference to the other. The relay step reads one engine's pending
events and writes new events (new `event_id`, same payload) addressed to the
other engine's session UUID. This means:

- You can observe traffic by hitting `GET /cycle/coire/snapshot` after a run.
- You can inject external events mid-run with `POST /cycle/coire/push`.
- Adding future evaluators (LilDaemon/LilDevil) is a matter of plugging into
  the same relay between steps 2 and 4 — no engine changes needed.

### Convergence

The cycle is considered **converged** when four conditions hold simultaneously
at the end of a cycle:

1. Prolog's Coire mailbox has zero pending events.
2. CLIPS's Coire mailbox has zero pending events.
3. The CLIPS agenda is empty (no rules ready to fire).
4. The snapshot of pending counts is identical to the snapshot from the
   previous cycle (delta == 0).

Condition 4 guards against a pathological case where rules continuously produce
and consume events at equilibrium without making forward progress.

### Prolog goal failure is non-fatal

If `query_once` returns an error (e.g. the goal fails, throws, or causes a
stack overflow), the cycle logs a `WARN` and continues. Only Coire or session
creation errors propagate as a fatal `CycleError`.

---

## API Reference

> **Important:** All `POST` requests to `/deduce` and `/cycle/coire/push` must
> include the header `Content-Type: application/json`. Without it actix-web's
> JSON extractor will reject the request with `400 Bad Request`.

### `POST /deduce` — start a deduction run

Returns `202 Accepted` immediately. The cycle executes in the background.

**Request body** (`application/json`)

```json
{
  "prolog_clauses":   ["man(stan).", "mortal(X) :- man(X)."],
  "clips_constructs": ["(defrule fire-if-mortal ...)"],
  "initial_goal":     "mortal(X)",
  "max_cycles":       100
}
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `prolog_clauses` | `string[]` | `[]` | Standard Prolog clause syntax (periods included). Loaded via `consult_string`. |
| `clips_constructs` | `string[]` | `[]` | CLIPS `deftemplate`, `defrule`, `defglobal`, etc. Each string is passed to `Build`. |
| `initial_goal` | `string \| null` | `null` | Prolog goal executed on cycle 0 only. Omit or set to `null` to run a no-op (`true`). |
| `max_cycles` | `uint \| null` | `100` | Cycle budget. Exhausting it without convergence results in `error` status. |

All fields are optional. An empty body `{}` is valid and will run a single
no-op cycle that converges immediately.

**Minimal smoke test**

```bash
curl -s -X POST http://localhost:8080/deduce \
  -H 'Content-Type: application/json' \
  -d '{}' | jq .
# → { "deduction_id": "<uuid>", "status": "running" }
```

**Response** `202 Accepted`

```json
{
  "deduction_id": "550e8400-e29b-41d4-a716-446655440000",
  "status":       "running"
}
```

---

### `GET /deduce/{deduction_id}` — poll status

**Response while running** `200 OK`

```json
{
  "deduction_id": "550e8400-e29b-41d4-a716-446655440000",
  "status":       "running",
  "cycles":       0
}
```

The `result` field is absent while the run is still executing. `cycles` reflects
the count completed so far (starts at `0`).

**Response when converged** `200 OK`

```json
{
  "deduction_id": "550e8400-e29b-41d4-a716-446655440000",
  "status":       "converged",
  "cycles":       3,
  "result": {
    "status":            "Converged",
    "cycles":            3,
    "prolog_session_id": "a1b2c3d4-...",
    "clips_session_id":  "e5f6a7b8-..."
  }
}
```

Note: `status` at the top level is a lowercase display string (`"converged"`).
`result.status` is the serialised Rust enum variant name (`"Converged"`).

**Response when interrupted** `200 OK`

```json
{
  "deduction_id": "550e8400-e29b-41d4-a716-446655440000",
  "status":       "interrupted",
  "cycles":       7,
  "result": {
    "status":            "Interrupted",
    "cycles":            7,
    "prolog_session_id": "a1b2c3d4-...",
    "clips_session_id":  "e5f6a7b8-..."
  }
}
```

`result` may be temporarily absent immediately after `DELETE /deduce/{id}` is
called — the interrupt flag is set optimistically and `result` is populated once
the background task actually exits.

**Response when error** `200 OK`

```json
{
  "deduction_id": "550e8400-e29b-41d4-a716-446655440000",
  "status":       "error: Max cycles (100) exceeded without convergence",
  "cycles":       100
}
```

`result` is **absent** for error status. The `status` string prefix matches the
underlying `CycleError` variant:

| Error prefix | Cause |
|---|---|
| `error: Max cycles (N) exceeded without convergence` | Cycle budget exhausted |
| `error: Prolog error: …` | Exception from the Prolog engine |
| `error: CLIPS error: …` | Exception from the CLIPS engine |
| `error: Coire error: …` | Coire mailbox failure |
| `error: Session creation failed: …` | Engine initialisation failure |

**Status values summary**

| `status` | Terminal? | `result` present? | Meaning |
|----------|-----------|-------------------|---------|
| `running` | No | No | Background task is active. |
| `converged` | Yes | Yes | Both engines reached stable state — happy path. |
| `interrupted` | Yes | Yes (once task exits) | Cancelled via `DELETE /deduce/{id}`. |
| `error: <msg>` | Yes | No | Unrecoverable failure or `max_cycles` exceeded. |

**Response** `404 Not Found` — unknown `deduction_id`.

---

### `DELETE /deduce/{deduction_id}` — interrupt a running deduction

Sets an atomic interrupt flag. The background task checks it at the end of every
cycle and exits cleanly. Status transitions to `interrupted` optimistically in
the response; `result` is populated once the background task confirms.

**Response** `200 OK`

```json
{
  "deduction_id": "550e8400-e29b-41d4-a716-446655440000",
  "status":       "interrupted"
}
```

**Response** `404 Not Found` — unknown `deduction_id`.

---

### `GET /cycle/coire/snapshot` — observe Coire state

Returns pending event counts for the Coire sessions of **converged or
interrupted** deduction runs. Error runs are excluded because their
`DeductionResult` is never stored. Useful for debugging relay behaviour.

**Response** `200 OK`

```json
{
  "sessions": [
    { "session_id": "<prolog-uuid>", "pending_count": 0 },
    { "session_id": "<clips-uuid>",  "pending_count": 0 }
  ]
}
```

`pending_count` will be `0` for a converged run — all events were consumed.
A non-zero count after convergence indicates events that were never consumed —
a signal that a rule or predicate failed silently.

---

### `POST /cycle/coire/push` — inject an event into a session

Write a synthetic event directly into a Coire session. The receiving engine
will pick it up via `consume_coire_events()` on its next cycle pass.

**Request body** (`application/json`)

```json
{
  "session_id":  "a1b2c3d4-e5f6-a7b8-c9d0-e1f2a3b4c5d6",
  "origin":      "external-test",
  "event_type":  "assert",
  "data":        "(person (name \"Alice\") (age 30))"
}
```

| Field | Description |
|-------|-------------|
| `session_id` | UUID from a `DeductionResult` (`prolog_session_id` or `clips_session_id`). |
| `origin` | Free-form label for the event source. |
| `event_type` | Stored in the event payload as `"type"`; interpretation is up to the engine's `consume_coire_events` logic. |
| `data` | Payload string stored as `"data"` in the event. |

**Response** `200 OK`

```json
{ "event_id": "<new-uuid>" }
```

---

## Walkthrough: basic Prolog-only deduction

```bash
# 1. Start a deduction with Prolog facts and a goal
curl -s -X POST http://localhost:8080/deduce \
  -H 'Content-Type: application/json' \
  -d '{
    "prolog_clauses": [
      "man(stan).",
      "has_plan(stan).",
      "man_with_the_plan(X) :- man(X), has_plan(X)."
    ],
    "initial_goal": "man_with_the_plan(X)"
  }' | jq .
# → { "deduction_id": "abc-...", "status": "running" }

# 2. Poll until done (fast — no CLIPS rules, converges in 1 cycle)
curl -s http://localhost:8080/deduce/abc-... | jq .
# → { "status": "converged", "cycles": 1, "result": { ... } }
```

---

## Walkthrough: Prolog + CLIPS cooperation

```bash
curl -s -X POST http://localhost:8080/deduce \
  -H 'Content-Type: application/json' \
  -d '{
    "prolog_clauses": [
      "temperature(35).",
      "high_temp(X) :- temperature(X), X > 30."
    ],
    "clips_constructs": [
      "(defrule alert-high-temp (coire-event (ev-type \"assert\") (data \"high_temp\")) => (assert (alert (level critical))))"
    ],
    "initial_goal": "high_temp(X)",
    "max_cycles": 10
  }' | jq .
```

Cycle 0:
1. Prolog runs `high_temp(X)` → succeeds; Coire publish from Prolog fires `high_temp`.
2. Relay moves that event to CLIPS's mailbox.
3. CLIPS consumes it → `(coire-event ...)` fact asserted → `alert-high-temp` rule fires → `(alert (level critical))` asserted.
4. CLIPS's Coire mailbox is empty; relay back to Prolog has nothing to move.
5. Both mailboxes empty, CLIPS agenda empty, snapshot stable → **converged**.

---

## Walkthrough: cancel a long-running deduction

To demonstrate cancellation, seed a CLIPS rule that continuously re-asserts
events so the cycle never converges:

```bash
# Start a non-converging deduction
ID=$(curl -s -X POST http://localhost:8080/deduce \
  -H 'Content-Type: application/json' \
  -d '{
    "clips_constructs": [
      "(defrule always-fire => (assert (tick)))"
    ],
    "max_cycles": 9999
  }' | jq -r .deduction_id)

# Cancel it
curl -s -X DELETE http://localhost:8080/deduce/$ID | jq .
# → { "deduction_id": "...", "status": "interrupted" }

# Confirm
curl -s http://localhost:8080/deduce/$ID | jq .status
# → "interrupted"
```

> **Note on `initial_goal`:** The initial goal only executes on cycle 0. Goals
> that would recurse infinitely (e.g. `loop :- loop.`) cause the Prolog engine
> to fail or overflow on that one call; the error is logged as a `WARN` and the
> cycle continues normally. To produce a genuinely long-running deduction for
> cancellation testing, use CLIPS rules that keep generating facts (as above)
> rather than a recursive Prolog goal.

---

## Suggestions for use

### Use `initial_goal` for targeted queries

The `initial_goal` fires on cycle 0 only. Use it to trigger a specific Prolog
derivation whose side-effects (via `coire_publish`) then ripple through CLIPS.
Leave it null when you only want CLIPS rules to react to seeded facts.

### Keep knowledge ephemeral

Each deduction creates a throwaway engine pair. If you need persistent knowledge
across multiple deductions, maintain it externally and re-seed each call. This
keeps runs reproducible and avoids hidden state bugs.

### Tune `max_cycles` conservatively

Start with the default of 100 and lower it once you understand your rule set's
convergence behaviour. A low `max_cycles` with a clear error on exhaustion is
easier to debug than a silent infinite loop.

### Poll with backoff

There is no WebSocket or long-poll for deduction results yet. Poll
`GET /deduce/{id}` with an exponential backoff (e.g. 50 ms → 100 ms → 200 ms
→ …, capped at 2 s) to avoid hammering the server during long runs.

### Observe via snapshot

`GET /cycle/coire/snapshot` shows post-run Coire state for converged and
interrupted runs (error runs are excluded). A fully-converged run should show
`pending_count: 0` for both sessions. Non-zero counts after convergence indicate
events that were never consumed — a useful signal that a rule or predicate failed
silently.

### Inject events for testing

`POST /cycle/coire/push` lets you write events into a session's mailbox outside
of a running deduction. This is useful for:
- Unit-testing CLIPS rules in isolation by injecting synthetic events.
- Exploring how CLIPS reacts to specific event payloads before wiring up the
  full Prolog side.
- Simulating LilDaemon/LilDevil output ahead of the evaluator-pass integration.

### Evaluator pass (coming)

Step 3 of each cycle is currently a no-op stub logged as
`CycleController: evaluator_pass (stub)`. When the FieryPit evaluator
integration lands, this step will invoke registered CycleMember LilDaemons
(LLM-based) and LilDevils (logic-based) between the Prolog and CLIPS passes,
allowing neural and symbolic reasoning to interleave within a single cycle.

### Debug logging

Run the server with `RUST_LOG=debug` to see per-cycle trace output from the
controller, including convergence check details and relay counts. The Logger
middleware also emits an `INFO` line per HTTP request.

---

## Status reference

| Status string | Terminal? | Meaning |
|---------------|-----------|---------|
| `running` | No | Background task is active. |
| `converged` | Yes | Both engines stable; deduction complete. |
| `interrupted` | Yes | Cancelled via `DELETE /deduce/{id}`. |
| `error: <msg>` | Yes | Unrecoverable failure (see error prefix table above). |
