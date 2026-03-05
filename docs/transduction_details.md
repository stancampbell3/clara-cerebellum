# Prolog → CLIPS Transduction

Transduction generates CLIPS `defrule`s from Prolog rules so that CLIPS's
forward-chaining engine can speculatively push head goals back to Prolog
whenever any body condition is asserted as a CLIPS fact. The result is
**agenda-driven, partial-information reasoning**: Prolog is asked to prove a
goal even when only one of its preconditions is currently known.

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
  PrologRule { head, body: [BodyGoal::Positive, ...] }
      │
      ▼  transduce()
  CLIPS defrule source (.clp)
      │
      │  loaded by clara-cycle before CLIPS constructs at runtime
      │
      │  assert(smoke(kitchen)) → relay → (smoke kitchen) in CLIPS
      │    ↳ transduced-fire-on-smoke-0 fires
      │    ↳ (coire-publish-goal "fire(kitchen)")
      │    ↳ relay forwards goal event → Prolog
      │    ↳ consume_coire_events() calls fire(kitchen)
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

The decorated `.pl` is the file you load into SWI-Prolog. The `.clp` is referenced
in the `clips_file` field of the deduce request (see **Integration** below).

> **Note:** Feed `--decorate` plain (undecorated) Prolog rules. If a rule already
> contains `coire_publish_assert` in its body it will be treated as a regular goal
> and an additional decoration will be appended — exactly like any other predicate.
> `test4.pl` shows what the decorated output is meant to look like, not a valid input.

---

## Integration with clara-cycle / Deduce API

The transduced `.clp` file is passed to a deduce request via the `clips_file`
option. Clara-cycle loads it before any `clips_constructs` so the defrules are
in place when the relay begins asserting facts.

Example deduce request body:

```json
{
  "clauses": [
    "fire(Where) :- smoke(Where).",
    "alarm(Where) :- smoke(Where)."
  ],
  "goal": "fire(kitchen)",
  "clips_file": "/path/to/fire_alarm_transduced.clp",
  "clips_constructs": [],
  "max_cycles": 50
}
```

The typical workflow:

1. Author Prolog rules in a `.pl` file.
2. Run `transduction rules.pl rules.clp` to generate the CLIPS defrules.
3. Place `rules.clp` where the server can read it.
4. Reference it in the `clips_file` field of the deduce request.
5. At runtime, when Prolog asserts a fact that matches a rule body, CLIPS fires
   the corresponding defrule and pushes the head goal back to Prolog.

---

## Parser Behaviour and Limitations

The rule parser handles:

| Construct | Supported |
|-----------|-----------|
| `head :- body.` | Yes |
| `head.` (bare fact) | Yes (skipped in output) |
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
| `clara-cycle/src/transduction.rs` | Parser, code generator, public API |
| `clara-cycle/src/transpile.rs` | `Term` AST, `render_clips_fact`, `render_prolog_term` (shared) |
| `clara-transduction/src/main.rs` | CLI entry point |
| `clara-transduction/Cargo.toml` | Binary crate manifest |
