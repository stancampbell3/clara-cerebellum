# Toolbox System

## Overview

The Clara Toolbox system provides a pluggable architecture for extending CLIPS rules with external capabilities. Tools can be invoked from CLIPS rules via the `clara-evaluate` callback, enabling integration with LLMs, APIs, databases, and other services.

## Architecture

```
┌──────────────────────────────────────┐
│       ToolboxManager (Singleton)     │
│  ┌────────────────────────────────┐  │
│  │ Tool Registry                  │  │
│  │  ├─ "echo" → EchoTool         │  │
│  │  ├─ "evaluate" → EvaluateTool │  │
│  │  └─ "custom" → CustomTool     │  │
│  └────────────────────────────────┘  │
│  ┌────────────────────────────────┐  │
│  │ Default Evaluator: "evaluate"  │  │
│  └────────────────────────────────┘  │
└──────────────────────────────────────┘
         │
         ├─▶ execute_tool(request)
         └─▶ evaluate(arguments)
```

---

## Core Components

### Tool Trait

All tools must implement the `Tool` trait:

```rust
pub trait Tool: Send + Sync {
    /// Unique identifier for this tool
    fn name(&self) -> &str;

    /// Human-readable description
    fn description(&self) -> &str;

    /// Execute the tool with given arguments
    fn execute(&self, args: Value) -> Result<Value, ToolError>;
}
```

### ToolRequest

Standard request format:

```rust
pub struct ToolRequest {
    pub tool: String,       // Tool name
    pub arguments: Value,   // Tool-specific JSON arguments
}
```

### ToolResponse

Standard response format:

```rust
pub struct ToolResponse {
    pub status: String,     // "success" or "error"
    pub result: Value,      // Tool-specific result data
}

impl ToolResponse {
    pub fn success(result: Value) -> Self;
    pub fn error(message: impl Into<String>) -> Self;
}
```

### ToolError

Error types for tool execution:

```rust
pub enum ToolError {
    NotFound(String),                    // Tool not registered
    ExecutionFailed(String),             // Tool execution error
    InvalidArguments(String),            // Malformed arguments
    Timeout,                             // Execution timeout
    NetworkError(String),                // Network-related error
}
```

---

## Built-in Tools

### EchoTool

**Purpose**: Simple echo for testing and debugging

**Arguments**: Arbitrary JSON

**Response**:
```json
{
  "status": "success",
  "echoed": { ... },  // Original arguments echoed back
  "message": "Echo tool received and returned your input"
}
```

**Example Usage**:
```clojure
(bind ?result (clara-evaluate
    "{\"tool\":\"echo\",\"arguments\":{\"message\":\"hello\"}}"))
; Returns: {"status":"success","echoed":{"message":"hello"},...}
```

**Implementation**:
```rust
pub struct EchoTool;

impl Tool for EchoTool {
    fn name(&self) -> &str { "echo" }

    fn description(&self) -> &str {
        "Echoes back the provided arguments (for testing)"
    }

    fn execute(&self, args: Value) -> Result<Value, ToolError> {
        Ok(json!({
            "echoed": args,
            "message": "Echo tool received and returned your input"
        }))
    }
}
```

---

### EvaluateTool

**Purpose**: Routes evaluation requests to a lil-daemon instance via DemonicVoice client

**Arguments**: Arbitrary JSON payload for the lil-daemon

**Response**: Whatever the lil-daemon returns

**Configuration**:
```rust
let daemon_voice = Arc::new(DemonicVoice::new("http://localhost:8000"));
let tool = EvaluateTool::new(daemon_voice);
```

**Example Usage**:
```clojure
; Simple form (uses default evaluator)
(bind ?result (clara-evaluate "{\"question\":\"what is 2+2?\"}"))

; Explicit form
(bind ?result (clara-evaluate
    "{\"tool\":\"evaluate\",\"arguments\":{\"question\":\"what is 2+2?\"}}"))
```

**Implementation**:
```rust
pub struct EvaluateTool {
    daemon_voice: Arc<DemonicVoice>,
}

impl Tool for EvaluateTool {
    fn name(&self) -> &str { "evaluate" }

    fn description(&self) -> &str {
        "Evaluates expressions via lil-daemon evaluation endpoint"
    }

    fn execute(&self, args: Value) -> Result<Value, ToolError> {
        self.daemon_voice.evaluate(args)
            .map_err(|e| ToolError::ExecutionFailed(format!("{}", e)))
    }
}
```

---

## ToolboxManager

### Singleton Pattern

```rust
lazy_static! {
    static ref GLOBAL_TOOLBOX: Mutex<ToolboxManager> =
        Mutex::new(ToolboxManager::new());
}

impl ToolboxManager {
    pub fn global() -> &'static Mutex<ToolboxManager> {
        &GLOBAL_TOOLBOX
    }
}
```

