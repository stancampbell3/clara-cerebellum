# Rituals 101

## Audience and purpose

This document is written for lildaemon developers who are responsible for **evaluator definition** and for coordinating the FieryPit side of Ritual integration with the Dagda / Cobbler team.

The goal is to give you a working mental model of what a Ritual is, how the three systems (Dis, FieryPit, and Kafka) interact, and what you as a lildaemon developer are responsible for — grounded in the current end-to-end smoke test.

The final section previews the **next joint project**: Ritual CRUD stored in `lildaemon.duc` associated with users, and the planned Ritual editor in Cobbler.

---

## What is a Ritual?

A **Ritual** is a named Kafka-backed coordination channel that connects a running
deduction cycle in **Dis** (clara-api) to one or more **FieryPit** evaluator
instances in lildaemon.

Without a Ritual, a deduction cycle is purely symbolic: Prolog and CLIPS pass
facts back and forth until they converge. With a Ritual attached, the cycle's
evaluator pass gains a live channel to neural / LLM reasoning: Prolog rules can
publish evaluation requests, pause until a peer evaluator responds, and then
incorporate the response as a new fact before continuing.

```
                         ┌────────────────────────────────────────┐
  POST /deduce           │           Deduction cycle (Dis)        │
  + ritual_id  ─────────►│                                        │
                         │  Prolog pass → relay → evaluator pass  │
                         │                  │            ▲        │
                         └──────────────────┼────────────┼────────┘
                                            │ Offering   │ Hohi/Tabu
                                            ▼            │
                                   ┌────────────────────────────┐
                                   │   Kafka topic              │
                                   │   dis.local.ritual.<uuid>  │
                                   └────────────────────────────┘
                                            │ Offering   ▲
                                            ▼            │ Hohi/Tabu
                         ┌────────────────────────────────────────┐
                         │          FieryPit (lildaemon)          │
                         │   RitualParticipant._run_loop()        │
                         │   consume Offering → evaluate → respond│
                         └────────────────────────────────────────┘
```

Three systems are involved:

| System | Role | Code |
|--------|------|------|
| **Dis** (clara-api) | Creates and terminates Rituals; drives the deduction cycle; publishes Offerings; consumes Hohi/Tabu | `clara-ritual/`, `clara-cycle/src/controller.rs` |
| **Kafka** | Single shared topic per Ritual; both sides read and write the same topic | `dis.local.ritual.<ritual_id>` |
| **FieryPit** (lildaemon) | Consumes Offerings; evaluates them using its wrangler; publishes Hohi or Tabu | `goat/models/RitualParticipant.py` |

---

## The message format: TephraEnvelope

Every message on the Ritual Kafka topic is a JSON-serialized `TephraEnvelope`:

```json
{
  "tephra_id":      "550e8400-e29b-41d4-a716-446655440000",
  "ritual_id":      "a2b3c4d5-e6f7-4890-ab12-cd3456789012",
  "performance_id": "f1e2d3c4-b5a6-4789-0123-456789abcdef",
  "label":          "offering",
  "ts_ms":          1744286400000,
  "ttl_ms":         60000,
  "producer_node":  "dis.local",
  "payload": {
    "type": "plaintext",
    "body": { "goal": "is the visitor confused?", "context": [] }
  }
}
```

| Field | Meaning |
|---|---|
| `tephra_id` | Unique envelope ID. lildaemon tracks these for deduplication. |
| `ritual_id` | Links the message to a Ritual. |
| `performance_id` | The deduction run's identity within the Ritual. Dis stamps its own; lildaemon copies it onto the response. |
| `label` | Message type — see table below. |
| `ts_ms` | Creation timestamp (Unix ms). |
| `ttl_ms` | Time-to-live in ms. Messages older than `ttl_ms` are silently dropped by consumers. |
| `producer_node` | The node that created this envelope. Used for **echo suppression** — see the critical note below. |
| `payload` | The actual content. `plaintext` for all Phase 1–6 work. |

### Well-known labels

| Label | Direction | Meaning |
|---|---|---|
| `offering` | Dis → FieryPit | An evaluation request. FieryPit should evaluate this and respond. |
| `hohi` | FieryPit → Dis | Successful evaluation response. Decrements Dis's pending counter. |
| `tabu` | FieryPit → Dis | Error response (timeout, exception). Also decrements the counter. |
| `prolog_fact` | FieryPit → Dis | Peer asserts a Prolog fact directly. |
| `clips_fire` | FieryPit → Dis | Peer triggers a CLIPS rule by name. |
| `clara_fy_hit` | FieryPit → Dis | Peer reports a classification result. |
| `deduction_event` | either direction | Generic structured event for rule consumption. |

