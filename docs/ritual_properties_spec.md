# Ritual properties: durability, persistence, lifecycle (working draft)

_Drafted 2026-09-24 from Stan's feedback on `frontdesk_analyst_consolidation_brainstorm.md`. Approach A (analysts as
complete Rituals) needs these properties pinned down first. Sections 1-6 are **today's behavior**, verified against the
code with file references; section 7 is **proposed** and explicitly open for Stan. Nothing here is built._

Paths: `cc/` = `clara-cerebellum/`, `ld/` = `lildaemon/goat/`.

## 1. The objects and their states

| Object | States today | Persisted where | Survives a restart? |
|---|---|---|---|
| **Dis Ritual** | `Active`, `Terminated` (`cc/clara-ritual/src/ritual.rs:6-9`) | Write-through to the CoireStore (DuckDB) `rituals` table, incl. participants map (`cc/clara-ritual/src/registry.rs`) | Yes if a store is configured; `restore_from_store` reloads on boot and re-ensures Kafka topics for active rituals (`registry.rs:252`). Memory-only without a store |
| **Performance** | **No state machine, and not observable:** the `performance_id` minted at join is never stored on the deduction entry or snapshot (`cc/clara-api/src/handlers/deduce_handler.rs:119-136`). A `performance_id` is minted per join, idempotent per participant key (`registry.rs:111-139`); anonymous joins aren't recorded | Keyed joins only | Ids stable across boot only for keyed joins |
| **Deduction** (nearest thing to a "run") | `Running`, `Converged`, `Interrupted`, `Error` (`cc/clara-cycle/src/result.rs:22-27`); max-cycles exhaustion is only the string `"error: Max cycles (N) exceeded..."` (`deduce_handler.rs:184-189`); no `pending` (start returns 202 "running") and no `expired` | In-memory `AppState.deductions`; durable only with `persist:true` (a `DeductionSnapshot`) | No, unless `persist:true` and a store is configured (silently ignored otherwise) |
| **RitualConfig** (lildaemon) | `draft` -> `active` -> `terminated`; **no way back to draft** (`ld/app/ritual_configs/store.py`) | DuckDB `ritual_configs` table | Definition yes; runtime no, and restart also kills the Dis ritual (§3) |
| **Participant / seat** | Joined imperatively; `RitualManager` holds `(ritual_id, node_id)` participants in a plain dict (`ld/models/RitualManager.py`) | Memory | No |

## 2. Reaping and TTLs

- Terminated Rituals are swept by CarrionPicker pass 5, which **reuses the snapshot TTL** (default 7 days) rather than
  a dedicated ritual TTL field (`cc/clara-coire/src/carrion_picker.rs:237-250`). **Active Rituals are never reaped.**
- In-memory deduction entries: `deduction_entry_ttl_seconds` = 3600, swept every 300s; 0 disables
  (`cc/clara-api/src/handlers/session_handler.rs`). Snapshots: 604800s. Evaluate cache: 14400s. Defaults in
  `cc/clara-config/src/defaults.rs:56-69`. Persistence is off by default.
- Per-message `ttl_ms` defaults to 60000.

## 3. Definition durability vs runtime durability

A RitualConfig's *definition* is durable, but its *runtime* is not. On lildaemon startup, every config still
`active` is force-terminated, and shutdown does the same (`ld/app/main.py:374-393`, `:566-584`), both through
`terminate_config` (`ld/app/ritual_configs/lifecycle.py:28-70`). That call **also deletes the Dis ritual**
(`delete_ritual` -> `registry.terminate()`, `cc/clara-ritual/src/registry.rs:179-194`), and a terminated Dis ritual
**rejects joins** (`registry.rs:123-128`), so the old `ritual_id` can never be reused. `terminated` has no exit: the
DB CHECK allows only `draft/active/terminated` (`ld/app/ritual_configs/store.py:32-33`) and activation is allowed only
from `draft` (`ld/app/ritual_configs/router.py:302-306`). For a long-lived analyst Ritual (the goal of A) a restart
therefore silently ends the partner, and its `ritual_id` with it.

What is *already* durable, if we simply stop terminating (verified):
- **Dis side.** A keyed `join` is idempotent and reuses the same `performance_id` (`registry.rs:115-160`);
  `restore_from_store` reloads rituals and the participants map and re-ensures topics (`registry.rs:245-310`, called at
  `cc/clara-api/src/server.rs:142`; test `keyed_join_performance_id_survives_restart`, `registry.rs:529`).
- **Python participants.** Consumer group `ritual-{ritual_id}-{node_id}`, `auto.offset.reset=earliest`, auto-commit
  (`ld/models/RitualParticipant.py:182-184`): a re-join with the same ids resumes from the last committed offset, so
  delivery is at-least-once and messages published while down are picked up.
- **Everything needed to rebuild a config's runtime is in its row:** `graph_layout` (nodes, `evaluatorName`,
  local/remote split, participant keys `self_url#node_id`, entry node), the `kafka_bootstrap`/`eval_timeout_s`
  columns, and the retained `ritual_id`.

What is *not* durable or not reusable today:
- **Activation is not idempotent**: 409 unless `draft`, a new Dis ritual per call, and `RitualManager.join` raises on a
  duplicate `(ritual_id, node_id)` key (`ld/models/RitualManager.py:58`). Only the entry node's source ids are
  persisted; per-node ids are not.
- **Remote peers**: re-joining a peer that is still up returns 409 "already joined"
  (`ld/app/ritual/router.py:113`); a peer that restarted has lost its state. There is no `leave_remote`.
- **Hermes seats** start lazily on first evaluate, are bound to one Ritual, and are reaped if their lease lapses
  (`lildaemon/seat_launcher/seats.py:347-349`); session continuity is keyed by ritual id
  (`lildaemon/seat_launcher/continuity.py`). _(Not traced end to end; to verify.)_
