# City of Dis Front Desk POC — Implementation Plan

## Context

Build `clara-frontdesk-poc` as a self-contained Rust binary crate that demonstrates the Clara reasoning stack in a humorously infernal "City of Dis" visitor intake scenario. The app uses:
- FieryPit `/evaluate` (port 6666) — LLM conversational responses
- clara-api `/deduce` (port 8080) — Prolog+CLIPS admittance logic & suggestions
- WebSocket + single-file HTML UI — visitor chat interface

The Prolog source `roost/front_desk_poc_reprise.pl` (already written) encodes the 5 admittance rules and suggestion predicates. It must be transduced into `_clara.pl` + `_clara.clp` before use.

---

## Pre-requisites (manual, done once)

```bash
# From workspace root, after cargo build:
transduction --decorate clara-frontdesk-poc/roost/front_desk_poc_reprise.pl
# Produces: roost/front_desk_poc_reprise_clara.pl
#           roost/front_desk_poc_reprise_clara.clp
```

The transduced files stay in `roost/` alongside the source. Their **absolute paths** are configured in the TOML config so clara-api can read them.

---

## File Structure

```
clara-frontdesk-poc/
├── Cargo.toml
├── config/
│   └── city_of_dis.toml       # company config, loaded via FRONTDESK_CONFIG env var
├── roost/
│   ├── front_desk_poc_reprise.pl          # existing source
│   ├── front_desk_poc_reprise_clara.pl    # generated (transduction --decorate)
│   └── front_desk_poc_reprise_clara.clp   # generated (transduction --decorate)
├── src/
│   ├── main.rs        # server setup, AppState init, route wiring
│   ├── config.rs      # FrontDeskConfig TOML structs
│   ├── state.rs       # AppState (shared across connections)
│   ├── ws.rs          # WebSocket actor (actix-web-actors)
│   ├── session.rs     # VisitorSession per-connection state
│   └── deduce.rs      # blocking deduce client (POST + poll loop)
└── static/
    └── index.html     # single-file chat UI (City of Dis theme)
```

---

## Cargo.toml Dependencies

```toml
[package]
name = "clara-frontdesk-poc"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "clara-frontdesk"
path = "src/main.rs"

[dependencies]
actix-web = "4"
actix-web-actors = "4"
actix = "0.13"
actix-files = "0.6"
fiery-pit-client = { path = "../fiery-pit-client" }
reqwest = { version = "0.11", features = ["blocking", "json"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
toml = "0.8"
env_logger = "0.10"
log = "0.4"
uuid = { version = "1", features = ["v4"] }
```

---

## Config (`config/city_of_dis.toml`)

```toml
[company]
name = "City of Dis Administrative Office"
agent_name = "Agent Minos"
# System persona injected into every LLM evaluate call
system_prompt = """
You are Agent Minos, the stern but fair front desk officer at the City of Dis.
Speak in a formal, slightly ominous tone befitting infernal bureaucracy.
Keep responses concise — two to four sentences.
"""

[server]
port = 8088

[paths]
# Absolute paths as seen by the clara-api server process
clara_api_url  = "http://localhost:8080"
fiery_pit_url  = "http://localhost:6666"
clara_pl_path  = "/abs/path/to/roost/front_desk_poc_reprise_clara.pl"
clara_clp_path = "/abs/path/to/roost/front_desk_poc_reprise_clara.clp"
```

---

## Key Data Types

### `config.rs`
```rust
#[derive(Deserialize)]
pub struct FrontDeskConfig {
    pub company: CompanyConfig,
    pub server:  ServerConfig,
    pub paths:   PathsConfig,
}
```

### `state.rs`
```rust
pub struct AppState {
    pub fiery_pit:      FieryPitClient,  // sync blocking — created before actix runtime
    pub clara_api_url:  String,
    pub clara_pl_path:  String,
    pub clara_clp_path: String,
    pub config:         Arc<FrontDeskConfig>,
}
```

### `session.rs`
```rust
pub enum VisitorStatus { Active, Admitted(String), Denied(String), Redirected(String) }

pub struct VisitorSession {
    pub visitor:      String,            // prolog atom, e.g. "visitor"
    pub conversation: Vec<serde_json::Value>,  // [{role, content}]
    pub facts:        HashSet<String>,   // "greeted", "urgent_message", etc.
    pub status:       VisitorStatus,
}

impl VisitorSession {
    /// Build prolog_clauses list for /deduce
    pub fn prolog_clauses(&self, pl_path: &str) -> Vec<String> {
        let mut clauses = vec![format!("consult('{}').", pl_path),
                               format!("visitor({}).", self.visitor)];
        for fact in &self.facts {
            clauses.push(format!("{}({}).", fact, self.visitor));
        }
        clauses
    }
}
```

---

## Deduce Client (`deduce.rs`)

Uses `reqwest::blocking::Client` (called via `spawn_blocking` from async WS handler):

```rust
pub fn run_deduce(
    clara_api_url: &str,
    prolog_clauses: Vec<String>,
    clips_file: &str,
    initial_goal: &str,
    context: Vec<serde_json::Value>,
    max_cycles: u32,
) -> Result<serde_json::Value, DeduceError> {
    // 1. POST /deduce → get deduction_id
    // 2. Poll GET /deduce/{id} with 100ms sleep until status != "running"
    // 3. Return result JSON or error
}

/// Extract string bindings for variable `var_name` from prolog_solutions
pub fn extract_solutions(result: &Value, var_name: &str) -> Vec<String>
```

