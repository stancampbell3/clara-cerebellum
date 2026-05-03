# Stoke the Coals — Clara Front Desk POC Update

**Branch:** `housbonde_lif`
**Date:** 2026-05-03
**Status:** Planned — ready to implement

---

## Summary

The Rust backend, frontend, and config are complete and implement the plan from
`front_desk_poc_update.md`. The remaining work is the Prolog rule layer: both
`.pl` files and the transduced `.clp` still use a side-effect-heavy routing
mechanism that the plan replaces with a clean `admit/2 + redirect/3` design.

---

## What Is Already Done

| Area | File(s) | Status |
|---|---|---|
| Single deduce call per turn via `daemonic_turn/5` | `src/ws.rs` | ✅ Done |
| `interpret_decision()` — atom matching, no string heuristics | `src/ws.rs` | ✅ Done |
| Patience counter + patience-exhausted `Denied` terminal path | `src/ws.rs`, `src/session.rs` | ✅ Done |
| Terminal WS frame with `type`, `status`, `reason`, `where` | `src/ws.rs` | ✅ Done |
| Outcome screen — medieval fonts, SVG gate graphics, three states | `static/index.html` | ✅ Done |
| Minos image state machine (6 states, fade transition) | `static/index.html` | ✅ Done |
| `model` + `patience` in config; `DevilishSupervisorConfig` | `src/config.rs`, both TOMLs | ✅ Done |
| `devilish_supervisor` prompt as deduction system prompt | `src/ws.rs`, `src/session.rs` | ✅ Done |
| `persist` flag wired through to `/deduce` | `src/deduce.rs`, `src/ws.rs` | ✅ Done |
| `evaluate_tephra()` typed response extraction | `src/ws.rs` | ✅ Done |
| `extract_named_solutions`, `extract_list_var`, `extract_str_var` | `src/deduce.rs` | ✅ Done |

---

## What Needs to Change

### Problem: `where_to_go/1` side-effect routing

Both `front_desk_poc_reprise.pl` (source) and `front_desk_poc_reprise_clara.pl`
(transduced) use an indirect routing mechanism:

1. `suggestion/2` and `admit/2` rules call `assertz(where_to_go(…))` as side
   effects during evaluation.
2. `effective_decision/2` reads those asserted facts to determine `Decision`.
3. `daemonic_turn/5` calls `admit(Visitor, Reason)` first (binding `Reason`) and
   then calls `effective_decision(Decision, Where)` separately — so `Reason`
   is always from an `admit/2` clause even when the effective decision is
   `redirected` or `pending`.

**Concrete bugs:**

- Rule 5 in `admit/2` (lost/confused) asserts `where_to_go('redirected')` instead
  of being a proper `redirect/3` clause. A redirected visitor still routes through
  `admit/2`, which is semantically wrong.
- `suggestion/2` rules 2, 3, 6 assertz `where_to_go` side effects. Suggestions
  are meant to be pure guidance for Agent Minos — they must not drive routing.
- Within a single `daemonic_turn/5` call, `findall` runs all `suggestion/2` rules
  (potentially assertz-ing routing facts), then `admit/2` may assertz more. The
  `where_to_go` accumulation within one call is non-deterministic in practice.
- `where_to_go` has no cleanup between calls (each `/deduce` gets a fresh session,
  so it doesn't bleed across turns, but it's still design debt).

---

## Planned Changes

### 1. `roost/front_desk_poc_reprise.pl` — rewrite rules

**Remove:**
- `where_to_go/1` dynamic declaration
- `effective_decision/2` predicate
- All `assertz(where_to_go(…))` calls from `admit/2` and `suggestion/2`
- Rule 5 from `admit/2` (lost/confused → redirect)

**Add:**
- `redirect/3` predicate for the lost/confused case

**Update:**
- `daemonic_turn/5` to use the direct pattern:

```prolog
daemonic_turn(Visitor, Suggestions, Decision, Reason, Where) :-
    visitor(Visitor),
    findall(S, suggestion(Visitor, S), Suggestions),
    (   admit(Visitor, R)        -> Decision = admitted,   Reason = R, Where = ''
    ;   redirect(Visitor, R, W)  -> Decision = redirected, Reason = R, Where = W
    ;                               Decision = pending,    Reason = '', Where = ''
    ).
```

- `admit/2` rules — remove `assertz(where_to_go(…))` tail, clean reason strings
  (drop "Grant entry." suffix; the `Decision = admitted` atom carries that semantic):

