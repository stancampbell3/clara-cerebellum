# Clara Cerebellum Architecture

## Overview

Clara Cerebellum is a rule-based reasoning system built on CLIPS (C Language Integrated Production System) with Rust bindings and integration with external LLM-based evaluators via the **DemonicVoice** client.

## Core Components

### 1. CLIPS Rule Engine (`clara-clips`)

**Purpose**: Provides Rust FFI bindings to the CLIPS C library for rule-based reasoning.

**Key Features**:
- Compiles CLIPS C source into a static library
- Exposes safe Rust API via `ClipsEnvironment`
- Implements callback system for Rust tool invocation from CLIPS rules
- Provides standalone REPL binary with full callback support

**Main Types**:
- `ClipsEnvironment` - Safe wrapper around CLIPS environment
- `rust_clara_evaluate()` - FFI callback exposed to CLIPS C code
- `rust_free_string()` - Memory management for callback responses

**Location**: `clara-clips/`

---

### 2. Toolbox System (`clara-toolbox`)

**Purpose**: Registry and execution framework for tools that can be invoked from CLIPS rules.

**Architecture**:
```
ToolboxManager (singleton)
  ├─ Tool Registry (HashMap<String, Arc<dyn Tool>>)
  ├─ Default Evaluator (String)
  └─ Execution Router
```

**Core Types**:
- `ToolboxManager` - Singleton registry managing all tools
- `Tool` trait - Interface all tools must implement
- `ToolRequest` / `ToolResponse` - Request/response structures
- `ToolError` - Error types for tool execution

**Built-in Tools**:
- **EchoTool** - Simple echo for testing (no dependencies)
- **EvaluateTool** - Routes to lil-daemon via DemonicVoice client

**Configuration**:
- Default evaluator: `"evaluate"` (can be changed to `"echo"` for testing)
- Tools registered at startup via `ToolboxManager::init_global()`

**Location**: `clara-toolbox/`

---

### 3. DemonicVoice Client (`demonic-voice`)

**Purpose**: Synchronous HTTP client for communicating with lil-daemon REST instances.

**What is a lil-daemon?**
A lil-daemon is a REST service that provides an `/evaluate` endpoint for arbitrary JSON evaluation. It can be backed by:
- LLM reasoning (OpenAI, Anthropic, etc.)
- Rule-based systems (CLIPS, Prolog, etc.)
- Other computational engines

**API**:
```rust
let voice = DemonicVoice::new("http://localhost:8000");
let response = voice.evaluate(json_payload)?;
```

**Protocol**:
- **Endpoint**: `POST /evaluate`
- **Request**: Arbitrary JSON payload
- **Response**: JSON result
- **Error Handling**: HTTP status codes + JSON error responses

**Error Types**:
- `DemonicVoiceError::Http` - Network/HTTP errors
- `DemonicVoiceError::Status` - Non-2xx responses with body
- `DemonicVoiceError::InvalidBaseUrl` - Configuration errors

**Location**: `demonic-voice/`

---

### 4. Session Management (`clara-session`)

**Purpose**: Manages CLIPS environment lifecycle and persistence.

**Key Responsibilities**:
- Session creation and destruction
- Fact and rule persistence to CLIPS files
- Session isolation (multiple concurrent sessions)
- Integration with clara-persistence for long-term storage

**Location**: `clara-session/`

---

### 5. REST API (`clara-api`)

**Purpose**: Actix-web HTTP server exposing CLIPS functionality.

**Endpoints**:
- `POST /evaluate` - Evaluate CLIPS expressions
- `POST /sessions` - Create new session
- `GET /sessions/:id` - Get session state
- `DELETE /sessions/:id` - Delete session
- See `docs/fiery_pit_endpoints.md` for full API spec

**Location**: `clara-api/`

---

## Data Flow

### Simple Evaluation (Default Mode)

```
User/Rule
  │
  ├─ (clara-evaluate "{\"question\":\"weather?\"}")
  │
  ├─▶ rust_clara_evaluate() [FFI callback]
  │
  ├─▶ ToolboxManager::evaluate()  [uses default evaluator]
  │
  ├─▶ EvaluateTool::execute()
  │
  ├─▶ DemonicVoice::evaluate()
  │
  ├─▶ POST http://localhost:8000/evaluate
  │
  └─▶ LLM/Evaluator Response ─▶ CLIPS Rule
```

### Explicit Tool Selection

```
User/Rule
  │
  ├─ (clara-evaluate "{\"tool\":\"echo\",\"arguments\":{...}}")
  │
  ├─▶ rust_clara_evaluate() [FFI callback]
  │
  ├─▶ ToolboxManager::execute_tool()  [routes to named tool]
  │
  ├─▶ EchoTool::execute()
  │
  └─▶ Echo Response ─▶ CLIPS Rule
```

---

## Build Architecture

### CLIPS Integration

The `clara-clips` package uses a custom `build.rs` script:

1. **Collects C source files** from `clips-src/core/`
   - Filters out macOS metadata files (`._*`)
2. **Compiles static library** using `cc` crate
   - Platform-specific flags for macOS/Linux
3. **Links library** into Rust binary
4. **Exposes FFI callbacks** via `#[no_mangle]` exports

