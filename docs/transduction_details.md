# Prolog → CLIPS Transduction

Transduction generates CLIPS `defrule`s from Prolog rules so that CLIPS's
forward-chaining engine can speculatively push head goals back to Prolog
whenever any body condition is asserted as a CLIPS fact. The result is
**agenda-driven, partial-information reasoning**: Prolog is asked to prove a
goal even when only one of its preconditions is currently known.

The same parsed rule representation also drives **DOT graph visualization** —
a static dependency diagram of the rule set that can be colorized with live
truth values from the Dagda tableau during trace playback.

---

## Concept

In the clara-cycle reasoning loop, Prolog assertions are relayed into CLIPS as
ordered facts via clara-coire. Transduction adds a layer on top: for every
positive body goal in a Prolog rule it emits a CLIPS defrule whose LHS watches
for that fact. When CLIPS fires the rule, its RHS calls `coire-publish-goal`
with the Prolog head goal string, which the relay forwards to Prolog for
evaluation.

```
Prolog source (.pl)
      │
      ▼  parse_prolog_rules()
  Vec<PrologRule> { head: Term, body: Vec<BodyGoal> }
      │
      ├──▶  decorate_source()
      │         <stem>_clara.pl — original source + prolog_listen directives
      │         + updated/3 relay rule (publishes asserted facts to CLIPS)
      │
      ├──▶  transduce()
      │         <stem>_clara.clp — CLIPS defrule source
      │         loaded by clara-cycle before CLIPS constructs at runtime
      │
      │         assert(smoke(kitchen)) → relay → (smoke kitchen) in CLIPS
      │           ↳ transduced-fire-on-smoke-0 fires
      │           ↳ (coire-publish-goal "fire(kitchen)")
      │           ↳ relay forwards goal event → Prolog
      │           ↳ consume_coire_events() calls fire(kitchen)
      │
      └──▶  generate_dot(rules, coloring?, opts)
                <stem>_clara.dot — static dependency diagram
                (also cached as "dot" / "parsed_rules" artifacts in source_registry
                 with truth-value coloring during trace playback)
```

---

## Rule Mapping

### Simple rule — one body condition

```prolog
fire(Where) :- smoke(Where).
```

```clips
; Transduced from: fire(Where) :- smoke(Where).
(defrule transduced-fire-on-smoke-0
    (smoke ?Where)
    =>
    (coire-publish-goal (str-cat "fire(" ?Where ")")))
```

`?Where` is bound by the LHS pattern, so its runtime value is interpolated
directly into the goal string.

### Conjunction — one defrule per positive condition

```prolog
lemonade(Drink) :- sour(Drink), sweet(Drink).
```

```clips
; Transduced from: lemonade(Drink) :- sour(Drink), sweet(Drink).
(defrule transduced-lemonade-on-sour-2
    (sour ?Drink)
    =>
    (coire-publish-goal (str-cat "lemonade(" ?Drink ")")))

; Transduced from: lemonade(Drink) :- sour(Drink), sweet(Drink).
(defrule transduced-lemonade-on-sweet-3
    (sweet ?Drink)
    =>
    (coire-publish-goal (str-cat "lemonade(" ?Drink ")")))
```

Asserting *either* `sour(glass_1)` or `sweet(glass_1)` independently triggers
Prolog to attempt `lemonade(glass_1)`, even without full information.

### Disjunction — treated the same as conjunction

Semicolons in a rule body are flattened alongside commas. Each goal becomes an
independent trigger.

### Unbound head variables

When the head has variables that are **not** bound by the triggering condition,
they are emitted as literal variable-name strings. Prolog receives them as free
variables, causing it to search for any binding.

```prolog
head(A, B) :- cond(A).
```

```clips
(defrule transduced-head-on-cond-0
    (cond ?A)
    =>
    (coire-publish-goal (str-cat "head(" ?A ",B)")))
```

`B` appears as the string `"B"` in the goal; SWI-Prolog will treat it as an
unbound variable when the goal is called.

