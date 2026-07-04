# Cobbler Ritual Editor: Deduce Source & Edge Qualifier Authoring (Plan)

**Status:** Proposed — pending team review
**Date:** 2026-07-03 (revised)
**Scope:** Cobbler editor authoring only (no runtime execution wiring yet)

---

## Context

`docs/ritual_deduction_support.md` asks to expand Clara's deduction support so
that a user can attach Prolog/CLIPS reasoning to a Ritual graph in Cobbler —
so an incoming Offering routed to a deduction-capable evaluator (a
`KindlingEvaluator` subclass, e.g. `ClaraMindSplinter`) has source code
available at performance time, and edges carry lightweight qualifiers
governing when/what gets forwarded downstream on the Coire/Kafka mailbox.

Research across the three repos (clara-cerebellum/clara-api, lildaemon,
dagda/cobbler) confirmed:

- **clara-api already has everything needed to *run* a rule once we have
  source text**: `POST /source` registers Prolog/CLIPS content and returns a
  `prolog_source_id`/`clips_source_id` that `POST /deduce` accepts directly
  (`clara-api/src/handlers/source_handler.rs`,
  `clara-api/src/models/request.rs:104-165`).
- **lildaemon's `KindlingEvaluator`/`ClaraMindSplinter`** already POSTs to
  Dis's `/deduce` for direct `/evaluate` calls
  (`goat/evaluators/custom/kindling_evaluator.py:549-590`), but the
  Ritual/Kafka Offering path is generic JSON pass-through with no source
  attached today, and `RitualParticipant` binds one evaluator per
  participant with no per-node source lookup
  (`goat/models/RitualParticipant.py`).