**Critical for macOS**: The build script skips `._*` metadata files to prevent compilation errors on external drives.

---

## Configuration

### Environment Variables

- `RUST_LOG` - Logging level (default: `info`)
- `RUST_BACKTRACE` - Enable backtraces on panic

### REPL Flags

```bash
clips-repl [--evaluator TOOL]

Options:
  --evaluator evaluate    # Default: Routes to DemonicVoice at localhost:8000
  --evaluator echo        # Testing: No network calls, just echoes
```

---

## Dependencies

### External Systems

- **CLIPS** - C rule engine (bundled in `clara-clips/clips-src/`)
- **lil-daemon** - REST evaluator service (external, runs on port 8000)

### Rust Crates

**Core**:
- `actix-web` - HTTP server framework
- `reqwest` - HTTP client (for DemonicVoice)
- `serde_json` - JSON serialization
- `cc` - C compiler integration for CLIPS build

**Utilities**:
- `lazy_static` - Global singleton for ToolboxManager
- `thiserror` - Error type derivation
- `env_logger` - Logging infrastructure

---

## Security Considerations

### Memory Safety

- All CLIPS FFI calls use `unsafe` blocks
- String ownership carefully managed between C and Rust
- `rust_free_string()` ensures no memory leaks

### Network Security

- DemonicVoice uses HTTPS when configured
- No authentication currently implemented (TODO)
- Input validation on all JSON payloads

### Session Isolation

- Each session maintains separate CLIPS environment
- No shared state between sessions (TODO: verify)

---

## Performance Characteristics

### CLIPS Engine
- **Rule matching**: O(n×m) where n=facts, m=rules
- **Evaluation**: Microseconds for simple rules
- **Memory**: ~1MB per environment

### DemonicVoice Client
- **Latency**: Network-dependent (typically 100-1000ms for LLM)
- **Timeout**: Configurable (default: 30s)
- **Connection pooling**: Yes (via reqwest)

### Toolbox Overhead
- **Tool lookup**: O(1) HashMap access
- **Serialization**: JSON parsing on each call
- **Lock contention**: Global ToolboxManager uses Mutex

---

## Deployment Topology

```
┌─────────────────┐
│  Client/User    │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│   clara-api     │  (Actix-web server, port 8080)
│   REST Server   │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  clara-session  │  (Session management)
│   ┌──────────┐  │
│   │  CLIPS   │  │  (Rule engine)
│   │  Engine  │  │
│   └────┬─────┘  │
└────────┼────────┘
         │
         ├─▶ ToolboxManager
         │    ├─ EchoTool
         │    └─ EvaluateTool
         │         │
         │         ▼
         │    DemonicVoice
         │         │
         ▼         ▼
    ┌──────────────────┐
    │   lil-daemon     │  (External LLM service, port 8000)
    │   /evaluate      │
    └──────────────────┘
```

---

## Extension Points

### Adding New Tools

1. Implement `Tool` trait in `clara-toolbox/src/tools/`
2. Export from `tools/mod.rs`
3. Register in REPL or API initialization

```rust
pub struct MyTool;

impl Tool for MyTool {
    fn name(&self) -> &str { "my-tool" }
    fn description(&self) -> &str { "My custom tool" }
    fn execute(&self, args: Value) -> Result<Value, ToolError> {
        // Implementation
    }
}
```

### Adding New Endpoints

1. Define handler in `clara-api/src/handlers/`
2. Register route in `clara-api/src/main.rs`
3. Update `fiery_pit_endpoints.md`

### Supporting New Evaluator Backends

Implement the lil-daemon protocol:
- `POST /evaluate` accepting arbitrary JSON
- Return JSON response
- Use as DemonicVoice target URL

---

## Testing Strategy

### Integration Tests

- `clara-clips/tests/` - FFI callback tests
- `clara-toolbox/tests/` - Tool execution tests

### Testing Without lil-daemon

```bash
# Use echo evaluator to avoid network calls
clips-repl --evaluator echo

# In CLIPS:
(clara-evaluate "{\"test\":\"data\"}")
# Returns: {"status":"success","echoed":{"test":"data"},...}
```

### Unit Tests

Each crate has `#[cfg(test)]` modules testing:
- Serialization/deserialization
- Error handling
- Tool registration and execution

---

## Future Enhancements

### Planned Features

- **Authentication** - JWT tokens for API and lil-daemon
- **Session persistence** - Save/restore CLIPS state to disk
- **Async evaluation** - Non-blocking tool execution
- **Tool composition** - Chains of tool calls
- **Observability** - Prometheus metrics, tracing

### Known Limitations

- No connection pooling for DemonicVoice (single reqwest Client)
- Global ToolboxManager limits concurrent configuration changes
- No retry logic for failed lil-daemon calls
- macOS-specific build workarounds required

---

For specific subsystem documentation, see:
- `SESSION_LIFECYCLE.md` - Session management details
- `DEMONIC_VOICE_PROTOCOL.md` - lil-daemon communication
- `CLIPS_CALLBACKS.md` - Callback system internals
- `TOOLBOX_SYSTEM.md` - Tool development guide
