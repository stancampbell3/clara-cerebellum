# Phase 6 — Ritual E2E Test Plan
# Prolog Goal Satisfied by Peer Evaluator via Forward-Chained CLIPS Rule

_Branch: `housbonde_lif` · Created: 2026-04-15_

---

## Scenario

A deduction session starts with a Prolog goal that cannot be proved from local knowledge
alone.  A CLIPS forward-chaining rule detects the need for peer evaluation, emits an
Offering to the Ritual Kafka topic, and a peer lildaemon instance (`Chanter`) responds.
The Hohi response is routed back through CLIPS, which asserts the answer into Prolog,
allowing the goal to be proved on re-evaluation.

```
Prolog goal: peer_answered(hello, Answer)

Cycle 0
  ┌── Prolog pass: peer_answered fails (no answered/2)
  │   Relay P→C: nothing
  │   CLIPS pass: (need-peer-eval "hello") fact present
  │                → ask-peer-chanter rule fires
  │                → coire-emit to Prolog mailbox: "evaluator/ask-chanter"
  │   Relay C→P: nothing
  │   Evaluator pass: publish_evaluator_events drains "evaluator/ask-chanter"
  │                   → publishes Offering to Kafka topic
  │                   → pending_evaluator_responses = 1
  └── Convergence: pending > 0 → NOT CONVERGED

    [ Chanter lildaemon polls topic, evaluates Offering, publishes Hohi ]

Cycle 1
  ┌── Prolog pass: still fails
  │   Relay P→C: nothing
  │   CLIPS pass: (run) — no rules fire yet (coire-event not yet asserted)
  │   Relay C→P: nothing
  │   Evaluator pass: poll_incoming → Hohi arrives
  │                   ingest_tephra: pending = 0
  │                     writes ritual/hohi to Prolog mailbox
  │                     writes ritual/hohi to CLIPS mailbox  [dual-write ← new]
  └── Convergence: clips_pending = 1 (ritual/hohi unprocessed) → NOT CONVERGED

Cycle 2
  ┌── Prolog pass: still fails
  │   Relay P→C: nothing
  │   CLIPS pass: consume_coire_events dispatches ritual/hohi
  │                → (coire-event (origin "ritual/hohi") ...) fact asserted
  │                → (run): receive-hohi-answer rule fires
  │                → (coire-publish-assert "answered(hello, chanter_responded)")
  │   Relay C→P: picks up assert event → answered(hello, chanter_responded)
  │              added to Prolog mailbox as relay-clips event
  │   Evaluator pass: nothing
  └── Convergence: prolog_pending = 1 (relay event) → NOT CONVERGED

Cycle 3
  ┌── Prolog pass: consume_coire_events → assertz(answered(hello, chanter_responded))
  │              peer_answered(hello, A) succeeds!  A = chanter_responded
  │   (all passes quiet)
  └── Convergence: mailboxes empty, pending = 0, root goal resolved → CONVERGED ✓
```

---

## Required Changes

Four focused changes to `clara-cerebrum` are needed.  No lildaemon changes required
for the InMemoryBroker Rust test (Chanter is already built for the live Kafka variant).

---

### Change 1 — Inject Prolog session ID into CLIPS

**File:** `clara-cycle/src/session.rs` (or wherever `ClipsEnvironment` is initialised)

The CLIPS rule that emits the `evaluator/` event must write directly to the **Prolog**
session's Coire mailbox.  `publish_evaluator_events` reads from `self.session.prolog_id`,
so the CLIPS rule must call `(coire-emit ?*prolog-session-id* "evaluator/..." ...)`.

At `ClipsEnvironment` creation time, inject the Prolog session UUID as a CLIPS global:

```rust
// After binding ?*coire-session-id*:
env.eval(&format!(
    r#"(bind ?*prolog-session-id* "{}")"#,
    prolog_session_id
))?;
```

And add the global declaration to `the_coire.clp`:

```clp
;;; UUID of the paired Prolog engine.  Set by Rust at ClipsEnvironment creation.
;;; Use for cross-engine Coire writes (e.g., emitting evaluator/ events).
(defglobal ?*prolog-session-id* = "")
```

---

### Change 2 — Reorder the cycle loop: CLIPS before evaluator

**File:** `clara-cycle/src/controller.rs`, `CycleController::run()`

