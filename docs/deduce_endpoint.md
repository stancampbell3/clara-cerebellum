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
 DeductionSession::new()          ← fresh isolated Prolog + CLIPS pair + Dagda tableau
 session.seed_prolog(clauses)     ← assert facts/rules into Prolog
 session.seed_clips_file(path)?   ← load .clp file (if clips_file provided)
 session.seed_clips(constructs)   ← build inline defrules/deftemplates into CLIPS
 session.seed_context(context)?   ← assert deduce_context_json/1 into Prolog (if context provided)
     │
     ▼
 ┌──────────────────────────────────────────────────────────────┐
 │                   CYCLE  (repeats up to max_cycles)          │
 │                                                              │
 │  1. Prolog pass                                              │
 │     • consume_coire_events() — dispatch any Coire            │
 │       events waiting in Prolog's mailbox                     │
 │     • cycle 0: query_with_bindings(initial_goal | "true")    │
 │       — all solutions collected with variable bindings       │
 │       and stored in DeductionResult.prolog_solutions         │
 │     • cycle 1+: query_once("true") — engine tick only        │
 │                                                              │
 │  2. Relay Prolog → CLIPS                                     │
 │     • drain Prolog's Coire mailbox, re-emit each event       │
 │       into CLIPS's Coire mailbox with a fresh event_id       │
 │     • [trace] record tableau snapshot ("prolog_to_clips")    │
 │                                                              │
 │  3. Evaluator pass  [ritual feature — FieryPit peer eval]    │
 │     • if ritual_handle present:                              │
 │       – drain Prolog Coire events with evaluator origin      │
 │         → publish each as an "offering" TephraEnvelope on    │
 │           the Ritual Kafka topic; increment                  │
 │           pending_evaluator_responses                        │
 │       – poll Ritual topic for new TephraEnvelopes            │
 │         → ingest each as a "ritual/<label>" Coire event in   │
 │           Prolog's mailbox (Hohi/Tabu decrement counter)     │
 │       – patience: if peer silent for                         │
 │         evaluator_patience_cycles (default 10) cycles,       │
 │         assert "ritual/tabu-timeout" and clear counter       │
 │                                                              │
 │  4. CLIPS pass                                               │
 │     • consume_coire_events() — dispatch relayed events       │
 │       as facts / (coire-event …) template asserts            │
 │     • (run) — fire inference engine to saturation            │
 │                                                              │
 │  5. Relay CLIPS → Prolog                                     │
 │     • same mechanism in reverse                              │
 │     • [trace] record tableau snapshot ("clips_to_prolog")    │
 │                                                              │
 │  6. Convergence check                                        │
 │     • Prolog's Coire mailbox has zero pending events         │
 │     • CLIPS's Coire mailbox has zero pending events          │
 │     • CLIPS agenda empty (no rules ready to fire)            │
 │     • pending-event snapshot unchanged from last cycle       │
 │     • Dagda tableau unchanged since last cycle (fixed point) │
 │     • pending_evaluator_responses == 0 (when ritual enabled) │
 │     → if all six true: CONVERGED, exit loop                  │
 │     • [trace] record tableau snapshot ("final_converged")    │
 │                                                              │
 │  7. Interrupt check                                          │
 │     • if DELETE /deduce/{id} was called: INTERRUPTED         │
 │                                                              │
 └──────────────────────────────────────────────────────────────┘
     │
     ▼
 DeductionResult { status, cycles, prolog_session_id, clips_session_id,
                   prolog_solutions?, goal_bindings?, tableau, trace? }
