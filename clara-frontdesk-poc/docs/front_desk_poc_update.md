# Clara Front Desk POC — Implementation Plan (revised)

**Branch:** `housbonde_lif`  
**Date:** 2026-04-07  
**Authors:** Stan Campbell / team review  
**Status:** Approved with team responses incorporated

---

## Overview

Three work streams, with responses from Stan incorporated inline:

1. **`fiery-pit-client`** — Align the Rust client with the Python `lildaemon-client` where there are gaps.
2. **`clara-frontdesk-poc`** — Overhaul the conversational loop: single combined deduction per turn, structured context threading, reliable state transitions.
3. **State machine** — Explicit entry/exit states driven solely by the deduction result; patience-based termination; outcome screen.

---

## Stream 1 — `fiery-pit-client` Alignment

### 1.1 Use typed response extraction everywhere

`ws.rs` currently navigates the raw JSON response:
```rust
tephra["hohi"]["response"]["content"]
```

`evaluate_tephra()` already returns a typed `Tephra`, and `Tephra::response()` traverses this path safely. All callers switch to `evaluate_tephra()` + `Tephra::response()`.

**Action:** `fp_client.evaluate(payload)` → `fp_client.evaluate_tephra(payload)`, extract content via `tephra.response()` or `tephra.into_response()`.

### 1.2 Typed Prolog query response

Add a typed struct mirroring the Python `models.py`:

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

### 1.3 `from_env()` constructor

```rust
impl FieryPitClient {
    pub fn from_env() -> Result<Self, FieryPitError> {
        let url = std::env::var("FIERY_PIT_URL")
            .unwrap_or_else(|_| "http://localhost:6666".into());
        Ok(Self::new(&url))
    }
}
```

### 1.4 REPL endpoints — do not add

The Python API has `/repl/` session endpoints used during development. Application code must never use the REPL path. Do not add these to the Rust client.

### 1.5 No async Rust client

The existing architecture uses blocking reqwest inside `spawn_blocking`. This is intentional and correct for the actix-web actor model. An async Rust client would conflict with it.

---

## Stream 2 — Conversational Loop Overhaul

### 2.1 One deduce call per turn via `daemonic_turn/5`

`run_turn()` currently fires two sequential `run_deduce()` calls (suggestion + admit), each with its own HTTP round-trip and separate Prolog session. Collapse into one call via a meta-goal:

```prolog
%% Application entry point — called once per conversation turn.
%% "daemonic" reflects that outcomes are LLM-mediated (lildaemon context).
daemonic_turn(Visitor, Suggestions, Decision, Reason, Where) :-
    visitor(Visitor),
    findall(S, suggestion(Visitor, S), Suggestions),
    (   admit(Visitor, R)        -> Decision = admitted,   Reason = R, Where = ''
    ;   redirect(Visitor, R, W)  -> Decision = redirected, Reason = R, Where = W
    ;                               Decision = pending,    Reason = '', Where = ''
    ).
```

Initial goal becomes:
```
daemonic_turn(visitor, Suggestions, Decision, Reason, Where).
```

All five variables are extracted from `prolog_solutions` in a single result. One HTTP round-trip per turn.

### 2.2 Structured decision matching replaces string heuristics

`interpret_admit()` (fragile substring matching on natural-language atoms) is replaced by direct atom matching on the `Decision` variable:

```rust
fn interpret_decision(solutions: &[HashMap<String, Value>]) -> Option<VisitorStatus> {
    let sol = solutions.first()?;
    let reason = sol.get("Reason")?.as_str().unwrap_or("").to_string();
    let where_  = sol.get("Where")?.as_str().unwrap_or("").to_string();
    match sol.get("Decision")?.as_str()? {
        "admitted"   => Some(VisitorStatus::Admitted(reason)),
        "denied"     => Some(VisitorStatus::Denied(reason)),
        "redirected" => Some(VisitorStatus::Redirected(reason, where_)),
        _            => None,
    }
}
```

### 2.3 Context threading

`VisitorSession::deduce_context()` already builds the full conversation history correctly and passes it in the `/deduce` `context` field. The KindlingEvaluator bug (tracked in `memory/bug_kindling_evaluator_async_drops_context.md`) may silently drop context for LLM sub-calls from within Prolog (`meets_condition/2`). No Rust-side workaround is possible; add a code comment noting the dependency on the lildaemon fix.

### 2.4 Fact propagation via coire / forward chaining

