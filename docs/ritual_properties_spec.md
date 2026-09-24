# Ritual properties: durability, persistence, lifecycle (working draft)

_Drafted 2026-09-24 from Stan's feedback on `frontdesk_analyst_consolidation_brainstorm.md`. Approach A (analysts as
complete Rituals) needs these properties pinned down first. Sections 1-6 are **today's behavior**, verified against the
code with file references; section 7 is **proposed** and explicitly open for Stan. Nothing here is built._

Paths: `cc/` = `clara-cerebellum/`, `ld/` = `lildaemon/goat/`.

## 1. The objects and their states

| Object | States today | Persisted where | Survives a restart? |
|---|---|---|---|
| **Dis Ritual** | `Active`, `Terminated` (`cc/clara-ritual/src/ritual.rs:6-9`) | Write-through to the CoireStore (DuckDB) `rituals` table, incl. participants map (`cc/clara-ritual/src/registry.rs`) | Yes if a store is configured; `restore_from_store` reloads on boot and re-ensures Kafka topics for active rituals (`registry.rs:252`). Memory-only without a store |
| **Performance** | **No state machine.** A `performance_id` is minted per join, idempotent per participant key (`registry.rs:111-139`); anonymous joins aren't recorded | Keyed joins only | Ids stable across boot only for keyed joins |
| **Deduction** (nearest thing to a "run") | `Running`, `Converged`, `Interrupted`, `Error` (`cc/clara-cycle/src/result.rs:22`) | In-memory `AppState.deductions`; durable only with `persist:true` (a `DeductionSnapshot`) | No, unless `persist:true` and a store is configured (silently ignored otherwise) |
| **RitualConfig** (lildaemon) | `draft` -> `active` -> `terminated`; **no way back to draft** (`ld/app/ritual_configs/store.py`) | DuckDB `ritual_configs` table | Definition yes; runtime no (see §3) |
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
`active` is force-terminated because its in-memory participants are gone; shutdown does the same
(`ld/app/main.py:374-393`, `:566-584`). Reactivation is a manual step, and the state machine has no
`terminated -> draft` edge. For a long-lived analyst Ritual (the goal of A) this is the main gap: a restart
silently ends the partner.

## 4. Bounds on a run

- `max_cycles` defaults to 100 (`cc/clara-api/src/handlers/deduce_handler.rs:41`); **no server-side maximum**.
  Exceeding it returns `MaxCyclesExceeded`.
- `evaluator_patience_cycles` defaults to 10 and is **per offer**; expiry injects a `ritual/tabu-timeout`
  (`cc/clara-cycle/src/controller.rs:1179-1206`). Cycles are paced at 250 ms only while offers are outstanding.
- Callers already pick very different points: RitualConfig Run uses `max_cycles` 100 with
  `patience = max(10, eval_timeout_s/0.25)` (`ld/app/ritual_configs/router.py:633-634`); the assistant deliberation
  uses patience 3500 and `max_cycles` 4000 (`ld/app/assistant/runtime.py:216-217`).
- **There is no wall-clock deadline.** A deduction is interruptible (AtomicBool) but never times out on its own,
  so "a long, *not eternal* timeout with huge cycle limits" (Stan) cannot be expressed today except indirectly
  through cycle counts.

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

**P1. One Performance model, two presets.** Give Performance a first-class state
(`pending / running / converged / interrupted / failed / expired`) and a wall-clock `deadline_ms` alongside
`max_cycles` and patience. "One-shot" = small cycles, short deadline; "long, bounded" = huge cycles, long deadline.
Same fields, no special cases, and a deduction that outlives its deadline becomes `expired` rather than eternal.

**P2. Separate definition durability from runtime durability.** Keep RitualConfig definitions durable (already true).
Add a runtime story: on boot, *re-activate* configs marked to persist instead of force-terminating them (or add a
`terminated -> draft` edge and let an operator/worker re-activate). Open: which configs opt in, and whether Hermes
seats (which hold leases) re-attach or are recreated.

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
2. P2: which Ritual configs should survive a restart, and is re-activation automatic or operator-driven?
3. P1: is `expired` a terminal state distinct from `interrupted`, and who owns the deadline (caller, config default,
   or server ceiling)?
4. P3: should child Ritual lifecycle be tied to the parent's?