- **Cobbler's graph canvas** has no per-edge properties panel today (only a
  one-shot `FlowKindDialog` at draw time) and no code-editor widget — just
  plain `<textarea>` (as used in `NodePropertiesPanel.tsx`'s `DaemonPanel`).
  `graph_layout` is persisted as an opaque JSON string that neither dagda's
  backend proxy (`cobbler/backend/routers/ritual_configs.py`) nor lildaemon
  parses — new fields round-trip for free with **no backend schema change
  required**.
- `/repl/evaluators` (already fetched by `EvaluatorPalette.tsx` via
  `listEvaluators()` in `api/client.ts`) returns an `evaluator_class` field
  per evaluator, which is enough to tell a `ClaraMindSplinter`/`GroqEvaluator`
  node apart from a plain `OllamaEvaluator` node client-side, with no backend
  change.
- **`ClaraFish` (`goat/repl/fishes/clara_fish.py`)** has been updated to
  qualify REPL input as a deduce request: text wrapped in `¿...?`/`??...??`
  syntactic sugar (double-quote wrapping is planned but not yet in this
  file) builds a `deduce` Offering with
  `initial_goal: "reasoned_response(Q,C,R)"` (`clara_fish.py:54`) — this
  independently confirms `reasoned_response/3` as the right contract name
  below. It's currently a stub: `Q`/`C` aren't bound to the actual query
  text/context, and `prolog_source_id`/`clips_source_id` stay `None`
  (`clara_fish.py:49-59`) — exactly the gap that node-side source
  registration (Follow-on work #1) needs to close.

### Revision: source lives on the node, not the edge

Original framing put a rule editor on edges. That's wrong: a `/deduce`
request's `prolog_source_id`/`clips_source_id` must resolve to code that is
available **at the node, at performance time** — the node is what actually
runs the deduction cycle when it receives a `deduce`-shaped Offering. Edges
don't run anything; they only gate and route. So:

- **Node** (daemon node whose evaluator is deduce-capable) owns the Prolog
  and CLIPS source that will be registered/consulted when it processes a
  `deduce` Offering.
- **Edge** owns a lightweight *qualifier* — either nothing, a dynamic
  incoming assertion, or a boolean guard over existing variables/predicates
  — that governs whether/how an Offering or Tephra crosses that edge.

## Data model changes

### `cobbler/frontend/src/components/GraphCanvas/types.ts`

```ts
export interface DaemonNode {
  id: string;
  type: 'daemon';
  evaluatorName: string;
  label: string;
  url?: string;
  position: { x: number; y: number };
  parentId?: string;
  config: Record<string, unknown>;
  status: 'idle' | 'active' | 'error';
  prologSource?: string;   // new — consulted at performance time via prolog_source_id
  clipsSource?: string;    // new — consulted at performance time via clips_source_id
}

export type EdgeQualifierKind = 'none' | 'assertion' | 'boolean';

export interface FlowEdge {
  id: string;
  source: string;
  target: string;
  flowKind: EdgeFlowKind;
  envelopeLabel?: string;
  qualifierKind?: EdgeQualifierKind;   // default 'none' when absent
  qualifierValue?: string;             // assertion text or boolean expression; unused when kind = 'none'
}
```

`qualifierKind`/`qualifierValue` are flat strings rather than a nested
object, matching the existing `flowKind`/`envelopeLabel` convention — this
keeps them directly styleable via cytoscape attribute selectors
(`edge[qualifierKind = "boolean"]`) and avoids nested-object edge cases in
`graphSerializer.ts`.

- **`none`** — no field shown beyond an informational note: *"Fires once
  per Offering; forwards unchanged to the target node(s) via Coire/Kafka."*
  This is a meaningful state (see Runtime semantics below), not just an
  empty string.
- **`assertion`** — one free-text field: a dynamic incoming declaration to
  assert on arrival (e.g. `core_temp(9500).`).
- **`boolean`** — one free-text field: a guard over existing bound
  variables/predicates, either a literal comparison (`core_temp > 9000`) or
  a natural-language claim in the `clara_fy`/`ponder_text_with_context`
  style (`"temperature of the antimatter core is critical"`). Parsing/
  evaluating this string is a runtime-phase concern; the editor just
  captures author intent.

Extend the `'editor'` variant of `GraphCanvasMode` with
`onEdgeSelect: (edgeId: string | null) => void;` (edges are still
selectable/inspectable — just for the qualifier, not source).

### New `deduceCapable.ts` (small shared constant)

```ts
export const DEDUCE_CAPABLE_EVALUATOR_CLASSES = new Set([
  'ClaraMindSplinter',
  'GroqEvaluator',
]);
```

Extend this set as new `KindlingEvaluator` subclasses are registered in
lildaemon. Used to gate the "Deduce Source" section in `DaemonPanel` — see
below.

### `reasoned_response/3` contract

Any deduce-capable node's Prolog source is expected to define at least:

```prolog
reasoned_response(Query, Context, Response)
```

When a deduce-capable node's `prologSource` is empty, the editor prefills it
with a default implementation:

```prolog
reasoned_response(Query, Context, Response) :-
    ponder_text_with_context(Query, Context, Response).
```

(`ponder_text_with_context/3` — `clara-prolog/prolog-lib/the_rabbit.pl`,
already documented in `docs/deduce_endpoint.md`'s context walkthrough.) This
is a starting template, not enforced by validation — fully editable/
overridable by the user.

## UI changes

### 1. `RitualEditorCanvas.tsx`

- On mount, call `listEvaluators()` (already used by `EvaluatorPalette`) and
  build `evaluatorClassByName: Record<string, string | null>`; pass it down
  to `NodePropertiesPanel` → `DaemonPanel`.
- Add `selectedEdgeId` state alongside `selectedNodeId`; wire
  `onEdgeSelect: setSelectedEdgeId` into `GraphCanvas`'s editor mode.
- Render `EdgeQualifierPanel` when `selectedEdgeId` is set,
  `NodePropertiesPanel` when `selectedNodeId` is set, in the same right-hand
  panel slot (mutually exclusive — selecting one clears the other, handled
  in the `GraphCanvas.tsx` tap handlers).

### 2. `GraphCanvas.tsx` (`EditorCanvas`)

- Add `onEdgeSelect` prop, threaded from `GraphCanvas`'s mode-dispatch
  (mirrors `onNodeSelect`).
- Wire `cy.on('tap', 'edge', evt => onEdgeSelect(evt.target.id()))`.
- On node tap, clear edge selection; on edge tap, clear node selection; on
  background tap, clear both.

### 3. `NodePropertiesPanel.tsx` — `DaemonPanel` grows a "Deduce Source" section

- New prop: `evaluatorClassByName: Record<string, string | null>`.
- Compute `isDeduceCapable = DEDUCE_CAPABLE_EVALUATOR_CLASSES.has(evaluatorClassByName[fields.evaluatorName] ?? '')`.
- When `isDeduceCapable`, render below the existing Config field:
  - **Prolog source** — `<textarea rows={10}>`, prefilled with the
    `reasoned_response/3` default template when empty at panel-open time.
  - **CLIPS source** — `<textarea rows={10}>`, no default template.
  - Both follow the existing `DaemonPanel.update()` pattern:
    `cy.getElementById(nodeId).data('prologSource'|'clipsSource', value)` +
    `onDirty()`.
- When not deduce-capable, the section is omitted entirely (no empty
  placeholder clutter for e.g. a plain `OllamaEvaluator` node).

### 4. New `EdgeQualifierPanel.tsx` (parallel to `DaemonPanel`)

- Props: `cyRef`, `selectedEdgeId`, `onDirty`.
- Loads `qualifierKind`/`qualifierValue` off the selected edge's `data()` on
  selection change (mirrors `DaemonPanel`'s `useEffect` load pattern).
- Three-way toggle (button group or radio): **None / Assertion / Boolean**.
  - `None` shows only the informational note above; no text field.
  - `Assertion` shows one `<input>` (placeholder: `e.g. core_temp(9500).`).
  - `Boolean` shows one `<input>` (placeholder: `e.g. core_temp > 9000, or "the antimatter core is critical"`).
- Same update-on-change pattern as `DaemonPanel`, writing both
  `qualifierKind` and `qualifierValue` to the edge's cytoscape data.

### 5. `graphSerializer.ts`

- `graphToElements`: include `prologSource`/`clipsSource` in daemon node
  `data`; include `qualifierKind`/`qualifierValue` in edge `data` (default
  `qualifierKind: 'none'` when absent, matching the `envelopeLabel ?? ''`
  pattern already used).
- `elementsToGraph`: read them back into `DaemonNode`/`FlowEdge`, mirroring
  the existing `envelopeLabel || undefined` pattern.

### 6. `cytoscapeStyles.ts` (`EDITOR_STYLESHEET`) — visual affordance

```ts
{ selector: 'edge[qualifierKind = "assertion"]', style: { 'line-color': '#20b06a', 'target-arrow-color': '#20b06a' } },
{ selector: 'edge[qualifierKind = "boolean"]',   style: { 'line-style': 'dotted', 'line-color': '#c98a1f', 'target-arrow-color': '#c98a1f' } },
```
(Illustrative colors — mirrors the existing `edge[flowKind = "broadcast"]`
pattern; exact palette is a polish detail, not load-bearing.)

## Runtime semantics (spec only — not built in this pass)

Captured here precisely so the Phase 2 (runtime wiring) plan has an
unambiguous target:

- **`qualifierKind: 'none'`** — the edge compiles to a CLIPS
  forward-chaining rule that fires exactly once per incoming
  Offering/Tephra and emits it unchanged to the target node(s) via
  Coire/Kafka. No condition, no transformation.
- **`qualifierKind: 'assertion'`** — on arrival, the `qualifierValue` is
  asserted as a fact/predicate (a "dynamic incoming declaration") before
  the Offering is forwarded — i.e. it enriches the target node's session
  with new ground truth rather than gating forwarding.
- **`qualifierKind: 'boolean'`** — the `qualifierValue` is evaluated as a
  guard (either a literal comparison over existing bound variables/
  predicates, or a `clara_fy`-style natural-language truth classification
  via `ponder_text_with_context/3`); the Offering/Tephra is forwarded only
  if the guard holds.
- **Node-side**: when a deduce-capable node receives a `deduce`-shaped
  Offering, it resolves `prologSource`/`clipsSource` (registered via
  clara-api's `POST /source` at activation time, yielding
  `prolog_source_id`/`clips_source_id`) and calls `reasoned_response(Query,
  Context, Response)` as the entry point, returning `Response` in the Hohi.

## Explicitly out of scope for this pass

- Any backend schema change (lildaemon `ritual_configs`/`ritual_participants`,
  or dagda's proxy) — `graph_layout` already round-trips arbitrary new
  fields.
- Registering node source with clara-api's `POST /source`, or wiring
  `KindlingEvaluator`/`RitualParticipant` to actually evaluate qualifiers
  and invoke `reasoned_response/3` at runtime — this is Phase 2, using the
  "Runtime semantics" spec above as its contract.
- Monaco/CodeMirror — plain `<textarea>`, consistent with `DaemonPanel`/
  `ReplPanel`/`ConfigUpload`.
- Changes to `FlowKindDialog.tsx` (the one-shot creation dialog) — qualifier
  authoring happens post-creation via `EdgeQualifierPanel`, keeping edge
  creation as lightweight as it is today.
- CLIPS-side default template for `reasoned_response/3` (Prolog-only, since
  `ponder_text_with_context/3` is a Prolog predicate).

## Verification

1. `cd dagda/cobbler/frontend && npm run dev`, open the Ritual editor for an
   existing (or new) ritual config that includes a `ClaraMindSplinter`
   daemon node and a plain (e.g. Ollama) daemon node.
2. Select the `ClaraMindSplinter` node — confirm the "Deduce Source" section
   appears with the Prolog textarea prefilled with the `reasoned_response/3`
   default template, and an empty CLIPS textarea.
3. Select the plain Ollama node — confirm no "Deduce Source" section
   appears at all.
4. Draw an edge between two nodes; confirm `FlowKindDialog` behavior is
   unchanged. Click the new edge — confirm `EdgeQualifierPanel` appears
   (not `NodePropertiesPanel`), defaulted to `None` with the informational
   note visible.
5. Switch the qualifier to `Assertion`, enter a sample fact; switch to
   `Boolean`, enter a sample guard — confirm the toolbar's Save button goes
   dirty and the edge re-colors per the stylesheet rules in each case.
6. Click a node, then the edge again — confirm mutual exclusivity of the two
   panels, and that previously-entered node source / edge qualifier values
   persist across selection changes (proves round-trip through `cy` element
   data, not just local component state).
7. Save, reload the page (or navigate away and back to the same config in
   the list) — confirm node source and edge qualifier survive a full
   `graph_layout` JSON round-trip.
8. Check the browser console for cytoscape data-mapping warnings from
   edgehandles' ghost/preview edges lacking the new fields (the existing
   codebase is careful about this — see the `edge[envelopeLabel]` vs bare
   `edge` note in `phase_d_checkpoint.md`).

## Follow-on work (not in this plan)

1. **lildaemon persistence & registration** — promote per-node source out of
   the opaque `graph_layout` blob into a first-class, queryable store, and
   register it with clara-api's `POST /source` at activation time so
   `prolog_source_id`/`clips_source_id` are available for `/deduce`.
2. **Runtime firing** — wire `RitualParticipant`/`KindlingEvaluator` to
   implement the "Runtime semantics" section above: compile `none` edges to
   the fire-once CLIPS forwarding rule, evaluate `assertion`/`boolean`
   qualifiers, and dispatch `deduce` Offerings to `reasoned_response/3` on
   the target node's registered source. Also finish `ClaraFish.translate()`
   (`clara_fish.py:45-63`): bind `Q`/`C` in `initial_goal` to the actual
   query text and conversation context, resolve `prolog_source_id`/
   `clips_source_id` from the node's registered source instead of leaving
   them `None`, and extend `is_question_wrapped()` with the double-quote
   (`"..."`) sugar alongside the existing `¿...?`/`??...??` forms.
3. **CAWS/transduction swap** — replace hand-authored source with
   Clara-transduction-generated Prolog/CLIPS, per the original doc's stated
   direction.
