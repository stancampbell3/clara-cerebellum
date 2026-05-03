# Transduction — Role-Aware Forward Chaining

**Branch:** `housbonde_lif`
**Date:** 2026-05-03
**Status:** Planning — deferred until `transduction_first_class.md` Paths A–C are stable
**Follows:** `transduction_first_class.md`

---

## The Problem

The transducer currently generates the same CLIPS forward-chaining pattern for every
multi-clause predicate. When a relevant Prolog fact is asserted via the coire relay,
a CLIPS rule fires and pushes a goal back to Prolog for re-evaluation.

For `suggestion/2` this is correct. For `admit/2` and `redirect/3` it is not.

### Why suggestion works

When `lost_or_confused(visitor)` is asserted into the Prolog knowledge base, a CLIPS
rule fires:

```clp
(defrule transduced-suggestion-on-lost_or_confused-17
    (lost_or_confused ?Visitor)
    =>
    (coire-publish-goal "suggestion(visitor,'Direct the visitor to the nearest map kiosk.')"))
```

Prolog re-evaluates `suggestion(visitor,'Direct the visitor to the nearest map kiosk.')`,
it succeeds, and the suggestion is established as a live Prolog fact. The cycle sees a
new suggestion in the knowledge base. This is the correct behaviour — suggestions are
guidance queries and re-evaluating them in isolation makes sense.

### Why admit and redirect do not work the same way

The transduced CLIPS rule for `admit/2` would fire when `visitor(visitor)` is asserted
and push something like:

```clp
(coire-publish-goal "admit(visitor,'Summoned visitor bearing the required artifacts.')")
```

Prolog re-evaluates `admit(visitor,...)`, and it may succeed or fail. But this
re-evaluation has no effect on the decision state. The decision state lives in
`daemonic_turn/5`:

```prolog
daemonic_turn(Visitor, Suggestions, Decision, Reason, Where) :-
    visitor(Visitor),
    findall(S, suggestion(Visitor, S), Suggestions),
    (   admit(Visitor, R)        -> Decision = admitted,   Reason = R, Where = ''
    ;   redirect(Visitor, R, W)  -> Decision = redirected, Reason = R, Where = W
    ;                               Decision = pending,    Reason = '', Where = ''
    ).
```

`Decision`, `Reason`, and `Where` are only bound when `daemonic_turn/5` runs. A
standalone probe of `admit/2` by a CLIPS rule does not update those bindings. The cycle
may have already converged with `Decision = pending` before the CLIPS rule fires — and
even if it fires mid-cycle, the result of the isolated probe is not fed back into
`daemonic_turn/5`.

**The structural issue:** `admit/2` and `redirect/3` are not standalone queries. They
are sub-goals of an entry point predicate. Re-evaluating them in isolation, outside
that entry point, is semantically wrong. What the cycle needs when an admit-relevant
fact changes is a fresh `daemonic_turn/5` evaluation — not a probe of `admit` in
isolation.

---

## Predicate Roles

Three roles are present in the front desk Prolog source:

| Role | Examples | Semantics |
|---|---|---|
| `entry_point` | `daemonic_turn/5` | The root goal called once per reasoning turn. No CLIPS rules should target this predicate — it is the Prolog-side entry to the cycle. |
| `decision` | `admit/2`, `redirect/3` | Sub-goals of the entry point evaluated in priority order. When a decision-relevant fact changes, the correct response is to re-run the entry point, not probe the decision predicate in isolation. |
| `suggestion` | `suggestion/2` | Pure guidance queries. Re-evaluating in isolation when relevant facts change is correct and sufficient. |

The current transducer has no concept of these roles. It generates the `suggestion`
pattern for everything.

---

## The Fix: Role-Aware CLIPS Generation

For each role, the transducer should generate different CLIPS behaviour:

**`suggestion` — unchanged (current behaviour)**
CLIPS rules push the specific grounded suggestion goal. The suggestion is re-established
as a Prolog fact. No change needed.

**`entry_point` — no CLIPS rules generated**
The entry point is the Prolog-side root. CLIPS rules targeting it directly would cause
re-entrancy. Nothing is generated for entry point predicates.

**`decision` — CLIPS rules push the entry point goal, not the decision predicate**
When an admit-relevant or redirect-relevant fact changes, the CLIPS rule pushes
`daemonic_turn/5` (the entry point goal) rather than `admit/2` or `redirect/3`:

```clp
; When summoned_by becomes known, re-run the full decision turn.
(defrule role-decision-admit-on-summoned_by
    (summoned_by ?Visitor ?)
    =>
    (coire-publish-goal (str-cat "daemonic_turn(" ?Visitor ",_Suggestions,_Decision,_Reason,_Where)")))
```

This means any new fact relevant to admittance or redirect triggers a complete fresh
`daemonic_turn/5` evaluation — `admit/2`, `redirect/3`, and `suggestion/2` all
re-evaluated in the correct relational context.

### Triggering-fact analysis for decision predicates

The transducer needs to identify which facts trigger re-evaluation of each decision
predicate. This is the same analysis it already does for `suggestion` predicates — walk
the rule bodies and collect the predicates whose assertion should fire the CLIPS rule.