lildaemon only needs to care about `offering` (consume it) and `hohi`/`tabu` (produce them). The others are available for more advanced rule-driven coordination.

---

## Step-by-step: how a Ritual runs

### 1. Create the Ritual (Dis)

Someone calls `POST /ritual` on Dis:

```bash
curl -s -X POST http://localhost:8080/ritual \
  -H 'Content-Type: application/json' \
  -d '{"name": "smoke-test", "participants": []}'
```

Dis creates a `ritual_id`, derives a Kafka topic name
(`dis.local.ritual.<ritual_id>`), and ensures the topic exists in the broker.
The `participants` list can contain FieryPit base URLs for automatic bootstrap;
an empty list skips that step (and avoids a current auth limitation — see
notes below).

**Response:**
```json
{ "ritual_id": "a2b3c4d5-e6f7-4890-ab12-cd3456789012" }
```

### 2. FieryPit joins the Ritual

FieryPit calls `GET /ritual/{id}/join` on Dis to obtain routing information:

```bash
curl -s "http://localhost:8080/ritual/$RITUAL_ID/join?participant=http://fiery-pit-1:6666"
```

The `?participant=` key makes the join idempotent — the same key always gets
the same `performance_id`. This lets FieryPit reconnect after a restart without
orphaning its prior identity on the topic. Omitting the key generates a fresh
`performance_id` on every call.

**Response:**
```json
{
  "ritual_id":      "a2b3c4d5-...",
  "performance_id": "f1e2d3c4-...",
  "topic":          "dis.local.ritual.a2b3c4d5",
  "dis_domain":     "dis.local"
}
```

FieryPit then calls its own `POST /ritual/join` to register with the
`RitualManager` and start the Kafka consumer:

```python
# lildaemon: goat/app/ritual/router.py
POST /ritual/join
{
  "ritual_id":        "a2b3c4d5-...",
  "topic":            "dis.local.ritual.a2b3c4d5",
  "bootstrap_servers": "localhost:9092",
  "dis_domain":        "fierypit.local",   # ← THIS node's identity, not Dis's
  "evaluator":         null,
  "session_stateful":  false,
  "eval_timeout_s":    30.0
}
```

The `RitualManager` creates a `RitualParticipant` and starts its background
`_run_loop()`. The consumer group ID is
`ritual-{ritual_id}-{dis_domain}`, which means restarting with the same
`dis_domain` resumes from the last committed Kafka offset — no message replay.

### 3. Deduction runs with `ritual_id`

The deduction caller supplies `ritual_id` in `POST /deduce`:

```bash
curl -s -X POST http://localhost:8080/deduce \
  -H 'Content-Type: application/json' \
  -d "{
    \"prolog_clauses\": [
      \"peer_query(hello) :- coire_publish(evaluator/offering, json([text=hello])).\",
      \"peer_answered(Input, Answer) :- coire_poll(ritual/hohi, Env),\",
      \"    get_dict(result, Env, Answer).\"
    ],
    \"initial_goal\": \"peer_query(hello)\",
    \"ritual_id\":    \"$RITUAL_ID\"
  }"
```

The `CycleController` joins the Ritual anonymously (fresh `performance_id` per
deduction run) and runs the evaluator pass on every cycle:

1. **Drain outbound** — Coire events with origin prefix `evaluator/` are
   published to Kafka as `offering` Tephras. Each one increments an internal
   `pending_evaluator_responses` counter.
2. **Poll inbound** — New Tephras are fetched from the Kafka topic, filtered
   for freshness (`ttl_ms`), and injected into Prolog's Coire mailbox as
   `ritual/<label>` events. A `hohi` or `tabu` decrements the counter.

The cycle will **not converge** until `pending_evaluator_responses == 0`.

### 4. lildaemon evaluates the Offering

`RitualParticipant._run_loop()` polls the Kafka topic and processes each message:

```
poll topic
  → decode TephraEnvelope
  → skip if label != "offering"
  → skip if producer_node == self.dis_domain  (echo suppression)
  → skip if tephra_id already seen            (dedup)
  → _evaluate_with_timeout(offering_payload)
      ├─ success → wrap in TephraEnvelope, label="hohi"
      └─ timeout/error → wrap in TephraEnvelope, label="tabu"
  → produce back to same Kafka topic
```