- **In-flight deductions/Performances and outstanding offers** live in memory and are lost (see §1, §4).

## 4. Bounds on a run

- **Run loop** (`cc/clara-cycle/src/controller.rs:324-453`): `for cycle in 0..max_cycles`; each cycle runs the Prolog
  pass, relay, CLIPS pass, relay, evaluator pass, then the convergence check. The interrupt flag is read **only at the
  end of a cycle** (`controller.rs:421`), and `converged` wins if both hold. `max_cycles` (default 100,
  `deduce_handler.rs:41`, no server-side maximum) is enforced by the loop bound alone.
- **Pacing:** a blocking 250 ms `std::thread::sleep` (`controller.rs:634-637`) runs only while offers are pending and
  nothing was ingested; it has no cancel check.
- `evaluator_patience_cycles` defaults to 10 and is **per offer**, not a run bound; expiry injects a
  `ritual/tabu-timeout` (`controller.rs:1179-1206`).
- Callers already pick very different points: RitualConfig Run uses `max_cycles` 100 with
  `patience = max(10, eval_timeout_s/0.25)` (`ld/app/ritual_configs/router.py:620-655`); the assistant deliberation
  uses patience 3500 and `max_cycles` 4000 (`ld/app/assistant/runtime.py:212-226`).
- **There is no wall-clock deadline anywhere in the deduction path.** The only durations are the 250 ms sleep and
  actix's 30 s connection timeout (`cc/clara-api/src/server.rs:186-201`). A deduction blocked inside a Prolog/CLIPS FFI
  call cannot be interrupted until it returns (no timeout/cancel hook found by grep; FFI internals not read).
- `interrupted` today means only an explicit `DELETE /deduce/{id}` (`deduce_handler.rs:563-584`), which optimistically
  flips the entry before the run confirms; the background task later overwrites the final status (`:172-201`).
- **Nothing reaps a `Running` entry.** The reaper (`session_handler.rs:107-146`) evicts only non-running entries, aged
  from *creation*. A client that abandons a poll (`ld/app/dis_client.py:226-`: `poll_deduction` returns
  `timed_out` and never cancels) leaves the run going until it converges or exhausts `max_cycles`.

## 5. Composition and shared code