Currently:
```
1. Prolog pass
2. Relay P→C
3. Evaluator pass   ← runs before CLIPS
4. CLIPS pass
5. Relay C→P
6. Convergence
```

Required:
```
1. Prolog pass
2. Relay P→C
3. CLIPS pass       ← CLIPS fires rules first (may emit evaluator/ events)
4. Relay C→P
5. Evaluator pass   ← sees CLIPS-emitted evaluator/ events
6. Convergence
```

**Rationale:** CLIPS rules determine _what_ needs evaluating; the evaluator pass then
publishes those requests and ingests responses.  With the current ordering, CLIPS-emitted
`evaluator/` events are not visible to `publish_evaluator_events` until the _next_ cycle.
The reordering makes cycle 0 CLIPS output visible to cycle 0 evaluator pass.

**Backward compatibility:** Existing tests that pre-seed `evaluator/` events in Prolog's
Coire mailbox before running the controller are unaffected — the pre-seeded event is still
present when the evaluator pass runs (now at step 5 instead of step 3), and CLIPS (step 3)
is a no-op when no CLIPS KB is loaded.

---

### Change 3 — `ingest_tephra` dual-writes to CLIPS mailbox

**File:** `clara-cycle/src/controller.rs`, `CycleController::ingest_tephra()`

Currently `ingest_tephra` writes the incoming `ritual/{label}` event only to the Prolog
session's Coire mailbox.  CLIPS cannot react to it unless it also receives a copy.

After the existing Prolog write, also write to the CLIPS mailbox:

```rust
// Existing write to Prolog:
let prolog_event = clara_coire::ClaraEvent::new(
    self.session.prolog_id,
    format!("ritual/{}", tephra.label),
    body.clone(),
);
coire.write_event(&prolog_event)?;

// New: also write to CLIPS so rules can react to Hohi/Tabu responses.
let clips_event = clara_coire::ClaraEvent::new(
    self.session.clips_id,
    format!("ritual/{}", tephra.label),
    body,
);
coire.write_event(&clips_event)?;
```

`consume_coire_events()` in the CLIPS pass will dispatch it as a `(coire-event ...)`
template fact.  The `clips_pending` snapshot count will be 1 until CLIPS processes it,
which correctly prevents premature convergence.

---

### Change 4 — Test KB files and Rust test

#### `clara-cycle/tests/resources/ritual_chanter_test.pl`

Base Prolog KB:

```prolog
:- use_module(library(the_coire)).

:- dynamic answered/2.

% Goal to prove — succeeds once CLIPS relays the chanter response.
peer_answered(Prompt, Answer) :- answered(Prompt, Answer).
```

#### `clara-cycle/tests/resources/ritual_chanter_test_clara.pl`

Clara-augmented Prolog (boilerplate `updated/3` hook):

```prolog
% ── Clara integration ────────────────────────────────────────────────────────
updated(Pred, Action, Context) :-
    clause(Head, _Body, Context),
    coire_publish_assert(Head),
    format('Updated ~w with action ~w in context ~p~n', [Pred, Action, Head]).
% ── End Clara integration ────────────────────────────────────────────────────

:- use_module(library(the_coire)).

:- dynamic answered/2.

peer_answered(Prompt, Answer) :- answered(Prompt, Answer).
```

#### `clara-cycle/tests/resources/ritual_chanter_test_clara.clp`

```clp
;;; ritual_chanter_test_clara.clp
;;;
;;; Two rules driving the Ritual peer-evaluation round-trip test:
;;;
;;;   ask-peer-chanter   — fires on (need-peer-eval ?prompt), emits an
;;;                        Offering to the Prolog evaluator/ Coire channel.
;;;   receive-hohi-answer — fires when the peer's Hohi arrives back as a
;;;                         (coire-event (origin "ritual/hohi")) fact,
;;;                         publishing the answer back to Prolog.

;;; (need-peer-eval "hello") is asserted by the test setup before (run).

(defrule ask-peer-chanter
    ?f <- (need-peer-eval ?prompt)
    =>
    (retract ?f)    ; consume once
    (coire-emit ?*prolog-session-id*
                "evaluator/ask-chanter"
                (str-cat "{\"prompt\":\"" ?prompt "\"}")))

(defrule receive-hohi-answer
    ?ev <- (coire-event (origin "ritual/hohi"))
    =>
    (retract ?ev)   ; consume once
    (coire-publish-assert "answered(hello, chanter_responded)"))
```