The response Tephra copies `ritual_id` and `performance_id` from the incoming
Offering and sets `producer_node = self.dis_domain` (FieryPit's own identity,
e.g. `"fierypit.local"`). The TTL on responses is 5 minutes (300 000 ms).

### 5. Dis consumes the Hohi and converges

`CycleController.ingest_tephra()` picks up the Hohi, writes a
`ritual/hohi` Coire event into Prolog's mailbox, and decrements
`pending_evaluator_responses`. On the next cycle Prolog's `coire_poll/2`
unifies with it and the derivation proceeds.

Once all six convergence conditions hold — including
`pending_evaluator_responses == 0` — the cycle exits with status `converged`.

### 6. Terminate the Ritual

```bash
curl -s -X DELETE http://localhost:8080/ritual/$RITUAL_ID
```

Marks the Ritual `terminated`. Existing handles in running deductions continue
functioning until the Kafka topic is cleaned up. New `join` calls return
`409 Conflict`.

---

## The smoke test, end to end

The current smoke test at `scripts/smoke_test.sh` exercises the Ritual
lifecycle on the Dis side. It does **not** require a live lildaemon or Kafka —
it uses Dis's `InMemoryBroker` (the default in dev/test mode) so all messages
stay in-process.

```bash
#!/bin/bash
set -e

# 1. Create a Ritual (no participants — skip auto-bootstrap)
CREATE_RESPONSE=$(curl -s -X POST http://localhost:8080/ritual \
  -H 'Content-Type: application/json' \
  -d '{"name":"smoke-test","participants":[]}')
RITUAL_ID=$(echo "$CREATE_RESPONSE" | jq -r '.ritual_id')

# 2. Join with a stable participant key
JOIN_1=$(curl -s "http://localhost:8080/ritual/$RITUAL_ID/join?participant=http://fiery-pit-1:8080")
JOIN_2=$(curl -s "http://localhost:8080/ritual/$RITUAL_ID/join?participant=http://fiery-pit-1:8080")

# Idempotency check — same key must return same performance_id
PID_1=$(echo "$JOIN_1" | jq -r '.performance_id')
PID_2=$(echo "$JOIN_2" | jq -r '.performance_id')
[ "$PID_1" = "$PID_2" ] || { echo "ERROR: performance_id changed"; exit 1; }

# 3. Check status
curl -s http://localhost:8080/ritual/$RITUAL_ID/status | jq .

# 4. Terminate
curl -s -X DELETE http://localhost:8080/ritual/$RITUAL_ID | jq .

# 5. Confirm join is rejected after termination (expect 409)
HTTP=$(curl -s -o /dev/null -w "%{http_code}" \
  "http://localhost:8080/ritual/$RITUAL_ID/join")
[ "$HTTP" = "409" ] || { echo "ERROR: Expected 409, got $HTTP"; exit 1; }

echo "Smoke test passed."
```

The Phase 6 e2e test (documented in `lildaemon/docs/ritual_e2e_smoke_test_findings.md`)
extends this to a full Dis + real lildaemon Kafka round-trip using a CLIPS
chanter rule. That test verified the complete Offering → Hohi path and
uncovered four integration bugs that are now fixed (see notes below).

---

## Critical implementation notes

### The `dis_domain` identity trap

**The single most common integration mistake.** The `dis_domain` parameter
passed to `POST /ritual/join` on lildaemon is **this node's own identity**, not
Dis's domain. It becomes the `producer_node` stamp on every Hohi/Tabu response.

```
# WRONG — lildaemon will suppress all Offerings (echo suppression fires)
"dis_domain": "dis.local"

# CORRECT — use FieryPit's own node identity
"dis_domain": "fierypit.local"
```

Echo suppression works by checking `producer_node == self.dis_domain`. If you
pass Dis's domain, lildaemon thinks Dis is itself and drops every Offering it
receives.

### Auto-bootstrap: token provisioning

When you pass `"participants": ["http://fiery-pit-1:6666"]` to `POST /ritual`,
Dis calls `POST /ritual/join` on lildaemon via `FieryPitClient`. lildaemon's
`/ritual/join` requires a valid Bearer JWT. Dis handles this automatically
through **lazy token acquisition** — no manual JWT copying required.

#### Normal setup (automated)