### API

```rust
impl ToolboxManager {
    // Create new manager (private, use global())
    fn new() -> Self;

    // Register a tool
    pub fn register_tool(&mut self, tool: Arc<dyn Tool>);

    // Execute named tool
    pub fn execute_tool(&self, request: &ToolRequest)
        -> Result<ToolResponse, ToolError>;

    // Execute default evaluator
    pub fn evaluate(&self, arguments: Value)
        -> Result<ToolResponse, ToolError>;

    // Set default evaluator
    pub fn set_default_evaluator(&mut self, tool_name: impl Into<String>);

    // Get default evaluator name
    pub fn get_default_evaluator(&self) -> &str;

    // List registered tools
    pub fn list_tools(&self) -> Vec<String>;
}
```

---

## Tool Registration

### At Startup (REPL)

```rust
// Initialize with built-in tools
ToolboxManager::init_global();

// Register custom tools
{
    let mut manager = ToolboxManager::global().lock().unwrap();

    // Add DemonicVoice-backed evaluator
    let daemon_voice = Arc::new(DemonicVoice::new("http://localhost:8000"));
    manager.register_tool(Arc::new(EvaluateTool::new(daemon_voice)));

    // Set default evaluator
    manager.set_default_evaluator("evaluate");
}
```

### Dynamic Registration

```rust
// At runtime
let mut manager = ToolboxManager::global().lock().unwrap();
manager.register_tool(Arc::new(MyCustomTool::new()));
```

---

## Creating Custom Tools

### Step 1: Implement Tool Trait

```rust
use clara_toolbox::{Tool, ToolError};
use serde_json::Value;

pub struct WeatherTool {
    api_key: String,
}

impl WeatherTool {
    pub fn new(api_key: String) -> Self {
        Self { api_key }
    }
}

impl Tool for WeatherTool {
    fn name(&self) -> &str {
        "weather"
    }

    fn description(&self) -> &str {
        "Fetches weather data for a given location"
    }

    fn execute(&self, args: Value) -> Result<Value, ToolError> {
        // Extract location from arguments
        let location = args.get("location")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArguments(
                "Missing 'location' field".into()
            ))?;

        // Call weather API
        let response = fetch_weather(location, &self.api_key)
            .map_err(|e| ToolError::NetworkError(format!("{}", e)))?;

        // Return result
        Ok(json!({
            "location": location,
            "temperature": response.temperature,
            "conditions": response.conditions,
        }))
    }
}
```

### Step 2: Register Tool

```rust
let mut manager = ToolboxManager::global().lock().unwrap();
let weather_tool = WeatherTool::new("API_KEY".to_string());
manager.register_tool(Arc::new(weather_tool));
```

### Step 3: Use from CLIPS

```clojure
(defrule check-weather
    (need-weather ?city)
=>
    (bind ?response (clara-evaluate
        (str-cat "{\"tool\":\"weather\",\"arguments\":{\"location\":\""
                 ?city "\"}}")))
    (printout t "Weather: " ?response crlf))
```

---

## Tool Development Best Practices

### Argument Validation

```rust
impl Tool for MyTool {
    fn execute(&self, args: Value) -> Result<Value, ToolError> {
        // Validate required fields
        let required = args.get("required_field")
            .ok_or_else(|| ToolError::InvalidArguments(
                "Missing required_field".into()
            ))?;

        // Validate types
        let count = args.get("count")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| ToolError::InvalidArguments(
                "count must be a number".into()
            ))?;

        // Proceed with execution
        // ...
    }
}
```

### Error Handling

```rust
impl Tool for MyTool {
    fn execute(&self, args: Value) -> Result<Value, ToolError> {
        match external_api_call(args) {
            Ok(result) => Ok(result),
            Err(ApiError::Timeout) => Err(ToolError::Timeout),
            Err(ApiError::Network(e)) => Err(ToolError::NetworkError(format!("{}", e))),
            Err(e) => Err(ToolError::ExecutionFailed(format!("{}", e))),
        }
    }
}
```

### Logging

```rust
impl Tool for MyTool {
    fn execute(&self, args: Value) -> Result<Value, ToolError> {
        log::debug!("MyTool executing with args: {}", args);

        let result = perform_work(args)?;

        log::info!("MyTool completed successfully");
        Ok(result)
    }
}
```

### Resource Management

```rust
pub struct DatabaseTool {
    pool: Arc<DbPool>,  // Shared connection pool
}

impl Tool for DatabaseTool {
    fn execute(&self, args: Value) -> Result<Value, ToolError> {
        // Get connection from pool (not creating new connection each time)
        let mut conn = self.pool.get()
            .map_err(|e| ToolError::ExecutionFailed(format!("{}", e)))?;

        // Execute query
        let result = conn.query(args)?;

        Ok(result)
    }
}
```

