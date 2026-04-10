# Clara Front Desk POC — Implementation Plan (response by Stan and team)

**Branch:** `housbonde_lif`  
**Date:** 2026-04-07  

Responses are inline and marked as lines starting with 'STAN:'
---

## Overview

Three parallel work streams:

1. **`fiery-pit-client`** — Align the Rust client with the Python `lildaemon-client` where there are gaps.
2. **`clara-frontdesk-poc`** — Overhaul the conversational loop: single combined deduction per turn, structured context threading, reliable state transitions.
3. **State machine** — Make entry/exit states explicit, unambiguous, and driven solely by the deduction result — never by ad-hoc string matching.

---

## Stream 1 — `fiery-pit-client` Alignment

### Current state

The Rust client (`fiery-pit-client/src/lib.rs`) already covers the full API surface (56 methods) with typed response structs (`Tephra`, `Hohi`, `Tabu`). The Python `lildaemon-client` is a parallel sync wrapper over the same REST API. The gap is not missing endpoints but **consistency and correctness** in a few areas.

### Changes

#### 1.1 Use typed response extraction everywhere

`ws.rs` currently navigates the raw JSON response:
```rust
tephra["hohi"]["response"]["content"]
```

`evaluate_tephra()` already returns a typed `Tephra`, and `Tephra::response()` already traverses this path safely. All callers should use `evaluate_tephra()` + `Tephra::response()` — raw JSON navigation of the response envelope is eliminated.

**Action:** Replace `fp_client.evaluate(payload)` → `fp_client.evaluate_tephra(payload)`, use `tephra.response()` (or `tephra.into_response()`) for the content string.

#### 1.2 Typed Prolog query response

`prolog_query()` returns `Result<Value, FieryPitError>`. Prolog solutions have a consistent shape. Add a typed struct mirroring the Python `models.py`:

```rust
pub struct PrologQueryResponse {
    pub session_id: String,
    pub goal: String,
    pub solutions: Vec<HashMap<String, Value>>,
    pub all_solutions: bool,
}
```

Add `prolog_query_typed()` returning `Result<PrologQueryResponse, FieryPitError>`.  
Keep the raw `prolog_query()` for callers that need the full value.

#### 1.3 `from_env()` constructor

The Python `config.py` reads `LILDAEMON_URL` from the environment. The Rust client constructor takes only a plain `&str`. Add:

```rust
impl FieryPitClient {
    pub fn from_env() -> Result<Self, FieryPitError> {
        let url = std::env::var("FIERY_PIT_URL")
            .unwrap_or_else(|_| "http://localhost:6666".into());
        Ok(Self::new(&url))
    }
}
```

#### 1.4 REPL endpoints — do not add

The Python API has `/repl/` session endpoints used during development. Per project decision: **do not add REPL endpoints to the Rust client.** Application code must never use the REPL path.

#### 1.5 No async Rust client

The existing architecture uses blocking reqwest inside `spawn_blocking`. The Python `AsyncLilDaemonClient` has no Rust equivalent and **should not be added** — it would conflict with the actix-web actor model. The current `spawn_blocking` pattern is correct.

---

## Stream 2 — Conversational Loop Overhaul

### Current issues

#### 2.1 Two deduce calls per turn

`run_turn()` fires two sequential `run_deduce()` calls — one for `suggestion(visitor, S).` and one for `admit(visitor, Reason).` Each call is: HTTP POST → poll loop → HTTP GET (×N). This doubles latency, doubles Clara API load, and the two calls run in separate Prolog sessions with no shared state between them.

**Fix:** Collapse into a single `run_deduce()` call via a **meta-goal predicate** added to the Prolog rule file:

```prolog
%% Application entry point called once per conversation turn.
front_desk_turn(Visitor, Suggestions, Decision, Reason, Where) :-
    visitor(Visitor),
    findall(S, suggestion(Visitor, S), Suggestions),
    (   admit(Visitor, R)       -> Decision = admitted,   Reason = R, Where = ''
    ;   redirect(Visitor, R, W) -> Decision = redirected, Reason = R, Where = W
    ;                              Decision = pending,    Reason = '', Where = ''
    ).
```

The initial goal becomes:
```
front_desk_turn(visitor, Suggestions, Decision, Reason, Where).
```

All five variables are extracted from `prolog_solutions` in a single result. Logic stays in Prolog where it belongs; the Rust turn handler is simplified to one HTTP round-trip.

#### 2.2 Fragile string matching for state transitions

`interpret_admit()` checks `lower.contains("grant entry")` and `lower.contains("direct to")`. This breaks if Prolog atom wording changes.

**Fix:** The meta-goal above (§2.1) returns `Decision` as one of the atoms `admitted`, `redirected`, `denied`, or `pending`. Rust matches on this atom directly — no string contains logic anywhere.