Set the same `LILDAEMON_SERVICE_SECRET` value on both services:

```bash
# docker/.env (same secret on both sides)
LILDAEMON_SERVICE_SECRET=change-me-to-a-strong-shared-secret
```

At bootstrap time Dis calls `POST /auth/service-token` on the participant URL
using this secret, caches the returned JWT, and attaches it to every
`POST /ritual/join`. On a `401` response (e.g. stale cached token), Dis clears
the cache and re-acquires a fresh token before retrying once. The service
account (`dis-bootstrap`) is upserted on first call — no prior registration
required.

**Token lifetime**: 30 days by default (configurable via
`LILDAEMON_SERVICE_TOKEN_TTL_DAYS`). Dis re-acquires automatically whenever
the cached token is absent or has expired, so there is no rotation burden.

#### Static override (operator-managed)

If `FIERYPIT_SERVICE_KEY` is set in Dis's environment, it is used as-is and
lazy acquisition is skipped. This is useful for tightly controlled production
deployments where a human operator manages the token lifecycle:

```bash
# Dis side: set FIERYPIT_SERVICE_KEY to a pre-issued JWT
export FIERYPIT_SERVICE_KEY="<long-lived JWT from lildaemon>"

# lildaemon side: issue the JWT once
curl -s -X POST http://localhost:6666/auth/service-token \
  -H "Content-Type: application/json" \
  -d '{"service_name":"dis-bootstrap","service_secret":"change-me-in-prod"}' \
  | jq -r .access_token
```

#### Fallback when no credentials are configured

If neither `LILDAEMON_SERVICE_SECRET` nor `FIERYPIT_SERVICE_KEY` is set, Dis
attempts unauthenticated bootstrap calls, which `401` immediately. The Ritual
is still created; participants can join manually via
`GET /ritual/{id}/join?participant=<url>` and their own `POST /ritual/join`.
Pass `"participants": []` to skip bootstrap entirely and avoid the logged
warnings.

### Consumer group semantics

lildaemon's Kafka consumer group is named `ritual-{ritual_id}-{dis_domain}`.

- **Restart resilience** — same group ID on restart means Kafka resumes from
  the last committed offset; no replays.
- **Horizontal scaling** — multiple lildaemon instances sharing the same
  `dis_domain` will load-balance partitions automatically.
- **Isolation** — different `dis_domain` values get independent consumer groups
  and will each receive all messages. This is the intended model for multiple
  independent evaluator nodes joining the same Ritual.

### CLIPS rule seeding requires `(reset)` — and `(reset)` breaks `defglobal`

Discovered in the Phase 6 e2e test: if you seed a CLIPS environment with
`deffacts`, you **must** call `(reset)` to instantiate them as working-memory
facts before the first `(run)`. However, `(reset)` wipes `defglobal` values,
which breaks `coire-emit` (which stores session UUIDs in globals). The fix is to
re-bind session UUIDs after every `(reset)`. The `CycleController` now handles
this internally.

### Patience timeout

If lildaemon is slow or offline, the cycle does not hang indefinitely. After
`evaluator_patience_cycles` consecutive cycles (default 10) with no inbound
Hohi or Tabu, the `CycleController` injects a synthetic
`ritual/tabu-timeout` Coire event into Prolog's mailbox and clears the counter
so the cycle can continue. Prolog rules can pattern-match on this event to
implement graceful degradation.

---

## What lildaemon owns

As the lildaemon team, your surface area in the Ritual system is:

| Responsibility | Code |
|---|---|
| **Joining a Ritual** and starting the Kafka consumer | `goat/app/ritual/router.py`, `RitualManager.join()` |
| **Evaluating Offerings** via the wrangler | `RitualParticipant._handle_message()`, `_evaluate_with_timeout()` |
| **Publishing Hohi/Tabu** responses | `RitualParticipant._publish_tephra()` |
| **Leaving a Ritual** and stopping the consumer | `goat/app/ritual/router.py` DELETE, `RitualManager.leave()` |
| **Listing active Rituals** | `GET /ritual` on lildaemon |

Dis owns the Ritual lifecycle (create, terminate, topic provisioning). Kafka is
infrastructure neither side owns. You do not need to manage topic creation —
Dis does that via `broker.ensure_topic()` before any participant joins.

---

## Quick reference: Ritual lifecycle API

### Dis (clara-api, port 8080)

