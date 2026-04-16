# lildaemon — Review Items for Phase 6 Sign-off

_Raised by: Dis / clara-cerebrum team · 2026-04-15_

These items need confirmation, review, or a fix from the lildaemon team before the
Phase 6 ritual integration can be considered fully production-ready.

---

## 1. Bug: `KindlingEvaluator.evaluate_async()` drops conversation context

**Severity:** Medium — breaks multi-turn LLM evaluations silently.

**File:** `goat/evaluators/kindling_evaluator.py` ~line 514

**What happens:** When `evaluate_async` reconstructs the `Offering` dict to dispatch to
`OllamaEvaluator`, it does not forward the `"context"` key (the conversation history array).
Every LLM evaluation therefore sees an empty context regardless of what was passed in.

**Proposed fix:**
```python
# In the reconstructed Offering dict inside evaluate_async:
"context": data.get("context", []),
```

**Test:** No existing test covers this path — a test that asserts `offering.context` is
forwarded to the evaluator would prevent regression.

---

## 2. Confirmation: `Chanter` evaluator registration

**File:** `goat/evaluators/` — wherever `bootstrap_default_evaluators()` lives.

**Request:** Confirm that `Chanter` is registered under the key `"chanter"` with metadata
`{"type": "ritual", "builtin": True}` and that `GET /evaluators` (or equivalent) lists it.

The Dis `POST /ritual` bootstrap path passes `"evaluator": "chanter"` when joining.  If
`Chanter` is not registered, the FieryPit `/ritual/join` endpoint returns `400 Bad Request`
and ritual bootstrapping fails silently (Dis treats bootstrap failures as non-fatal warnings).

---

## 3. Confirmation: JWT requirement on `/ritual/*` endpoints

**File:** `goat/app/ritual/router.py`

Both `/ritual/join` (POST) and `/ritual/{ritual_id}` (DELETE) currently require
`Authorization: Bearer <jwt>` via `Depends(get_current_user)`.

**Request:** Confirm whether this is intentional for the Phase 6 integration, or whether the
Dis bootstrap path (server-to-server, no human user in the loop) should use an API key or
a service account JWT instead.  Dis currently has no mechanism to obtain or refresh a JWT
automatically — the bootstrap call will fail with `401` unless a token is pre-configured.

**Options to discuss:**
- Dis holds a long-lived service JWT in its config (simple, fragile on rotation)
- lildaemon adds a service-account endpoint that issues tokens for S2S calls
- `/ritual/*` endpoints are exempted from auth for internal (loopback / VPC) callers
- Dis sends a pre-shared API key header instead of a JWT

[STAN] Let's have lildaemon issue service to service tokens via a new service-account endpoint.

---

## 4. Confirmation: `RitualParticipant` echo-suppression logic

**File:** `goat/models/RitualParticipant.py`

The participant skips envelopes where `envelope.producer_node == self.dis_domain`.
This prevents a FieryPit instance from evaluating Offerings it published itself (e.g., if
FieryPit were ever to act as an Offering producer).

**Request:** Confirm that `producer_node` in the `TephraEnvelope` published by Dis will
always be set to the Dis domain string (e.g., `"dis.local"`) so echo suppression fires
correctly.  The Dis `TephraEnvelope::new()` constructor does not currently set
`producer_node` — it defaults to an empty string, which means echo suppression would never
trigger (not a bug today, but worth aligning on the field semantics).

[STAN] Good point.

---

## 5. Confirmation: consumer group ID and offset semantics under restart

**File:** `goat/models/RitualParticipant.py`

Consumer group id: `"ritual-{ritual_id}-{dis_domain}"`.
Offset at join: `latest` — the participant intentionally skips history.

**Requests:**
- If a FieryPit process restarts and rejoins the same ritual, does Kafka's committed offset
  for the group cause it to replay messages, or does the `latest` start override the
  committed offset?  (Depends on `auto.offset.reset` setting — worth documenting.)

[STAN] If the evaluator is a new instance (restarted or freshly joined with no context) then we should replay.
Otherwise, beginning where we left off should keep us at the right point in the evaluator's reasoning.

- If two FieryPit instances share the same `dis_domain` string and join the same ritual,
  they share a consumer group ID and will load-balance Offerings rather than both seeing
  every Offering.  Is that the intended behaviour?

[STAN] Yes.  Individual instances of the same Evaluator class on the FieryPit side should load-balance by default.  We will introduce routing evaluators later.

---

## 6. Nice-to-have: `active_rituals()` endpoint

`RitualManager.active_rituals()` is implemented but there is no REST endpoint exposing it.
A `GET /ritual` (or `GET /ritual/active`) would let Dis poll which rituals a FieryPit
instance is currently participating in — useful for health checks and debugging.  Not a
blocker for Phase 6 but worth adding before external exposure.

[STAN] This would enable creating dashboards showing in progress deductions.  We should plan for it.
