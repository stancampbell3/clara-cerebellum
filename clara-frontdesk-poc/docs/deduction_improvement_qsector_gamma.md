# Deduction Improvement — Q-Sector Gamma

Improve the deduction path in `clara-frontdesk-poc` with three changes:

1. **Devilish supervisor prompt** — use `[devilish_supervisor].prompt` from the config as the system prompt for deduction calls, replacing the conversational `[company].system_prompt`.
2. **Deduction persistence flag** — add a per-request `persist` option (and a config default) so deductions can be retained in `clara-api` for debugging via Coire snapshots.
3. **Per-flow model selection** — `[devilish_supervisor].model` overrides `[company].model` for the evaluate call made after a deduction cycle, allowing different LLMs for conversational vs. logic-driven flows.

---

## Resolved Design Decisions

| # | Question | Resolution |
|---|---|---|
| 1 | `city_of_dis.toml` fallback | `devilish_supervisor` is **required** (non-optional). Add the section to `city_of_dis.toml` as well as `localnet_dis.toml`. |
| 2 | `clara-api` persist support | Already fully implemented. `DeduceRequest.persist` triggers a `DeductionSnapshot` save to the Coire store at cycle completion (`clara-api/src/handlers/deduce_handler.rs:158`). The frontdesk just needs to send the flag. |
| 3 | Devilish supervisor model | Enabled. `[devilish_supervisor].model` (required `String`) overrides `[company].model` when building the `evaluate_data` payload in `run_turn`. |

---

## Files Affected

| File | Change |
|---|---|
| `config/localnet_dis.toml` | Add `model` to `[devilish_supervisor]`; add `[deduction]` section |
| `config/city_of_dis.toml` | Add `[devilish_supervisor]` section; add `[deduction]` section |
| `src/config.rs` | Add `DevilishSupervisorConfig`, `DeductionConfig`; make `devilish_supervisor` required; add `FrontDeskConfig::deduction_model()` |
| `src/deduce.rs` | Add `persist: bool` param to `run_deduce` |
| `src/ws.rs` | Snapshot deduction prompt + model + persist flag; thread through `run_turn` |

---

## Step 1 — `config/localnet_dis.toml`

The `[devilish_supervisor]` section already has `prompt` and `model`. Add `[deduction]`:

```toml
[devilish_supervisor]
prompt = """..."""
model = "gemma3:latest"

[deduction]
persist = false
```

---

## Step 2 — `config/city_of_dis.toml`

Add both new sections. Use the same prompt text as `localnet_dis.toml` and set the production model. Set `persist = false` as the safe production default:

```toml
[devilish_supervisor]
prompt = """You are Agent Minos, the stern but fair front desk officer at the City of Dis administrative offices. \
You process visitors with formal infernal bureaucratic authority. \
You will be asked to comment on the visitor's demeanor, attitude, and emotional state. \
When asked a question such as 'does the visitor have a black cat?' use the conversation history to determine the answer.
ONLY respond with 'yes', 'no', or 'unresolved'.
If the visitor has not provided enough information to answer the question, respond with 'unresolved'."""
model = "qwen-clara:latest"

[deduction]
persist = false
```

---

## Step 3 — `src/config.rs`

Add two new structs and update `FrontDeskConfig`. `devilish_supervisor` is non-optional — the config will panic at startup if the section is absent, which is the right failure mode for a required field.

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct DevilishSupervisorConfig {
    pub prompt: String,
    pub model: String,
}

