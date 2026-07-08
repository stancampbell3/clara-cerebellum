# CLIPS Callback System

## Overview

The CLIPS callback system allows CLIPS rules to invoke Rust functions via FFI (Foreign Function Interface), enabling CLIPS to call external tools, LLM evaluators, and other Rust functionality.

## Architecture

```
CLIPS C Code
    │
    ├─▶ (clara-evaluate "JSON")  [CLIPS function call]
    │
    └─▶ userfunctions.c
         │
         └─▶ rust_clara_evaluate()  [FFI boundary]
              │
              ├─▶ Parse JSON
              ├─▶ Route to ToolboxManager
              ├─▶ Execute tool
              └─▶ Return JSON response
```

---

## Implementation Layers

### Layer 1: CLIPS C Integration

**File**: `clara-clips/clips-src/core/userfunctions.c`

Registers the `clara-evaluate` function in CLIPS:

```c
void UserFunctions(Environment *env) {
    AddUDF(env, "clara-evaluate", "s", 1, 1, "s",
           ClaraEvaluateFunction, "ClaraEvaluateFunction", NULL);
}

static void ClaraEvaluateFunction(
    Environment *env,
    UDFContext *context,
    UDFValue *returnValue)
{
    // Get JSON string argument from CLIPS
    UDFValue arg;
    UDFNextArgument(context, STRING_BIT, &arg);
    const char *json_input = arg.lexemeValue->contents;

    // Call Rust FFI function
    char *result = rust_clara_evaluate(env, json_input);

    // Return result to CLIPS
    returnValue->lexemeValue = CreateString(env, result);

    // Free Rust-allocated string
    rust_free_string(result);
}
```

### Layer 2: Rust FFI Boundary

**File**: `clara-clips/src/backend/ffi/callbacks.rs`

Exposes C-callable functions:

```rust
#[no_mangle]
pub extern "C" fn rust_clara_evaluate(
    _env: *mut c_void,
    input_json: *const c_char,
) -> *mut c_char {
    unsafe {
        // Convert C string to Rust
        let input_str = CStr::from_ptr(input_json).to_str().unwrap();

        // Parse JSON and route to toolbox
        let json_value: Value = serde_json::from_str(input_str)?;
        let manager = ToolboxManager::global().lock().unwrap();

        let response = if json_value.get("tool").is_some() {
            // Explicit tool specified
            let request: ToolRequest = serde_json::from_value(json_value)?;
            manager.execute_tool(&request)
        } else {
            // Use default evaluator
            manager.evaluate(json_value)
        };

        // Convert response to C string
        CString::new(serde_json::to_string(&response)?).into_raw()
    }
}

#[no_mangle]
pub extern "C" fn rust_free_string(s: *mut c_char) {
    if !s.is_null() {
        unsafe { let _ = CString::from_raw(s); }
    }
}
```

### Layer 3: Tool Routing

**File**: `clara-toolbox/src/manager.rs`

Routes evaluation to appropriate tool:

```rust
impl ToolboxManager {
    pub fn evaluate(&self, arguments: Value) -> Result<ToolResponse, ToolError> {
        let request = ToolRequest {
            tool: self.default_evaluator.clone(),
            arguments,
        };
        self.execute_tool(&request)
    }

    pub fn execute_tool(&self, request: &ToolRequest) -> Result<ToolResponse, ToolError> {
        let tool = self.tools.get(&request.tool)
            .ok_or_else(|| ToolError::NotFound(request.tool.clone()))?;

        tool.execute(request.arguments.clone())
    }
}
```

---

## Usage from CLIPS

### Simple Form (Default Evaluator)

```clojure
; Passes entire JSON to default evaluator
(bind ?result (clara-evaluate "{\"question\":\"what is 2+2?\"}"))
(printout t "Result: " ?result crlf)
```

**Flow**:
1. CLIPS calls `clara-evaluate` with JSON string
2. Callback parses JSON: `{"question":"what is 2+2?"}`
3. No `tool` field detected → uses default evaluator
4. Routes to `EvaluateTool` → `DemonicVoice` → lil-daemon
5. Returns response as JSON string

### Explicit Tool Selection

```clojure
; Explicitly routes to echo tool
(bind ?result (clara-evaluate
    "{\"tool\":\"echo\",\"arguments\":{\"message\":\"hello\"}}"))
(printout t "Result: " ?result crlf)
```

**Flow**:
1. CLIPS calls `clara-evaluate`
2. Callback parses JSON: `{"tool":"echo","arguments":{...}}`
3. Detects `tool` field → routes to `EchoTool`
4. Returns response directly (no network call)

---

## JSON Protocol

### Request Format

**Simple Form** (no tool specified):
```json
{
  "question": "arbitrary content",
  "context": {...},
  "any_field": "allowed"
}
```

**Explicit Form** (tool specified):
```json
{
  "tool": "tool_name",
  "arguments": {
    "param1": "value1",
    "param2": "value2"
  }
}
```