- **No flow creates a child Ritual today.** What the docs call a "Ritual of Rituals"
  (`lildaemon/docs/assistant_demo.md:1246`, `id_ritual_of_rituals_planning.md`) is **one standing ritual plus more nodes
  plus background `/deduce`s**: committee referral is a `caws_offer` to snek/edgequakeingest already in the same ritual
  (`ld/app/assistant/rulesets/id_analyst.pl:142-143`); `_ensure_standing_ritual` creates `assistant-demo` once and never
  terminates it (`ld/app/assistant/runtime.py:498-577`); `_ensure_deliberation_seats` (`:579`) joins seats onto it lazily
  and never leaves. `peer_consult` (`ld/models/RitualParticipant.py:307-313`) is intra-ritual despite the "nested"
  wording. The only parent/child-ish link is the async queue `assistant_pending_research`
  (`ld/app/assistant/research_queue.py:70-105`, advanced by `advance_pending_research` at `:462`), and nothing cancels
  child work when a parent deduction ends (`cancel_deduction`'s only callers are `mcp/ego_gate/superego.py:173,179`).
- **No cross-ritual addressing, no parent/child fields.** A Ritual is one precomputed topic
  (`cc/clara-ritual/src/ritual.rs:31-41`: id, config, state, topic, participants). `target_node_id` is matched only
  against the participant's own node id on its own topic (`RitualParticipant.py:236-246`).
- **`ritual-group` is presentation-only.** Cobbler defines it with optional `ritualConfigId`/`ritualId`
  (`dagda/cobbler/frontend/src/components/GraphCanvas/types.ts:107-116`), but its only creator
  (`RitualEditorCanvas.tsx:75-90`) sets neither and merely reparents daemons (a Cytoscape compound node). Activation and
  Run select only `type == "daemon"` nodes with an `evaluatorName` (`ld/app/ritual_configs/router.py:328-330,577`).
- **Orphans would accumulate.** Terminated rituals are swept after the snapshot TTL
  (`cc/clara-coire/src/carrion_picker.rs:236-250`); active ones never are.
- **One flat source per node.** Edge snippets plus the authored `prologSource` are joined and registered as a single
  content-addressed source via `register_source` (`router.py:158-190`).
- **The registry** (`cc/clara-api/src/handlers/source_handler.rs`, `cc/clara-coire/src/source.rs`, table at
  `cc/clara-coire/src/store.rs:173-183`) is a DuckDB `source_registry`, content-addressed by SHA-256 of the content
  only (the label is not hashed), unique on `(content_hash, source_type)`. `source_type` is **not validated**, so a
  new type needs no server change to be stored. A duplicate registration returns the existing id and **does not
  refresh its TTL**; rows with a NULL expiry are never reaped (CarrionPicker pass 4, `carrion_picker.rs:226`).
- **How a source is loaded** (`cc/clara-api/src/handlers/deduce_handler.rs:596-639` -> `cc/clara-cycle/src/session.rs:72`
  -> `consult_string`, `cc/clara-prolog/src/backend/ffi/environment.rs:363-400`): a `read_term` loop that `assertz`s
  each clause into module **`user`**, declaring each `F/A` `thread_local` and `retractall`ing it on first sight. Loaded
  code is therefore **per-engine isolated but lives in one flat namespace**. The same `F/A` arriving from a later
  source in the *same* call **silently merges** (clauses append); a *separate* call would `retractall` the earlier
  one. `:- G` directives run as `ignore(call(G))`, so **`:- module(...)` is swallowed** (no module is created) and
  `use_module`/`consult` facts also run via `ignore(call(...))`.
- **A missing `prolog_source_id` only logs a warning** and falls back to inline clauses (`deduce_handler.rs`).
  Acceptable for a node's own source; wrong for a dependency.
- **Only singular `prolog_source_id`/`clips_source_id` exist on `/deduce`**; nothing lists several sources.
- **Cost:** each deduction builds a fresh engine and re-reads and re-parses its sources; there is no cache.
- **Real SWI modules are process-global** (non-`thread_local` predicates are shared across engines), and all `PL_call`
  must run on the main-engine thread (`environment.rs:59-62`), so loading true modules is safe only for stateless code.
- **Shared libraries are a hardcoded list**: `["the_coire","the_rabbit","the_cow","the_rat","the_leannan"]`
  (`cc/clara-prolog/src/backend/ffi/environment.rs:115`), copied into SWI's library dir by `build.rs`. Authored
  source may `use_module(library(...))` only what that overlay ships. There is **no per-Ritual or per-node module
  dependency mechanism**. This is exactly why promoting boilerplate (Approach B) meant a clara-cerebellum rebuild and
  redeploy each time, and it is the concrete form of Stan's Rituals-of-Rituals question.

## 6. How participants are assembled today

_Note:_ a FieryPit slots evaluators by bare `node_id`, so two rituals reusing a node id share one evaluator; this is why
`ego_node_ids(ritual_id)` tags ids per ritual (`ld/app/assistant/runtime.py:246-250`, found live 2026-09-21).

Nothing composes a roster declaratively. Each caller does `dis_client.join_ritual` plus `RitualManager.join`:

- RitualConfig activation joins local nodes (`router.py:373-410`) and remote ones via
  `fiery_pit_peer_client.join_remote` (`router.py:412-430`).
- `_ensure_standing_ritual` (`ld/app/assistant/runtime.py:498-577`) joins snek, edgequakeingest and groq-splinter
  once under a lock; `_ensure_deliberation_seats` (`:579`) adds chair/members/cow lazily.
- `_ensure_ego_ritual(session_id)` (`:1100`) creates one Ritual per session with `hermes_ego` plus a reviewer.
- `HermesAgentEvaluator._ensure_seat` starts a seat lazily on first evaluate, bound to one Ritual (409 otherwise),
  with a renewed lease, released when idle (`ld/evaluators/custom/hermes_agent_evaluator.py`,
  `ld/ritual_space/scope.py:63`).

## 7. Target properties (P1-P4 decided 2026-09-24; P5 proposed; ordering in §9)

Each is a proposal, not a decision.

**P1. A bounded, observable Performance (decided 2026-09-24).** Two decisions from Stan: **`expired` is a distinct,
resumable terminal state**, and **the deadline is layered: request > RitualConfig default > server ceiling**.

- **State set.** `running`, `converged`, `interrupted`, `error`, **`expired`**. `pending` is dropped (there is no queue)
  and the wire string stays `error` rather than being renamed `failed` (a rename breaks clients for no gain). Replace
  the stringly-typed max-cycles error with a structured **`reason`** on non-converged results (`max_cycles`,
  `deadline`, `interrupted`, `error:<msg>`), keeping today's human string for compatibility.
- **`deadline_ms`.** An optional wall-clock budget from deduction start on `DeduceRequest`
  (`cc/clara-api/src/models/request.rs:105-185`). Resolution: the request value, else the RitualConfig
  `default_deadline_ms` (new column via `_MIGRATIONS`, used by Run), else a server default; a configured
  **`max_deduction_deadline_ms` ceiling clamps every case**, so no run is eternal. Presets: one-shot = small cycles +
  short deadline; long = huge cycles + long deadline.
- **Enforcement is cooperative.** Check the deadline at the top of each cycle and beside the interrupt check, and
  **cap the 250 ms pacing sleep to the remaining budget**. Precedence at a cycle boundary: `converged` >
  `interrupted` (explicit) > `expired`. State plainly that a call stuck inside Prolog/CLIPS FFI overruns until it
  returns, so the effective bound is the deadline plus the longest single call plus one pacing sleep.
- **`expired` semantics.** Stops at the next cycle boundary and returns the partial result. With `persist:true` it
  saves a snapshot so **`/deduce/resume` continues it** (`deduce_handler.rs:259-455` already overrides `max_cycles`;
  add a `deadline_ms` override, and an optional `#[serde(default)]` `deadline_ms` on `DeductionSnapshot`,
  `cc/clara-coire/src/store.rs:21-64`; its `status` is a free string, so the new state needs no schema change). A later
  optimistic flip must not clobber the final status; fix the DELETE-versus-task race when implementing.
- **Make a Performance observable.** Store `ritual_id`, `performance_id`, `max_cycles`, `deadline_ms` and `started_at`
  on `DeductionEntry` (`session_handler.rs:29-38`) and the snapshot, and return them from `GET /deduce/{id}`. This is
  the smallest step to a first-class Performance without inventing a new object.
- **Reaper.** Measure a terminal entry's age from **completion**, not creation (today it can evict a long run's entry
  soon after it finishes). Running entries are bounded by the deadline ceiling instead.
- **Client alignment.** Callers pass a `deadline_ms` matched to their own poll budget (Run: derived from
  `eval_timeout_s`; assistant runtime: per-mode constants), and `poll_deduction` should send `DELETE` when its budget
  lapses so an abandoned poll stops the server-side run.
- **Compatibility.** Every addition is optional and additive. `expired` is a new terminal status that existing callers
  already treat as non-converged (they check for `converged`).