```rust
fn interpret_decision(solutions: &[HashMap<String, Value>]) -> Option<VisitorStatus> {
    let sol = solutions.first()?;
    let reason = sol.get("Reason")?.as_str().unwrap_or("").to_string();
    let where_  = sol.get("Where")?.as_str().unwrap_or("").to_string();
    match sol.get("Decision")?.as_str()? {
        "admitted"   => Some(VisitorStatus::Admitted(reason)),
        "denied"     => Some(VisitorStatus::Denied(reason)),
        "redirected" => Some(VisitorStatus::Redirected(format!("{} → {}", reason, where_))),
        _            => None,
    }
}
```

#### 2.3 Context threading

The `/deduce` endpoint accepts a `context` field (full conversation history). `VisitorSession::deduce_context()` builds this correctly. However, the KindlingEvaluator bug (tracked in `memory/bug_kindling_evaluator_async_drops_context.md`) means context may be silently dropped by lildaemon for LLM sub-calls from within Prolog (`meets_condition/2`).

**No Rust-side workaround is possible.** Keep passing context. Add a code comment noting the dependency on the lildaemon fix. Once that fix is deployed, the `meets_condition` calls will automatically benefit.

#### 2.4 Facts not persisted across turns

When the Prolog deduction fires `meets_condition/2` and the LLM determines e.g. "visitor has critical info", that fact is not fed back into `VisitorSession.facts`. The next turn starts fresh, re-running the same LLM checks.

**Defer to v2.** The meta-goal could expose a `NewFacts` list, but this requires the Prolog rules to track what `meets_condition` established. For now, document the limitation.

### Updated `run_turn()` flow

```
1. Build prolog_clauses  (consult + visitor + known facts)
2. Build context         (system prompt + full conversation history)
3. POST /deduce          goal: "front_desk_turn(visitor, Suggestions, Decision, Reason, Where)."
4. Poll GET /deduce/{id} until status != "running"
5. Extract Suggestions   list from solutions["Suggestions"]
6. Extract Decision      atom from solutions["Decision"]
7. Map Decision          → VisitorStatus (admitted/denied/redirected/none)
8. Augment system prompt with suggestions + decision
9. POST /evaluate        with augmented payload
10. Tephra::response()   → assistant_text
11. Return TurnResult { assistant_text, new_status }
```

---

## Stream 3 — State Machine & Entry/Exit Contracts

### Visitor lifecycle

```
                     WebSocket connect
                            │
                     VisitorStatus::Active
                            │
               ┌────────────┴────────────┐
               │   per-turn loop          │
               │  (deduce → evaluate)     │
               └────────────┬────────────┘
                            │
          Decision atom from deduce result ONLY
          ┌─────────────────┼──────────────────┐
          │                 │                  │
       "admitted"        "denied"         "redirected"
          │                 │                  │
  VisitorStatus::    VisitorStatus::    VisitorStatus::
  Admitted(reason)   Denied(reason)     Redirected(reason)
          │                 │                  │
          └─────────────────┴──────────────────┘
                            │
                       is_terminal() == true
                            │
               WebSocket close + browser state change
```

**Rule:** The browser state changes to `admitted` (or `denied`/`redirected`) **if and only if**:
1. The deduce call returns `status: "converged"`
2. `prolog_solutions[0]["Decision"]` is `"admitted"`, `"denied"`, or `"redirected"`

No other code path may change visitor status. String matching on natural language is eliminated.

### Prolog rule changes

Restructure `admit/2` to use structured decision terms and add a `redirect/3` predicate:

```prolog
%% Rule 1: Summoned visitors with 3 artifacts OR who claim them.
admit(Visitor, 'Summoned visitor with required artifacts. Grant entry.') :-
    visitor(Visitor),
    summoned_by(Visitor, _Official),
    (   findall(A, has_artifact(Visitor, A), Arts), length(Arts, N), N >= 3
    ;   meets_condition(Visitor, "Does the visitor claim to possess the three required artifacts?")
    ).

%% ... (rules 2-4 updated similarly with clean reason strings) ...

%% Rule 5 becomes redirect, not admit:
redirect(Visitor, 'Visitor appears lost or confused.', 'map kiosk') :-
    visitor(Visitor),
    (   lost_or_confused(Visitor)
    ;   meets_condition(Visitor, "Does the visitor appear lost or confused about where they are?")
    ).

%% Meta-goal called once per conversation turn.
front_desk_turn(Visitor, Suggestions, Decision, Reason, Where) :-
    visitor(Visitor),
    findall(S, suggestion(Visitor, S), Suggestions),
    (   admit(Visitor, R)       -> Decision = admitted,   Reason = R, Where = ''
    ;   redirect(Visitor, R, W) -> Decision = redirected, Reason = R, Where = W
    ;                              Decision = pending,    Reason = '', Where = ''
    ).
```