Note: `?f <- ...` / `(retract ?f)` and `?ev <- ...` / `(retract ?ev)` ensure each rule
fires exactly once per fact, preventing repeated firing if the session runs past
convergence.

#### Rust test: `run_loop_ritual_chanter_e2e` (in `controller.rs` test module)

```rust
/// E2E test for the full Ritual peer-evaluation round-trip driven by CLIPS rules.
///
/// Flow:
///   CLIPS (need-peer-eval "hello") fact
///     → ask-peer-chanter rule emits evaluator/ask-chanter (cycle 0 CLIPS pass)
///     → evaluator pass publishes Offering (cycle 0, pending=1)
///     → mock evaluator thread responds with Hohi
///     → ingest_tephra dual-writes ritual/hohi to Prolog + CLIPS mailboxes
///     → receive-hohi-answer CLIPS rule fires → assert answered/hello relay
///     → Prolog asserts answered(hello, chanter_responded)
///     → peer_answered(hello, chanter_responded) succeeds → Converged
///
/// Uses InMemoryBroker — no Kafka or lildaemon required.
#[test]
fn run_loop_ritual_chanter_e2e() {
    // 1. Setup Coire + ritual
    setup_coire();
    let broker   = Arc::new(InMemoryBroker::new());
    let registry = RitualRegistry::new("dis.test", broker.clone());
    let ritual_id = registry
        .create(RitualConfig { name: "chanter-e2e".into(), participants: vec![] })
        .unwrap();
    let topic = topic_name("dis.test", ritual_id).unwrap();
    let handle = registry.join(ritual_id, Some("cc")).unwrap();

    // 2. Create DeductionSession, seed KB files
    let mut session = DeductionSession::new().unwrap();
    // Consult Prolog KB
    session.prolog.consult_file("tests/resources/ritual_chanter_test.pl").unwrap();
    // Load CLIPS KB
    session.clips.load_rules_from_file(
        "tests/resources/ritual_chanter_test_clara.clp"
    ).unwrap();
    // Seed CLIPS with initial working-memory fact
    session.clips.eval("(assert (need-peer-eval \"hello\"))").unwrap();

    // 3. Mock evaluator thread: poll topic for Offering, respond with Hohi
    let mock_broker = broker.clone();
    let mock_topic  = topic.clone();
    let mock_thread = std::thread::spawn(move || {
        let mut offset = 0i64;
        for _ in 0..200 {
            let (envelopes, next) = mock_broker.poll(&mock_topic, offset).unwrap();
            offset = next;
            for env in &envelopes {
                if env.label == clara_ritual::label::OFFERING {
                    let hohi = TephraEnvelope::new(
                        ritual_id,
                        Uuid::new_v4(),
                        clara_ritual::label::HOHI,
                        60_000,
                        "chanter.test",
                        TephraPayload::Plaintext {
                            body: serde_json::json!({
                                "responder": "chanter",
                                "data": { "prompt": "hello" }
                            }),
                        },
                    );
                    mock_broker.publish(&mock_topic, &hohi).unwrap();
                    return;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    });

    // 4. Run CycleController
    let goal = "peer_answered(hello, Answer)".to_string();
    let mut ctrl = CycleController::new(
        session,
        50,
        Some(goal),
        Arc::new(AtomicBool::new(false)),
    ).with_ritual(handle);

    let result = ctrl.run().expect("run() should converge");

    // 5. Assertions
    assert_eq!(result.status, CycleStatus::Converged);
    assert!(result.cycles >= 3,
        "need at least 3 cycles for CLIPS→evaluator→Hohi→relay→Prolog path; \
         got {}", result.cycles);

    mock_thread.join().expect("mock evaluator panicked");

    // Root goal should be resolved KnownTrue in the tableau
    let bindings = result.goal_bindings.expect("goal bindings should be present");
    assert!(bindings["Answer"].as_str().is_some(),
        "Answer variable should be bound in goal_bindings");
}
```

---

## Implementation Order