- **Would touch:** `request.rs`, `DeductionEntry`, `controller.rs` (deadline + sleep cap), `deduce_handler.rs` (layered
  resolution, status/reason, resume override), `store.rs`, `clara-config` (default and ceiling), `session_handler.rs`,
  lildaemon `dis_client.py`, `ritual_configs/{store,router}.py`, `assistant/runtime.py`. Tests needed: expiry at a
  cycle boundary, sleep cap, precedence, resume-after-expired, ceiling clamp, reaper-from-completion.

**P2. Persistent RitualConfigs survive a restart (decided 2026-09-24).** Separate definition durability from runtime
durability by re-attaching, not re-creating. Two decisions from Stan: an **opt-in per-config flag, default off**, and
**eager background resume on boot plus a manual operator endpoint**.

- **`persist` flag.** A new boolean column (default false) added through `_MIGRATIONS` (`store.py:51-55`), exposed on
  the config model and API. It follows the per-request `persist:true` precedent on `/deduce`
  (`ld/app/assistant/runtime.py:785-794`). Non-persistent configs keep today's terminate-on-restart behavior unchanged.
- **Shutdown and boot converge.** For a persistent config, shutdown **does not** delete the Dis ritual and leaves the
  row `active`; a crash leaves the same state. Graceful restart and crash then share one recovery path. Shutdown only
  stops local consumers.
- **`runtime_state` in memory, `status` unchanged.** Add `resuming | live | degraded`, surfaced by GET and enforced by
  Run (refuse with 409/503 unless `live`). This keeps the DB CHECK constraint and needs no migration on `status`.
- **`resume_config(id)`** (new, in `lifecycle.py`, distinct from activation). Requires `status=active` and `persist`;
  **reuses the stored `ritual_id`** (required for Dis participant ids, Kafka offsets and Hermes session continuity).
  Per local node: re-`join_ritual` (idempotent, same key and `performance_id`), re-`spawn_evaluator`, `manager.join`.
  Per remote node: `join_ritual` + `join_remote`, treating **409 already-joined as attached**. Re-register node sources
  (content-addressed, so idempotent; do not rely on sources surviving a clara-api restart, which is unverified). If Dis
  reports the ritual terminated or missing, fall back to today's `terminate_config` and mark the config `terminated`.
- **Eager background resume.** On boot, spawn a background task (never block startup: today the reconcile loop is
  awaited inline, `main.py:380-392`) that resumes each persistent active config with per-config try/except and
  bounded retry with backoff, since remote peers have no compose ordering
  (`cc/docker/docker-compose.yml:288-290` only orders `lildaemon` after `clara-api`). When retries are exhausted it
  falls back to `terminate_config`. Non-persistent active configs are terminated exactly as today.
- **Manual endpoint.** `POST /ritual-config/{id}/resume` runs the same `resume_config` for operators.
- **What resume does not preserve.** In-flight deductions/Performances (that is P1's `persist:true` snapshot and
  `/deduce/resume`); outstanding offers are redelivered at-least-once; a Hermes seat is recreated lazily on the next
  evaluate, and its continuity holds only because the ritual id is preserved.
- **Tests needed.** `tests/test_ritual_config_lifecycle.py` covers `terminate_config` only and nothing exercises the
  `main.py` startup loop. Resume needs: idempotent re-join, 409-as-attached, terminated-Dis fallback, peer-down retry,
  and a startup-loop test.
- **Deliberately left out:** a `terminated -> draft` reset for non-persistent configs. It remains a separate small
  question; resume avoids needing it.

**P3. Rituals of Rituals (decided 2026-09-24).** Two decisions from Stan: a child Ritual appears to its parent as
**one participant**, and lifecycle is **owned by default, borrowed opt-in**.

- **Child as one participant.** A new evaluator kind (working name `RitualEvaluator`) in the parent wraps a child
  RitualConfig. An Offering to it starts a **Performance on the child's entry node** (the fixed entry-goal contract in
  `router.py`) and returns the result as Hohi; failure or `expired` returns Tabu. This reuses the participant/edge model
  and needs **no cross-ritual addressing**. The child's internal nodes are unreachable from outside by design; only its
  edge metadata is exposed.
- **Declaration.** A `ritual-group` node becomes real: `ritualConfigId` references a child config (validated at save)
  and `ownership: owned | borrowed` (default `owned`) selects the coupling. Persist a
  `ritual_config_children (parent_id, node_id, child_id, ownership)` link table rather than a parent column, since a
  borrowed child may have many parents. Cobbler's group creation must set `ritualConfigId`.
