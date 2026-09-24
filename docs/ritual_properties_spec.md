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

- **No nesting in the backend.** No parent/child ritual ids anywhere in the Rust crates. `ritual-group` exists only
  in Cobbler's types (`dagda/cobbler/frontend/src/components/GraphCanvas/types.ts:107-115`); activation selects
  only `type == "daemon"` nodes with an `evaluatorName` (`ld/app/ritual_configs/router.py:328-330`), so a group node
  is **presentation-only**.
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

Nothing composes a roster declaratively. Each caller does `dis_client.join_ritual` plus `RitualManager.join`:

- RitualConfig activation joins local nodes (`router.py:373-410`) and remote ones via
  `fiery_pit_peer_client.join_remote` (`router.py:412-430`).
- `_ensure_standing_ritual` (`ld/app/assistant/runtime.py:498-577`) joins snek, edgequakeingest and groq-splinter
  once under a lock; `_ensure_deliberation_seats` (`:579`) adds chair/members/cow lazily.
- `_ensure_ego_ritual(session_id)` (`:1100`) creates one Ritual per session with `hermes_ego` plus a reviewer.
- `HermesAgentEvaluator._ensure_seat` starts a seat lazily on first evaluate, bound to one Ritual (409 otherwise),
  with a renewed lease, released when idle (`ld/evaluators/custom/hermes_agent_evaluator.py`,
  `ld/ritual_space/scope.py:63`).

## 7. Proposed target properties (for Stan to react to)

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

**P3. Rituals of Rituals by reference.** Make `ritual-group` functional: a group node references a child
RitualConfig, activation activates the child, and the parent sees it as one participant with its own edge
metadata. Open: does the child's lifecycle follow the parent's (activate/terminate together), and how does a
Performance on the parent fan out to the child?

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
4. P3: should child Ritual lifecycle be tied to the parent's?