### Negation (`\+`) — skipped, comment emitted

Negative conditions have no positive fact to watch, so no defrule is generated.
A comment is emitted in the output to document the skip.

```prolog
ok(X) :- good(X), \+ bad(X).
```

```clips
; Transduced from: ok(X) :- good(X), \+ bad(X).
(defrule transduced-ok-on-good-0
    (good ?X)
    =>
    (coire-publish-goal (str-cat "ok(" ?X ")")))
; NOTE: \+ bad(X) is a negative condition — skipped as trigger source.
```

### Facts — silently skipped

Bare facts (`mortal(stan).`) have no body and produce no defrules.

---

## DOT Graph Generation

`generate_dot(rules, coloring, opts)` converts a `Vec<PrologRule>` into a
Graphviz DOT string that visualizes the rule/fact dependency graph.

### Node types

| Shape | Default fill | Meaning |
|-------|-------------|---------|
| Ellipse | `#d4edda` (green) | Bare fact — no body goals |
| Box | `#cce5ff` (blue) | Rule head with at least one body goal |
| Box (darker, heavier border) | `#7ba7d4` (dark blue) | Synthetic umbrella node — groups 2+ clauses sharing the same head functor/arity |
| Dashed ellipse | `#fff3cd` (amber) | Leaf condition — not bridged to another rule head |

### Edge types

| Style | Color | Label | Meaning |
|-------|-------|-------|---------|
| Solid | Black | `requires` | Rule head → leaf condition |
| Solid | Blue | *(none)* | Assert-bridge: head A → head B when A's condition is asserted by B |
| Dashed | Blue | `chains-to` | Condition → rule head whose functor/arity it directly matches |
| Dotted | Gray | `clause` | Umbrella node → concrete clause node |
| Dashed | Gray | `satisfies` | Fact → condition whose functor/arity it matches |
| Dashed | Gray | *(undirected)* | Shared-condition link, when `DotOptions.link_shared_conditions = true` |

### Truth-value coloring

When a `NodeColoring` is supplied (built from a Dagda tableau snapshot via
`coloring_from_entries`), structural fill colors are replaced by:

| Color | Truth value |
|-------|-------------|
| `#28a745` (green) | `KnownTrue` |
| `#dc3545` (red) | `KnownFalse` |
| `#ffc107` (amber) | `KnownUnresolved` — mixed or conflicting entries for the same functor |
| `#adb5bd` (gray) | `Unknown` |

Nodes absent from the tableau keep their structural defaults.

### `coloring_from_entries`

Builds a `NodeColoring` from a `&[PredicateEntry]` tableau snapshot.

Each entry contributes its functor → truth value. When multiple entries share
the same functor (e.g. `tumbler/2` with different concrete arguments), values
are merged:

- All entries agree → use that value.
- Any disagreement (including `KnownTrue` + `KnownFalse`) → `KnownUnresolved` (amber).