- **Owned (default).** Activating the parent activates its owned children first, each in its own Dis ritual; a failure
  rolls back the children already activated (and closes the remote-join leak in today's rollback, `router.py:436-462`).
  Terminating the parent terminates the children **it activated**. A child that was already active when the parent
  activated is treated as borrowed and left running.
- **Borrowed (opt-in).** The parent requires the child to be `active` and `live` (P2's `runtime_state`); a missing or
  detached child fails activation and Run with a clear error. Terminating the parent never touches it.
- **With P2 (persist).** A persistent parent requires its owned children to be persistent; `resume_config` recurses
  children-first with the same retry and backoff, and a child that cannot resume marks the parent `degraded`.
- **With P1 (deadline).** The child Performance's deadline is `min(the Offering's deadline, the parent's remaining
  budget)`. When the parent expires or is interrupted it **cancels the in-flight child deduction**
  (`DELETE /deduce/{id}`), which nothing does today. A child `expired` reaches the parent as Tabu with
  `reason: deadline`.
- **Graph validity.** The parent-to-child reference graph must be a **DAG**: reject cycles and self-reference at save
  and at activation. The child has its own `ritual_id`, so evaluator slots stay distinct; reuse the per-ritual node-id
  tagging pattern (§6) where a slot name could otherwise repeat.
- **With P4 (modules).** Unchanged: each node deduces on its own engine, so parent and child module sets cannot collide.
- **Orphan safety net.** Active rituals are never reaped, so owned-child teardown must be part of parent terminate and
  rollback. Children are configs too, so today's terminate-on-restart for non-persistent configs already covers them.
- **Non-goals for now:** cross-ritual addressing; non-config (imperatively created) children such as the standing
  assistant ritual; dynamically spawned children. The existing "Ritual of Rituals" assistant flows keep working as they
  are on one ritual; moving them onto real parent/child is a separate later decision.

**P4. Ritual-scoped module dependencies.** Requirements: no clara-cerebellum rebuild per shared predicate;
**version-controlled and testable**; specified before graph or lifecycle work is instantiated. The design is
two-tier; build Tier 1 first.

*Tier 1 - flat "module fragments" (recommended first step).* A module is a registered source of a new type
`prolog-module`, whose clauses are loaded into the node's own namespace **before** the node source, in declared order.
This fits how loading already works (§5) and needs no FFI change.

- **Declaration.** A RitualConfig node (optionally with a Ritual-level default) carries
  `prologModules: [{name, version, sha256}]`. lildaemon registers each module and `/deduce` gains an ordered
  `prolog_module_source_ids`. **Name-to-hash pinning lives in the RitualConfig/manifest, not in Dis**: Dis's label is
  neither unique nor hashed, so Dis stays a plain content store.
- **Version control.** Module sources live in git (lildaemon, e.g. a `ritual_modules/` directory) beside a checked-in
  **lockfile** of `name, version, sha256`; a test fails if a file's hash drifts from the lock. Pinning by content hash
  makes a Ritual reproducible, and each version immutable.
- **Collision rule (the key hazard).** Same-`F/A` clauses merge silently in one namespace, so loading must **fail**
  when two dependencies, or a dependency and the node source, define the same `F/A`, or when a dependency redefines a
  name the compiled-in overlay exports, unless the node source explicitly opts in to an override.
- **A missing dependency is a hard error**, not today's warn-and-fall-back.
- **TTL.** Register module sources with no expiry (or refresh on activation): a duplicate registration will not extend
  an existing row's TTL, and the sweeper would otherwise delete a live dependency.
- **Rituals of Rituals.** Each node deduces on its own engine, so a child Ritual's modules cannot collide with the
  parent's; collisions exist only inside one node's dependency set. Composition needs no cross-Ritual module logic.
- **Coexistence.** The compiled-in overlay stays the always-present core (`the_coire`, `the_rabbit`, ...); registered
  modules are additive and Ritual-scoped. Nothing is superseded.
- **Testability.** Test a module with a harness that registers *only* that module plus a probe clause and asserts its
  predicates resolve (the pattern of `lildaemon/tests/test_approach_b_promotion.py`). Authoring rule: modules are
  clause-only; directives are limited to `use_module` of overlay libraries; no `:- module`.

*Tier 2 - true hash-named modules (deferred).* Load pure, stateless modules once per process on the main engine as
`m_<hash>` and import their exports into `user`. This adds real namespacing and caching (removing the per-deduction
re-parse) but needs main-engine marshalling and a stateless-only rule. Pursue only if flat-namespace collisions or
parse cost prove painful.

_Assembly brainstorm (2026-09-24, input only, not a decision):_ run past Clara's deliberative assembly, it mostly
restated this doc. Two points were kept: the module mechanism must be versioned and testable, and it should be
specified before graph/lifecycle instantiation. Its suggestion to supersede the compiled-in overlay was not adopted;
the overlay stays as the core (above).

**P5. Declarative roster.** Replace per-caller imperative joins with a roster on the RitualConfig that a Performance
resolves ("bring together the needed participants"), covering native Clara evaluators, Hermes seats and remote
FieryPits uniformly. Id/Ego/Superego stay provided natively or by Hermes; ad hoc Rituals name resources resolved
at Performance time.

**Spec shape (unchanged from Stan):** graph of nodes, edges with augmented metadata, and application code for
deduce-backed ("devil") nodes; Prolog plus generated CLIPS now. The typed superset and CLIPS generation stay out of
scope until P1-P5 settle.

## 8. Decisions needed

1. ~~P4: confirm Tier 1 (flat fragments + lockfile + collision check) as the first step, with Tier 2 deferred?~~
   **Confirmed by Stan 2026-09-24:** Tier 1 first; Tier 2 deferred.
2. ~~P2: which Ritual configs should survive a restart, and is re-activation automatic or operator-driven?~~
   **Decided by Stan 2026-09-24:** per-config `persist` flag (default off); eager background resume on boot plus a
   manual resume endpoint.
3. ~~P1: is `expired` a terminal state distinct from `interrupted`, and who owns the deadline?~~
   **Decided by Stan 2026-09-24:** `expired` is distinct and resumable; deadline layered request > RitualConfig
   default > server ceiling.
4. ~~P3: should child Ritual lifecycle be tied to the parent's?~~
   **Decided by Stan 2026-09-24:** child appears as one participant; owned by default, borrowed opt-in.

_All four decisions are now made. The next planning step is an implementation-ordering pass across P1-P5._

## 9. Implementation ordering (agreed 2026-09-24)

Vertical slices, each built, tested, redeployed and **verified live** against the real stack before the next. Only S1 is
fixed as first; the rest is the recommended order and is revisitable after S1.

Dependencies: S1 -> S2 -> S3. S4, S5, S6 are independent of P1 and of each other. S7 needs S1-S4 and S6.

| # | Slice | Repo / language | Depends on | Deploy |
|---|---|---|---|---|
| **S1** | **P1a: `deadline_ms`, `expired`, structured `reason`.** Request field; layered resolution (request > server default) with a server ceiling clamp (`clara-config`); check at the top of each cycle and beside the interrupt check; cap the 250 ms pacing sleep to the remaining budget; precedence converged > interrupted > expired; new `CycleStatus::Expired` (`request.rs`, `result.rs`, `controller.rs`, `deduce_handler.rs`, `clara-config/defaults.rs`) | clara-cerebellum / Rust | none | rebuild + redeploy `clara-api` on limbic |
| **S2** | **P1b: observable Performance and hygiene.** `ritual_id`/`performance_id`/`max_cycles`/`deadline_ms`/`started_at` on `DeductionEntry` and the snapshot, returned by `GET /deduce/{id}`; reaper ages from completion; fix the DELETE-versus-task status race; `/deduce/resume` deadline override plus optional snapshot `deadline_ms` | Rust | S1 | redeploy `clara-api` |
| **S3** | **P1c: client and config alignment.** `poll_deduction` sends `DELETE` when its budget lapses; Run and `assistant/runtime.py` pass a matching `deadline_ms`; RitualConfig `default_deadline_ms` column used by Run | lildaemon / Python | S1 | lildaemon image rebuild (image bakes `.env`: blank unset compose vars) |
| **S4** | **P2: persistent RitualConfigs.** `persist` column, in-memory `runtime_state`, `resume_config`, background resume with retry/backoff and terminate fallback, `POST /ritual-config/{id}/resume`, shutdown leaves persistent configs `active`, Run gated on `live` | lildaemon / Python | none | lildaemon rebuild |
| **S5** | **P4 Tier 1: module fragments.** Rust: ordered `prolog_module_source_ids` on `/deduce` and resume, hard error on a missing dependency, collision check (same `F/A` across sources, or against overlay exports). lildaemon: `ritual_modules/`, lockfile and hash-drift test, no-expiry registration, `prologModules` on nodes through activation. The slice's own plan must decide where the collision check lives (`consult_string`'s `$cs_seen` versus a pre-parse) | both | none (batch the Rust deploy with S1/S2 if timing allows) | redeploy `clara-api` + lildaemon rebuild |
| **S6** | **P3a: composition data model.** `ritual_config_children` table; `ritual-group` `ritualConfigId` + `ownership` validated at save; DAG check; owned-child activate/terminate/rollback cascade, releasing remote joins on rollback. No `RitualEvaluator` yet | lildaemon / Python | none | lildaemon rebuild |
| **S7** | **P3b: `RitualEvaluator`.** Wrap a child config as one participant; start a Performance on its entry node; Hohi/Tabu mapping; deadline = min(offer, parent's remaining); cancel the in-flight child on parent expire/interrupt; recursive resume (children first); `degraded` propagation | lildaemon / Python | S1, S2, S4, S6 | lildaemon rebuild |
| **S8** | **Cobbler.** Group creation sets `ritualConfigId`; ownership control in the properties panel | dagda / TypeScript | S6 | cobbler build |

**Status (2026-09-24):** **S1 built and live-verified** (uncommitted at time of writing): `deadline_ms`, `expired`, `reason`, sleep cap, layered policy (defaults 3600 s / ceiling 14400 s in `PersistenceConfig`, `0` disables), resume bounded by the server default. Live: a 1.5 s deadline on a never-answered offer ended `expired`/`deadline` in 1.81 s with a partial result; `deadline_ms: 0` -> 400; converging runs unaffected; an expired persisted run accepted by `/deduce/resume`. Controller tests: expiry, sleep-cap helper, converged > interrupted > expired precedence. Note: an end-to-end timing assertion for the sleep cap was unreliable under parallel tests (cycle time varies with the shared Coire), so the cap is tested on the pure `capped_wait` helper.

**Status (2026-09-24):** **S2 built and live-verified** (uncommitted at time of writing). `GET /deduce/{id}` now returns `ritual_id`, `performance_id`, `max_cycles`, `deadline_ms`, `started_at_ms`, `completed_at_ms`, `resumed_from`; snapshots persist `ritual_id`/`performance_id`/`deadline_ms` (three new DuckDB columns, migrated live on the existing store; `store.rs` read paths refactored onto one column list + row mapper). `DELETE /deduce/{id}` no longer rewrites `status` (polls report `interrupted` immediately via `effective_status`, the task finalizes once), and the reaper ages entries from completion and never evicts an in-flight run. `/deduce/resume` takes `deadline_ms` (request > snapshot > server default, ceiling-clamped, fresh budget, `0` -> 400) and its final state now sets `reason` too (S1 had missed the resume path). Live: ids present mid-run and after; DELETE -> immediate `interrupted`, finalized 0.1 s later with a result and `completed_at_ms`; expired persisted run resumed with `deadline_ms: 800`, `resumed_from` set, no `performance_id`. Tests: workspace 563 passed / clippy clean; lildaemon 1889 passed.

**Status (2026-09-24):** **S3 built and live-verified** (lildaemon, uncommitted at time of writing). `poll_deduction(cancel_on_lapse=)` cancels a lapsed *blocking* poll (Run, `_run_deduce`; never the pending-queue re-polls); `deadline_ms_for_patience` (2x the patience's wall-clock, 60 s floor) bounds every assistant, superego and participant deduction; RitualConfig gains a nullable `default_deadline_ms` (migrated live on the existing DB; Run falls back to `eval_timeout_s + 30`). **Gap closed that `expired` exposed:** the pending-research pollers treated any non-converged status as "still running", so a run that ended `expired`/`interrupted`/`error` left its row `researching` until Dis reaped the entry; they now raise `DeductionFailed` and the queue marks the row failed promptly (5 kinds). Live: a config with `default_deadline_ms: 1500` Ran to `expired` in 1.6 s; the fallback sent 31000 ms; `default_deadline_ms: 0` -> 422; a real expired deduction failed its pending row at the next advance. Tests: lildaemon 1934 passed (45 new, including real-Dis `test_deadline_s3.py`). Cleared 112 stale `assistant_pending_research` rows first (all delivered/ready/failed, none in flight), per Stan's standing permission. Residuals: the sync `KindlingEvaluator` still does not cancel (the deadline is its backstop); the example scripts post raw and get the server default; `default_deadline_ms` cannot be cleared back to NULL.

**Status (2026-09-24):** **S4 built and live-verified** (lildaemon, uncommitted at time of writing). Persistent RitualConfigs (`persist`, default off) now survive a lildaemon restart: shutdown leaves them `active` and keeps the Dis ritual, boot resumes each in the background with backoff (2, 5, 10, 20, 40, 60, 60, 60 s; `RITUAL_RESUME_BACKOFF_S`) and falls back to `terminated` when the Dis ritual is gone or attempts run out; `POST /ritual-configs/{id}/resume` is the operator path; `runtime_state` (`resuming | live | degraded`, in memory) is on config/status responses and gates Run. The join sequence moved out of `router.py` into `attach.py` (`attach_participants`, shared by activation and resume; the 86 existing config/router/lifecycle tests passed unchanged). Live on limbic: a persistent config came back `live` with the **same `ritual_id`**, Dis state `active` and a working Run after a graceful restart **and** after `docker kill`; the non-persistent config was terminated; with clara-api stopped the config sat `degraded` while resume retried, then went `live` on its own once Dis returned (across a Dis restart); deleting the Dis ritual made resume terminate the config cleanly. Bug found and fixed on the way: `RitualConfigStore.__init__` blindly re-ran `ADD COLUMN ... DEFAULT` for existing columns, which aborts the shared DuckDB connection's transaction, so a **second store on the same connection failed its first query** (82 test errors; would have broken live boot, since the router and `main.py` each build a store); it now only adds missing columns. Tests: lildaemon 1965 passed (31 new). Not done: Hermes seat re-attach (lazy on next evaluate), Cobbler `persist` toggle (S8), `terminated -> draft`. Cleared 4 leftover terminated test configs first (permitted).

**Status (2026-09-24):** **S5 built and live-verified** (both repos, uncommitted at time of writing). P4 Tier 1: a module is a registered source of type `prolog-module` (no expiry), loaded ahead of the node source in one `seed_prolog` call. **Rust:** `the_coire.pl` gains `source_defined_predicates/2` and `overlay_exports/1` (same head extraction as `consult_string`); `POST /deduce` takes `prolog_module_source_ids` and refuses the run (`error`, reason `module_dependency`, before anything loads) on a missing or wrongly typed module, a syntax error, a predicate defined by two sources, or a module redefining an overlay export; `POST /source/check-modules` is the dry run; runs without modules are never checked. **Found and fixed:** a resumed run started from a registered source id came back with **no Prolog program** (the snapshot kept only the request's empty inline clauses); resume now re-resolves the source ids and module ids (snapshot column `prolog_module_source_ids`). **lildaemon:** `goat/app/ritual_modules/` (`<name>@<version>.pl`, git `modules.lock.json`, SHA-256 verify, immutable versions, `python -m goat.app.ritual_modules check|lock`, `RITUAL_MODULES_DIR`), node `prologModules: [{name, version, sha256?}]`, activation registers + dry-run checks and fails with the conflict text, ids flow to participants, remote joins (only when set) and Run. **Live headline:** with the stack up and no rebuild, a new module dropped into a mounted directory was used by two nodes in one config (Run answered `hello_from_a_module:42`); node-vs-module and module-vs-overlay collisions, an unknown version and an edited-in-place module each failed activation with a precise message and reverted the config to draft. Tests: workspace 589 passed / clippy clean; lildaemon 1999 passed. **Hazard found, pre-existing, not changed:** `consult_string`'s `thread_local(F/A)` is process-wide in Dis, so a *node source* that defines an overlay-exported name (e.g. `strip_think/2`) shadows the shared library for **every later deduction until clara-api restarts**; my own test did exactly this and broke unrelated suites until Dis was restarted. Modules are now protected from it; node sources are not. Recommend a follow-up: always check node sources against the overlay exports (a behavior change, so a decision for Stan). Conflict labels come from whichever registration created the content row first (registration is content-addressed).

**Status (2026-09-24):** **S6 built and live-verified** (lildaemon only, uncommitted at time of writing). A `ritual-group` node with a `ritualConfigId` is a reference to a child RitualConfig (groups without one stay drawing aids); `ownership` is `owned` (default) or `borrowed`. Link table `ritual_config_children` (with `activated_by_parent`). Save-time validation (422): unknown/foreign/self/duplicate child, a second owner (one owner, any number of borrowers), cycles (DAG), persistent parent needs persistent owned children. Guards (409): deleting a referenced config, deactivating a child that an active parent references. Activation (`activation.py`, extracted from the router) cascades children first, treats a borrowed or already-active child as not the parent's, and any failure reverts the parent **and** every child it activated (Dis rituals deleted, remote joins released). Terminate cascades only to children the parent activated; remote peers now accept the peer token on `DELETE /ritual/{id}` and `terminate_config` releases remote joins. Responses carry `children` and `referenced_by`. **Live headline:** parent+child activate to two distinct Dis rituals; child deactivate/delete 409; parent deactivate terminates both and leaves no Dis ritual; a parent with an unknown evaluator fails with both back to draft and no ritual id; cycle and second-owner 422; borrowed draft child 409; persistent parent+child both come back `live` with the same ritual ids after a lildaemon restart, and deactivating the parent still cascades. Tests: lildaemon 2030 passed. Not done: `RitualEvaluator` (S7), Cobbler (S8), config-acceptance overlay check (Stan's idea).

**Status (2026-09-24):** **S7 built and live-verified** (lildaemon only, uncommitted at time of writing). A `ritual-group` node with a `ritualConfigId` is expanded (`composition.expand_child_nodes`) into a daemon node hosting the new `ritual` evaluator, so the existing transducer, source registration, participant join and `consult_<label>/2` helpers reach a child Ritual like any peer, with **no Rust change**. The evaluator (`goat/evaluators/custom/ritual_evaluator.py`) runs a Performance on the child's entry node via `ritual_configs/performance.perform()` (extracted from Run, no behavior change): converged -> Hohi `{response, deduction_id, child_config_id}`; expired/failed/child down -> Tabu with `reason`. The child's deadline is capped by the parent participant's `eval_timeout_s` minus 2 s and a lapsed wait cancels the child deduction (S3). **Found and fixed:** wrangler slots are keyed by node id alone, so two active parents using the same node id share one slot and its params; the evaluator now resolves its child from the ritual scope (ritual id + node id -> parent config -> link). `effective_runtime_state` reports a parent `degraded` while any child is not active+live (read-time, heals with the child) and gates Run; `resume_config` refuses to attach a parent until its children are live (the existing backoff is the ordering; a terminated child exhausts to terminate). **Live headline:** parent Prolog `consult_kid/2` reached the child's answer through a real second Dis ritual; child deadline showed as 6000 ms for an 8 s parent; persistent parent+child restart showed parent `degraded` then `live`, Run worked after. Tests: lildaemon 2048 passed. **Known limits:** the parent Performance's own remaining budget is not carried on the envelope (needs a Rust change), and cancellation is cooperative (an FFI-blocking child like Prolog `sleep/1` runs to its end in Dis). Not done: Cobbler (S8).

**Status (2026-09-24):** **S8 built and live-verified** (dagda/cobbler + one lildaemon guard, uncommitted at time of writing) — **this completes the S1-S8 implementation ordering.** Cobbler: an **Add Child Ritual** toolbar action (picker of the user's other configs, owned/borrowed, disabled with a reason for self/terminated/already-used/borrowed-needs-active) adds a *leaf* `ritual-group` node with `ritualConfigId` + `ownership`; edges may now end at a daemon **or** a child-ritual node; a child properties panel (status, ownership selector while draft, Open child, Remove reference); child status refreshed from `config.children`; parent `runtime_state` badge with Run disabled unless live; a `persist` checkbox; lildaemon's `{"error"}` bodies shown as sentences (`errorMessage`); `/resume` added to the backend proxy; the `ritual` evaluator is hidden from the palette (only valid behind a reference); the list refreshes after activate/deactivate because a cascade moves children. **Server guard added:** a referencing group may not also contain nodes (422). **Live (headless Chromium against the deployed image via the Vite dev server):** add child + draw edge + save + reload round-trips ownership and edge; Activate from the UI took parent live and the child node active; Run through the dialog returned `parent: ui child heard hello` (parent Prolog -> child Ritual); deactivating the child under an active parent, a second owner, and the persistence rule each showed a readable sentence; borrowed save succeeded; deactivating the parent terminated both. Tests: lildaemon 2049 passed; `tsc -b` clean and eslint clean on touched files (the repo already has unrelated lint errors elsewhere). **Not done:** no automated frontend tests exist (evidence is the headless run); remote children; envelope-carried parent deadline.

**Recommended order:** S1, S2, S3, S4, S5, S6, S7, S8. P1 first for safety; then the cheap Python-only durability slice;
then the consolidation-value slice; composition last because it depends on nearly everything.

**Not scheduled:** P5 declarative roster (proposed in §7, not among the four decisions; revisit after S7); P4 Tier 2;
a `terminated -> draft` reset; the `free_c_string` interface review; preserving in-flight deductions beyond
snapshot/resume.

**Definition of done, every slice.**
1. Unit tests plus a real-Dis integration test (the `tests/test_*_promotion.py` pattern). Rust slices keep
   `cargo clippy --workspace --all-targets` clean (a blocking CI job) and `cargo test --workspace` green.
2. Rebuild and redeploy on limbic, live-verify against the real stack (ComfyUI off if anything touches the GPU), then
   run the full suites of the touched repos.
3. Update this spec's status and memory; commit each repo separately; push only when asked.

**Exit checks ("verified live").**
- **S1:** a `/deduce` with a tiny `deadline_ms` on a run that would otherwise loop returns `expired` with a partial
  result and `reason: deadline`; an oversized request is clamped to the ceiling; converged runs are unaffected.
- **S2:** `GET /deduce/{id}` returns `performance_id`, `ritual_id` and the limits; a finished long run's entry survives
  its TTL counted from completion; interrupt-then-finish no longer clobbers the final status; resume after `expired`
  continues.
- **S3:** an abandoned poll now stops the server-side run; Run and assistant deductions carry a deadline.
- **S4:** restart lildaemon with a `persist` config active: same `ritual_id`, Run works once `live`; non-persistent
  configs still terminate; peer-down retry and then fallback are exercised.
- **S5:** two nodes load a shared module via the lockfile with no clara-cerebellum rebuild; collision and
  missing-dependency cases fail loudly.
- **S6/S7:** parent activation cascades and rolls back; a parent Performance drives a child and cancels it on expiry.

**Risks.** FFI calls can overrun a deadline (cooperative only). Rust redeploys interleave with S5, so batch where
possible. lildaemon rebuilds must blank unset compose vars. DuckDB `ALTER TABLE` migrations (S3, S4, S6) need an
idempotent-migration test. S7 is the riskiest slice and gets its own design pass first.
