# Ritual Multi-Evaluator Peer Consultation (docs/ritual_multi_evaluator.md)

## Context

`docs/ritual_multi_evaluator.md` asks us to prove out interoperability between
multiple evaluators inside one Ritual: a `ClaraMindSplinter` node ("Clara")
answers via its own `ponder`, but also draws an edge to a `GroqEvaluator` node
("Clara/Groq") on the **same FieryPit**, gets Groq's independent answer back
over Coire/Kafka, and synthesizes a combined final answer.

Every prior Ritual phase (`ritual_deduce_runtime_wiring.md`,
`ritual_run_multi_fierypit.md`) explicitly deferred exactly this capability —
"edge-qualifier-driven routing/evaluation between nodes" and "nested/peer
evaluator chains" both appear on their "explicitly out of scope" lists. This
plan closes that gap. Research (direct reads of `RitualManager.py`,
`RitualParticipant.py`, `GoatWrangler.py`, `clara-cycle/src/controller.rs`,
`clara-ritual/src/handle.rs`, `clara-api/src/handlers/ritual_handler.rs`, and
the three most recent planning docs) surfaced four concrete reasons the
system **cannot** run this test today, beyond the obvious "no edge-drafted
Prolog exists yet":

1. **`RitualManager` allows exactly one participant per process, full stop.**
   `_participants: Dict[str, RitualParticipant]` is keyed by `ritual_id` alone
   and `join()` raises if that key exists — `partition_nodes_by_target()` in
   `goat/app/ritual_configs/router.py` enforces this as a hard 409. Clara and
   Clara/Groq **cannot** both be local nodes on one FieryPit today; the doc's
   "same FieryPit" premise is currently impossible.
2. **`RitualParticipant.evaluator_name` is dead code.** It's stored in
   `__init__` but never read anywhere — `_evaluate_with_timeout` always calls
   `self._wrangler.eval(offering)`, which dispatches to whatever evaluator
   happens to be `GoatWrangler._focused_evaluator` (one shared, mutable,
   process-global field). Two participants in one process would both
   silently answer as whichever evaluator is currently focused — there is no
   way today for one participant to reliably mean "always Groq" and another
   "always Clara."
3. **No message addressing exists at any layer.** `TephraEnvelope` (Rust,
   `clara-ritual/src/envelope.rs`) has no target field, and
   `RitualParticipant._handle_message` filters only by echo-suppression
   (`producer_node == self._self_node_id`) and `tephra_id` dedup — every
   Offering is visible to every participant on the topic. There is no way to
   say "this Offering is for Groq specifically."
4. **All nodes on a Ritual currently share one Kafka consumer group.**
   `activate_ritual_config` calls `dis_client.join_ritual(ritual_id, self_url)`
   **once** and reuses the same `routing["dis_domain"]` for every local and
   remote node's `manager.join(...)` / `join_remote(...)` call.
   `_create_kafka_clients`'s `group.id` is `ritual-{ritual_id}-{dis_domain}` —
   identical for every node today. Per Kafka competing-consumer semantics,
   an Offering intended for Groq could just as easily be delivered to Clara's
   own participant instead. This is the exact gap flagged (but not fixed) in
   `ritual_run_multi_fierypit.md`'s "Known gaps" section, and it affects every
   existing multi-node Ritual, not just the new same-process case.