> **Stan:** This is already built into the Clara `clara-cycle` deduce pipeline. The Prolog source is transduced (via `clara-transduction`) into Prolog + CLIPS source, implementing backward and forward chaining respectively. Facts pushed onto the **coire** message bus are forwarded across the reasoning cycle: CLIPS forward-chaining rules can take a freshly asserted fact and emit a coire message that triggers a new Prolog goal on the next cycle.

This is a key feature of the architecture and **should be highlighted in the demo**, not treated as a future item. The CLIPS file path must always be provided to `/deduce` — forward chaining is integral, not optional.

For the current sprint: each deduce call is context-driven and independent. The context (full conversation history) is the primary mechanism by which prior turns influence the current deduction. Optimise convergence (coire-based fact accumulation across turns) in a future sprint.

### 2.5 Model name moved to config

`"qwen-clara:latest"` is currently hardcoded in `session.rs`. Move to `city_of_dis.toml`:

```toml
[company]
name = "City of Dis"
agent_name = "The Keeper"
model = "qwen-clara:latest"
system_prompt = "..."
```

`FrontDeskConfig::company.model` field added; `session.rs::evaluate_data()` reads it from config via `AppState`.

### 2.6 Updated `run_turn()` flow

```
1. Build prolog_clauses  (consult + visitor + known facts)
2. Build context         (system prompt + full conversation history)
3. POST /deduce          goal: "daemonic_turn(visitor, Suggestions, Decision, Reason, Where)."
4. Poll GET /deduce/{id} until status != "running"
5. Extract Suggestions   list from solutions["Suggestions"]
6. Extract Decision      atom → VisitorStatus (admitted/denied/redirected/none)
7. Check patience        if exchange_count >= patience_limit → VisitorStatus::Denied("patience exhausted")
8. Augment system prompt with suggestions + decision + patience warning if near limit
9. POST /evaluate        with augmented payload, model from config
10. Tephra::response()   → assistant_text
11. Return TurnResult { assistant_text, new_status }
```

---

## Stream 3 — State Machine, Patience & Outcome Screen

### 3.1 Visitor lifecycle

```
                     WebSocket connect
                            │
                     VisitorStatus::Active
                            │
               ┌────────────┴────────────────────────────┐
               │   per-turn loop                          │
               │   exchange_count increments each turn    │
               └────────────┬────────────────────────────┘
                            │
         Decision from deduce result OR patience exhausted
         ┌──────────────────┼──────────────────┬──────────────┐
         │                  │                  │              │
      "admitted"        "denied"          "redirected"   patience limit
         │                  │                  │              │
 VisitorStatus::    VisitorStatus::    VisitorStatus::  VisitorStatus::
 Admitted(reason)   Denied(reason)     Redirected(r,w)  Denied("The Keeper has lost patience.")
         │                  │                  │              │
         └──────────────────┴──────────────────┴──────────────┘
                            │
                       is_terminal() == true
                            │
               WebSocket close → browser navigates to outcome screen
```

**Rule:** `VisitorStatus` changes **only** when:
1. Deduce returns `status: "converged"` and `Decision` is one of `admitted`/`denied`/`redirected`, **or**
2. `exchange_count >= patience_limit` (patience exhausted, yields `Denied`)

No string matching anywhere in the decision path.

### 3.2 Patience

Add `patience_limit: u32` and `exchange_count: u32` to `VisitorSession`. Default patience configurable in TOML (e.g. `patience = 8`). On each turn:

- Increment `exchange_count`
- If `exchange_count >= patience_limit` before the LLM response is generated, set status to `Denied("The Keeper has grown weary of this interview.")` and skip the evaluate call — send the terminal frame directly

A warning can be added to the augmented system prompt at e.g. `patience_limit - 2` turns: `"You are growing impatient. This is the visitor's last chance."`

### 3.3 Outcome screen

On `is_terminal()`, the server sends a terminal WS frame and the browser navigates away from the chat to an **outcome screen** (full-page replacement, not a panel).

The WS message for terminal states carries:
```json
{
  "type":     "terminal",
  "status":   "admitted" | "denied" | "redirected",
  "reason":   "...",
  "where":    "..."
}
```

The outcome screen (`static/outcome.html` or injected into `index.html` via JS):