```

### Engine isolation

Each `/deduce` call creates a **completely fresh** Prolog engine, CLIPS
environment, and Dagda tableau. They share no state with sessions created
through the `/sessions` or `/devils/sessions` endpoints. This means:

- Concurrent deductions never interfere.
- Seeded knowledge is ephemeral — it only lives for that run.
- Coire session UUIDs are auto-assigned; the relay uses them to route events
  between engines without either engine knowing about the other.

### Coire as the sole inter-engine channel

Prolog and CLIPS communicate **exclusively** through the Coire event mailbox.
Neither engine holds a reference to the other. The relay step reads one
engine's pending events and writes new events (new `event_id`, same payload)
addressed to the other engine's session UUID. This means:

- You can observe traffic by hitting `GET /cycle/coire/snapshot` after a run.
- You can inject external events mid-run with `POST /cycle/coire/push`.
- Adding future evaluators (LilDaemon/LilDevil) is a matter of plugging into
  the same relay between steps 2 and 4 — no engine changes needed.

### Convergence

The cycle is considered **converged** when all of the following conditions hold
simultaneously at the end of a cycle:

1. Prolog's Coire mailbox has zero pending events.
2. CLIPS's Coire mailbox has zero pending events.
3. The CLIPS agenda is empty (no rules ready to fire).
4. The snapshot of pending counts is identical to the snapshot from the
   previous cycle (delta == 0).
5. The Dagda tableau has not changed since the start of this cycle
   (truth-value fixed point).
6. `pending_evaluator_responses == 0` — all Offerings published to the Ritual
   topic have been answered by a Hohi or Tabu from peer evaluators. (Only
   evaluated when the `ritual` feature is enabled and a `ritual_id` was
   supplied; always satisfied otherwise.)

Condition 4 guards against a pathological case where rules continuously produce
and consume events at equilibrium without making forward progress. Condition 5
ensures that the truth-value assignments tracked in the Dagda tableau have
stabilised — necessary because rule firings can change truth values without
producing Coire events. Condition 6 prevents convergence while outstanding
Offerings are awaiting peer evaluation responses; if a peer is silent for
`evaluator_patience_cycles` consecutive cycles (default 10), a synthetic
`ritual/tabu-timeout` Coire event is injected and the counter is cleared so the
cycle can continue.

### Prolog goal failure is non-fatal

If the cycle-0 goal fails or throws an exception, `prolog_solutions` is set to
`[]` and the cycle logs a `WARN` but continues. Only Coire or session creation
errors propagate as a fatal `CycleError`. Subsequent cycles run `true` as a
no-op tick and do not contribute to `prolog_solutions`.

---

## API Reference

> **Important:** All `POST` requests to `/deduce` and `/cycle/coire/push` must
> include the header `Content-Type: application/json`. Without it actix-web's
> JSON extractor will reject the request with `400 Bad Request`.

### `GET /deduce` — list persisted deductions

Returns a summary of recent deduction runs, newest first. Useful for discovering
UUIDs to inspect with the trace endpoints or `baloroptik`.

Accepts an optional `?limit=N` query parameter (default 50, server cap 500).

Requires persistence (Coire store) to be configured. Returns `503` if not.

**Response** `200 OK`

```json
{
  "deductions": [
    {
      "deduction_id":  "550e8400-e29b-41d4-a716-446655440000",
      "status":        "converged",
      "cycles_run":    3,
      "initial_goal":  "mortal(X)",
      "created_at_ms": 1744286400000
    },
    {
      "deduction_id":  "7cf5e9cf-1052-4435-b8fa-0c7c7e6cd371",
      "status":        "converged",
      "cycles_run":    3,
      "initial_goal":  "omelette(bob, X).",
      "created_at_ms": 1744283600000
    }
  ]
}
```

**Response** `503 Service Unavailable` — persistence not enabled.

---

### `POST /deduce` — start a deduction run

Returns `202 Accepted` immediately. The cycle executes in the background.

**Request body** (`application/json`)

```json
{
  "prolog_clauses":   ["man(stan).", "mortal(X) :- man(X)."],
  "clips_constructs": ["(defrule fire-if-mortal ...)"],
  "clips_file":       "/srv/rules/base.clp",
  "prolog_source_id": "3fa85f64-5717-4562-b3fc-2c963f66afa6",
  "clips_source_id":  "7cb98e21-1234-5678-abcd-ef0123456789",
  "initial_goal":     "mortal(X)",
  "max_cycles":       100,
  "persist":          false,
  "trace":            false,
  "ritual_id":        "a2b3c4d5-e6f7-4890-ab12-cd3456789012",
  "context": [
    {"role": "user",      "content": "I need help finding the exit."},
    {"role": "assistant", "content": "I can help with that."}
  ]
}
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `prolog_clauses` | `string[]` | `[]` | Standard Prolog clause syntax (periods included). Loaded via `consult_string`. Ignored when `prolog_source_id` is set. |
| `clips_constructs` | `string[]` | `[]` | CLIPS `deftemplate`, `defrule`, `defglobal`, etc. Each string is passed to `Build`. Ignored when `clips_source_id` is set. |
| `clips_file` | `string \| null` | `null` | Server-side path to a `.clp` file. Loaded **before** `clips_constructs`. Ignored when `clips_source_id` is set. |
| `prolog_source_id` | `uuid \| null` | `null` | Pre-registered Prolog source from `POST /source`. When set, `prolog_clauses` is ignored and the stored content is used instead. The source's artifacts (parsed rules, DOT) are also available for trace visualization. |
| `clips_source_id` | `uuid \| null` | `null` | Pre-registered CLIPS source from `POST /source`. When set, `clips_file` and `clips_constructs` are ignored. |
| `initial_goal` | `string \| null` | `null` | Prolog goal executed on cycle 0 only. Omit or set to `null` to run a no-op (`true`). |
| `max_cycles` | `uint \| null` | `100` | Cycle budget. Exhausting it without convergence results in `error` status. |
| `persist` | `bool` | `false` | When `true` and persistence is configured, save a full snapshot on completion for later resumption via `POST /deduce/resume`. |
| `trace` | `bool` | `false` | When `true`, record a Dagda tableau snapshot after each relay phase. With a store configured, snapshots are written to `tableau_changes` and queryable via `GET /deduce/{id}/trace`. Without a store, the trace is returned inline in `DeductionResult.trace`. |
| `context` | `object[]` | `[]` | Optional conversational context (external message history). Each element is a free-form JSON object — typically `{"role": "...", "content": "..."}`. Made available to Prolog rules via `current_context/1` and forwarded to LLM evaluate calls that accept a `context` field. |
| `ritual_id` | `uuid \| null` | `null` | Join an existing active Ritual so that step 3 of every cycle publishes outbound evaluator-tagged Coire events to the Ritual Kafka topic and ingests incoming Tephras from peer FieryPit evaluators. When `null` (default), the evaluator pass is a no-op. |

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
    "clips_session_id":  "e5f6a7b8-...",
    "prolog_solutions":  [{"Man": "stan"}],
    "goal_bindings":     "Man = stan",
    "tableau":           [/* PredicateEntry objects */],
    "trace":             null
  }
}
```

Note: `status` at the top level is a lowercase display string (`"converged"`).
`result.status` is the serialised Rust enum variant name (`"Converged"`).

`result.prolog_solutions` is a JSON array of all solutions produced by
`initial_goal` on cycle 0, each element being an object mapping Prolog
variable name to value (e.g. `[{"Man": "stan"}]`). Special cases:

| `prolog_solutions` value | Meaning |
|---|---|
| `[{"X": val, …}, …]` | Goal succeeded — one object per solution |
| `[true]` | Goal succeeded with no unbound variables (ground query) |
| `[]` | Goal failed — no solutions |
| absent | No `initial_goal` was provided, or the run ended in error |

`result.trace` is non-null only when `trace: true` was requested **and** no
Coire store is configured. With a store it is `null` — use
`GET /deduce/{id}/trace` to fetch the recorded phases.

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
    "clips_session_id":  "e5f6a7b8-...",
    "prolog_solutions":  [{"Man": "stan"}]
  }
}
```

