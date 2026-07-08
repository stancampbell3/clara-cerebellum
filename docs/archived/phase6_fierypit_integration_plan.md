# Phase 6 — FieryPit Integration: Design Review

_Branch: `housbonde_lif` · Created: 2026-04-15 · Status: **Awaiting design decision**_

---

## GoatWrangler Status: What Exists vs. What Phase 6 Needs

### What exists in lildaemon

The GoatWrangler is a mature multi-evaluator orchestration layer. Relevant existing
primitives for Phase 6:

| Component | Relevance to Phase 6 |
|---|---|
| `POST /evaluate` | Synchronous evaluation — the call Phase 6 will ultimately route Offerings through |
| `GoatWrangler.eval(offering)` | The internal Python path that `POST /evaluate` drives |
| `Tephra / Hohi / Offering` | Data structures already defined in `FieryPit.py` — vocabulary matches Dis's |
| `POST /evaluators/set` | Lets Dis configure which evaluator handles a ritual's evaluations |
| `EvaluationRegistry` | Async task tracking — useful for non-blocking evaluation |

### What does NOT exist

- **No Kafka** — zero Kafka dependency in lildaemon. All evaluation is sync HTTP or
  asyncio WebSocket.
- **No ritual endpoints** — `/ritual/*` does not exist at all.
- **lildaemon has its own "Ritual" concept** — but it is unrelated: a `BleatSession`
  resume/replay wrapper (`goat/models/Ritual.py`, planned but not yet implemented).
  The name collision needs to be managed carefully. Suggest calling the lildaemon concept
  "Ceremony" to avoid confusion with the Dis Kafka-coordination Ritual.

---

## The Key Design Question

Before writing any code there is a fundamental architecture decision to make.

---

### Option A — Dis bridges Kafka ↔ lildaemon (REST-only on lildaemon side)

```
Kafka topic
    ↑↓  Dis polls / publishes via RsKafkaClient
Dis CycleController (evaluator_pass_ritual)
    ↓  POST /ritual/{id}/offering   — Dis pushes Offering to lildaemon
    ↑  GET  /ritual/{id}/result     — Dis polls lildaemon for Hohi
lildaemon GoatWrangler
    → evaluator.evaluate(offering) → Hohi
```

**How it works:**

1. Dis creates a Ritual (as today) and lists one or more FieryPit URLs as participants.
2. `RitualRegistry::bootstrap_participant()` calls `FieryPitClient::ritual_join(pit_url, ritual_id)`
   — lildaemon registers the ritual ID and allocates an internal result queue.
3. During `evaluator_pass_ritual`, Dis pulls Offerings off the Kafka topic, then calls
   `FieryPitClient::ritual_publish(pit_url, ritual_id, offering)` → lildaemon evaluates
   asynchronously, queues the Hohi.
4. Dis calls `FieryPitClient::ritual_poll(pit_url, ritual_id)` → lildaemon drains its
   result queue and returns any completed Hohi responses.
5. Dis publishes those Hohi responses back to the Kafka topic, decrementing
   `pending_evaluator_responses`.

**lildaemon endpoints needed (4 new):**

| Method | Path | What it does |
|---|---|---|
| `POST` | `/ritual/join` | Register a ritual ID; allocate result queue. Body: `{ritual_id, dis_domain}` |
| `POST` | `/ritual/{id}/offering` | Accept an Offering JSON; evaluate async; queue Hohi. Body: Offering payload |
| `GET` | `/ritual/{id}/poll` | Return and drain any completed Hohi responses |
| `DELETE` | `/ritual/{id}` | Deregister ritual; discard queue |

**FieryPitClient Rust methods needed (4 new):**

```rust
fn ritual_join(&self, ritual_id: Uuid, dis_domain: &str) -> Result<(), FieryPitError>;
fn ritual_publish(&self, ritual_id: Uuid, offering: &Value) -> Result<(), FieryPitError>;
fn ritual_poll(&self, ritual_id: Uuid) -> Result<Vec<Value>, FieryPitError>;
fn ritual_leave(&self, ritual_id: Uuid) -> Result<(), FieryPitError>;
```