- Full-page, matching the dark ember/ash theme
- **Centered graphic** (SVG or image): gate open for `admitted`, gate barred for `denied`/`redirected`
- **Spooky/medieval font** (e.g. MedievalSharp, UnifrakturMaguntia, or Cinzel from Google Fonts)
- Large status text: `ENTRY GRANTED` / `ENTRY DENIED` / `YOU HAVE BEEN REDIRECTED`
- Reason text below in smaller type
- `Where` field shown for `redirected` outcomes: `"Proceed to: {where}"`
- No input controls

### 3.4 Prolog rule changes

```prolog
%% Refactored admit rules use clean reason strings (no "Grant entry." suffix —
%% the Decision atom carries that semantic).

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

%% Lost or confused visitors are redirected, not denied.
redirect(Visitor, 'This visitor appears lost or confused and cannot proceed.', 'map kiosk') :-
    visitor(Visitor),
    (   lost_or_confused(Visitor)
    ;   meets_condition(Visitor, "Does the visitor appear lost or confused about where they are or why they are here?")
    ).

%% Meta-goal called by the application once per conversation turn.
daemonic_turn(Visitor, Suggestions, Decision, Reason, Where) :-
    visitor(Visitor),
    findall(S, suggestion(Visitor, S), Suggestions),
    (   admit(Visitor, R)       -> Decision = admitted,   Reason = R, Where = ''
    ;   redirect(Visitor, R, W) -> Decision = redirected, Reason = R, Where = W
    ;                              Decision = pending,    Reason = '', Where = ''
    ).
```

The CLIPS file (`front_desk_poc_reprise_clara.clp`) is regenerated from the updated Prolog via the Clara transducer after rule changes.

---

## File Change Map

| File | Change |
|------|--------|
| `fiery-pit-client/src/lib.rs` | Add `PrologQueryResponse`, `prolog_query_typed()`, `FieryPitClient::from_env()` |
| `clara-frontdesk-poc/src/ws.rs` | Use `evaluate_tephra()`; single deduce call; `interpret_decision()` replaces `interpret_admit()`; patience check; extend WS terminal frame with `reason`/`where`; `"terminal"` type on close |
| `clara-frontdesk-poc/src/deduce.rs` | Add `extract_named_solutions() -> HashMap<String, Value>` for multi-variable extraction |
| `clara-frontdesk-poc/src/session.rs` | Add `exchange_count`, `patience_limit` fields; wire `Denied` variant; update `evaluate_data()` to read model from config; update `VisitorStatus::Redirected` to carry `(reason, where)` |
| `clara-frontdesk-poc/src/config.rs` | Add `model: String` and `patience: u32` to `CompanyConfig` |
| `clara-frontdesk-poc/config/city_of_dis.toml` | Add `model = "qwen-clara:latest"` and `patience = 8` under `[company]` |
| `clara-frontdesk-poc/roost/front_desk_poc_reprise_clara.pl` | Add `daemonic_turn/5`, refactor `admit/2` with clean reason strings, add `redirect/3`, remove old rule 5 from `admit/2` |
| `clara-frontdesk-poc/roost/front_desk_poc_reprise_clara.clp` | Regenerate from updated Prolog via Clara transducer |
| `clara-frontdesk-poc/static/index.html` | Handle `"terminal"` WS message type; navigate to / inject outcome screen; medieval/spooky font; centered graphic per status; no input on terminal |

---

## Out of Scope

- REPL API endpoints in the Rust client (development-only)
- Async Rust client (conflicts with actor model)
- Multi-visitor sessions (single visitor per WebSocket is intentional)
- KindlingEvaluator context-drop bug fix (lives in lildaemon Python; tracked separately)
- Authentication / JWT (POC only)
- Coire-based fact accumulation across turns (optimise convergence in a later sprint)

---

## Resolved Questions

| # | Question | Resolution |
|---|----------|------------|
| 1 | Meta-goal name | `daemonic_turn/5` — reflects LLM-mediated (lildaemon) context |
| 2 | `Denied` vs `Redirected`; patience | Introduce patience counter; exhausted patience yields `Denied`; outcome screen on all terminal states with medieval font + centered graphic |
| 3 | Model name in config | Move to `city_of_dis.toml` under `[company]`; `qwen-clara:latest` as default |
| 4 | CLIPS file requirement | Always provide — forward chaining via coire is integral to the architecture; more complex CLIPS logic comes in future sprints |
| 5 | NewFacts propagation | Each deduce call is context-driven and independent this sprint; coire/forward-chaining is the long-term mechanism; optimise in a future sprint |