`prolog_solutions` is included if cycle 0 had already run before the interrupt
was processed. `result` may be temporarily absent immediately after
`DELETE /deduce/{id}` is called — the interrupt flag is set optimistically and
`result` is populated once the background task actually exits.

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
| `error: CLIPS error: …` | Exception from the CLIPS engine, including `.clp` file load failures |
| `error: Coire error: …` | Coire mailbox failure |
| `error: Session creation failed: …` | Engine initialisation failure |
| `error: Context seeding failed: …` | Could not serialise or assert the `context` payload |

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

### `GET /deduce/{id}/trace` — list recorded tableau phases

Returns the ordered list of tableau snapshots recorded during a `trace: true`
run. Each element is a phase summary without the full entry payload — use the
`/entries` sub-endpoint for that.

Requires persistence (Coire store) to be configured. Returns `503` if not.
Returns an empty `trace: []` if the run completed without `trace: true`.

**Response** `200 OK`

```json
{
  "trace": [
    {
      "change_id":      "c1d2e3f4-...",
      "deduction_id":   "550e8400-...",
      "cycle_num":      0,
      "phase":          "initial",
      "event_origin":   null,
      "event_type":     null,
      "recorded_at_ms": 1744286400000
    },
    {
      "change_id":      "d2e3f4a5-...",
      "deduction_id":   "550e8400-...",
      "cycle_num":      0,
      "phase":          "prolog_to_clips",
      "event_origin":   "prolog",
      "event_type":     "assert",
      "recorded_at_ms": 1744286400050
    }
  ]
}
```

| Phase value | When recorded |
|---|---|
| `"initial"` | Before the first cycle |
| `"prolog_to_clips"` | After each Prolog → CLIPS relay |
| `"clips_to_prolog"` | After each CLIPS → Prolog relay |
| `"final_converged"` | At convergence |
| `"final_interrupted"` | When interrupted |
| `"final_max_cycles"` | When the cycle budget is exhausted |

---

### `GET /deduce/{id}/trace/{change_id}/dot` — colorized DOT for one phase

Returns a Graphviz DOT string for the rule/fact graph, with node fill colors
reflecting the truth values recorded in the tableau at that phase.

The DOT is built from parsed Prolog rules associated with the deduction's
`prolog_source_id`. The `parsed_rules` artifact is generated on the first
request and cached — subsequent calls skip re-parsing.

Requires a `prolog_source_id` to be set on the deduction's snapshot (i.e. the
original request must have used `prolog_source_id` or had inline clauses
auto-registered). Returns `422 Unprocessable Entity` if no source ID is
available.

**Response** `200 OK` — `text/plain; charset=utf-8` (raw DOT source)

```dot
digraph Clara {
    rankdir=LR
    ...
    mortal_0 [label="mortal(X)" shape=box style=filled fillcolor="#28a745"]
    man_0    [label="man(stan)" shape=ellipse style=filled fillcolor="#d4edda"]
    ...
}
```

**Truth-value fill colors**

| Color | Value |
|---|---|
| `#28a745` (green) | `KnownTrue` |
| `#dc3545` (red) | `KnownFalse` |
| `#ffc107` (amber) | `KnownUnresolved` — mixed or conflicting entries for the same functor |
| `#adb5bd` (gray) | `Unknown` |
| Structural default | Not yet in the tableau |

**Response** `404 Not Found` — unknown `change_id` or deduction.
**Response** `422 Unprocessable Entity` — no `prolog_source_id` on the snapshot.
**Response** `503 Service Unavailable` — persistence not enabled.

---

### `GET /deduce/{id}/trace/{change_id}/entries` — raw predicate entries for one phase

Returns the full `PredicateEntry` slice recorded at a specific phase.

**Response** `200 OK`

```json
{
  "change_id":      "c1d2e3f4-...",
  "deduction_id":   "550e8400-...",
  "cycle_num":      0,
  "phase":          "prolog_to_clips",
  "recorded_at_ms": 1744286400050,
  "entries": [
    {
      "entry_id":    "a1b2c3d4-...",
      "session_id":  "550e8400-...",
      "kind":        "Predicate",
      "functor":     "mortal",
      "arity":       1,
      "source":      "prolog",
      "bound_vars":  ["X"],
      "bindings":    [{"var": "X", "val": "stan"}],
      "truth_value": "KnownTrue",
      "parent_id":   null
    }
  ]
}
```

**Response** `404 Not Found` — unknown `change_id` or deduction.
**Response** `503 Service Unavailable` — persistence not enabled.

---

### `GET /deduce/{id}/trace/export` — full tableau changes export

Returns the complete ordered `Vec<TableauChange>` for a deduction run,
including the full `entries_json` payload for each phase. Intended for offline
replay with `baloroptik replay` — save the response to a file and use it with
a snapshot JSON to reconstruct the trace without a running server.

Requires persistence (Coire store) to be configured. Returns `503` if not.

**Response** `200 OK` — JSON array

```json
[
  {
    "change_id":      "c1d2e3f4-...",
    "deduction_id":   "550e8400-...",
    "cycle_num":      0,
    "phase":          "initial",
    "event_origin":   null,
    "event_type":     null,
    "event_data":     null,
    "entries_json":   "[{\"functor\":\"mortal\",...}]",
    "recorded_at_ms": 1744286400000
  },
  {
    "change_id":      "d2e3f4a5-...",
    "deduction_id":   "550e8400-...",
    "cycle_num":      0,
    "phase":          "prolog_to_clips",
    "event_origin":   "prolog",
    "event_type":     "assert",
    "event_data":     "{...}",
    "entries_json":   "[{\"functor\":\"mortal\",...},{\"functor\":\"man\",...}]",
    "recorded_at_ms": 1744286400050
  }
]
```

**Typical usage with `baloroptik replay`:**