**Pros:**
- lildaemon stays Kafka-free — no new Python dependencies
- Dis already owns the Kafka connection via `RsKafkaClient`
- Minimal lildaemon changes (4 endpoints + internal result queue)
- FieryPitClient methods map cleanly to the existing blocking-client pattern
- Consistent with the `ritual_poll` / `ritual_publish` names already in the review doc

**Cons:**
- Dis is on the critical path for every Offering→Hohi round trip — CycleController must
  actively drive the exchange on every evaluator pass
- lildaemon cannot participate in a ritual without Dis alive and polling
- If Dis restarts mid-ritual, in-flight Offerings are lost

[STAN] We'll go with Option B, see my comments below.
---

### Option B — lildaemon owns its Kafka consumer (fully autonomous)

```
Kafka topic  ←→  lildaemon Kafka consumer/producer
                  ↓ evaluator.evaluate(offering) → Hohi → publish back to topic
Dis calls POST /ritual/join (hands over topic name) then forgets about lildaemon
```

**How it works:**

1. Dis creates a Ritual, calls `FieryPitClient::ritual_join(pit_url, ritual_id, topic, bootstrap)`
   — lildaemon spins up its own `confluent-kafka` consumer on `topic` and a producer.
2. lildaemon autonomously polls the topic, evaluates each Offering, publishes Hohi.
3. Dis's `evaluator_pass_ritual` still polls the same Kafka topic, sees the Hohi, and
   decrements `pending_evaluator_responses` normally.
4. On `RitualRegistry::terminate`, Dis calls `FieryPitClient::ritual_leave` to stop
   lildaemon's consumer.

**lildaemon endpoints needed (2 new):**

| Method | Path | What it does |
|---|---|---|
| `POST` | `/ritual/join` | Start Kafka consumer on topic; begin autonomous evaluate→publish loop. Body: `{ritual_id, topic, bootstrap_servers, dis_domain}` |
| `DELETE` | `/ritual/{id}` | Stop consumer; clean up |

**FieryPitClient Rust methods needed (2 new):**

```rust
fn ritual_join(&self, ritual_id: Uuid, topic: &str, bootstrap: &str, dis_domain: &str)
    -> Result<(), FieryPitError>;
fn ritual_leave(&self, ritual_id: Uuid) -> Result<(), FieryPitError>;
```

**Pros:**
- lildaemon is a true first-class Kafka participant — operates independently of Dis uptime
- Simpler CycleController: no need to ferry Offerings manually; Dis just publishes to Kafka
  as normal and waits for Hohi
- Naturally scales: multiple lildaemon instances each run their own consumer
- Better failure isolation: Dis restart doesn't lose in-flight evaluations

**Cons:**
- Requires adding `confluent-kafka` (or `aiokafka`) to lildaemon's Python dependencies
- Kafka bootstrap address must be reachable from the lildaemon host
- Background consumer thread/task management in Python adds complexity
- Docker Kafka or equivalent required in all environments where lildaemon runs
- More work in lildaemon for Phase 6

> **[STAN]** This is the approach we need. 
i'm thinking about a test involving:
1. summon a claramindsplinter and an custom subclass of echo evaluator (chanter)
2. have them participate in a new ritual
3. submit an offering to clara which should trigger an evaluation message for our chanter
4. chanter should see and pull the evaluation from kafka, pushing a tephra echo response onto kafka as a response
5. clara picks up the response and converges. we may need to gate the convergence by a patience (timeout) and enough of a cycle limit to let it complete.

our real-world analog is submitting a question to the first evaluator which it cannot resolve by itself.  it (clara in this case) passes on the offering which gets either addressed entirely or results in assertions which resolve it eventually converging in an answer.

---

## Naming Collision Note

lildaemon's `docs/ritual_implementation_plan.md` uses "Ritual" to mean a
BleatSession resume/replay wrapper — a single-evaluator conversational concept.
The Dis "Ritual" means multi-evaluator Kafka coordination.

Suggested resolution: rename the lildaemon concept to **"Ceremony"** or **"Invocation"**
in its implementation, reserving "Ritual" for the Kafka coordination protocol.