| Method | Path | Description |
|---|---|---|
| `GET` | `/ritual` | List all active Rituals (`ritual_id`, `name`, `state`, `topic`) |
| `POST` | `/ritual` | Create Ritual, provision Kafka topic |
| `GET` | `/ritual/{id}/join` | Get routing info; idempotent with `?participant=` key |
| `GET` | `/ritual/{id}/status` | Check state (`active` / `terminated`) |
| `DELETE` | `/ritual/{id}` | Terminate Ritual |
| `POST` | `/deduce` | Start deduction with `"ritual_id"` to attach |
| `GET` | `/cycle/coire/sessions` | Per-session event counts for all sessions with events (including failed deductions) |
| `GET` | `/cycle/coire/snapshot` | Per-session pending counts for sessions tied to completed deductions |
| `POST` | `/cycle/coire/push` | Inject a synthetic Coire event for testing |

### lildaemon (FieryPit, port 6666)

| Method | Path | Description |
|---|---|---|
| `POST` | `/ritual/join` | Register and start Kafka consumer |
| `DELETE` | `/ritual/{id}` | Stop consumer and leave Ritual |
| `GET` | `/ritual` | List active Ritual IDs |

---

## Observability: `coire-watch`

`coire-watch` is a standalone CLI tool in `dagda/scripts/coire_watch.py` for
monitoring Ritual Kafka traffic in real time. It subscribes to one or more
Ritual topics, decodes `TephraEnvelope` messages, and prints them as they
arrive. Useful for debugging Ritual execution, verifying that Offerings are
flowing, and confirming that lildaemon is publishing Hohi/Tabu responses.

```bash
# Watch all active Rituals (uses GET /ritual for discovery)
python scripts/coire_watch.py --all

# Watch one Ritual, filter to errors only
python scripts/coire_watch.py --ritual <uuid> --label tabu

# Replay from offset 0 and include Coire session stats sidebar
python scripts/coire_watch.py --all --from-beginning --coire

# Write a JSONL log for post-mortem analysis
python scripts/coire_watch.py --all --format jsonl --log /tmp/ritual.jsonl
```

Topic discovery modes:

| Mode | Flag | How it works |
|---|---|---|
| Direct | `TOPIC` positional arg | Topic name supplied literally |
| By ritual ID | `--ritual UUID` | Calls `GET /ritual/{id}/join` on Dis to resolve the topic |
| Auto-discover | `--all` | Calls `GET /ritual` on Dis and subscribes to all active topics |

The `--coire` flag polls `GET /cycle/coire/sessions` every N seconds (default 5)
and prints a sidebar showing per-session pending/processed/drained counts. This
is particularly useful for spotting sessions from failed deductions that would
not appear in the older `/cycle/coire/snapshot` endpoint.

See `dagda/docs/coire_watch_design.md` for the full design and decision record.

---

## What's next: Ritual CRUD in lildaemon.duc

The next joint project is **user-associated Ritual persistence** on the
lildaemon side.

Today, Rituals are ephemeral: they live in the `RitualManager`'s in-memory
HashMap and are lost on restart. The plan is to persist Ritual definitions in
`lildaemon.duc` (the DuckDB database), associated with users, so that:

- A user can define a named Ritual configuration (evaluator, timeout,
  Kafka coordinates, participant URLs) and save it as a draft.
- The definition survives lildaemon restarts and can be listed, edited,
  and deleted via API.
- Cobbler (the Dagda browser front end) will expose a **Ritual editor page**
  separate from the REPL page — for configuring participants, connecting
  Offering input/output message paths, and performing Ritual CRUD.

The planned data flow for the editor is:

```
Cobbler (Dagda UI — editor page)
    └─► FieryPit (lildaemon)     POST /ritual-configs
            └─► Dis (clara-api)  POST /ritual (provision topic, on activation only)
```

### Identity: `ritual_config_id` vs. `ritual_id`

FieryPit is the facade for Dis. Clients (Cobbler, scripts) never call Dis
directly. This means two distinct identifiers exist:

- **`ritual_config_id`** — FieryPit's own UUID, assigned when the config is
  created. Always present; persists across the full lifecycle.
- **`ritual_id`** — Dis's UUID, returned by `POST /ritual`. Only present once
  the config has been **activated** (submitted to Dis). Null in draft state.

A ritual config can exist indefinitely without ever being activated. The
`ritual_id` is populated when FieryPit calls Dis on the user's behalf and
stores the result.

### Schema

