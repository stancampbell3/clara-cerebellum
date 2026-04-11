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
      ├──▶  transduce()
      │         CLIPS defrule source (.clp)
      │         loaded by clara-cycle before CLIPS constructs at runtime
      │
      │         assert(smoke(kitchen)) → relay → (smoke kitchen) in CLIPS
      │           ↳ transduced-fire-on-smoke-0 fires
      │           ↳ (coire-publish-goal "fire(kitchen)")
      │           ↳ relay forwards goal event → Prolog
      │           ↳ consume_coire_events() calls fire(kitchen)
      │
      └──▶  generate_dot(rules, coloring?, opts)
                DOT graph — node fill colors reflect Dagda truth values
                cached as "dot" / "parsed_rules" artifacts in source_registry
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
| Box | `#cfe2ff` (blue) | Rule head with at least one body goal |
| Dashed ellipse | `#fff3cd` (amber) | Leaf condition — not bridged to another head |

### Edge types

| Style | Color | Label | Meaning |
|-------|-------|-------|---------|
| Solid | Black | `requires` | Rule head → leaf condition |
| Solid | Blue | *(none)* | Assert-bridge: head A → head B when A's condition is asserted by B |
| Dashed | Blue | `chains-to` | Condition → rule head whose functor/arity it directly matches |
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

### `DotOptions`

| Field | Default | Effect |
|-------|---------|--------|
| `link_shared_conditions` | `false` | When `true`, adds dashed gray undirected edges between condition nodes that share the same label across rules. Useful for identifying shared sub-goals. |

---

## CLI Usage

```
transduction [--decorate] <input.pl> [output.clp]
```

### Without `--decorate` (CLIPS only)

- Reads Prolog source from `<input.pl>`
- Writes CLIPS defrules to `<output.clp>`, or to stdout if omitted
- Exits with code 1 on any I/O error

```bash
# Print CLIPS to stdout
transduction rules.pl

# Write CLIPS to file
transduction rules.pl rules.clp
```

### With `--decorate` (decorated pair)

Parses the input once, then writes **two files** beside the input:

| File | Contents |
|------|----------|
| `<stem>_clara.pl` | Prolog rules with `coire_publish_assert(Head)` appended to each rule body |
| `<stem>_clara.clp` | CLIPS defrules for speculative forward chaining |

Stdout is not used when `--decorate` is active.

```bash
transduction --decorate fire_alarm.pl
# Writes: fire_alarm_clara.pl  fire_alarm_clara.clp
```

**Input** (`fire_alarm.pl`):
```prolog
fire(Where) :- smoke(Where).
lemonade(Drink) :- sour(Drink), sweet(Drink).
```

**Output** (`fire_alarm_clara.pl`):
```prolog
fire(Where) :- smoke(Where), coire_publish_assert(fire(Where)).
lemonade(Drink) :- sour(Drink), sweet(Drink), coire_publish_assert(lemonade(Drink)).
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

The decorated `.pl` is the file you load into SWI-Prolog. The `.clp` is passed
as `clips_file` in the deduce request, or registered as a CLIPS source via
`POST /source` and referenced by `clips_source_id`.

> **Note:** Feed `--decorate` plain (undecorated) Prolog rules. If a rule already
> contains `coire_publish_assert` in its body it will be treated as a regular goal
> and an additional decoration will be appended. `test4.pl` shows what the
> decorated output is meant to look like, not a valid input.

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
POST /source          →  register Prolog source, get source_id
POST /deduce          →  prolog_source_id + trace: true + persist: true
GET  /deduce/{id}     →  poll until converged
GET  /deduce/{id}/trace               →  list phases
GET  /deduce/{id}/trace/{cid}/dot     →  colorized DOT (text/plain)
GET  /deduce/{id}/trace/{cid}/entries →  raw PredicateEntry slice
```

The DOT endpoint calls `coloring_from_entries` on the stored entries, applies
the resulting `NodeColoring` to the cached `Vec<PrologRule>`, and calls
`generate_dot`. Feed the output to Graphviz or `@viz-js/viz` in the browser.

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

| File | Purpose |
|------|---------|
| `clara-cycle/src/transduction.rs` | Parser, CLIPS code generator, DOT generator, `NodeColoring`, `coloring_from_entries`, public API |
| `clara-cycle/src/transpile.rs` | `Term` AST (with `Serialize`/`Deserialize`), `render_clips_fact`, `render_prolog_term` (shared) |
| `clara-transduction/src/main.rs` | CLI entry point |
| `clara-transduction/Cargo.toml` | Binary crate manifest |
| `clara-coire/src/source.rs` | `SourceRegistry` — `get_or_create_artifact` for `"parsed_rules"` and `"dot"` caching |
| `clara-api/src/handlers/trace_handler.rs` | `list_trace`, `trace_dot`, `trace_entries` HTTP handlers |
| `clara-api/src/handlers/source_handler.rs` | `register_source`, `get_source`, `get_source_artifact`, `delete_source` HTTP handlers |