---

## Default Evaluator Configuration

### Setting Default

The default evaluator is used when `clara-evaluate` is called without an explicit `tool` field:

```rust
let mut manager = ToolboxManager::global().lock().unwrap();

// Set to "evaluate" for production (calls lil-daemon)
manager.set_default_evaluator("evaluate");

// Set to "echo" for testing (no network calls)
manager.set_default_evaluator("echo");
```

### REPL Configuration

```bash
# Use evaluate (default)
clips-repl

# Use echo for testing
clips-repl --evaluator echo
```

### Usage Difference

**With evaluate (default)**:
```clojure
; Calls lil-daemon at http://localhost:8000
(clara-evaluate "{\"question\":\"what is 2+2?\"}")
```

**With echo**:
```clojure
; Returns echoed arguments (no network call)
(clara-evaluate "{\"test\":\"data\"}")
; → {"status":"success","echoed":{"test":"data"},...}
```

---

## Tool Composition

### Sequential Tool Calls

```clojure
(defrule multi-tool-workflow
    (start-workflow ?data)
=>
    ; Step 1: Process with tool A
    (bind ?step1 (clara-evaluate
        "{\"tool\":\"process\",\"arguments\":{\"data\":\"" ?data "\"}}"))

    ; Step 2: Enhance with tool B
    (bind ?step2 (clara-evaluate
        "{\"tool\":\"enhance\",\"arguments\":" ?step1 "}"))

    ; Step 3: Finalize with tool C
    (bind ?final (clara-evaluate
        "{\"tool\":\"finalize\",\"arguments\":" ?step2 "}"))

    (printout t "Final result: " ?final crlf))
```

### Conditional Tool Selection

```clojure
(defrule smart-evaluation
    (query ?type ?content)
=>
    (if (eq ?type "simple") then
        (bind ?tool "echo")
    else
        (bind ?tool "evaluate"))

    (bind ?result (clara-evaluate
        (str-cat "{\"tool\":\"" ?tool "\",\"arguments\":{\"content\":\""
                 ?content "\"}}")))

    (printout t "Result: " ?result crlf))
```

---

## Testing Tools

### Unit Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_my_tool_success() {
        let tool = MyTool::new();
        let args = json!({"param": "value"});

        let result = tool.execute(args).unwrap();

        assert_eq!(result["status"], "success");
        assert!(result["data"].is_object());
    }

    #[test]
    fn test_my_tool_missing_arguments() {
        let tool = MyTool::new();
        let args = json!({});  // Missing required fields

        let result = tool.execute(args);

        assert!(result.is_err());
        match result {
            Err(ToolError::InvalidArguments(_)) => (),
            _ => panic!("Expected InvalidArguments error"),
        }
    }
}
```

### Integration Testing

```rust
#[test]
fn test_tool_via_toolbox_manager() {
    let mut manager = ToolboxManager::new();
    manager.register_tool(Arc::new(EchoTool));

    let request = ToolRequest {
        tool: "echo".to_string(),
        arguments: json!({"message": "test"}),
    };

    let response = manager.execute_tool(&request).unwrap();

    assert_eq!(response.status, "success");
    assert_eq!(response.result["echoed"]["message"], "test");
}
```

### Testing Without External Dependencies

```bash
# Use echo evaluator to avoid lil-daemon dependency
clips-repl --evaluator echo

# Test tool directly
CLIPS[0]> (clara-evaluate "{\"test\":\"data\"}")
"{"status":"success","echoed":{"test":"data"},...}"
```

---

## Performance Considerations

### Tool Execution Latency

| Tool | Typical Latency |
|------|----------------|
| EchoTool | <1ms |
| EvaluateTool (local LLM) | 100-500ms |
| EvaluateTool (OpenAI) | 500-2000ms |
| Custom HTTP tool | Network-dependent |
| Database tool | 10-100ms |

### Optimization Strategies

**Connection Pooling**:
```rust
pub struct ApiTool {
    client: Arc<reqwest::Client>,  // Reuse HTTP client
}
```

**Caching**:
```rust
pub struct CachedTool {
    cache: Arc<Mutex<HashMap<String, Value>>>,
    inner: Arc<dyn Tool>,
}

impl Tool for CachedTool {
    fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let cache_key = format!("{:?}", args);

        // Check cache
        if let Some(cached) = self.cache.lock().unwrap().get(&cache_key) {
            return Ok(cached.clone());
        }

        // Execute and cache
        let result = self.inner.execute(args)?;
        self.cache.lock().unwrap().insert(cache_key, result.clone());

