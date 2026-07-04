# Run a Ritual: Multi-FieryPit Performance from Cobbler

**Status:** Implemented — pending team review
**Date:** 2026-07-04
**Builds on:** `docs/ritual_deduce_runtime_wiring.md` (Phase 2 — single-node deduce wiring)
**Repos touched:** `lildaemon` (primary), `dagda` (Cobbler backend + frontend). No `clara-api`/`clara-cycle` changes.

---

## Context

Phase 2 wired a single deduce-capable node running on the same lildaemon
process that activates the Ritual. This phase adds a **Run** button to
Cobbler's Rituals screen that actually performs a Ritual — which meant
closing three gaps:

1. The Cobbler graph editor already let a user author *multiple* daemon
   nodes per Ritual, each with its own `evaluatorName` — but activation only
   ever resolved **one** node (matching `config.evaluator`) and joined it
   locally. Multi-node graphs were authored but never performed as graphs.
2. `RitualManager` (lildaemon) is keyed by `ritual_id` only — one
   `RitualParticipant` per process. "Multiple evaluators" for one Ritual
   necessarily means multiple **processes** (multiple FieryPits), each
   joining the same `ritual_id` to host a different node.
3. No "Run" affordance existed anywhere in Cobbler, and no backend endpoint
   triggered a performance (`POST /deduce` with `ritual_id` set) from the UI.

Confirmed via research before implementation: "performing" a Ritual on Dis
already means `POST /deduce` with `ritual_id` set — no Rust changes needed.
`POST /ritual/join` (lildaemon) already exists as "host this evaluator in
this Ritual" on any FieryPit — what was missing was orchestration.

**Key discovery:** `DaemonNode.url` already existed end-to-end in the
Cobbler frontend (types, serializer, properties panel) but was never read
by the backend — it was exactly the "which FieryPit hosts this node" field
this feature needed. No new data model was required, just wiring + a label
change.

**Decisions locked from discussion:** true multi-node model (each graph
node a distinct evaluator, individually addressed, potentially on a
different FieryPit — not a redundant pool); Run prompts for a query wrapped
into `reasoned_response` against the entry node; cross-FieryPit join calls
authenticate via a shared static secret scoped to the Dis Domain
(`DIS_DOMAIN_PEER_TOKEN`), not JWTs.

---

## What was implemented

### Part A — lildaemon: peer auth for cross-FieryPit join
- **`goat/app/users/auth.py`** — new `get_current_user_or_peer()`: if the
  bearer token matches `DIS_DOMAIN_PEER_TOKEN`, returns a synthetic
  `User(role="service")`; otherwise falls through to normal JWT validation.
  Deliberately separate from JWTs/`create_service_token` — a single shared
  string, no per-deployment service-user provisioning.
- **`goat/app/ritual/router.py`** — `POST /ritual/join` now uses
  `get_current_user_or_peer`; `RitualJoinRequest` gained
  `prolog_source_id`/`clips_source_id`, threaded into the existing
  `manager.join(...)` call. `/ritual` (list) and `DELETE /ritual/{id}`
  (leave) intentionally remain user-JWT-only.
- `.env.example` — new commented `DIS_DOMAIN_PEER_TOKEN=` entry.

### Part B — lildaemon: multi-node activation
`goat/app/ritual_configs/router.py`, `activate_ritual_config` now loops
over **all** evaluator-bearing daemon nodes instead of resolving only the
one matching `config.evaluator`:
1. Registers every node's Prolog/CLIPS source with Dis (looped, was
   already idempotent/content-addressed).
2. New `partition_nodes_by_target()` (module-level, directly unit-tested)
   resolves each node to local (blank/self `url`) or remote; raises if more
   than one node resolves to local — `RitualManager`'s one-per-process
   design makes this a hard constraint, not a style choice.
3. **Legacy fallback, discovered during regression testing:** configs with
   no `graph_layout` (the pre-graph-editor `RitualConfigForm` path) produced
   zero nodes under the new loop, which silently stopped joining anyone —
   a real regression against the old unconditional single-join behavior.
   Fixed by synthesizing one local node from `config.evaluator` when no
   graph nodes exist, preserving old behavior exactly.