```bash
# Export (server must be running)
curl -s http://localhost:8080/deduce/550e8400-.../trace/export > changes.json

# Replay offline
baloroptik replay snapshot.json changes.json --out-dir ./eye --format html
```

**Response** `404 Not Found` — unknown `deduction_id`.
**Response** `503 Service Unavailable` — persistence not enabled.

---

### `POST /deduce/resume` — resume a persisted deduction

Looks up the snapshot saved for `deduction_id`, re-seeds fresh engine instances
from the stored knowledge (or the registered source if `prolog_source_id` /
`clips_source_id` are set on the snapshot), restores pending Coire events, and
runs the cycle again under a new `deduction_id`.

**Request body** (`application/json`)

```json
{
  "deduction_id": "550e8400-e29b-41d4-a716-446655440000",
  "max_cycles":   50,
  "persist":      true,
  "trace":        false,
  "context":      null
}
```

| Field | Type | Default | Description |
|---|---|---|---|
| `deduction_id` | UUID | required | The `deduction_id` from the original run |
| `max_cycles` | `uint \| null` | snapshot value | Override the cycle budget for this run |
| `persist` | `bool` | `false` | Save a new snapshot at completion to allow further chained resumes |
| `trace` | `bool` | `false` | Enable per-phase tableau recording for this resumed run |
| `context` | `object[] \| null` | snapshot value | Override the conversational context; uses the snapshot's context if omitted |

**Response** `202 Accepted`

```json
{
  "deduction_id": "9c1d4e8f-...",
  "status":       "running"
}
```

**Response** `409 Conflict` — original session engines still active.
**Response** `503 Service Unavailable` — persistence not enabled.

---

### `GET /deduce/{id}/snapshot` — inspect a persisted snapshot

**Response** `200 OK` — full `DeductionSnapshot` object including seed knowledge,
Coire session IDs, cycle count, final status, serialized tableau entries, and
`prolog_source_id` / `clips_source_id` when the run used registered sources.

**Response** `404 Not Found` — no snapshot for this ID.
**Response** `503 Service Unavailable` — persistence not enabled.

---

### `DELETE /deduce/{id}/snapshot` — delete a persisted snapshot

Removes the snapshot row and all associated Coire events.

**Response** `409 Conflict` — session still active.
**Response** `404 Not Found` — no snapshot for this ID.

**Response** `200 OK`

```json
{
  "deduction_id": "550e8400-e29b-41d4-a716-446655440000",
  "status":       "deleted"
}
```

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

### `POST /ritual` — create a Ritual

Creates a new Ritual, ensures the associated Kafka topic exists, and optionally
bootstraps listed FieryPit participant URLs by calling `POST /ritual/join` on
each one. Returns immediately with the new `ritual_id`.

**Request body** (`application/json`)

```json
{
  "name":         "my-eval-ritual",
  "participants": ["http://fiery-pit-1:6666", "http://fiery-pit-2:6666"]
}
```

| Field | Type | Required | Description |
|---|---|---|---|
| `name` | `string` | Yes | Human-readable label for the Ritual. Not used for routing. |
| `participants` | `string[]` | No | FieryPit base URLs to bootstrap. Each receives `POST /ritual/join` with the new topic and bootstrap server. Failures are logged as warnings and do not abort the response. |

**Response** `201 Created`

```json
{ "ritual_id": "a2b3c4d5-e6f7-4890-ab12-cd3456789012" }
```

**Response** `400 Bad Request` — invalid `dis_domain` produces an invalid Kafka
topic name.

---

### `GET /ritual/{id}/join` — join a Ritual

Returns routing information for a participant. Pass the result's `topic` and
`dis_domain` to the FieryPit consumer/producer when connecting to Kafka.

**Query parameters**

| Parameter | Description |
|---|---|
| `participant` (optional) | Stable caller-supplied key (e.g. FieryPit base URL). When provided, repeated calls with the same key return the same `performance_id` (idempotent). Omitting always generates a fresh `performance_id`. |

**Response** `200 OK`

```json
{
  "ritual_id":      "a2b3c4d5-e6f7-4890-ab12-cd3456789012",
  "performance_id": "f1e2d3c4-b5a6-4789-0123-456789abcdef",
  "topic":          "dis.local.ritual.a2b3c4d5",
  "dis_domain":     "dis.local"
}
```

**Response** `404 Not Found` — unknown `ritual_id`.
**Response** `409 Conflict` — Ritual is terminated.

---

### `GET /ritual/{id}/status` — inspect Ritual state

**Response** `200 OK`

```json
{
  "ritual_id": "a2b3c4d5-e6f7-4890-ab12-cd3456789012",
  "state":     "active"
}
```

`state` is `"active"` or `"terminated"`.

**Response** `404 Not Found` — unknown `ritual_id`.

---

### `DELETE /ritual/{id}` — terminate a Ritual

Marks the Ritual as terminated. Existing `RitualHandle`s held by running
`CycleController` instances continue to work until the Kafka topic is cleaned up
(future admin API). New `join` calls on a terminated Ritual are rejected with
`409 Conflict`.

**Response** `200 OK`

```json
{
  "ritual_id": "a2b3c4d5-e6f7-4890-ab12-cd3456789012",
  "status":    "terminated"
}
```

**Response** `404 Not Found` — unknown `ritual_id`.

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
# → {
#     "status": "converged",
#     "cycles": 1,
#     "result": {
#       "status": "Converged", "cycles": 1,
#       "prolog_session_id": "...", "clips_session_id": "...",
#       "prolog_solutions": [{"Man": "stan"}]
#     }
#   }
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
5. Both mailboxes empty, CLIPS agenda empty, snapshot stable, tableau fixed → **converged**.

---

## Walkthrough: trace visualization

Register the Prolog source first so the server can cache the parsed rule graph:

```bash
# 1. Register source
SRC=$(curl -s -X POST http://localhost:8080/source \
  -H 'Content-Type: application/json' \
  -d '{
    "source_type": "prolog",
    "label":       "fire_alarm",
    "content":     "fire(Where) :- smoke(Where).\nalarm(Place) :- fire(Place)."
  }' | jq -r .source_id)

# 2. Run a traced deduction using the registered source
ID=$(curl -s -X POST http://localhost:8080/deduce \
  -H 'Content-Type: application/json' \
  -d "{
    \"prolog_source_id\": \"$SRC\",
    \"initial_goal\":     \"fire(kitchen)\",
    \"trace\":            true,
    \"persist\":          true
  }" | jq -r .deduction_id)

# 3. Wait for convergence, then list phases
curl -s http://localhost:8080/deduce/$ID/trace | jq .

# 4. Grab the change_id of the final phase and fetch its colorized DOT
CHANGE=$(curl -s http://localhost:8080/deduce/$ID/trace \
  | jq -r '.trace[-1].change_id')
curl -s http://localhost:8080/deduce/$ID/trace/$CHANGE/dot
```

The DOT output can be fed directly to Graphviz (`dot -Tsvg`) or rendered in
the browser with `@viz-js/viz`.

---

## Walkthrough: context-grounded LLM reasoning

Pass a conversation history into the deduction so that Prolog rules can ask the
LLM to evaluate statements *in the light of what the user said*, rather than
in a vacuum.

```bash
curl -s -X POST http://localhost:8080/deduce \
  -H 'Content-Type: application/json' \
  -d '{
    "prolog_clauses": [
      "consult('\''reception_rules.pl'\'')."
    ],
    "initial_goal": "triage_visitor(alice, Outcome)",
    "context": [
      {"role": "user",      "content": "Where is the exit?"},
      {"role": "assistant", "content": "It is down the hall to the left."},
      {"role": "user",      "content": "I am completely lost."}
    ]
  }' | jq .
```

Inside `reception_rules.pl` the context is retrieved with `current_context/1`
and fed to `clara_fy/3`:

```prolog
:- use_module(library(the_rat)).

triage_visitor(Visitor, help_kiosk) :-
    current_context(Ctx),
    clara_fy('the visitor seems confused or lost', Ctx, true).

triage_visitor(Visitor, proceed) :-
    current_context(Ctx),
    clara_fy('the visitor knows where they are going', Ctx, true).
```

`clara_fy/3` calls the LLM via `ponder_text_with_context/3`, which forwards the
`context` array in the `/evaluate` payload to FieryPit. The LLM verifies the
statement against the conversation history and returns a judgement that
`descriminate_k` maps to `true`, `false`, or `unresolved`.

### Prolog predicates for context

All context predicates live in `library(the_rabbit)` (imported automatically by
`library(the_rat)`).

| Predicate | Arity | Description |
|-----------|-------|-------------|
| `current_context/1` | 1 | Retrieve the injected context as a list of dicts. Returns `[]` when no context was provided. |
| `ponder_text_with_context/3` | `(+Text, +Context, -Result)` | Call the LLM with `Text` grounded by `Context`. Returns raw JSON from FieryPit. |
| `descriminate_k_with_context/4` | `(+Text, +K, +Context, -Results)` | As `descriminate_k/3` but context-grounded. Drives classify pipeline. |
| `clara_fy/3` | `(+Text, +Context, -TruthValue)` | Context-aware truth classification. Returns `true`, `false`, or `unresolved`. |

The context is stored as the Prolog fact `deduce_context_json(JsonAtom)` in the
session's knowledge base (asserted by `seed_context` before the first cycle).

---

## Walkthrough: peer evaluation via Ritual

This example shows a deduction that routes an evaluator goal to a peer FieryPit
evaluator over Kafka and consumes the response in Prolog.

```bash
# 1. Create a Ritual that bootstraps the FieryPit evaluator
RITUAL=$(curl -s -X POST http://localhost:8080/ritual \
  -H 'Content-Type: application/json' \
  -d '{
    "name":         "eval-demo",
    "participants": ["http://fiery-pit-1:6666"]
  }' | jq -r .ritual_id)

# 2. Start a deduction that joins the Ritual
ID=$(curl -s -X POST http://localhost:8080/deduce \
  -H 'Content-Type: application/json' \
  -d "{
    \"prolog_clauses\": [
      \":- use_module(library(the_coire)).\",
      \"eval_via_peer(Text, Result) :-\",
      \"    coire_publish(evaluator/offering, json([text=Text])),\",
      \"    coire_poll(ritual/hohi, Env),\",
      \"    get_dict(result, Env, Result).\"
    ],
    \"initial_goal\": \"eval_via_peer('is the user lost?', R)\",
    \"ritual_id\":    \"$RITUAL\"
  }" | jq -r .deduction_id)

# 3. Poll until done
curl -s http://localhost:8080/deduce/$ID | jq .status

# 4. Inspect result
curl -s http://localhost:8080/deduce/$ID | jq .result.prolog_solutions

# 5. Terminate the Ritual
curl -s -X DELETE http://localhost:8080/ritual/$RITUAL | jq .
```

**What happens:**

- Cycle 0: Prolog runs `eval_via_peer/2`, which calls `coire_publish/2` with
  origin `evaluator/offering`. The evaluator pass picks this up and publishes a
  `TephraEnvelope` (label `offering`) to the Ritual Kafka topic.
- The bootstrapped FieryPit evaluator receives the Offering, evaluates it, and
  publishes a Hohi response (label `hohi`) back on the same topic.
- On the next cycle, the evaluator pass polls the topic, ingests the Hohi as a
  `ritual/hohi` Coire event in Prolog's mailbox, and decrements
  `pending_evaluator_responses` to 0.
- `coire_poll(ritual/hohi, Env)` unifies with the ingested event, binding
  `Result`.