[STAN]  it's actually the same concept, just surfacing now that the clara-cerebrum code is touching our browser based GUI editor for rituals (../dagda as ./scripts/cobbler.sh).  Rituals will be defined using a GUI front end such as Cobbler (it's a prototype to get the interfaces correct) and we'll include our deduction tracing (clara-transduction and clara-cycle through the coire) and a rule editor there as well.

so a ritual to Dagda's Coire browser based gui is the same as a Ritual on the Dis side.  it's composed of evaluators (lildaemons) running in the Fiery Pit.
the flow is between browser <-> cobbler/backend <-> fiery pit/goatwrangler <-> clara-cerebrum/Dis 
rule engine layer scales separately from the llm-intensive daemons and browser based clients are served through their own infrastructure

---

## CycleController Integration (both options)

Regardless of which option is chosen, the Dis-side `evaluator_pass_ritual` loop needs
one enhancement: it currently publishes Offerings from Coire events to the Kafka topic
and waits for Hohi to arrive on the same topic. Under Option A, it must also forward
those Offerings to lildaemon via `FieryPitClient::ritual_publish` before (or after)
publishing to Kafka. Under Option B, this forwarding step is absent — lildaemon handles
it directly from Kafka.

[STAN]  We should pull from the Python side when possible.  Kafka publish should trigger the consumer on the Evaluator/Fiery Pit side to see the new event which should be processed on a session (probably) with the evaluator.  we want to be able to have evaluators as stateless or stateful, so we shouldn't assume no session without considering it.

---

## Recommended Starting Point

Implement **Option A** for Phase 6. It is deliverable without touching lildaemon's
Kafka infrastructure, keeps the change surface small, and the architecture can be upgraded
to Option B in Phase 7 if autonomous participation becomes a requirement.

[STAN]  Let's go with B as it is needed to bring the whole system together.  Once we have the FieryPit/Dis layer solid, we'll wire in the Dagda/Cobbler.

---

## Open Questions Before Implementation

1. **Auth**: Should the new `/ritual/*` lildaemon endpoints require a JWT, or use a
   shared secret between Dis and lildaemon (e.g. `X-Dis-Token` header)?
   > **[STAN]**

[STAN] Since we expect the FieryPit to be accessible to Dagda's cobbler, which touches the web, let's use a JWT.

2. **Evaluator selection**: When lildaemon receives an Offering for a ritual, which
   evaluator should it use? Options:
   - Always the currently-focused evaluator (simplest)
   - Ritual-specific evaluator set at join time (`POST /ritual/join` body includes `evaluator`)
   - Offering metadata specifies the evaluator

[STAN] The rules (Prolog/CLIPS) should implement this.  I'm thinking CLIPS rules should fire on conditions (like we couldn't resolve the goal or a clue) to push an evaluation to the peer.  Our rituals will need a rule enhanced FSA to do this properly, but we can stub it out as a simple CLIPS rule for testing.  we'll work on the details of message coordination in synch with the enhanced FSA.   
   

3. **Concurrency**: Under Option A, should lildaemon evaluate Offerings synchronously
   (one at a time, blocking the `/offering` call until done) or asynchronously (accept
   immediately, queue, return results via `/poll`)?

[STAN] I think this is n/a.

4. **Timeout**: What timeout applies to lildaemon evaluation within a ritual? Should
   unanswered Offerings after N seconds produce a Tabu (error Hohi) instead of blocking
   forever?

[STAN] yes, if we haven't heard from a peer in a timeout we can treat that as a positive failure (evidence of absence) and assert the failure/push a Tephra with a Tabu (error).  we can continue trying to converge running the cycle until cycle limit, even if there's a failure.

5. **Bootstrap participant flow**: When `RitualConfig.participants` lists FieryPit URLs,
   should `bootstrap_participant()` be called at Ritual create time, or only when the
   first `POST /deduce` with that `ritual_id` is received?

[STAN] Let's make a note to revisit this question as managing the cohort/quorum is an open item.  For now, let's lazily establish the participant() at Ritual creation.