The plan below fixes these four things, wires the nested peer-consult path
(Clara's `reasoned_response/3` consulting Groq mid-deduction), and adds the
Cobbler affordance to draft the Prolog for it. No `clara-api`/`clara-cycle`
Rust changes are needed for *message content* (target addressing rides inside
the existing opaque JSON `body`, no schema change) — one small, self-contained
Rust change *is* needed for correlation safety (Part B below).

---

## Part A — lildaemon foundation: real multi-participant-per-process support

**`goat/models/RitualManager.py`**
- Rekey `_participants` as `Dict[Tuple[str, str], RitualParticipant]` —
  `(ritual_id, node_id)`. `join()` now takes a required `node_id: str` and
  only raises if that exact `(ritual_id, node_id)` pair already exists.
- `leave(ritual_id, node_id=None)`: leave one node, or every node joined
  under that `ritual_id` when `node_id` is omitted (needed so
  `deactivate_ritual_config`'s blanket leave keeps working unchanged).
- `get(ritual_id, node_id=None)` / new `get_all(ritual_id) -> list[...]` for
  status/list callers.
- `active_rituals()` dedupes to unique `ritual_id`s (used by lildaemon's
  `GET /ritual`).

**`goat/models/GoatWrangler.py`**
- New `eval_slot(self, slot_name: Optional[str], offering: Offering) -> Tephra`:
  if `slot_name` is set and present in `self._active_evaluators`, evaluates
  directly via `self._active_evaluators[slot_name].pit.eval(offering)` —
  **bypassing `_focused_evaluator` entirely**, so concurrent participants
  never race over shared mutable focus state. Falls back to `self.eval(...)`
  (today's focused/echo behavior) when `slot_name` is `None` or not active —
  preserves the legacy single-node path exactly.
- Reuses the existing `spawn_evaluator(evaluator_name, slot_name=...)` (already
  supports multiple simultaneous instances, confirmed unused for this purpose
  today) — no new spawning mechanism needed, just call it from activation.

**`goat/models/RitualParticipant.py`**
- New `node_id: str` param (the Cobbler graph node's stable `id`, e.g. `"n2"`)
  — a *design-time* addressable key, deliberately distinct from
  `self_node_id`/`dis_domain` (which are deployment-time Kafka/echo identity).
- `_evaluate_with_timeout` calls `self._wrangler.eval_slot(self.evaluator_name, offering)`
  instead of `self._wrangler.eval(offering)` — finally wires up the
  previously-dead `evaluator_name` field.
- `_handle_message`: right after extracting `offering_data`, check
  `target_node_id = offering_data.pop("target_node_id", None)`; if set and
  `target_node_id != self.node_id`, log and `return` (not for me) — before
  echo-suppression and before deduce-qualification. This is the whole
  addressing mechanism; it lives entirely in the JSON body, no envelope
  schema change.
- `_create_kafka_clients`: change `group.id` from
  `f"ritual-{self.ritual_id}-{self.dis_domain}"` to
  `f"ritual-{self.ritual_id}-{self._self_node_id}"`. This is the fix for gap
  4 — `self._self_node_id` is already unique per participant (see below),
  so every individually-addressed node gets its own consumer group and sees
  every message (broadcast), matching the "true multi-node, not a redundant
  pool" decision already locked in `ritual_run_multi_fierypit.md`. Document
  in the docstring that sharing a `self_node_id` across instances is how an
  operator would opt back into redundant-pool load-balancing later — not
  built here, just not precluded.

**`goat/app/ritual_configs/router.py`**
- `partition_nodes_by_target`: delete the ">1 local node raises" constraint
  and its test coverage note — multiple local nodes are now supported. Keep
  the function (still splits local vs. remote by URL).
- `activate_ritual_config`: loop over **all** `local_nodes` (not `[0]`).  For
  each node (local or remote):
  - Call `dis_client.join_ritual(ritual_id, f"{self_url}#{node['id']}")`
    **per node** (was: once, reused for everyone) — reuses Dis's existing
    idempotent `?participant=` keying, no Dis-side change. This gives each
    node its own `self_node_id` composite automatically.
  - Ensure the node's evaluator is spawned into a wrangler slot keyed by
    `node['id']`: call `goat_manager.spawn_evaluator(node['evaluatorName'], slot_name=node['id'])`
    if not already active (idempotent check against `_active_evaluators`).
  - `manager.join(ritual_id=ritual_id, node_id=node['id'], self_node_id=f"{self_url}#{node['id']}", evaluator_name=node['evaluatorName'], ...)`.
  - Same idea for `remote_nodes` → `fiery_pit_peer_client.join_remote(...)`,
    each with its own per-node participant key/self_node_id.
  - Rollback path: track the list of `(ritual_id, node_id)` pairs joined so
    far (was: one boolean flag) and leave all of them on failure.

---

## Part B — nested peer-consult wiring (Clara ↔ Groq round trip)

**`goat/models/RitualParticipant.py`** (`_handle_message`, deduce-qualification block)
- New constructor param `peer_consult: bool = False` — set at activation time
  when the graph has an edge whose `source == node['id']` and whose `target`
  is another evaluator-bearing node in the same graph (computed once in
  `activate_ritual_config` from `graph.get("edges", [])`, threaded through
  `manager.join(...)`).
- When qualifying a plain Offering into a deduce request, set
  `"ritual_id": self.ritual_id if self.peer_consult else None` (today
  hardcoded `None`). This is what lets Clara's *inner* deduction's
  `CycleController` join the same Kafka topic and use
  `coire_publish`/`coire_poll` from Prolog — the existing, already-built
  "peer evaluator" mechanism documented in `rituals_101.md` step 3, simply
  never turned on before because nothing needed it.

**`clara-cycle/src/controller.rs`** (small, scoped Rust change — the one
necessary exception to "no clara-cycle changes")
- `ingest_tephra`: only decrement `pending_evaluator_responses` and write the
  `ritual/{label}` mailbox event for `hohi`/`tabu` envelopes whose
  `performance_id` matches `self.ritual_handle`'s own `performance_id`.
  Today `ingest_tephra` has no correlation at all — it treats **any**
  Hohi/Tabu on the shared topic as an answer to **its own** outstanding
  Offering. That's already a latent risk for concurrent performances on one
  Ritual topic, and became a real one the moment an inner deduction joins the
  same topic anonymously while other activity (e.g. the outer performance
  that spawned it) may still be on it. `RitualHandle.performance_id` already
  exists and is already stamped correctly on both the outbound Offering and
  the peer's echoed-back Hohi/Tabu (`RitualParticipant._handle_message`
  copies `envelope.get("performance_id", ...)` onto its response) — this is
  a pure filter-tightening, no new plumbing. Update the existing
  `ingest_tephra`/`publish_evaluator_events` unit tests that construct
  mismatched `performance_id`s expecting a decrement.

**Payload contract** (documented, not enforced by code — matches the
project's existing "editor just captures author intent" philosophy):
- Targeting a **deduce-capable** peer: `json([target_node_id=NodeId, goal=Goal, context=Context])`.
- Targeting a **plain** evaluator (Groq here, no `prolog_source_id`):
  `json([target_node_id=NodeId, prompt=Prompt])` — matches
  `ClaraMindSplinter`/`GroqEvaluator._validate_input`'s required `"prompt"`
  key exactly (confirmed by reading `groq_evaluator.py`).

**Edge-drafted Prolog template** (what actually gets prefilled into Clara's
`prologSource` — see Part C for how it's generated):
```prolog
reasoned_response(Query, Context, Response) :-
    ponder_text_with_context(Query, Context, ClaraAnswer),
    coire_publish(evaluator/offering, json([target_node_id='<groq-node-id>', prompt=Query])),
    coire_poll(ritual/hohi, Env),
    get_dict(result, Env, GroqHohi),
    extract_field(GroqHohi, content, GroqAnswer),
    format(atom(Synth), "Peer answers to reconcile.~nClara: ~w~nGroq: ~w~nQuestion: ~w", [ClaraAnswer, GroqAnswer, Query]),
    ponder_text_with_context(Synth, Context, Response).
```
(Field names `result`/`content` to be confirmed against the actual Hohi
payload shape produced by `kindling_evaluator.py`'s Hohi construction and
`GroqEvaluator._evaluate`'s `hohi_resp = {"tool_call":..., "content":...}` —
verify exact dict shape in Part D live testing before finalizing the template
string.)

---

## Part C — Cobbler: draft the Prolog when the edge is drawn

No backend/schema changes (matches every prior phase — `graph_layout` is
opaque and already round-trips arbitrary fields).

**`dagda/cobbler/frontend/src/components/GraphCanvas/deduceCapable.ts`**
- New `buildPeerConsultSnippet(targetNodeId: string, targetIsDeduceCapable: boolean): string`
  producing the template above (swap `prompt=Query` for `goal=Query, context=Context`
  when `targetIsDeduceCapable`).

**`EdgeQualifierPanel.tsx`**
- New button, "Draft peer-consult Prolog", shown only when the edge's
  *source* node is deduce-capable (reuse the existing
  `evaluatorClassByName`/`DEDUCE_CAPABLE_EVALUATOR_CLASSES` plumbing already
  threaded into `RitualEditorCanvas.tsx`). Clicking it writes the generated
  snippet into the source node's `prologSource` (via the same
  `cy.getElementById(nodeId).data('prologSource', ...)` + `onDirty()` pattern
  `DaemonPanel` already uses) with a confirmation if `prologSource` already
  has non-default content (don't silently clobber hand-edited work).
- This is a deliberate manual trigger (not automatic on edge-draw) so the
  generated Prolog is always visible/reviewable in the textarea before save —
  consistent with the project's stated "editor captures author intent,
  CAWS/transduction is the eventual real code generator" direction.

---

## Part D — end-to-end verification (the actual test from the doc)

1. Unit tests (lildaemon): `RitualManager` multi-join/leave by `(ritual_id, node_id)`;
   `GoatWrangler.eval_slot` bypassing focus; `RitualParticipant` target-address
   filtering (own id / other id / absent); `partition_nodes_by_target` no
   longer raises for 2 local nodes; `activate_ritual_config` loop joins N
   local nodes with distinct participant keys and spawns each into its own
   slot.
2. Rust unit tests (clara-cycle): `ingest_tephra` performance_id correlation —
   matching id decrements/writes mailbox, mismatched id is ignored.
3. Live stack, one lildaemon process, real Kafka: create a ritual config with
   two daemon nodes — Clara (`ClaraMindSplinter`, deduce-capable, edge → Groq)
   and Clara/Groq (`GroqEvaluator`, no source attached), both blank `url`
   (local). Activate — confirm 409 no longer fires, confirm both nodes show
   as joined (`GET /ritual` lists one `ritual_id`; internally two
   `RitualParticipant`s exist). Use Cobbler's "Draft peer-consult Prolog" on
   the Clara→Groq edge, confirm the generated template appears. Hit the
   existing Run endpoint with a real query; confirm the deduction converges
   and the returned `Response` text reflects synthesis of both Clara's and
   Groq's independent answers (not just one or the other) — the strongest
   signal the whole chain actually exercised the Kafka round trip rather than
   short-circuiting.
4. Clean up any scratch ritual configs/users created during verification, per
   this repo's established convention in the last two phase write-ups.

---

## Explicitly out of scope for this pass

- Multiple simultaneous outstanding peer-consults per node (the
  `coire_poll(ritual/hohi, Env)` step assumes exactly one pending response;
  fan-out to several peers with correlation would need per-request
  correlation ids, not just performance_id).
- CAWS/transduction-generated Prolog (still hand-authored/templated).
- Cascading peer-consult chains beyond one hop (Groq consulting a third node).
- Any generic "redundant pool" / horizontal-scaling opt-in UI for sharing a
  `self_node_id` across replicas — the code no longer *prevents* it, but no
  affordance is built to configure it.