        Ok(result)
    }
}
```

**Async Tools** (future):
```rust
#[async_trait]
pub trait AsyncTool {
    async fn execute(&self, args: Value) -> Result<Value, ToolError>;
}
```

---

## Security Considerations

### Input Validation

Tools should validate all inputs:

```rust
impl Tool for DatabaseTool {
    fn execute(&self, args: Value) -> Result<Value, ToolError> {
        // Validate query
        let query = args.get("query")
            .and_then(|v| v.as_str())
            .ok_or(ToolError::InvalidArguments("Missing query".into()))?;

        // Prevent SQL injection
        if query.contains("DROP") || query.contains("DELETE") {
            return Err(ToolError::InvalidArguments(
                "Dangerous operations not allowed".into()
            ));
        }

        // Proceed safely
        // ...
    }
}
```

### Rate Limiting

```rust
pub struct RateLimitedTool {
    inner: Arc<dyn Tool>,
    limiter: Arc<Mutex<RateLimiter>>,
}

impl Tool for RateLimitedTool {
    fn execute(&self, args: Value) -> Result<Value, ToolError> {
        // Check rate limit
        if !self.limiter.lock().unwrap().allow() {
            return Err(ToolError::ExecutionFailed(
                "Rate limit exceeded".into()
            ));
        }

        self.inner.execute(args)
    }
}
```

### Authentication

```rust
pub struct AuthenticatedTool {
    api_key: String,
}

impl Tool for AuthenticatedTool {
    fn execute(&self, args: Value) -> Result<Value, ToolError> {
        // Verify tool caller has permission
        // Authenticate with external service
        // ...
    }
}
```

---

## Advanced Patterns

### Tool Middleware

```rust
pub struct LoggingMiddleware<T: Tool> {
    inner: T,
}

impl<T: Tool> Tool for LoggingMiddleware<T> {
    fn name(&self) -> &str { self.inner.name() }
    fn description(&self) -> &str { self.inner.description() }

    fn execute(&self, args: Value) -> Result<Value, ToolError> {
        log::info!("Tool {} called with: {}", self.name(), args);

        let start = Instant::now();
        let result = self.inner.execute(args);
        let duration = start.elapsed();

        log::info!("Tool {} completed in {:?}", self.name(), duration);

        result
    }
}
```

### Tool Chains

```rust
pub struct ToolChain {
    tools: Vec<Arc<dyn Tool>>,
}

impl Tool for ToolChain {
    fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let mut current = args;

        for tool in &self.tools {
            current = tool.execute(current)?;
        }

        Ok(current)
    }
}
```

### Fallback Tools

```rust
pub struct FallbackTool {
    primary: Arc<dyn Tool>,
    fallback: Arc<dyn Tool>,
}

impl Tool for FallbackTool {
    fn execute(&self, args: Value) -> Result<Value, ToolError> {
        match self.primary.execute(args.clone()) {
            Ok(result) => Ok(result),
            Err(e) => {
                log::warn!("Primary tool failed: {}, trying fallback", e);
                self.fallback.execute(args)
            }
        }
    }
}
```

---

## Future Enhancements

### Planned Features

- **Tool discovery** - Auto-register tools from plugins
- **Async tools** - Non-blocking execution
- **Tool permissions** - Role-based access control
- **Tool versioning** - Multiple versions of same tool
- **Streaming responses** - Long-running operations
- **Tool composition DSL** - Declarative tool chains

### Proposed APIs

**Tool Discovery**:
```rust
ToolboxManager::discover_plugins("./plugins")?;
```

**Async Execution**:
```rust
let handle = manager.execute_async(request).await?;
let result = handle.await?;
```

**Permissions**:
```rust
manager.register_tool_with_permissions(
    Arc::new(AdminTool),
    vec!["admin", "superuser"]
)?;
```

---

## Troubleshooting

### Tool Not Found

**Symptom**: `ToolError::NotFound("mytool")`

**Solutions**:
1. Verify tool is registered: `manager.list_tools()`
2. Check tool name spelling
3. Ensure `register_tool()` was called

### Tool Execution Timeout

**Symptom**: Tool never returns

**Solutions**:
1. Add timeout to tool implementation
2. Use async tools (future)
3. Check for deadlocks in tool code

### Memory Leak

**Symptom**: Memory grows with tool calls

**Solutions**:
1. Ensure `Arc<dyn Tool>` is dropped properly
2. Check for circular references
3. Use weak references where appropriate

---

## Reference

### Related Documentation

- `ARCHITECTURE.md` - Overall system design
- `CLIPS_CALLBACKS.md` - How tools are invoked from CLIPS
- `DEMONIC_VOICE_PROTOCOL.md` - EvaluateTool implementation

### Source Code

- Tool trait: `clara-toolbox/src/tool.rs`
- Manager: `clara-toolbox/src/manager.rs`
- Built-in tools: `clara-toolbox/src/tools/`
- Tests: `clara-toolbox/tests/`