- Convergence: all six conditions satisfied → `converged`.

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

> **Note on `initial_goal`:** The initial goal only executes on cycle 0. All
> solutions are collected via backtracking before the cycle proceeds, so goals
> that produce many solutions (e.g. `member(X, [a,b,c])`) will enumerate them
> all and return them in `prolog_solutions`. Goals that recurse infinitely or
> overflow the stack produce `prolog_solutions: []` and log a `WARN`; the cycle
> continues normally. To produce a genuinely long-running deduction for
> cancellation testing, use CLIPS rules that keep generating facts (as above)
> rather than a recursive Prolog goal.

---

## Suggestions for use

### Pre-register Prolog sources for trace visualization

Register source files via `POST /source` before running a deduction. Supply the
returned `prolog_source_id` in the deduce request to enable:

- Content-addressed dedup — the same source uploaded twice returns the same ID.
- `parsed_rules` artifact caching — the DOT generator parses the source once and
  caches the result; subsequent trace requests skip re-parsing.
- Colorized DOT graphs at each recorded phase via `GET /deduce/{id}/trace/{change_id}/dot`.

### Use `trace: true` selectively

Tracing records a full tableau snapshot after every relay phase. For large rule
sets or many cycles this can be expensive. Enable it during development and
debugging; leave it off in production unless you need per-phase observability.

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

### Evaluator pass (Ritual)

Step 3 of each cycle is the **evaluator pass**. When a `ritual_id` is supplied
and the `ritual` feature is compiled in, the pass does two things:

1. **Drain outbound** — reads Prolog Coire events whose `origin` starts with
   `evaluator/` and publishes each as a `TephraEnvelope` (label `offering`) on
   the Ritual Kafka topic. Each published Offering increments an internal
   `pending_evaluator_responses` counter.
2. **Ingest inbound** — polls the Ritual Kafka topic for new `TephraEnvelope`s
   and writes each as a `ritual/<label>` Coire event into Prolog's mailbox
   (accessible via `coire_poll/2` in Prolog rules). A `hohi` or `tabu` envelope
   decrements the counter; a `tabu` signals an error response from the peer.

If no `ritual_id` is set the pass is a no-op (logged at `debug` level).

#### Patience timeout

If `pending_evaluator_responses > 0` and the peer has been silent for
`evaluator_patience_cycles` consecutive cycles (default 10), the controller
injects a synthetic `ritual/tabu-timeout` Coire event and resets the counter to
zero so the cycle can proceed. Prolog/CLIPS rules may pattern-match on this
event to implement recovery logic or declare the goal unresolvable.

#### Well-known Tephra labels

| Label | Direction | Meaning |
|---|---|---|
| `offering` | outbound (clara-api → peer) | Evaluator request published from a Prolog rule via `coire_publish/2` with origin `evaluator/...` |
| `hohi` | inbound (peer → clara-api) | Successful evaluator response; decrements `pending_evaluator_responses` |
| `tabu` | inbound (peer → clara-api) | Error evaluator response; also decrements `pending_evaluator_responses` |
| `prolog_fact` | inbound | Peer asserts a Prolog fact into the session |
| `clips_fire` | inbound | Peer triggers a CLIPS rule by name |
| `clara_fy_hit` | inbound | Peer reports a classification result |
| `deduction_event` | inbound | Generic structured event for rule consumption |

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

---

## Source code index

File paths are relative to the workspace root. Line numbers reference the
function or type definition start.

### Pre-cycle setup

| Topic | File : line | Function / symbol |
|---|---|---|
| Create engine pair | `clara-cycle/src/session.rs:21` | `DeductionSession::new()` |
| Seed Prolog clauses | `clara-cycle/src/session.rs:33` | `DeductionSession::seed_prolog()` |
| — load via `consult_string` | `clara-prolog/src/backend/ffi/environment.rs:296` | `PrologEnvironment::consult_string()` |
| Seed CLIPS file (optional) | `clara-cycle/src/session.rs:45` | `DeductionSession::seed_clips_file()` |
| — load via CLIPS `Load()` | `clara-clips/src/backend/ffi/environment.rs` | `ClipsEnvironment::load()` |
| Seed CLIPS constructs | `clara-cycle/src/session.rs:52` | `DeductionSession::seed_clips()` |
| — compile via `build` | `clara-clips/src/backend/ffi/environment.rs:146` | `ClipsEnvironment::build()` |
| Seed context into Prolog | `clara-cycle/src/session.rs:60` | `DeductionSession::seed_context()` |
| — asserts `deduce_context_json/1` | `clara-prolog/src/backend/ffi/environment.rs:262` | `PrologEnvironment::assertz()` |
| Construct the controller | `clara-cycle/src/controller.rs:24` | `CycleController::new()` |
| Enable trace mode | `clara-cycle/src/controller.rs` | `CycleController::with_trace(bool)` |
| Start the cycle loop | `clara-cycle/src/controller.rs:38` | `CycleController::run()` |
| Resolve prolog_source_id | `clara-api/src/handlers/deduce_handler.rs` | `resolve_prolog_source()` |
| Resolve clips_source_id | `clara-api/src/handlers/deduce_handler.rs` | `resolve_clips_source()` |

### Cycle steps 1–7