Coloring is applied only from what is **directly present** in the tableau at
that phase. Logical inference is not performed — see [Trace coloring design](#trace-coloring-design)
for the rationale.

### `extract_consulted_files`

```rust
pub fn extract_consulted_files(rules: &[PrologRule]) -> Vec<String>
```

Scans bare-fact nodes in a parsed rule set for `consult(file)` terms and
returns the inner path strings. Used by the trace DOT endpoint to recover the
full rule graph when the registered source only contains seed clauses (e.g.
`consult('rules.pl').`, `day_of_week(saturday).`).

### `propagate_rule_coloring`

```rust
pub fn propagate_rule_coloring(rules: &[PrologRule], coloring: &mut NodeColoring)
```

Performs a fixpoint pass over the rule set: for each rule whose every positive,
non-meta body-condition functor is already `KnownTrue` in `coloring`, the rule
head functor is also marked `KnownTrue`. The loop repeats until no new entries
are added, so deep chains propagate correctly in a single call.

This function is **not called by the trace DOT endpoint** — see
[Trace coloring design](#trace-coloring-design). It is exported for
callers that need to compute a fully-inferred coloring from a static snapshot
(e.g. offline analysis tools or the `baloroptik file` subcommand).

### `DotOptions`

| Field | Default | Effect |
|-------|---------|--------|
| `link_shared_conditions` | `false` | When `true`, adds dashed gray undirected edges between condition nodes that share the same label across rules. Useful for identifying shared sub-goals. |

---

## Trace Coloring Design

### Coloring is observation-driven, not inference-driven

The trace DOT endpoint (`GET /deduce/{id}/trace/{change_id}/dot`) colors nodes
from the Dagda tableau snapshot recorded at that phase and **nothing else**.
`propagate_rule_coloring` is explicitly not called.

This is intentional. Propagation-based coloring would make the entire graph
green from the first frame whenever the seed facts are logically sufficient to
prove all goals — which is the normal case. The trace would show no progression
and would be useless for understanding the reasoning process.

Instead, coloring tracks what has **actually been recorded in the tableau** at
each phase:

| Phase | What enters the tableau |
|-------|------------------------|
| `initial` | Bare seed facts (`day_of_week`, `wet`, …) from `seed_tableau_from_source` |
| `prolog_to_clips` | One snapshot per relayed event; each snapshot reflects `record_event_in_tableau` on that event's payload |
| `clips_to_prolog` | Same, for CLIPS goal events (goal events are ignored by `record_event_in_tableau`, so no new green nodes until a fact assertion comes back) |
| `final_converged` | `re_evaluate_root_goal` marks the root goal `KnownTrue` just before this snapshot |

The result is a frame-by-frame record of what the system has actually computed,
not what it could compute.

`propagate_rule_coloring` remains available for offline use cases where the
caller wants a fully-inferred static picture — such as colorizing a snapshot
for documentation or export — and is willing to lose the temporal dimension.

### How intermediate predicates enter the tableau

Predicates like `wet_surface` and `not_sprinklers` that are proved by Prolog
backward chaining (not directly asserted) enter the tableau via the `updated/3`
hook in the decorated source (`_clara.pl`). The hook fires when any clause of a
`dynamic` predicate is assertz'd. The relay then calls `record_event_in_tableau`
on each published event, updating the Dagda tableau and snapshotting it.

The root goal (`it_rained`) is handled separately: `re_evaluate_root_goal` in
`CycleController` re-queries Prolog once all mailboxes are empty and writes the
result directly to the tableau before the `final_converged` snapshot.

### Known issue: `updated/3` fires for rule clause assertz

**Symptom**: when `ex2_clara.pl` (or any decorated source with synthetic groups)
is loaded via `consult`, the `updated/3` hook fires for every rule clause that
is assertz'd as part of loading the file — not just for runtime bare-fact
assertions. For example, when `wet_surface :- wet(ground).` is added as a
clause of the `dynamic` predicate `wet_surface/0`, the hook fires and publishes
`wet_surface` as a `KnownTrue` Coire event.

**Effect**: `wet_surface` and `not_sprinklers` appear as `KnownTrue` in the
tableau at the first `prolog_to_clips` snapshot (not at `initial`, since the
Coire events haven't been consumed yet). This is earlier than strictly correct —
the rules have been *loaded*, not *proved*. In practice the result is accurate
because both rules do hold given the seeded facts, but a more conservative
system would only publish after an explicit proof.

**Suggested fix**: Change the `updated/3` handler in `generate_listen_preamble`
(`clara-cycle/src/transduction.rs`) to check whether the clause body is `true`
before publishing:

```prolog
updated(Pred, Action, Context) :-
    clause(Head, Body, Context),
    ( Body == true -> coire_publish_assert(Head) ; true ),
    format('Updated ~w with action ~w in context ~p~n', [Pred, Action, Head]).
```

With this change only bare facts (`wet(sidewalk).`) trigger a Coire publish;
rule definitions (`wet_surface :- wet(ground).`) do not. Intermediate predicates
would then only appear in the tableau when the CLIPS-driven goal loop causes
Prolog to assertz the proved result explicitly, or when a separate
proof-recording mechanism is added to `consume_coire_events`.

---

## CLI Tools

Two CLI binaries work together: `transduction` prepares rule sources, and
`baloroptik` visualizes the deduction traces those sources produce.

### `transduction` — rule preparation

```
transduction <input.pl>
```

Parses `<input.pl>` once and writes **three files** beside the input:

| File | Contents |
|------|----------|
| `<stem>_clara.pl` | Original Prolog source prepended with `:- prolog_listen(...)` directives for every `dynamic` predicate and the `updated/3` relay rule that publishes asserted facts to CLIPS |
| `<stem>_clara.clp` | CLIPS defrules for speculative forward chaining |
| `<stem>_clara.dot` | Graphviz DOT graph showing facts, rule heads, conditions, and their chaining relationships |

Stdout is not used. Exits with code 1 on any I/O or argument error.

```bash
transduction fire_alarm.pl
# Writes: fire_alarm_clara.pl  fire_alarm_clara.clp  fire_alarm_clara.dot
```

**Input** (`fire_alarm.pl`):
```prolog
fire(Where) :- smoke(Where).
lemonade(Drink) :- sour(Drink), sweet(Drink).
```

**Output** (`fire_alarm_clara.clp`):
```clips
(defrule transduced-fire-on-smoke-0
    (smoke ?Where)
    =>
    (coire-publish-goal (str-cat "fire(" ?Where ")")))

(defrule transduced-lemonade-on-sour-1
    (sour ?Drink)
    =>
    (coire-publish-goal (str-cat "lemonade(" ?Drink ")")))

(defrule transduced-lemonade-on-sweet-2
    (sweet ?Drink)
    =>
    (coire-publish-goal (str-cat "lemonade(" ?Drink ")")))
```

The `_clara.pl` is the file you load into SWI-Prolog. The `_clara.clp` is passed
as `clips_file` in the deduce request, or registered as a CLIPS source via
`POST /source` and referenced by `clips_source_id`. The `_clara.dot` can be
rendered with Graphviz for a static dependency overview of the rule set.

> **Note:** Feed plain (undecorated) Prolog rules as input. The tool prepends
> the relay scaffolding; do not pre-decorate the source yourself.

---

### `baloroptik` — trace visualization

`baloroptik` ("eye of Balor") reads persisted deduction state and emits a
sequence of colored DOT graphs representing the reasoning trace. Five subcommands
cover offline, online, streaming, and replay scenarios.

#### `baloroptik file <SNAPSHOT>` — offline, final state

Reads a deduction snapshot JSON file and generates **one** DOT graph colorized
with the final Dagda tableau truth values. No running server required.

```bash
baloroptik file deduction_snapshots.json --out-dir ./eye
# Writes: ./eye/7cf5e9cf_final.dot

baloroptik file deduction_snapshots.json --format html --out-dir ./eye
# Writes: ./eye/7cf5e9cf_final.html  (viz.js browser viewer)
```

Output filename: `<first-8-chars-of-deduction-id>_final.<ext>`.

```
Deduction: 7cf5e9cf-1052-4435-b8fa-0c7c7e6cd371
Status:    converged (3 cycles)
Goal:      omelette(bob, X).
Tableau:   6 entries  (T:5 F:1 U:0 ?:0)

Wrote: ./eye/7cf5e9cf_final.dot
```

#### `baloroptik trace <DEDUCTION_ID>` — online, full sequence

Queries a running `clara-api` instance and downloads the pre-colorized DOT for
each recorded trace phase. Requires `trace: true` on the original run.

```bash
baloroptik trace 550e8400-e29b-41d4-a716-446655440000 \
    --api http://localhost:8080 \
    --out-dir ./eye \
    --format html
# Writes: ./eye/550e8400_trace.html  (step-through browser viewer)
```

For non-HTML formats, output files are named `<stem>_<i:03>_<phase>.<ext>`:

```
550e8400_000_initial.dot
550e8400_001_prolog_to_clips.dot
550e8400_002_clips_to_prolog.dot
550e8400_003_prolog_to_clips.dot
550e8400_004_final_converged.dot
```

The DOT string for each phase is fetched from
`GET /deduce/{id}/trace/{change_id}/dot` — the API applies
`coloring_from_entries` server-side, so no local re-parsing is needed.

#### `baloroptik list` — enumerate persisted deductions

Lists recent deductions from a running `clara-api` instance, newest first.
Useful for discovering UUIDs to pass to `trace`, `watch`, or the API directly.

```bash
baloroptik list --api http://localhost:8080 --limit 20
```

```
  Deduction ID                           Status          Cycles  Goal                         Created (UTC)
  ──────────────────────────────────────  ────────────    ──────  ──────────────────────────   ───────────────────────
  550e8400-e29b-41d4-a716-446655440000   converged            3   mortal(X)                   2026-04-10 14:32:00Z
  7cf5e9cf-1052-4435-b8fa-0c7c7e6cd371  converged            3   omelette(bob, X).           2026-04-10 13:55:00Z
```

Calls `GET /deduce?limit=N`. Requires persistence to be enabled on the server.

#### `baloroptik watch <DEDUCTION_ID>` — live stream

Polls a running deduction and writes DOT files to disk as each trace phase is
recorded. Exits automatically once the deduction reaches a terminal status.

```bash
baloroptik watch 550e8400-... \
    --api http://localhost:8080 \
    --out-dir ./eye \
    --poll-ms 500
```

Files arrive with the same `<stem>_<i:03>_<phase>.dot` naming as `trace`.
`--format html` is not supported in watch mode (HTML output requires all phases
at once); a warning is printed and the format falls back to `dot`.

#### `baloroptik replay <SNAPSHOT> <CHANGES>` — offline, full sequence

Replays a complete trace entirely offline — no running server. Takes a snapshot
JSON file and a `tableau_changes` export (obtained via
`GET /deduce/{id}/trace/export`), re-generates DOTs locally using
`coloring_from_entries` + `generate_dot`.

```bash
# Export the changes first
curl http://localhost:8080/deduce/<UUID>/trace/export > changes.json

# Replay offline (server can be down)
baloroptik replay deduction_snapshots.json changes.json \
    --out-dir ./eye \
    --format html
# Writes: ./eye/<stem>_replay.html
```

#### Common options

| Option | Default | Effect |
|--------|---------|--------|
| `--out-dir <DIR>` | `.` | Directory for output files (created if absent) |
| `--format dot\|svg\|both\|html` | `dot` | Output format; `html` generates a self-contained viz.js step-through viewer |
| `--link-shared` | off | Add shared-condition edges (`DotOptions.link_shared_conditions`) |
| `--api <URL>` | `http://localhost:8080` | clara-api base URL (online subcommands) |
| `--limit <N>` | `50` | Max results for `list` (server caps at 500) |
| `--poll-ms <MS>` | `500` | Polling interval in ms for `watch` |

SVG rendering requires Graphviz (`dot` in PATH); if not found a warning is
printed and only the `.dot` file is written.

The HTML viewer (`--format html`) is self-contained: viz.js is loaded from
CDN, DOT sources are embedded as JS strings, and the file works offline once
loaded. Navigation: Prev/Next buttons or arrow keys; a phase strip at the
bottom allows direct tab jumps.

---

## Integration with clara-cycle / Deduce API

### Using `clips_file` (server-side path)

The transduced `.clp` file can be passed to a deduce request via `clips_file`.
Clara-cycle loads it before any `clips_constructs` so the defrules are in place
when the relay begins asserting facts.

```json
{
  "prolog_clauses": [
    "fire(Where) :- smoke(Where).",
    "alarm(Where) :- smoke(Where)."
  ],
  "initial_goal": "fire(kitchen)",
  "clips_file":   "/path/to/fire_alarm_transduced.clp",
  "max_cycles":   50
}
```

### Using registered sources (preferred for trace visualization)

Register both the Prolog and CLIPS sources via `POST /source` and supply their
IDs in the deduce request. This enables:

- **Content-addressed dedup** — the same source uploaded twice returns the
  same ID without duplicating storage.
- **Artifact caching** — the first `GET /deduce/{id}/trace/{change_id}/dot`
  call parses the Prolog source and caches the result as a `"parsed_rules"`
  artifact. Subsequent calls deserialize the cached JSON instead of re-parsing.
- **Colorized DOT graphs** — truth values from the Dagda tableau are overlaid
  on the cached rule graph at each recorded phase.

```bash
# Register Prolog source
PROLOG_SRC=$(curl -s -X POST http://localhost:8080/source \
  -H 'Content-Type: application/json' \
  -d '{
    "source_type": "prolog",
    "label":       "fire_alarm",
    "content":     "fire(Where) :- smoke(Where).\nalarm(Place) :- fire(Place)."
  }' | jq -r .source_id)

# Register CLIPS source
CLIPS_SRC=$(curl -s -X POST http://localhost:8080/source \
  -H 'Content-Type: application/json' \
  -d '{
    "source_type": "clips",
    "label":       "fire_alarm_transduced",
    "content":     "(defrule transduced-fire-on-smoke-0 ...)"
  }' | jq -r .source_id)

# Run a traced deduction
curl -s -X POST http://localhost:8080/deduce \
  -H 'Content-Type: application/json' \
  -d "{
    \"prolog_source_id\": \"$PROLOG_SRC\",
    \"clips_source_id\":  \"$CLIPS_SRC\",
    \"initial_goal\":     \"fire(kitchen)\",
    \"trace\":            true,
    \"persist\":          true
  }"
```

### Trace playback workflow

```
GET  /deduce                           →  list persisted deductions (pick a UUID)
POST /source                           →  register Prolog source, get source_id
POST /deduce                           →  prolog_source_id + trace: true + persist: true
GET  /deduce/{id}                      →  poll until converged
GET  /deduce/{id}/trace                →  list phases (change_id per phase)
GET  /deduce/{id}/trace/{cid}/dot      →  colorized DOT (text/plain)
GET  /deduce/{id}/trace/{cid}/entries  →  raw PredicateEntry slice
GET  /deduce/{id}/trace/export         →  full Vec<TableauChange> for offline replay
```

The DOT endpoint:

1. Loads the cached `"parsed_rules"` artifact for the deduction's `prolog_source_id`.
2. If the parsed rules contain only bare facts (i.e. the registered source was
   inline `prolog_clauses` rather than a full rule file), follows any
   `consult(file)` entries via `extract_consulted_files` and extends the rule
   set by parsing those files from disk.
3. Calls `coloring_from_entries` on the stored tableau entries for that phase.
4. Calls `generate_dot` with the combined rule set and the coloring.

Feed the output to Graphviz or `@viz-js/viz` in the browser.

`baloroptik` automates the last four steps: `baloroptik trace` fetches all
phase DOTs in one command; `baloroptik replay` replays a previously exported
trace entirely offline.

---

## Serialization

`PrologRule`, `BodyGoal`, and `Term` all derive `serde::Serialize` /
`serde::Deserialize`. This enables JSON round-trips used by the source artifact
cache:

1. First trace request for a deduction → `parse_prolog_rules(source_content)` runs.
2. Result serialized to JSON, stored in `source_artifacts` as type `"parsed_rules"`.
3. Subsequent requests → deserialize from cache, skip re-parsing.

The same JSON representation is also returned by
`GET /source/{id}/artifact/parsed_rules` for offline inspection.

---

## Parser Behaviour and Limitations

The rule parser handles:

| Construct | Supported |
|-----------|-----------|
| `head :- body.` | Yes |
| `head.` (bare fact) | Yes (skipped in transduction output; included in DOT as fact node) |
| `,` conjunctions | Yes — each goal is an independent trigger |
| `;` disjunctions | Yes — flattened, same as conjunction |
| `\+` negation | Yes — skipped as trigger, comment emitted |
| `% line comments` | Yes — skipped |
| Blank lines | Yes — skipped |
| Quoted atoms (`'foo bar'`) | Yes |
| Variables (`X`, `_Anon`) | Yes |
| Integers and floats | Yes |
| Double-quoted strings | Yes |
| Empty list `[]` | Yes |
| Non-empty lists | No — clause skipped with error recovery |
| Nested compound args | Yes — rendered as string literals |
| Parenthesised sub-bodies | Partially — outermost term parsed; complex nested bodies may be skipped |
| Directives (`:- module(...)`) | Skipped via error recovery |

Clauses that fail to parse are silently skipped; the parser advances to the
next `.` and continues.

---

## Counter and Rule Naming

Defrule names follow the pattern:

```
transduced-<head_functor>-on-<trigger_functor>-<N>
```

`N` is a global counter that increments for every defrule emitted across all
rules in the file. This guarantees unique names within a single transduction
run. If you concatenate transduced files, ensure names do not collide (e.g.
pre-process each file with a unique prefix).

---

## Relevant Source Files

### Library / API

| File | Purpose |
|------|---------|
| `clara-cycle/src/transduction.rs` | Parser, CLIPS code generator, DOT generator, `NodeColoring`, `coloring_from_entries`, `extract_consulted_files`, `propagate_rule_coloring`, public API |
| `clara-cycle/src/transpile.rs` | `Term` AST (with `Serialize`/`Deserialize`), `render_clips_fact`, `render_prolog_term` (shared) |
| `clara-cycle/src/session.rs` | `DeductionSession`, `seed_tableau_from_source`, `record_event_in_tableau` |
| `clara-cycle/src/controller.rs` | `CycleController::run`, `re_evaluate_root_goal`, `record_tableau` |
| `clara-coire/src/source.rs` | `SourceRegistry` — `get_or_create_artifact` for `"parsed_rules"` and `"dot"` caching |
| `clara-coire/src/store.rs` | `CoireStore::list_snapshots` — list persisted deductions newest-first |
| `clara-api/src/handlers/deduce_handler.rs` | `list_deductions`, `resolve_prolog_source` (auto-registers inline clauses) |
| `clara-api/src/handlers/trace_handler.rs` | `list_trace`, `trace_dot` (follows `consult` directives, colors from tableau), `trace_entries`, `export_trace` |
| `clara-api/src/handlers/source_handler.rs` | `register_source`, `get_source`, `get_source_artifact`, `delete_source` HTTP handlers |

### CLI binaries

| File | Purpose |
|------|---------|
| `clara-transduction/src/main.rs` | `transduction` CLI — parses `.pl`, writes `_clara.pl`, `_clara.clp`, `_clara.dot` |
| `clara-transduction/Cargo.toml` | `transduction` binary crate manifest |
| `clara-baloroptik/src/main.rs` | `baloroptik` CLI — clap entry point, all five subcommands |
| `clara-baloroptik/src/file_mode.rs` | `baloroptik file` — offline snapshot → single colorized DOT or HTML |
| `clara-baloroptik/src/trace_mode.rs` | `baloroptik trace` — fetches phase DOTs from clara-api, writes sequenced files or HTML |
| `clara-baloroptik/src/list_mode.rs` | `baloroptik list` — prints summary table of persisted deductions |
| `clara-baloroptik/src/watch_mode.rs` | `baloroptik watch` — polls live deduction, streams DOT files as phases arrive |
| `clara-baloroptik/src/replay_mode.rs` | `baloroptik replay` — offline trace replay from snapshot + `tableau_changes` export |
| `clara-baloroptik/src/render.rs` | Shared output utilities: `write_dot`, `render_svg`, `write_html` (viz.js viewer), summary printers |
| `clara-baloroptik/Cargo.toml` | `baloroptik` binary crate manifest |
