# City of Dis — Front Desk POC

A self-contained Rust binary that demonstrates the Clara neurosymbolic reasoning stack in a visitor intake scenario set at the infernal City of Dis administrative offices.

## What it does

A visitor chats with **Agent Minos** via a browser-based WebSocket UI. Each visitor turn drives two parallel reasoning calls:

1. **Suggestions** — `clara-api /deduce` runs `suggestion(visitor, S)` against the Prolog+CLIPS knowledge base and returns actionable hints to guide the conversation.
2. **Admittance** — `clara-api /deduce` runs `admit(visitor, Reason)` to evaluate whether the visitor qualifies for entry under the five admittance rules.

Both results are injected into the system prompt of a **FieryPit `/evaluate`** call that generates Agent Minos's response. The session ends in one of three terminal states: **Admitted**, **Denied**, or **Redirected** (e.g., to the map kiosk).

## Architecture

```
Browser (WS) ──► clara-frontdesk (8088)
                      │
                      ├─► clara-api /deduce (8080)
                      │       └─► clara-cycle: Prolog + CLIPS + Dagda tableau
                      │
                      └─► FieryPit /evaluate (6666)
                              └─► KindlingEvaluator (LLM)
```

**Knowledge base source:** `roost/front_desk_poc_reprise.pl`
Five admittance rules and six suggestion predicates. Each rule may use either symbolic Prolog facts (asserted per turn) or `clara_fy/3` LLM-mediated checks for conditions that cannot be verified symbolically.

## File layout

```
clara-frontdesk-poc/
├── Cargo.toml
├── config/
│   └── city_of_dis.toml         # company persona + service URLs + file paths
├── roost/
│   ├── front_desk_poc_reprise.pl          # source Prolog (edit this)
│   ├── front_desk_poc_reprise_clara.pl    # generated — do not edit
│   └── front_desk_poc_reprise_clara.clp   # generated — do not edit
├── src/
│   ├── main.rs      # server init (FieryPitClient before actix runtime)
│   ├── config.rs    # TOML config structs
│   ├── state.rs     # AppState shared across WS connections
│   ├── session.rs   # VisitorSession per-connection state + fact accumulation
│   ├── deduce.rs    # blocking POST+poll client for /deduce
│   └── ws.rs        # WebSocket actor, per-turn reasoning loop
└── static/
    └── index.html   # single-file chat UI (infernal theme)
```

## Prerequisites

### 1. Build the workspace

```bash
cargo build -p clara-frontdesk-poc
```

### 2. Transduce the Prolog source

Run this once from the workspace root whenever `front_desk_poc_reprise.pl` changes:

```bash
transduction --decorate clara-frontdesk-poc/roost/front_desk_poc_reprise.pl
```

This produces:
- `roost/front_desk_poc_reprise_clara.pl`
- `roost/front_desk_poc_reprise_clara.clp`

### 3. Update the config with absolute paths

Edit `clara-frontdesk-poc/config/city_of_dis.toml` and replace the `CHANGE_ME` placeholders with the absolute paths to the generated files **as seen by the `clara-api` process**:

```toml
[paths]
clara_api_url  = "http://localhost:8080"
fiery_pit_url  = "http://localhost:6666"
clara_pl_path  = "/abs/path/to/clara-cerebrum/clara-frontdesk-poc/roost/front_desk_poc_reprise_clara.pl"
clara_clp_path = "/abs/path/to/clara-cerebrum/clara-frontdesk-poc/roost/front_desk_poc_reprise_clara.clp"
```

The paths must be absolute because `clara-api` reads these files from its own working directory.

### 4. Start the dependent services

| Service | Default port | How to start |
|---------|-------------|--------------|
| FieryPit (lildaemon) | 6666 | per your local setup |
| clara-api | 8080 | `cargo run -p clara-api` |

### 5. Run the front desk server

```bash
FRONTDESK_CONFIG=clara-frontdesk-poc/config/city_of_dis.toml cargo run -p clara-frontdesk-poc
```

Open `http://localhost:8088` in a browser.

## Visitor test scenarios

| Scenario | Facts to trigger | Expected outcome |
|----------|-----------------|-----------------|
| Summoned visitor with three artifacts | `summoned_by`, three `has_artifact` facts | Admitted |
| Urgent message, came directly | `urgent_message`, no `stopped_elsewhere` | Admitted |
| Flamefruit carrier before sundown | `carries_flamefruit`, no `after_sundown` | Admitted |
| Critical info + completed task | `has_critical_info`, `performed_task` | Admitted |
| Lost or confused visitor | LLM-detected via `clara_fy` | Redirected to map kiosk |

Facts are accumulated as Prolog clauses in `VisitorSession` and injected into every `/deduce` call. The LLM-mediated rules (`clara_fy/3`) extract conditions from conversation context when symbolic facts are absent.

## Configuration reference

```toml
[company]
name          = "City of Dis Administrative Office"
agent_name    = "Agent Minos"
system_prompt = "..."   # injected into every /evaluate call

[server]
port = 8088             # override with FRONTDESK_PORT not yet supported; edit this field

[paths]
clara_api_url  = "http://localhost:8080"
fiery_pit_url  = "http://localhost:6666"
clara_pl_path  = "..."  # absolute path to _clara.pl
clara_clp_path = "..."  # absolute path to _clara.clp
```

Config file location is read from the `FRONTDESK_CONFIG` environment variable; defaults to `clara-frontdesk-poc/config/city_of_dis.toml` (relative to workspace root).

## Known constraints

- **`FieryPitClient` must be created before the actix runtime.** The blocking `reqwest` client panics if dropped inside a tokio context. `main.rs` constructs it before `actix_web::rt::System::new()`.
- **Transduced files are not committed.** They are generated artefacts; regenerate after every edit to the source `.pl`.
- **Fixed visitor atom.** The Prolog atom `visitor` is used throughout for simplicity; dynamic name extraction from conversation is not implemented in the POC.
- **`/evaluate` payload shape.** The current payload uses `{ prompt, context, model }` matching the KindlingEvaluator's OllamaFish backend. Adjust `session.rs::evaluate_data()` if the evaluator changes.