---

## Per-Turn Flow (`ws.rs`)

On each user message received by the WebSocket actor:

```
1. session.conversation.push({role: "user", content: msg})

2. Run SUGGESTIONS deduce (spawn_blocking):
   - initial_goal: "suggestion(visitor, S)."
   - clauses: session.prolog_clauses(pl_path)
   - clips_file: clara_clp_path
   - context: session.conversation
   - max_cycles: 5
   → extract Vec<String> of suggestion strings

3. Run ADMIT deduce (spawn_blocking):
   - initial_goal: "admit(visitor, Reason)."
   - same clauses + context
   - max_cycles: 5
   → extract Vec<String> of admit reasons

4. Interpret admit results:
   - Any reason containing "Grant entry" → Admitted
   - Any reason containing "Do not admit" / "direct to" → Redirected
   - Empty → still Active

5. Build LLM system message:
   "[system_prompt]\n\nCurrent guidance from the admittance system:\n- <suggestions>\n<admit_status>"

6. POST to FieryPit /evaluate:
   data = { messages: session.conversation, system: <above> }

7. Extract assistant response text from Tephra response

8. session.conversation.push({role: "assistant", content: response})
9. session.facts.insert("greeted")   // after first exchange

10. Send response text to client via WS
11. If terminal state: send status update + close WS
```

---

## Main (`main.rs`)

```rust
fn main() -> std::io::Result<()> {
    env_logger::init();
    let config = load_config();   // reads FRONTDESK_CONFIG env var → TOML

    // FieryPitClient created BEFORE actix runtime (blocking reqwest inside)
    let fiery_pit = FieryPitClient::new(&config.paths.fiery_pit_url);

    let state = web::Data::new(AppState { fiery_pit, ... });
    let port = config.server.port;

    actix_web::rt::System::new().block_on(async {
        HttpServer::new(move || {
            App::new()
                .app_data(state.clone())
                .route("/ws", web::get().to(ws_index))
                .service(Files::new("/", "static").index_file("index.html"))
        })
        .bind(("0.0.0.0", port))?
        .run()
        .await
    })
}
```

---

## UI (`static/index.html`)

Single-file HTML/CSS/JS:
- Dark infernal theme (crimson, ash, ember colors)
- Chat bubble layout (visitor right, agent left)
- WebSocket connection on page load
- Status badge: Active / Admitted / Denied / Redirected
- Disable input on terminal state

---

## Verification

1. Run `transduction --decorate` on source .pl → check _clara.pl and _clara.clp generated
2. Update `config/city_of_dis.toml` with correct absolute paths
3. Ensure clara-api (port 8080) and FieryPit (port 6666) are running
4. `FRONTDESK_CONFIG=clara-frontdesk-poc/config/city_of_dis.toml cargo run -p clara-frontdesk-poc`
5. Open browser at `http://localhost:8088`
6. Visitor conversation scenarios to test:
   - Lost visitor → "Direct to map kiosk" suggestion
   - Summoned visitor with artifacts → Grant entry
   - Flamefruit carrier before sundown → Grant entry
   - Urgent message, came directly → Grant entry

---

## Evaluate Payload — Confirmed from Logs

The KindlingEvaluator (active when `"prompt"` key is present) routes to OllamaEvaluator.
The OllamaFish formats it as a chat completion, appending `prompt` as the final user turn.

**`FieryPitClient.evaluate(data)` call:**
```rust
fp_client.evaluate(json!({
    "prompt":  last_user_message,       // appended as final user turn by OllamaFish
    "context": [                         // prepended messages (system + history)
        {"role": "system",    "content": "<Minos persona>\n\nGuidance:\n- <suggestions>"},
        {"role": "user",      "content": "..."},
        {"role": "assistant", "content": "..."},
        // ... up to but NOT including the last user message
    ],
    "model": "qwen2.5:7b"               // optional; evaluator has a default
}))?
```

**Response navigation** (raw `Value` from `fp_client.evaluate`):
```rust
let text = response["hohi"]["response"]["response"].as_str().unwrap_or("");
```

Tephra shape: `{ hohi: { response: { model, prompt, response: "<text>", ... }, code: 200 }, tabu: null }`

**Deduce context** (separate from evaluate context — both use `role/content` objects):
- In `/deduce` → stored as `deduce_context_json/1` in Prolog → `current_context/1` → used by `clara_fy/3`
- In `/evaluate` → forwarded to Ollama as the messages array prefix

---

## Resolved Design Decisions

1. **`greeted` fact timing**: Assert after first agent response is sent.
2. **Visitor name**: Use fixed atom `visitor` throughout (simplest for POC).
3. **Evaluate model**: `"qwen2.5:7b"` — same as used in the existing example run.
4. **Two deduce calls per turn**: one for `suggestion/2`, one for `admit/2`. Both use `max_cycles: 2`
   (sufficient per the example run — suggestions converged in 2 cycles).