| Step | Topic | File : line | Function / symbol |
|---|---|---|---|
| **1** | Prolog pass (dispatcher) | `clara-cycle/src/controller.rs:125` | `CycleController::prolog_pass()` |
| 1a | Consume Coire events — Rust side | `clara-prolog/src/backend/ffi/environment.rs:209` | `PrologEnvironment::consume_coire_events()` |
| 1a | Dispatch events — Prolog side | `clara-prolog/prolog-lib/the_coire.pl:32` | `coire_consume/0` |
| 1a | Publish events from Prolog rules | `clara-prolog/prolog-lib/the_coire.pl:21` | `coire_publish/2` (+ `_assert/1`, `_retract/1`, `_goal/1`) |
| 1b | Execute goal — cycle 0, all solutions + bindings | `clara-prolog/src/backend/ffi/environment.rs:249` | `PrologEnvironment::query_with_bindings()` |
| 1b | Execute goal — cycle 1+, engine tick | `clara-prolog/src/backend/ffi/environment.rs:236` | `PrologEnvironment::query_once("true")` |
| **2** | Relay Prolog → CLIPS | `clara-cycle/src/relay.rs:13` | `relay_prolog_to_clips()` |
| 2t | [trace] Record tableau after relay | `clara-cycle/src/controller.rs` | `CycleController::record_tableau()` |
| **3** | Evaluator pass (stub) | `clara-cycle/src/controller.rs:136` | `CycleController::evaluator_pass()` |
| **4** | CLIPS pass (dispatcher) | `clara-cycle/src/controller.rs:142` | `CycleController::clips_pass()` |
| 4a | Consume Coire events — Rust side | `clara-clips/src/backend/ffi/environment.rs:183` | `ClipsEnvironment::consume_coire_events()` |
| 4a | Event type dispatch (`assert` / `goal` / other) | `clara-clips/src/backend/ffi/environment.rs:204` | `match ev_type.as_str()` in `consume_coire_events` |
| 4a | `(coire-event …)` template definition | `clara-clips/clp-lib/the_coire.clp:37` | `(deftemplate coire-event ...)` |
| 4b | Run inference engine to saturation | `clara-clips/src/backend/ffi/environment.rs:87` | `ClipsEnvironment::eval()` called with `"(run)"` |
| **5** | Relay CLIPS → Prolog | `clara-cycle/src/relay.rs:34` | `relay_clips_to_prolog()` |
| 5t | [trace] Record tableau after relay | `clara-cycle/src/controller.rs` | `CycleController::record_tableau()` |
| **6** | Convergence check | `clara-cycle/src/controller.rs:169` | `CycleController::has_converged()` |
| 6a | Per-cycle snapshot (conditions 1 & 2) | `clara-cycle/src/controller.rs:159` | `CycleController::snapshot()` |
| 6a | Snapshot type | `clara-cycle/src/result.rs:34` | `CoireSnapshot` |
| 6b | CLIPS agenda empty check (condition 3) | `clara-cycle/src/controller.rs:172` | `clips.eval("(= (length$ (get-agenda)) 0)")` |
| 6c | Snapshot stability check (condition 4) | `clara-cycle/src/controller.rs:178` | `prev == curr` in `has_converged` |
| 6d | Tableau fixed-point check (condition 5) | `clara-dagda/src/lib.rs` | `Dagda::tableau_changed_since()` |
| **7** | Interrupt check | `clara-cycle/src/controller.rs:93` | `interrupt.load(Ordering::SeqCst)` in `run` |
| 7 | Set interrupt flag via HTTP | `clara-api/src/handlers/deduce_handler.rs:116` | `interrupt_deduce()` |

### Supporting types and errors

| Symbol | File : line | Notes |
|---|---|---|
| `CycleStatus` | `clara-cycle/src/result.rs:6` | `Running \| Converged \| Interrupted \| Error(String)` |
| `DeductionResult` | `clara-cycle/src/result.rs:25` | Returned by `run()`; stored in `DeductionEntry.result` |
| `DeductionResult.prolog_solutions` | `clara-cycle/src/result.rs` | `Option<serde_json::Value>` — all cycle-0 solutions with variable bindings |
| `DeductionResult.goal_bindings` | `clara-cycle/src/result.rs` | `Option<String>` — human-readable binding string from cycle-0 goal |
| `DeductionResult.tableau` | `clara-cycle/src/result.rs` | `Vec<PredicateEntry>` — final Dagda tableau state |
| `DeductionResult.trace` | `clara-cycle/src/result.rs` | `Option<Vec<InMemoryTraceEntry>>` — per-phase snapshots when tracing without a store |
| `InMemoryTraceEntry` | `clara-cycle/src/result.rs` | `cycle_num`, `phase`, `recorded_at_ms`, `entries: Vec<PredicateEntry>` |
| `CoireSnapshot` | `clara-cycle/src/result.rs:43` | `prolog_pending` + `clips_pending` counts |
| `CycleError` | `clara-cycle/src/error.rs:4` | All fatal error variants with `thiserror` Display strings |
| `DeduceRequest.trace` | `clara-api/src/models/request.rs` | `bool` — enables per-phase tableau recording |
| `DeduceRequest.prolog_source_id` | `clara-api/src/models/request.rs` | `Option<Uuid>` — registered source supersedes `prolog_clauses` |
| `DeduceRequest.clips_source_id` | `clara-api/src/models/request.rs` | `Option<Uuid>` — registered source supersedes `clips_file` + `clips_constructs` |
| `DeductionSnapshot.prolog_source_id` | `clara-coire/src/store.rs` | FK into `source_registry` — enables DOT generation on trace |
| `DeductionSnapshot.clips_source_id` | `clara-coire/src/store.rs` | FK into `source_registry` |
| `DeductionSnapshot.dot_artifact_id` | `clara-coire/src/store.rs` | FK into `source_artifacts` for the cached base DOT |
| `DeductionSession` | `clara-cycle/src/session.rs:12` | Holds `PrologEnvironment` + `ClipsEnvironment` + `Dagda` tableau + their UUIDs |
| `CycleController` | `clara-cycle/src/controller.rs:14` | Owns the session; drives the loop |
| `DeductionEntry` | `clara-api/src/handlers/session_handler.rs:16` | In-flight record stored in `AppState::deductions` |

### HTTP handler wiring