fn default_persist() -> bool {
    false
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct DeductionConfig {
    #[serde(default = "default_persist")]
    pub persist: bool,
}
```

Update `FrontDeskConfig` (field order matches TOML section order for readability):

```rust
pub struct FrontDeskConfig {
    pub company: CompanyConfig,
    pub devilish_supervisor: DevilishSupervisorConfig,
    #[serde(default)]
    pub deduction: DeductionConfig,
    pub server: ServerConfig,
    pub paths: PathsConfig,
}
```

Add convenience methods so call sites stay clean:

```rust
impl FrontDeskConfig {
    /// System prompt used for the deduction LLM evaluate call.
    pub fn deduction_system_prompt(&self) -> &str {
        &self.devilish_supervisor.prompt
    }

    /// Model used for the deduction LLM evaluate call.
    pub fn deduction_model(&self) -> &str {
        &self.devilish_supervisor.model
    }
}
```

---

## Step 4 — `src/deduce.rs`

Add `persist: bool` as the last parameter of `run_deduce` and include it in the POST body. The clara-api `DeduceRequest` already has this field and acts on it.

```rust
pub fn run_deduce(
    client: &Client,
    clara_api_url: &str,
    prolog_clauses: Vec<String>,
    clips_file: &str,
    initial_goal: &str,
    context: Vec<Value>,
    max_cycles: u32,
    persist: bool,          // new
) -> Result<Value, DeduceError> {
    let body = json!({
        "prolog_clauses":   prolog_clauses,
        "clips_constructs": [],
        "clips_file":       clips_file,
        "initial_goal":     initial_goal,
        "context":          context,
        "max_cycles":       max_cycles,
        "persist":          persist      // new — triggers Coire snapshot in clara-api
    });
    // ... rest unchanged
}
```

---

## Step 5 — `src/ws.rs`

### 5a. Snapshot config values in `StreamHandler`

In the `StreamHandler::handle` block where actor state is snapshotted for the blocking closure (~line 94):

```rust
// Before:
let system_prompt = self.state.config.company.system_prompt.clone();
let model         = self.state.config.company.model.clone();

// After:
let system_prompt     = self.state.config.company.system_prompt.clone();
let deduction_prompt  = self.state.config.deduction_system_prompt().to_string();
let model             = self.state.config.company.model.clone();
let deduction_model   = self.state.config.deduction_model().to_string();
let persist           = self.state.config.deduction.persist;
```

Change `deduce_context` to use `deduction_prompt`, and pass `deduction_model` into `evaluate_data` so the blocking closure has both:

```rust
// Before:
let deduce_context = self.session.deduce_context(&system_prompt);
let evaluate_data  = self.session.evaluate_data(&system_prompt, &model);

// After:
let deduce_context = self.session.deduce_context(&deduction_prompt);
let evaluate_data  = self.session.evaluate_data(&system_prompt, &model, &deduction_model);
```

Pass `persist` into `run_turn`:

```rust
run_turn(
    &clara_api_url,
    &clara_clp_path,
    prolog_clauses,
    deduce_context,
    evaluate_data,
    remaining,
    &fp_client,
    persist,            // new
)
```

### 5b. `session.rs` — `evaluate_data` signature

`evaluate_data` currently embeds `model` directly in the JSON. Extend it to also carry `deduction_model` so `run_turn` can swap the model for the post-deduction evaluate call:

```rust
pub fn evaluate_data(
    &self,
    system_message: &str,
    model: &str,
    deduction_model: &str,  // new
) -> Value {
    // ... existing body unchanged, but add deduction_model to the returned object:
    json!({
        "prompt":           prompt,
        "system":           system_message,
        "context":          history,
        "model":            model,
        "deduction_model":  deduction_model   // new — used by run_turn after deduce
    })
}
```

### 5c. `run_turn` — use `deduction_model` for evaluate call

Add `persist: bool` to the signature and extract `deduction_model` from `evaluate_data` before building the evaluate payload:

```rust
fn run_turn(
    clara_api_url: &str,
    clara_clp_path: &str,
    prolog_clauses: Vec<String>,
    deduce_context: Vec<Value>,
    evaluate_data: Value,
    exchanges_remaining: u32,
    fp_client: &fiery_pit_client::FieryPitClient,
    persist: bool,          // new
) -> Result<TurnResult, Box<dyn std::error::Error + Send + Sync>> {

    // Pull deduction_model out before consuming evaluate_data.
    let deduction_model = evaluate_data["deduction_model"]
        .as_str()
        .unwrap_or_else(|| evaluate_data["model"].as_str().unwrap_or(""))
        .to_string();

    // run_deduce call — add persist arg.
    let (suggestions, new_status) = match run_deduce(
        &http,
        clara_api_url,
        prolog_clauses,
        clara_clp_path,
        "daemonic_turn(visitor, Suggestions, Decision, Reason, Where).",
        deduce_context,
        5,
        persist,            // new
    ) { ... };

    // Override model in eval payload with the deduction-specific model.
    let mut eval_payload = evaluate_data;
    eval_payload["model"]  = Value::String(deduction_model);
    eval_payload["system"] = Value::String(augmented_system);

    // fp_client.evaluate_tephra(eval_payload) — unchanged
```

---

## Notes

- The `deduction_model` field is carried through `evaluate_data` as an in-band JSON key rather than a separate function argument. This keeps the `run_turn` signature from growing further and makes it easy to inspect the deduction payload in debug logs.
- `persist = false` is the default in both configs. Set to `true` in `localnet_dis.toml` when debugging a specific flow; Coire snapshots can then be retrieved via `GET /cycle/coire/snapshot` on clara-api.
