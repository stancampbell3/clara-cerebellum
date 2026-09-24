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

**P4. Module dependencies — the fork to decide.**

| Option | How | For | Against |
|---|---|---|---|
| (a) grow the compiled-in overlay | keep adding to `prolog-lib/` | simple, fast, already works | every shared predicate is a cross-repo rebuild+redeploy; unsuitable for ad hoc Rituals whose needs aren't known in advance |
| (b) **Dis-registered module sources** | register module source content-addressed; a RitualConfig/node declares dependencies; Dis loads them before the node's source | no rebuild, Ritual-scoped, fits ad hoc and composed Rituals, mirrors the existing content-addressed `register_source` | new server work; need module isolation/versioning so two Rituals' same-named modules don't collide |
| (c) inline-bundle at activation | concatenate dependency source into each node's flat source | no server change | duplicates source per node — recreates today's copy-paste problem, just generated |

Recommendation: **(b)** for ad hoc and composed Rituals, keeping **(a)** for the stable core libs
(`the_coire`, `the_rabbit`, ...). A typed Prolog superset could later compile *to* (b)'s module declarations.

**P5. Declarative roster.** Replace per-caller imperative joins with a roster on the RitualConfig that a Performance
resolves ("bring together the needed participants"), covering native Clara evaluators, Hermes seats and remote
FieryPits uniformly. Id/Ego/Superego stay provided natively or by Hermes; ad hoc Rituals name resources resolved
at Performance time.

**Spec shape (unchanged from Stan):** graph of nodes, edges with augmented metadata, and application code for
deduce-backed ("devil") nodes; Prolog plus generated CLIPS now. The typed superset and CLIPS generation stay out of
scope until P1-P5 settle.

## 8. Decisions needed

1. P4: (a)/(b)/(c), or (b) plus (a) as recommended?
2. P2: which Ritual configs should survive a restart, and is re-activation automatic or operator-driven?
3. P1: is `expired` a terminal state distinct from `interrupted`, and who owns the deadline (caller, config default,
   or server ceiling)?
4. P3: should child Ritual lifecycle be tied to the parent's?