| Endpoint | File | Handler |
|---|---|---|
| `POST /deduce` | `clara-api/src/handlers/deduce_handler.rs` | `start_deduce()` |
| `GET /deduce/{id}` | `clara-api/src/handlers/deduce_handler.rs` | `poll_deduce()` |
| `DELETE /deduce/{id}` | `clara-api/src/handlers/deduce_handler.rs` | `interrupt_deduce()` |
| `POST /deduce/resume` | `clara-api/src/handlers/deduce_handler.rs` | `resume_deduce()` |
| `GET /deduce/{id}/snapshot` | `clara-api/src/handlers/deduce_handler.rs` | `get_snapshot()` |
| `DELETE /deduce/{id}/snapshot` | `clara-api/src/handlers/deduce_handler.rs` | `delete_snapshot()` |
| `GET /deduce/{id}/trace` | `clara-api/src/handlers/trace_handler.rs` | `list_trace()` |
| `GET /deduce/{id}/trace/{change_id}/dot` | `clara-api/src/handlers/trace_handler.rs` | `trace_dot()` |
| `GET /deduce/{id}/trace/{change_id}/entries` | `clara-api/src/handlers/trace_handler.rs` | `trace_entries()` |
| `GET /cycle/coire/snapshot` | `clara-api/src/handlers/coire_handler.rs` | `snapshot()` |
| `POST /cycle/coire/push` | `clara-api/src/handlers/coire_handler.rs` | `push()` |
| `POST /ritual` | `clara-api/src/handlers/ritual_handler.rs` | `create_ritual()` |
| `GET /ritual/{id}/join` | `clara-api/src/handlers/ritual_handler.rs` | `join_ritual()` |
| `GET /ritual/{id}/status` | `clara-api/src/handlers/ritual_handler.rs` | `ritual_status()` |
| `DELETE /ritual/{id}` | `clara-api/src/handlers/ritual_handler.rs` | `terminate_ritual()` |
| Route registration | `clara-api/src/routes/mod.rs` | `configure()` |

### Ritual (evaluator pass integration)

| Symbol / Topic | File | Notes |
|---|---|---|
| `CycleController::with_ritual()` | `clara-cycle/src/controller.rs` | Attaches a `RitualHandle`; compiled only with `ritual` feature |
| `evaluator_pass_ritual()` | `clara-cycle/src/controller.rs` | Drains outbound Offerings + ingests inbound Tephras |
| `ingest_tephra()` | `clara-cycle/src/controller.rs` | Writes a Tephra payload as `ritual/<label>` Coire event into Prolog mailbox |
| `publish_evaluator_events()` | `clara-cycle/src/controller.rs` | Publishes evaluator-origin Coire events as `offering` Tephras |
| `assert_evaluator_timeout_tabu()` | `clara-cycle/src/controller.rs` | Synthesises a `ritual/tabu-timeout` Coire event after patience timeout |
| `RitualRegistry` | `clara-ritual/src/registry.rs` | Server-level registry; owns broker; lives in `AppState` |
| `RitualHandle` | `clara-ritual/src/handle.rs` | Lightweight per-performance handle for publish/poll; cheap to clone |
| `TephraEnvelope` | `clara-ritual/src/envelope.rs` | Kafka message wrapper: `tephra_id`, `ritual_id`, `performance_id`, `label`, `ttl_ms`, `payload` |
| `TephraPayload` | `clara-ritual/src/envelope.rs` | `Plaintext { body }` or `Encrypted { … }` (Phase 7 stub) |
| `RitualConfig` | `clara-ritual/src/envelope.rs` | `{ name, participants }` — POST /ritual request body |
| `label::*` | `clara-ritual/src/envelope.rs` | Well-known label constants: `OFFERING`, `HOHI`, `TABU`, `PROLOG_FACT`, `CLIPS_FIRE`, `CLARA_FY_HIT`, `DEDUCTION_EVENT` |
| `KafkaBridge` | `clara-ritual/src/broker.rs` | Trait: `ensure_topic`, `publish`, `poll`, `latest_offset` |
| `InMemoryBroker` | `clara-ritual/src/broker.rs` | In-process broker for tests and dev (no Kafka required) |
| `topic_name()` | `clara-ritual/src/topic.rs` | Derives Kafka topic from `dis_domain` + `ritual_id` |
| `AppState::ritual_registry` | `clara-api/src/handlers/session_handler.rs` | `Arc<RitualRegistry>` — shared across all handlers |
| `AppState::dis_domain` | `clara-api/src/handlers/session_handler.rs` | Passed to `topic_name()` and forwarded to FieryPit participants at bootstrap |
| `AppState::kafka_bootstrap` | `clara-api/src/handlers/session_handler.rs` | Kafka broker address forwarded to FieryPit at bootstrap; `None` in dev/test |
| `DeduceRequest::ritual_id` | `clara-api/src/models/request.rs` | `Option<Uuid>` — join ritual for evaluator pass |

### Prolog library predicates (context-related)

| Predicate | File | Notes |
|---|---|---|
| `current_context/1` | `clara-prolog/prolog-lib/the_rabbit.pl` | Parses `deduce_context_json/1` to a list of dicts; returns `[]` as fallback |
| `ponder_text_with_context/3` | `clara-prolog/prolog-lib/the_rabbit.pl` | Adds `context` field to the FieryPit `/evaluate` JSON payload |
| `descriminate_k_with_context/4` | `clara-prolog/prolog-lib/the_rabbit.pl` | Context-aware LLM + fastText classify pipeline |
| `clara_fy/3` | `clara-prolog/prolog-lib/the_rat.pl` | `(+Text, +Context, -TruthValue)` — top-level context-aware classification |
| `top_status_with_context/3` | `clara-prolog/prolog-lib/the_rat.pl` | Returns top-1 truth atom for Text given Context |
| `extract_top_k_labels_with_context/4` | `clara-prolog/prolog-lib/the_rat.pl` | Returns top-K simplified labels for Text given Context |
