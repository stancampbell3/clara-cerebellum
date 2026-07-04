# Runtime Wiring: Deduce on an Active Ritual → `reasoned_response` Returned

**Status:** Implemented — pending team review
**Date:** 2026-07-04
**Builds on:** `docs/cobbler_ritual_edge_deduce_plan.md` (Cobbler editor authoring, Phase 1)
**Repos touched:** `lildaemon` (primary), `dagda` (one-line fix). No `clara-api`/`clara-cycle` changes.

---

## Context

Phase 1 let a user author Prolog/CLIPS source on a deduce-capable Cobbler node
and a lightweight qualifier on an edge, persisted in `graph_layout`. None of
it did anything at runtime. This phase closes the loop end to end: activate a
Ritual whose node carries a `reasoned_response/3` implementation, publish an
Offering onto that Ritual, and get back a Hohi whose payload is the actual
bound `Response` — not just a bare `deduction_id`.

Research (three Explore passes across `lildaemon` + `clara-api`, plus direct
reads of every file changed) surfaced four concrete gaps, all necessary for
the stated goal:

1. **Activation never touched `graph_layout`** — `activate_ritual_config`
   loaded it but never parsed it.
2. **`POST /source` was unused from lildaemon** — no existing client method.
   Confirmed content-addressed/idempotent (dedup key
   `sha256(content)+source_type`), so registering on every activation is safe.
3. **`initial_goal` has no template engine** — clara-api passes it to the
   Prolog engine as a literal goal string. The only way to bind `Query` is
   string-splicing an escaped Prolog atom into that string — a real injection
   surface, addressed with a dedicated, tested escape function rather than
   deferred.
4. **`_handle_deduction` never polled to completion** — it POSTed to
   `/deduce` and returned the raw `202 {deduction_id, status: "running"}`
   body as the Hohi. Since `/deduce` is asynchronous by design, nobody
   automatically got the resolved `Response` back — this was the piece most
   directly blocking "see the reasoned_response being returned."

Also fixed in passing: Cobbler's `DEFAULT_REASONED_RESPONSE_PROLOG` template
was missing `:- use_module(library(the_rat)).`, without which
`ponder_text_with_context/3` and `current_context/1` aren't in scope.

**Scope boundary (deliberate, not a gap):** `ritual_configs.evaluator` is a
single string today — one evaluator per Ritual. This phase resolves source
from the one graph node whose `evaluatorName == config.evaluator`, matching
that existing model. Multiple deduce-capable nodes per Ritual (one
`RitualParticipant` per node) is a bigger structural change, intentionally
out of scope. `ritual_id` is also deliberately left `None` on the
constructed deduce request — setting it would make the inner deduction's own
evaluator-pass try to join the same Kafka topic recursively for
peer-evaluation, a real future capability (nested/peer evaluator chains) but
not needed for a single `reasoned_response` call.

---

## What was implemented

### Part 0 — Cobbler default template fix
`dagda/cobbler/frontend/src/components/GraphCanvas/deduceCapable.ts`:
`DEFAULT_REASONED_RESPONSE_PROLOG` now leads with
`:- use_module(library(the_rat)).`.

### Part A — Activation-time source registration (lildaemon)
- **`goat/app/dis_client.py`** — new `register_source(source_type, content, label=None) -> (source_id, is_new)`, matching the existing `create_ritual`/`join_ritual` style.
- **`goat/app/ritual_configs/store.py`** — new migration adds `prolog_source_id`/`clips_source_id` columns (same pattern as the existing `graph_layout` migration); new `set_source_ids()` method.
- **`goat/app/ritual_configs/models.py`** — `RitualConfigResponse` exposes the two new fields (system-populated, not settable via Create/Update).
- **`goat/app/ritual_configs/router.py`** — `activate_ritual_config` now parses `graph_layout`, finds the node matching `config.evaluator`, registers its `prologSource`/`clipsSource` with Dis, persists the resolved ids, and passes them into `manager.join(...)`. Registration failure participates in the existing rollback path (activation fails cleanly rather than producing a Ritual that can never deduce).
- **`goat/models/RitualManager.py` / `RitualParticipant.py`** — thread `prolog_source_id`/`clips_source_id` through; `RitualParticipant` derives `self.deduce_capable`.

### Part B — Offering qualification (`goat/models/RitualParticipant.py`)
- New **`goat/utils/prolog_escape.py`** — `prolog_quote_atom()`, ISO-style Prolog atom escaping (backslash, single quote, newline, carriage return) for safely splicing arbitrary text into a goal string.
- `_handle_message` now qualifies a plain Offering (`{"goal": ..., "context": [...]}`, the shape already documented in `docs/rituals_101.md`) into a deduce request when the node is deduce-capable — building `current_context(C), reasoned_response(<escaped query>, C, R)` as the goal. An Offering that already carries a `"deduce"` key passes through untouched.

### Part C — Resolve, don't just start (`goat/evaluators/custom/kindling_evaluator.py`)
`_handle_deduction` now polls `GET /deduce/{id}` with backoff (50ms → capped
at 2s, bounded by its own `max_wait_s`, independent of any outer caller
timeout — an `asyncio.wait_for` cancellation doesn't stop this thread's
blocking loop, so it must return on its own) until the run leaves
`"running"`. On convergence, it extracts `prolog_solutions[0]["R"]` and
returns `Hohi(response={"response": <R>, "deduction": <full result>})`.
Non-converged/timed-out results become a `Tabu` (code `504`), including the
`deduction_id` for manual follow-up via `deduction_poll`.