4. `dis_client.create_ritual(config.name, [])` — now called with an empty
   participants list. Dis's own participant-bootstrap can't carry per-node
   evaluator/source ids, so this feature replaces that generic mechanism
   with the explicit joins below (intentional behavior change).
5. Local node (if any) joins via the existing in-process `manager.join(...)`.
6. Each remote node joins via new **`goat/app/fiery_pit_peer_client.py`** —
   a plain httpx POST to `{base_url}/ritual/join` with the peer token.
7. Rollback extended: on any failure, already-joined remote FieryPits are
   *not* told to leave (`DELETE /ritual/{id}` is intentionally user-JWT-only,
   so a peer token can't call it) — logged as a known limitation, matching
   the existing accepted gap that normal deactivation doesn't cascade a leave.

### Part C — lildaemon: the Run endpoint
- **`goat/app/dis_client.py`** — new `start_deduce()` (`POST /deduce`) and
  `poll_deduction()` (`GET /deduce/{id}`, async bounded backoff — same
  algorithm as `kindling_evaluator.py`'s sync poller, reimplemented async
  since this runs inside a FastAPI handler, not an evaluator's thread).
- **`goat/app/ritual_configs/router.py`** — new `POST
  /ritual-configs/{id}/run` (body: `{query}`): 409 if not active, 400 if no
  entry-node `prolog_source_id`, builds
  `current_context(C), reasoned_response(<query>, C, R)` via the existing
  `prolog_quote_atom`, starts and polls the deduce, returns
  `{deduction_id, status, response}` (200 even when not converged — this is
  UI-facing, so the frontend renders `status` rather than parsing an error
  body, unlike the internal evaluator-dispatch convention).

### Part D — dagda/Cobbler backend
`cobbler/backend/routers/ritual_configs.py` — new `POST
/api/ritual-configs/{id}/run`, thin passthrough matching the existing
activate/deactivate pattern exactly.

### Part E — dagda/Cobbler frontend
- `api/client.ts` — `runRitualConfig(id, query)`.
- New `RunRitualDialog.tsx` (follows `FlowKindDialog.tsx`'s backdrop/panel
  pattern) — query textbox → Run → in-place result view (response text +
  deduction_id) → Close.
- `RitualEditorCanvas.tsx` toolbar — new "▶ Run" button when
  `config.status === 'active'`.
- `NodePropertiesPanel.tsx` — relabeled "URL" → "FieryPit URL (optional)"
  with a hint explaining blank = local, set = remote FieryPit. No new field.
- Fixed a small pre-existing gap needed for the new activation validation to
  be visible at all: Activate/Deactivate handlers were silently swallowing
  errors (`.catch(() => {})`) despite `gce-toolbar__error` already existing
  in the JSX for exactly this purpose.

---

## Testing

**Unit tests (new, 27 total):**
- `tests/test_peer_auth.py` (4) — peer token accepted; wrong/missing token
  still requires a real JWT; valid user JWT still authenticates normally.
- `tests/test_partition_nodes.py` (7) — local/remote partitioning, blank vs.
  self-matching vs. different URL, multiple-local-nodes error, empty input.
- `tests/test_dis_client.py` (+8) — `start_deduce`/`poll_deduction`:
  converged-on-first-poll, backoff-until-converged, timeout.
- `tests/test_ritual_config_router.py` (+8) — Run endpoint: draft→409,
  missing-source→400, converged→response extracted, non-converged→status
  passed through, empty query→422, auth required.

**Full lildaemon suite:** 897 passed (no regressions), both before and
after live verification.

**Regression catch:** the initial multi-node rewrite broke activation for
configs with no `graph_layout` at all (legacy `RitualConfigForm` path) —
caught by the pre-existing `test_activate_draft_returns_active` test
failing, not by new tests. Fixed via the legacy-fallback described in Part B.

## Live end-to-end verification

Stood up the real stack against **real Kafka** (the `docker-kafka-1`
container, not `InMemoryBroker`/dev fallback) — clara-api (Dis) with
persistence temporarily enabled (reverted after), plus two genuinely
separate lildaemon processes on different ports with separate DuckDB files.

1. **Single-node regression:** created + activated a one-node config
   (blank `url`), confirmed `prolog_source_id` populated, called the new Run
   endpoint — converged with a real Ollama-backed LLM response ("Four.").
2. **Multi-node, cross-process:** created a two-node config — one blank
   `url` (local), one pointing at the second lildaemon process's port.
   Activation succeeded; confirmed **both** processes independently list the
   same `ritual_id` via `GET /ritual` (each with its own JWT) — the second
   process's log shows the peer-token `POST /ritual/join` call landing,
   `RitualParticipant` starting, and its consumer loop running.
3. **Validation:** a two-local-node config correctly failed activation with
   409 and the exact planned message.
4. **Run on the multi-node ritual:** converged with a real LLM response,
   confirming Run's behavior is unaffected by additional joined peers.

All test/verification data (users, ritual configs, scratch DB files, the
`data/coire.duckdb` artifact from temporarily-enabled persistence) was
cleaned up afterward; `config/default.toml` reverted to a clean `git status`.

## Known gaps / limitations discovered

- **Kafka consumer-group targeting:** `RitualParticipant`'s consumer
  `group.id` is `ritual-{ritual_id}-{dis_domain}` — shared by every
  participant of the same Ritual, by design, for horizontal scaling of
  *redundant* instances of the same evaluator (existing, pre-existing
  behavior, not introduced here). This means if the entry node's Prolog
  source is ever authored to `coire_publish(evaluator/...)` to reach a
  *specific* peer node by name (true cross-node routing), Kafka's
  competing-consumers semantics could deliver that message to either joined
  participant, not reliably the intended one. **Not exercised by this
  phase** — the default `reasoned_response/3` template never calls
  `coire_publish`, so Run's basic flow doesn't hit this. It's a real
  constraint on any future edge-qualifier-driven routing work (already
  out of scope per Phase 1/2), surfaced here for visibility rather than
  solved.
- No cascading "leave" to remote FieryPits on deactivation or on
  activation-rollback (flagged in the plan as explicitly out of scope;
  `DELETE /ritual/{id}` intentionally stays user-JWT-only).
- No service discovery/autoscaling — remote FieryPit URLs must already be
  reachable and pre-configured with the same `DIS_DOMAIN_PEER_TOKEN`,
  `dis_url`, and Kafka bootstrap.
- Cobbler backend proxy route (Part D) wasn't independently exercised live
  in this pass (no headless browser, and standing up the Cobbler
  backend/frontend dev servers was out of scope for this verification round)
  — it's structurally identical to the already-verified activate/deactivate
  proxies, so risk is low, but it's not click-tested.

## Explicitly out of scope (unchanged from the plan)

- Edge-qualifier-driven routing/evaluation between nodes.
- Cascading "leave" to remote FieryPits on deactivation.
- Service discovery, autoscaling, or spawning new FieryPit processes.
- Any change to clara-api/clara-cycle (Rust side).
- Peer auth for `GET /ritual` (list) or `DELETE /ritual/{id}` (leave).

## Files changed

```
lildaemon/
  goat/app/users/auth.py
  goat/app/ritual/router.py
  goat/app/ritual_configs/router.py
  goat/app/ritual_configs/models.py
  goat/app/dis_client.py
  goat/app/fiery_pit_peer_client.py            (new)
  .env.example
  tests/test_peer_auth.py                      (new)
  tests/test_partition_nodes.py                 (new)
  tests/test_dis_client.py
  tests/test_ritual_config_router.py

dagda/
  cobbler/backend/routers/ritual_configs.py
  cobbler/frontend/src/api/client.ts
  cobbler/frontend/src/components/GraphCanvas/RunRitualDialog.tsx   (new)
  cobbler/frontend/src/components/GraphCanvas/RitualEditorCanvas.tsx
  cobbler/frontend/src/components/GraphCanvas/NodePropertiesPanel.tsx
  cobbler/frontend/src/components/GraphCanvas/GraphCanvas.css
```

Nothing has been committed yet in any repo — this write-up is for team
review before that happens.