```
ritual_configs (lildaemon.duc)
  ritual_config_id  UUID     PK
  user_id           TEXT     FK → users
  name              TEXT
  status            TEXT     CHECK status IN ('draft','active','terminated')
  ritual_id         UUID     nullable — Dis's ID, populated on activation
  evaluator         TEXT     nullable
  eval_timeout_s    REAL     default 30.0
  kafka_bootstrap   TEXT
  dis_url           TEXT
  created_at        INTEGER  (ms)
  updated_at        INTEGER  (ms)

ritual_participants (lildaemon.duc)
  participant_id    UUID     PK
  ritual_config_id  UUID     FK → ritual_configs
  url               TEXT     — FieryPit instance base URL
  role              TEXT     nullable
```

### Lifecycle states

```
draft ──(activate)──► active ──(terminate)──► terminated
  ▲                                               │
  └──────────────(re-draft / edit)────────────────┘ (optional)
```

- **draft** — Config saved; no Kafka topic, no Dis record.
- **active** — FieryPit has called `POST /ritual` on Dis; `ritual_id` populated;
  Kafka topic provisioned; auto-bootstrap attempted for all participants.
- **terminated** — FieryPit has called `DELETE /ritual/{id}` on Dis. Config
  record is retained for history; `ritual_id` kept for reference.

### JWT auth for service-to-service calls (FieryPitClient)

The auto-bootstrap path and all lildaemon → Dis calls require authentication.
Options ranked by implementation cost:

**Option A — Pre-shared service API key (recommended for Phase 1)**
A long-lived API key is configured in FieryPit's environment (`DIS_SERVICE_KEY`).
`FieryPitClient` attaches it as `Authorization: Bearer <key>`. Dis validates
against a static list in its own config. Simple, no moving parts, easy to rotate.

**Option B — Signed service JWT (recommended for Phase 2)**
FieryPit holds an RS256 private key; Dis holds the matching public key. On each
call FieryPit mints a short-lived JWT (`iss=fierypit`, `aud=dis`, `exp=+5min`).
No shared secret storage; each side is independently rotatable. Suitable for
multi-instance deployments.

**Option C — User-token forwarding**
When the Cobbler editor triggers an activation, it passes the logged-in user's
JWT down through FieryPit to Dis. No service credential needed, but every
automated/background call (e.g. restart re-join) would have no token. Not
sufficient alone; suitable as a complement to A or B.

**Option D — Mutual TLS**
Both services present client certificates; no Bearer token needed. High
operational overhead; suitable only if the deployment already mandates mTLS.

**Decision for now:** Implement Option A for Phase 1 unblocking. Design Option B
interfaces so A can be replaced without API changes.

### Cobbler: two distinct pages

| Page | Purpose |
|---|---|
| **REPL page** (existing) | Interact with and test individual evaluators; send Offerings manually; observe Hohi/Tabu responses live |
| **Ritual editor page** (new) | Create/edit Ritual configs; manage participants; connect Offering input/output message paths; activate / terminate; view status |

The editor page calls FieryPit's `/ritual-configs` REST API through Cobbler's
existing proxy at port 5001. No direct Dis calls from the browser.

---

## Further reading

- `docs/deduce_endpoint.md` — full Ritual integration in the deduction cycle,
  convergence conditions, and source code index
- `docs/demonic_voice_api.md` — all Ritual and deduce HTTP endpoints on Dis
- `lildaemon/docs/ritual_e2e_smoke_test_findings.md` — Phase 6 e2e test findings
  and the four bugs caught during integration testing
- `lildaemon/docs/ritual_implementation_plan.md` — original Ritual implementation plan
- `dagda/docs/coire_watch_design.md` — design and decision record for the `coire-watch` monitoring CLI
- `dagda/scripts/coire_watch.py` — the monitoring tool itself (`--help` for usage)
- `clara-ritual/src/envelope.rs` — `TephraEnvelope`, `TephraPayload`, label constants
- `clara-ritual/src/broker.rs` — `KafkaBridge` trait, `InMemoryBroker`, `RsKafkaClient`
- `clara-ritual/src/registry.rs` — `RitualRegistry`, `RitualSummary`, `list_active()`
- `clara-coire/src/coire.rs` — `Coire`, `SessionSummary`, `list_sessions()`
- `clara-cycle/src/controller.rs` — `evaluator_pass_ritual()`,
  `ingest_tephra()`, `publish_evaluator_events()` (search `#[cfg(feature = "ritual")]`)