### Part D — `ClaraFish` REPL stub fixed (`goat/repl/fishes/clara_fish.py`)
No Ritual/node exists in a REPL session to resolve a `prolog_source_id`
from, so the fix here inlines the default clause via `prolog_clauses`
instead: `Query`/`Context` are now correctly bound (previously literal,
unbound Prolog variable names `Q`/`C`), and the `context` parameter — 
previously hardcoded to `[]` regardless of what was passed to `translate()` — 
is now threaded through.

---

## Testing

- **9 new tests** (`tests/test_prolog_escape.py`) — adversarial input for
  `prolog_quote_atom`: embedded quotes, backslashes, newlines, empty string,
  unicode, and a literal goal-injection attempt (`x'), evil_goal(C`),
  asserting the payload's quote is escaped rather than terminating the atom.
- **4 new tests** (`tests/test_deduce_poll_to_completion.py`) — converge on
  first poll, converge after several backoff cycles, max-cycles-exceeded →
  `Tabu`, and the internal poll deadline actually stopping rather than
  looping forever.
- **Full existing suite re-run**: 781 passed. 82 pre-existing errors are
  `_duckdb.IOException` lock contention from a live lildaemon process left
  running for manual verification (same on-disk `lildaemon.duc`) — confirmed
  unrelated to this change; none of the 82 touch files modified here, and
  all 84 tests across the five touched/new test files pass cleanly.

## Live end-to-end verification

Stood up the real stack: lildaemon, Cobbler backend, and clara-api (Dis) —
the last of these newly required by this phase, run with
`KAFKA_BOOTSTRAP` unset (Dis's `InMemoryBroker`, matching the documented
dev/test default) and persistence temporarily enabled (reverted after).

1. Created and activated a real `ritual_config` (`evaluator: "clara_mind_splinter"`) with a `graph_layout` containing one node carrying the corrected default Prolog template. Activation response came back with a populated `prolog_source_id` — confirmed via the API, not just log output.
2. Called `RitualParticipant._handle_message` directly (bypassing Kafka — a synthetic envelope, per the plan's own allowance for avoiding a live Kafka dependency) with a plain `{"goal": "is the visitor lost?", "context": [...]}` Offering. It qualified correctly and the resulting deduce request, run against the **real** clara-api, converged — Dis called back to lildaemon's `/evaluate` for `ponder_text_with_context/3`, which called the configured LLM, and the actual generated answer text came back bound to `R`, extracted and returned as the Hohi's `response` field. `current_context/1` also correctly bound `C` to the real conversation history passed in.
3. Confirmed an already-`"deduce"`-shaped Offering passes through with its explicit fields untouched (no double-qualification).
4. Confirmed `ClaraFish.translate()` now builds a fully-bound goal for REPL input.
5. Confirmed `POST /source` idempotency: a second, separate ritual config with identical `prologSource` content received the **same** `prolog_source_id` on activation — no duplicate rows.

## Known gaps / outstanding items

- **No automated visual/interactive QA of the Cobbler-side fix (Part 0)** — same sandbox limitation as Phase 1 (no headless browser available); it's a one-line template string change, low risk, but not clicked-through.
- **Reactivation of a terminated config isn't supported by the existing API** (`_require_draft` gate — `draft → active → terminated` has no reverse arrow today, matching `rituals_101.md`'s documented lifecycle). Verification step 8 in the plan assumed reactivation would work; it doesn't, so idempotency was instead verified via two separate configs sharing identical source content — a stronger test of the dedup mechanism anyway, but worth knowing this isn't a code gap, it's existing, documented behavior.
- **`ClaraFish`'s constructed `Query` includes the raw `¿...?`/`??...??` wrapping punctuation**, not the stripped inner text — cosmetic, likely harmless to the LLM, but not something this pass corrected since the original code never stripped it either (no prior behavior to preserve or regress).
- **Housekeeping from verification, not the feature itself**: two test ritual configs and a `verify_bot` lildaemon user from this session's manual testing remain in the respective stores (harmless, but exist); a `tests/lildaemon.duc` file appeared as a test-run artifact and is currently untracked/not gitignored.
- Nothing in this phase changes edge qualifier (`none`/`assertion`/`boolean`) *evaluation* — Phase 1's qualifiers still don't gate or transform anything at runtime; only the node-level `reasoned_response/3` path is wired.

## Explicitly out of scope (unchanged from the plan)

- Multiple deduce-capable nodes / multiple `RitualParticipant`s per Ritual.
- Edge qualifier evaluation semantics.
- Nested/peer evaluator chains (`ritual_id` set on the inner deduce request).
- Any change to `clara-api`/`clara-cycle` (Rust side).

## Files changed

```
lildaemon/
  goat/app/dis_client.py
  goat/app/ritual_configs/store.py
  goat/app/ritual_configs/models.py
  goat/app/ritual_configs/router.py
  goat/models/RitualManager.py
  goat/models/RitualParticipant.py
  goat/evaluators/custom/kindling_evaluator.py
  goat/repl/fishes/clara_fish.py
  goat/utils/__init__.py                      (new)
  goat/utils/prolog_escape.py                  (new)
  tests/test_prolog_escape.py                  (new)
  tests/test_deduce_poll_to_completion.py      (new)

dagda/
  cobbler/frontend/src/components/GraphCanvas/deduceCapable.ts
```

Nothing has been committed yet in any repo — this write-up is for team
review before that happens.