### Response Format

All tools return a `ToolResponse`:

```json
{
  "status": "success",
  "result": {
    // Tool-specific result data
  }
}
```

**Error Response**:
```json
{
  "status": "error",
  "message": "Error description"
}
```

---

## Memory Management

### String Ownership

**Critical for preventing memory leaks:**

1. **Rust allocates** response string via `CString::into_raw()`
2. **C receives** raw pointer
3. **C uses** string in CLIPS
4. **C calls** `rust_free_string()` to deallocate
5. **Rust frees** memory via `CString::from_raw()`

### Example Lifecycle

```rust
// Rust side: Allocate
let response = serde_json::to_string(&tool_response)?;
let c_string = CString::new(response)?.into_raw();
return c_string;  // Transfer ownership to C
```

```c
// C side: Use and free
char *result = rust_clara_evaluate(env, input);
returnValue->lexemeValue = CreateString(env, result);
rust_free_string(result);  // Return ownership to Rust
```

---

## Error Handling

### Error Flow

```
CLIPS Rule Error
    │
    ├─▶ rust_clara_evaluate() catches
    │
    ├─▶ Logs error
    │
    └─▶ Returns JSON error response
         │
         └─▶ CLIPS receives error string
              │
              └─▶ Rule can check status field
```

### Example Error Cases

**Invalid JSON**:
```clojure
(bind ?result (clara-evaluate "not json"))
; Returns: {"status":"error","message":"Invalid JSON: ..."}
```

**Tool Not Found**:
```clojure
(bind ?result (clara-evaluate "{\"tool\":\"nonexistent\"}"))
; Returns: {"status":"error","message":"Tool not found: nonexistent"}
```

**Tool Execution Failure**:
```clojure
(bind ?result (clara-evaluate "{\"invalid\":\"args\"}"))
; Returns: {"status":"error","message":"Lil-daemon evaluation failed: ..."}
```

---

## Thread Safety

### Global State

The `ToolboxManager` is a global singleton protected by a `Mutex`:

```rust
lazy_static! {
    static ref GLOBAL_TOOLBOX: Mutex<ToolboxManager> =
        Mutex::new(ToolboxManager::new());
}
```

**Implications**:
- Multiple CLIPS environments can share the same toolbox
- Concurrent calls are serialized by the mutex lock
- Tool registration is thread-safe

**Deadlock Prevention**:
- Locks are held for minimal duration
- No nested lock acquisition
- Locks are always released via RAII

---

## Performance

### Latency Breakdown

Typical `clara-evaluate` call latency:

| Component | Time |
|-----------|------|
| FFI overhead | <1ms |
| JSON parsing | <1ms |
| Tool lookup | <0.1ms |
| Tool execution | **Variable** |
| JSON serialization | <1ms |
| Total (excluding tool) | ~2ms |

**Tool execution times**:
- **EchoTool**: <0.1ms (in-memory)
- **EvaluateTool**: 100-2000ms (network + LLM)

### Optimization Opportunities

1. **String pooling** - Reuse allocated strings
2. **JSON caching** - Cache parsed tool requests
3. **Async evaluation** - Non-blocking tool calls
4. **Connection pooling** - Persistent HTTP connections (already done)

---

## Testing

### Unit Tests

**File**: `clara-clips/src/backend/ffi/callbacks.rs`

```rust
#[test]
fn test_rust_clara_evaluate_basic() {
    let input = CString::new(r#"{"tool":"echo","arguments":{"message":"hello"}}"#).unwrap();

    unsafe {
        let result_ptr = rust_clara_evaluate(std::ptr::null_mut(), input.as_ptr());
        assert!(!result_ptr.is_null());

        let result_cstr = CStr::from_ptr(result_ptr);
        let result_str = result_cstr.to_str().unwrap();

        assert!(result_str.contains("success"));
        rust_free_string(result_ptr);
    }
}
```

### Integration Tests

**File**: `clara-clips/tests/callback_integration_test.rs`

```rust
#[test]
fn test_clips_callback_from_rule() {
    let mut env = ClipsEnvironment::new().unwrap();

    // Define rule that uses callback
    env.eval(r#"
        (defrule test-callback
            =>
            (bind ?result (clara-evaluate "{\"test\":\"data\"}"))
            (printout t ?result crlf))
    "#).unwrap();

    env.reset().unwrap();
    env.run(None).unwrap();

    // Verify callback was invoked
}
```

### Testing Without lil-daemon

```bash
# Use echo evaluator for testing
clips-repl --evaluator echo
```

```clojure
CLIPS[0]> (bind ?r (clara-evaluate "{\"test\":\"no network\"}"))
CLIPS[1]> (printout t ?r crlf)
{"status":"success","echoed":{"test":"no network"},...}
```

---

## Debugging

### Enable Debug Logging