| # | Task | File(s) | Notes |
|---|------|---------|-------|
| 1 | Add `?*prolog-session-id*` global declaration to `the_coire.clp` | `clara-clips/clp-lib/the_coire.clp` | One `defglobal` line |
| 2 | Inject prolog_session_id into CLIPS at session creation | `clara-cycle/src/session.rs` or wherever ClipsEnv is created | Requires DeductionSession to pass prolog_id to ClipsEnvironment |
| 3 | Reorder `run()` loop: CLIPS + relay before evaluator | `clara-cycle/src/controller.rs` | Swap steps 3-4 with 5 |
| 4 | `ingest_tephra` dual-write to CLIPS mailbox | `clara-cycle/src/controller.rs` | ~5 lines |
| 5 | Write test KB files | `clara-cycle/tests/resources/` | `.pl`, `_clara.pl`, `_clara.clp` |
| 6 | Write Rust test | `clara-cycle/src/controller.rs` | Inside `#[cfg(all(test, feature = "ritual"))]` |

---

## Test Assertions

| Assertion | Verifies |
|-----------|----------|
| `result.status == Converged` | Cycle did not exhaust max_cycles or hit patience timeout |
| `result.cycles >= 3` | Pending counter blocked premature convergence; full CLIPS→evaluator→Hohi→Prolog path ran |
| `result.goal_bindings["Answer"] == "chanter_responded"` | The answer was relayed from mock Hohi through CLIPS to Prolog |
| Kafka topic has exactly 2 messages: one `"offering"`, one `"hohi"` | The full Kafka roundtrip fired (assertable from InMemoryBroker state) |

---

## Loop Reorder Impact on Existing Tests

| Test | Current behaviour | After reorder |
|------|-------------------|---------------|
| `run_loop_converges_with_mock_evaluator_hohi` | Pre-seeded `evaluator/` event picked up on cycle 0 step 3 | Same event picked up on cycle 0 step 5 — same result |
| `evaluator_pass_noop_when_no_handle` | No ritual handle; evaluator_pass is no-op | No change |
| All other `controller.rs` tests | No ritual handle or no CLIPS KB | CLIPS pass is no-op; loop reorder has no effect |

The reordering is safe.  All 57 existing cycle tests should pass unchanged.

---

## Live Kafka Variant (not required for merge)

For a fully live E2E test (requires Kafka + running lildaemon):

```bash
# 1. Start Kafka
docker run -d -p 9092:9092 apache/kafka:latest

# 2. Start lildaemon on port 6666 with Chanter registered
cd /mnt/vastness/home/stanc/Development/lildaemon
python -m goat.app.main

# 3. Start Dis with kafka_bootstrap configured
# config/local.toml: kafka_bootstrap = "localhost:9092"

# 4. Create ritual with lildaemon as participant
curl -s -X POST http://localhost:8080/ritual \
  -H 'Content-Type: application/json' \
  -d '{"name":"chanter-e2e","participants":["http://localhost:6666"]}' | jq
# → Dis calls POST /ritual/join on lildaemon, Chanter consumer starts

# 5. POST /deduce with ritual_id and the Prolog/CLIPS KB
curl -s -X POST http://localhost:8080/deduce \
  -H 'Content-Type: application/json' \
  -d '{
    "ritual_id": "<uuid from step 4>",
    "prolog_clauses": [
      ":- dynamic answered/2.",
      "peer_answered(Prompt, Answer) :- answered(Prompt, Answer)."
    ],
    "clips_rules": [
      "(assert (need-peer-eval \"hello\"))",
      "(defrule ask-peer-chanter (need-peer-eval ?p) => (retract ?f) (coire-emit ?*prolog-session-id* \"evaluator/ask-chanter\" (str-cat \"{\\\"prompt\\\":\\\"\" ?p \"\\\"}\" )))"
    ],
    "goal": "peer_answered(hello, Answer)"
  }' | jq
```

Expected: `status: "Converged"`, `goal_bindings.Answer` contains Chanter's echo response.

---

## Open Questions

| Question | Notes |
|----------|-------|
| Where exactly is `?*prolog-session-id*` injected? | Need to trace `DeductionSession::new()` → `ClipsEnvironment::new()` call chain to find the right injection point |
| Does the CLIPS `(coire-event ...)` template need an `data` slot update? | The `data` slot in `the_coire.clp`'s deftemplate is a STRING; `ingest_tephra` payload is JSON. May need to serialize to string before asserting. |
| `CLIPS fact (need-peer-eval ...)` source in live test | In the Rust test, seeded directly. In a real deduce request, this fact would come from an initial CLIPS assert in the request body or from Prolog via coire_publish_assert. |