```prolog
admit(Visitor, 'Summoned visitor bearing the required artifacts.') :-
    visitor(Visitor),
    summoned_by(Visitor, _Official),
    (   findall(A, has_artifact(Visitor, A), Arts), length(Arts, N), N >= 3
    ;   meets_condition(Visitor, "Does the visitor claim to possess the three required artifacts for their summons?")
    ).

admit(Visitor, 'Bearer of an urgent message who came directly, without prior stops.') :-
    visitor(Visitor),
    urgent_message(Visitor),
    (   \+ stopped_elsewhere(Visitor)
    ;   meets_condition(Visitor, "Does the visitor assert they came directly here without stopping elsewhere?")
    ).

admit(Visitor, 'Flamefruit carrier arriving before sundown.') :-
    visitor(Visitor),
    (   carries_flamefruit(Visitor)
    ;   meets_condition(Visitor, "Is the visitor carrying the rare Flamefruit?")
    ),
    \+ after_sundown.

admit(Visitor, 'Bearer of critical intelligence who has proven reliability.') :-
    visitor(Visitor),
    (   has_critical_info(Visitor)
    ;   meets_condition(Visitor, "Does the visitor claim to possess critical information for the City?")
    ),
    (   performed_task(Visitor)
    ;   meets_condition(Visitor, "Has the visitor demonstrated reliability by completing the requested task?")
    ).
```

- New `redirect/3`:

```prolog
redirect(Visitor, 'This visitor appears lost or confused and cannot proceed.', 'nearest map kiosk') :-
    visitor(Visitor),
    (   lost_or_confused(Visitor)
    ;   meets_condition(Visitor, "Does the visitor appear lost or confused about where they are or why they are here?")
    ).
```

- `suggestion/2` — remove all `assertz(where_to_go(…))` calls; all rules become
  pure queries:

```prolog
suggestion(Visitor, 'Greet the visitor.') :-
    visitor(Visitor), \+ greeted(Visitor).

suggestion(Visitor, 'Direct the visitor to the nearest map kiosk.') :-
    visitor(Visitor),
    (   lost_or_confused(Visitor)
    ;   meets_condition(Visitor, "Based on the conversation so far, does the visitor seem lost or confused?")
    ).

suggestion(Visitor, 'Request the three required artifacts for summoned visitors.') :-
    visitor(Visitor),
    summoned_by(Visitor, _),
    findall(A, has_artifact(Visitor, A), Artifacts),
    length(Artifacts, N), N < 3.

suggestion(Visitor, 'Verify that the visitor made no prior stops before delivering their urgent message.') :-
    visitor(Visitor),
    urgent_message(Visitor),
    \+ stopped_elsewhere(Visitor).

suggestion(Visitor, 'Ask the visitor to perform a simple reliability task.') :-
    visitor(Visitor),
    has_critical_info(Visitor),
    \+ performed_task(Visitor).

suggestion(Visitor, 'Advise the visitor to wait until dawn before entry.') :-
    visitor(Visitor),
    carries_flamefruit(Visitor),
    after_sundown.
```

---

### 2. `roost/front_desk_poc_reprise_clara.pl` — update transduced file

Same rule changes as above, plus updates to the Clara integration header:

**Remove from header:**
- `where_to_go/1` from `:- dynamic(…)` and `:- prolog_listen(…, updated(…))` listings

**Add to synthetic groups section:**
```prolog
:- dynamic(redirect/3).
:- prolog_listen(redirect/3, updated(redirect/3)).
```

---

### 3. `roost/front_desk_poc_reprise_clara.clp` — add `redirect/3` forward chaining

Add CLIPS rules to re-evaluate `redirect/3` when relevant facts arrive via coire
(following the same pattern as the existing `suggestion/2` rules):

```clp
; Transduced from: redirect(Visitor,Reason,Where) :- visitor(Visitor),
;     (lost_or_confused(Visitor) ; meets_condition(Visitor,"..."))
(defrule transduced-redirect-on-visitor-30
    (visitor ?Visitor)
    =>
    (coire-publish-goal (str-cat "redirect(" ?Visitor ",_Reason,_Where)")))

(defrule transduced-redirect-on-lost_or_confused-31
    (lost_or_confused ?Visitor)
    =>
    (coire-publish-goal (str-cat "redirect(" ?Visitor ",_Reason,_Where)")))
```

Clean up source comments on existing `suggestion/2` rules to remove references
to the old `assertz(where_to_go(…))` pattern.

---

## File Change Map

| File | Change |
|---|---|
| `roost/front_desk_poc_reprise.pl` | Remove `where_to_go/1`, `effective_decision/2`, all `assertz(where_to_go)` side effects; add `redirect/3`; clean `admit/2` reason strings; pure `suggestion/2` |
| `roost/front_desk_poc_reprise_clara.pl` | Same rule changes + update Clara integration header: remove `where_to_go` listener, add `redirect/3` synthetic group |
| `roost/front_desk_poc_reprise_clara.clp` | Add `redirect/3` forward chaining rules; update source comments |

No Rust or frontend changes required.

---

## Out of Scope (this feature)

- `fiery-pit-client` typed additions (`PrologQueryResponse`, `from_env()`) — not needed for demo
- KindlingEvaluator context-drop bug (tracked separately in lildaemon)
- Coire-based fact accumulation across turns — deferred sprint
- `NewFacts` propagation from deduction back into session — deferred sprint