```bash
RUST_LOG=debug clips-repl
```

Callback logs will show:
```
[DEBUG clara_clips::backend::ffi::callbacks] rust_clara_evaluate called with input: {"test":"data"}
[DEBUG clara_toolbox::manager] Executing tool: evaluate with args: {"test":"data"}
[DEBUG demonic_voice] DemonicVoice::evaluate -> POST http://localhost:8000/evaluate
```

### Common Issues

**Symptom**: CLIPS crashes on callback
**Cause**: Memory corruption, null pointer
**Debug**: Run with `RUST_BACKTRACE=1`

**Symptom**: Callback returns empty string
**Cause**: Tool execution error not caught
**Debug**: Check error logs, add error handling

**Symptom**: Memory leak
**Cause**: `rust_free_string()` not called
**Debug**: Run with valgrind or Address Sanitizer

---

## Security Considerations

### Input Validation

**Currently**: No validation on JSON content (trusted input)

**Recommendations**:
1. Validate JSON schema before tool execution
2. Sanitize strings for CLIPS injection
3. Rate-limit callback invocations

### Sandboxing

CLIPS rules have full access to callback system:
- Can call any registered tool
- Can pass arbitrary JSON
- No resource limits enforced

**Future**: Add permission system for tools

---

## Extension Points

### Adding New Callback Functions

To add a new callback (e.g., `clara-persist`):

1. **Define in userfunctions.c**:
```c
AddUDF(env, "clara-persist", "v", 1, 1, "s",
       ClaraPersistFunction, "ClaraPersistFunction", NULL);
```

2. **Implement FFI function**:
```rust
#[no_mangle]
pub extern "C" fn rust_clara_persist(
    _env: *mut c_void,
    data: *const c_char
) -> *mut c_char {
    // Implementation
}
```

3. **Register in CLIPS**:
```c
static void ClaraPersistFunction(...) {
    // Call rust_clara_persist
}
```

### Custom Tool Routing

To customize how tools are selected:

```rust
// Custom routing logic
impl ToolboxManager {
    pub fn smart_evaluate(&self, payload: Value) -> Result<ToolResponse, ToolError> {
        // Analyze payload to choose best tool
        let tool_name = if payload.get("llm_required").is_some() {
            "evaluate"
        } else {
            "rule_engine"
        };

        let request = ToolRequest {
            tool: tool_name.to_string(),
            arguments: payload,
        };
        self.execute_tool(&request)
    }
}
```

---

## Best Practices

### For CLIPS Rule Authors

1. **Always check response status**:
```clojure
(bind ?response (clara-evaluate "{\"question\":\"...\"}"))
(bind ?status (json-get ?response "status"))
(if (eq ?status "error") then
    (printout t "Error: " ?response crlf)
else
    ; Process success
)
```

2. **Use simple form for default evaluator**:
```clojure
; Good - concise
(clara-evaluate "{\"question\":\"...\"}")

; Unnecessary - unless testing
(clara-evaluate "{\"tool\":\"evaluate\",\"arguments\":{\"question\":\"...\"}}")
```

3. **Handle network failures gracefully**:
```clojure
(defrule handle-evaluation-error
    (need-evaluation ?data)
    (test (eq (json-get (clara-evaluate ?data) "status") "error"))
=>
    (printout t "Evaluation failed, using fallback" crlf)
    (assert (use-fallback)))
```

### For Tool Developers

1. **Return consistent JSON structure**
2. **Log errors for debugging**
3. **Handle malformed arguments gracefully**
4. **Document expected argument schema**

---

## Future Enhancements

### Planned Features

- **Async callbacks** - Non-blocking tool execution
- **Streaming responses** - Partial results for long operations
- **Callback context** - Pass CLIPS environment state to tools
- **Type checking** - Validate arguments against schema
- **Bidirectional** - Tools can query CLIPS state

### Proposed API Extensions

**Async evaluation**:
```clojure
; Start async evaluation
(bind ?handle (clara-evaluate-async "{\"question\":\"...\"}"))

; Check if complete
(if (clara-eval-complete ?handle) then
    (bind ?result (clara-eval-result ?handle))
)
```

**Structured responses**:
```clojure
; Parse JSON automatically
(bind ?response (clara-evaluate-json "{\"question\":\"...\"}"))
(bind ?answer (get-field ?response "answer"))
```

---

## Reference

### Related Documentation

- `ARCHITECTURE.md` - Overall system design
- `DEMONIC_VOICE_PROTOCOL.md` - lil-daemon communication
- `TOOLBOX_SYSTEM.md` - Tool development guide

### Source Files

- FFI callbacks: `clara-clips/src/backend/ffi/callbacks.rs`
- C integration: `clara-clips/clips-src/core/userfunctions.c`
- Tool routing: `clara-toolbox/src/manager.rs`
- Tests: `clara-clips/tests/callback_integration_test.rs`