For `admit/2` the triggering facts across all four rules are:
`visitor/1`, `summoned_by/2`, `has_artifact/2`, `urgent_message/1`,
`stopped_elsewhere/1`, `carries_flamefruit/1`, `after_sundown/0`,
`has_critical_info/1`, `performed_task/1`

For `redirect/3`:
`visitor/1`, `lost_or_confused/1`

Each triggering fact gets a CLIPS rule that pushes `daemonic_turn/5`.

---

## Role Annotation: Bridge Solution for Hand-Written Prolog

Since roles cannot currently be inferred structurally, they are declared via source
annotations:

```prolog
%% @clara:role entry_point
daemonic_turn(Visitor, Suggestions, Decision, Reason, Where) :- ...

%% @clara:role decision
admit(Visitor, Reason) :- ...

%% @clara:role decision
redirect(Visitor, Reason, Where) :- ...

%% @clara:role suggestion
suggestion(Visitor, Text) :- ...
```

The transducer reads these annotations and adjusts generation accordingly. Predicates
with no annotation fall back to the current `suggestion` pattern (safe default — it
never produces wrong cycle behaviour, only suboptimal behaviour).

### Annotation scope

A single `@clara:role` annotation before the first clause of a predicate covers all
clauses of that predicate. The transducer matches on `name/arity`.

### Entry point goal construction

For `decision` predicates, the transducer needs to construct the entry point goal
string. The `entry_point` annotation on `daemonic_turn/5` tells it which predicate and
arity to target. For variable positions in the entry point goal, all non-visitor output
variables are anonymous (`_Name` form), consistent with the C-vars decision in
`transduction_first_class.md`.

---

## Cycle Controller Implications

When CLIPS pushes `daemonic_turn(visitor,_Suggestions,_Decision,_Reason,_Where)` as a
coire goal, Prolog evaluates it. The result — including `Decision`, `Reason`,
`Suggestions`, and `Where` bindings — becomes available in the Prolog knowledge base
for the remainder of the cycle.

Two questions about the cycle controller:

**Q1: Does the cycle controller currently re-run the root goal when a coire goal
succeeds mid-cycle?**

If yes, and if the root goal is `daemonic_turn/5`, then pushing `daemonic_turn` via
CLIPS naturally produces a fresh decision result without any cycle controller changes.

If no — if the root goal is only evaluated at the start of each cycle — then the coire
goal would evaluate `daemonic_turn` as a sub-goal but the result might not be captured
as the deduction's final decision. This needs confirmation before implementing.

**Q2: Can the same predicate be both the coire goal (pushed by CLIPS) and the root goal
(evaluated by the cycle controller)?**

If re-entrancy is guarded, this is fine. If not, a mid-cycle CLIPS push of `daemonic_turn`
while the cycle is already evaluating `daemonic_turn` as the root goal could cause
unexpected behaviour.

These questions need to be answered against the current `clara-cycle` source before
implementing. The answers may change the approach — for instance, rather than pushing
the entry point goal directly, decision-predicate CLIPS rules could push a synthetic
`reconsider(visitor)` fact that the cycle controller recognises as a trigger to
re-evaluate the root goal at the start of the next cycle.

---

## Relationship to CAWS

In hand-written Prolog, roles are declared by annotation — a workaround for the absence
of structural role information in plain Prolog syntax.

In CAWS, roles are structural. A CAWS `decision` predicate is typed as a sub-goal of a
reasoning chain entry point. A `suggestion` predicate is typed as a pure query. The
transducer receives this information from the CAWS type system, not from inline
comments. There is no annotation step.

The annotation approach implemented here is therefore a bridge: it formalises the role
concept in a way that works for hand-written Prolog today and maps cleanly onto CAWS
types when CAWS becomes the upstream source. The `@clara:role` annotations in Prolog
source can be seen as an early, informal CAWS type system expressed as comments.

---

## Implementation Order

1. **Resolve cycle controller questions (Q1, Q2 above)** — before writing any code.
   Read `clara-cycle/src/controller.rs` to confirm coire goal evaluation and root goal
   re-entry behaviour.

2. **Implement `@clara:role` annotation parsing in the transducer** — read annotation,
   tag predicate with role.

3. **Implement role-differentiated CLIPS generation** — `entry_point` → no rules;
   `decision` → entry point goal; `suggestion` → unchanged.

4. **Update `front_desk_poc_reprise.pl`** — add `@clara:role` annotations to
   `daemonic_turn/5`, `admit/2`, `redirect/3`, `suggestion/2`.

5. **Regenerate with `make transduct`** — verify generated CLIPS rules match
   expectation.

6. **Run smoke tests** — confirm `daemonic_turn/5` still converges correctly with the
   new forward-chaining rules; confirm `redirect/3` triggers a full turn re-evaluation
   when `lost_or_confused` is asserted mid-cycle.

---

## Open Questions

| # | Question | Needed before |
|---|---|---|
| 1 | Does the cycle controller re-evaluate the root goal when a coire goal succeeds mid-cycle? | Step 1 above |
| 2 | Is re-entrancy on `daemonic_turn/5` safe, or is a synthetic `reconsider` fact the better trigger? | Step 1 above |
| 3 | Should all triggering facts for `decision` predicates share a single CLIPS rule per fact, or one rule per (predicate, fact) pair? | Step 3 above |
| 4 | Do `@clara:role` annotations belong on each clause or once per predicate name? | Step 2 above |