The CLIPS file (`front_desk_poc_reprise_clara.clp`) must be regenerated from the updated Prolog via the Clara transducer.

### Browser-side state change

The WS frame already carries `"status"` on every message. Extend the server-side JSON to include `reason` and `where` on terminal frames:

```json
{
  "type":   "agent",
  "text":   "You may proceed. The gate will open.",
  "status": "admitted",
  "reason": "Visitor carries an urgent message and came directly.",
  "where":  ""
}
```

Update `static/index.html` to handle terminal states visually:

- `admitted` → green "ACCESS GRANTED" banner, disable input
- `denied` → red "ACCESS DENIED" banner, disable input  
- `redirected` → amber "REDIRECTED TO: {where}" banner, disable input

The badge already updates; add a full-width status panel above the input bar that appears only on terminal states.

---

## File Change Map

| File | Change |
|------|--------|
| `fiery-pit-client/src/lib.rs` | Add `PrologQueryResponse`, `prolog_query_typed()`, `FieryPitClient::from_env()` |
| `clara-frontdesk-poc/src/ws.rs` | Use `evaluate_tephra()`, single deduce call, `interpret_decision()` replaces `interpret_admit()`, extend WS message with `reason`/`where` |
| `clara-frontdesk-poc/src/deduce.rs` | Add `extract_named_solutions() -> HashMap<String, Value>` for multi-variable extraction |
| `clara-frontdesk-poc/src/session.rs` | Wire up `Denied` variant; no other structural changes needed |
| `clara-frontdesk-poc/roost/front_desk_poc_reprise_clara.pl` | Add `front_desk_turn/5`, refactor `admit/2` to clean reason strings, add `redirect/3` |
| `clara-frontdesk-poc/roost/front_desk_poc_reprise_clara.clp` | Regenerate from updated Prolog via Clara transducer |
| `clara-frontdesk-poc/static/index.html` | Terminal state panels, extend WS message parsing for `reason`/`where` |

---

## Out of Scope

- REPL API endpoints in the Rust client (development-only; not for application code)
- Async Rust client (conflicts with actor model)
- Multi-visitor sessions (single visitor per WebSocket is intentional)
- KindlingEvaluator context-drop bug fix (lives in lildaemon Python; tracked separately)
- Authentication / JWT on the front desk server (POC only)
- `NewFacts` propagation from deduce back into session (deferred to v2)
STAN: This is actually part of the Clara clara-cycle's processing of a deduce request.  The prolog source is transduced (clara-transduction) into prolog and CLIPS source which implements forward and backward chaining. Facts pushed onto the "coire" message system can be forwarded down cycle for CLIPS or other evaluators to operate upon.  CLIPS forward chaining can take a fresh asserted fact and yield a coire message for a new goal to Prolog on the next cycle.
So, this is already built in and is one of the main features we would love to highlight.

---

## Open Questions for Team

1. **Meta-goal naming:** `front_desk_turn/5` vs something more domain-neutral? If this pattern generalises to other agent POCs, a common convention helps.
STAN: good point.  let's use 'daemonic_turn/5' emphasizing that we are using context from the LLM (lildaemon mediated) conversation.

2. **`Denied` vs `Redirected`:** Currently `VisitorStatus::Denied` is declared but never set. If the Prolog rules have no distinct `deny` outcome (rule 5 is redirect-only), collapse `Denied` into `Redirected` or document when `denied` is used.
STAN: Let's introduce a variable patience for the front desk agent in terms of exchanges.  This will limit abuse if we make this demo live as well as imbue the infernal surliness of the agent.  When we terminate, let's redirect to an outcome screen.  Leave room for a centered graphic and use a spooky or medieval font.

3. **Model name in config:** `"qwen-clara:latest"` is hardcoded in `session.rs`. Should it come from `city_of_dis.toml`?
STAN: yes.  let's add it to city_of_dis.toml and make it the default.

4. **CLIPS file requirement:** The `/deduce` call currently requires a `clips_file` path even when the CLIPS engine adds nothing. Can the Clara API accept an empty string, or must we always provide the `.clp` path?
STAN: See my earlier discussion of the /deduce endpoint for the Fiery Pit and Clara's support of forward chaining.  It's part of the system and we'll introduce more complicated logic involving foward chaining later.

5. **NewFacts v2 design:** How should facts established by `meets_condition` during a deduction be propagated back to the session? Options: (a) expose a `NewFacts` list in `front_desk_turn/5`, (b) use the Dagda tableau snapshot already returned in `DeductionResult`, (c) leave it to LLM re-evaluation each turn (current behaviour).
STAN: The /deduce using LilDaemon will exercise the rules on the given context.  For now, each run is independent and the context should lead to the outcome directly without another mechanism.  We'll optimise convergence on a later sprint.

