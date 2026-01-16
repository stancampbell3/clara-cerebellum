# DemonicVoice Protocol

## Overview

**DemonicVoice** is a synchronous HTTP client for communicating with **lil-daemon** REST instances. A lil-daemon is any REST service that implements the `/evaluate` endpoint for arbitrary JSON evaluation.

## Protocol Specification

### Base URL

The DemonicVoice client requires a base URL pointing to a lil-daemon instance:

```rust
let voice = DemonicVoice::new("http://localhost:8000");
```

Default port: `8000` (configurable)

---

## Endpoints

### POST /evaluate

**Purpose**: Evaluate arbitrary JSON payload and return result.

**Request**:
```http
POST /evaluate HTTP/1.1
Host: localhost:8000
Content-Type: application/json

{
  "question": "What is the weather in Paris?",
  "context": {...},
  "options": {...}
}
```

**Response (Success)**:
```http
HTTP/1.1 200 OK
Content-Type: application/json

{
  "result": "The weather in Paris is sunny, 22°C",
  "confidence": 0.95,
  "sources": [...]
}
```

**Response (Error)**:
```http
HTTP/1.1 400 Bad Request
Content-Type: application/json

{
  "error": "Invalid input format",
  "details": "..."
}
```

---

## Client API

### Creating a Client

```rust
use demonic_voice::DemonicVoice;

// Connect to local lil-daemon
let voice = DemonicVoice::new("http://localhost:8000");

// Connect to remote service
let voice = DemonicVoice::new("https://api.example.com");
```

### Making Requests

```rust
use serde_json::json;

let payload = json!({
    "question": "What is 2+2?",
    "format": "brief"
});

match voice.evaluate(payload) {
    Ok(response) => {
        println!("Result: {}", response);
    }
    Err(e) => {
        eprintln!("Evaluation failed: {}", e);
    }
}
```

---

## Error Handling

### Error Types

```rust
pub enum DemonicVoiceError {
    /// HTTP/network error (connection refused, timeout, etc.)
    Http(reqwest::Error),

    /// Non-success HTTP status with response body
    Status(StatusCode, Value),

    /// Invalid base URL configuration
    InvalidBaseUrl(String),
}
```

### Error Examples

**Connection Refused**:
```rust
Err(DemonicVoiceError::Http(
    reqwest::Error { kind: Connect, ... }
))
```

**400 Bad Request**:
```rust
Err(DemonicVoiceError::Status(
    StatusCode::BAD_REQUEST,
    json!({"error": "Invalid input"})
))
```

**500 Internal Server Error**:
```rust
Err(DemonicVoiceError::Status(
    StatusCode::INTERNAL_SERVER_ERROR,
    json!({"error": "LLM API timeout"})
))
```

---

## Lil-Daemon Implementations

### Reference Implementations

Different backends can power a lil-daemon:

1. **LLM-based** (OpenAI, Anthropic, etc.)
   - Uses GPT/Claude for natural language evaluation
   - Returns generated responses as JSON

2. **Rule-based** (CLIPS, Prolog)
   - Evaluates against rule database
   - Returns inferred facts or conclusions

3. **Hybrid**
   - Routes to LLM or rules based on input
   - Combines symbolic and neural reasoning

### Minimal lil-daemon Example

```python
from flask import Flask, request, jsonify

app = Flask(__name__)

@app.route('/evaluate', methods=['POST'])
def evaluate():
    payload = request.json

    # Your evaluation logic here
    result = process_evaluation(payload)

    return jsonify(result)

if __name__ == '__main__':
    app.run(port=8000)
```

---

## LilDevils (Prolog) REST API

The **LilDevils** subsystem provides SWI-Prolog integration via the `/devils/*` endpoints. This enables backward-chaining logic programming alongside CLIPS's forward-chaining rules.

### Prolog Session Endpoints

#### POST /devils/sessions

Create a new Prolog session.

**Request**:
```json
{
  "user_id": "user-123",
  "config": {
    "max_facts": 1000,
    "max_rules": 500,
    "max_memory_mb": 128
  }
}
```

**Response**:
```json
{
  "session_id": "sess-abc123",
  "user_id": "user-123",
  "started": "2025-01-16T12:00:00Z",
  "touched": "2025-01-16T12:00:00Z",
  "status": "active",
  "resources": { "facts": 0, "rules": 0, "objects": 0 },
  "limits": { "facts": 1000, "rules": 500, "memory_mb": 128 }
}
```

#### GET /devils/sessions

List all Prolog sessions.

**Response**:
```json
{
  "sessions": [...],
  "total": 5
}
```

#### GET /devils/sessions/{session_id}

Get details for a specific Prolog session.

#### DELETE /devils/sessions/{session_id}

Terminate a Prolog session.

**Response**:
```json
{
  "session_id": "sess-abc123",
  "status": "terminated",
  "saved": false
}
```

### Prolog Query Endpoints

#### POST /devils/sessions/{session_id}/query

Execute a Prolog query in the session.

**Request**:
```json
{
  "goal": "member(X, [1, 2, 3])",
  "all_solutions": false
}
```

**Response**:
```json
{
  "result": "X = 1",
  "success": true,
  "runtime_ms": 2
}
```

**Parameters**:
- `goal`: Prolog goal to execute (required)
- `all_solutions`: If `true`, return all solutions; if `false`, return first solution only (default: `false`)

#### POST /devils/sessions/{session_id}/consult

Load Prolog clauses (facts and rules) into the session's knowledge base.

**Request**:
```json
{
  "clauses": [
    "parent(tom, mary)",
    "parent(tom, john)",
    "grandparent(X, Z) :- parent(X, Y), parent(Y, Z)"
  ]
}
```

**Response**:
```json
{
  "status": "clauses_loaded",
  "count": 3
}
```

### Example Prolog Workflow

```bash
# 1. Create a session
curl -X POST http://localhost:8080/devils/sessions \
  -H "Content-Type: application/json" \
  -d '{"user_id": "demo"}'

# Response: {"session_id": "sess-xyz", ...}

# 2. Load knowledge base
curl -X POST http://localhost:8080/devils/sessions/sess-xyz/consult \
  -H "Content-Type: application/json" \
  -d '{
    "clauses": [
      "likes(mary, food)",
      "likes(mary, wine)",
      "likes(john, wine)",
      "likes(john, mary)"
    ]
  }'

# 3. Query the knowledge base
curl -X POST http://localhost:8080/devils/sessions/sess-xyz/query \
  -H "Content-Type: application/json" \
  -d '{"goal": "likes(mary, X)"}'

# Response: {"result": "X = food", "success": true, "runtime_ms": 1}

# 4. Get all solutions
curl -X POST http://localhost:8080/devils/sessions/sess-xyz/query \
  -H "Content-Type: application/json" \
  -d '{"goal": "likes(X, wine)", "all_solutions": true}'

# 5. Terminate session
curl -X DELETE http://localhost:8080/devils/sessions/sess-xyz
```

---

## Prolog MCP Adapter

The `prolog-mcp-adapter` provides MCP (Model Context Protocol) integration for Prolog sessions, enabling use with Claude Desktop and other MCP-compatible clients.

### MCP Tools

| Tool | Description |
|------|-------------|
| `prolog.query` | Execute a Prolog goal, optionally returning all solutions |
| `prolog.consult` | Load clauses into the knowledge base |
| `prolog.retract` | Remove clauses from the knowledge base |
| `prolog.status` | Get session and engine status |

### Running the MCP Adapter

```bash
# Set REST API URL (defaults to http://localhost:8080)
export REST_API_URL=http://localhost:8080

# Run the adapter (communicates via stdin/stdout)
./target/release/prolog-mcp-adapter
```

### MCP Tool Schemas

**prolog.query**:
```json
{
  "goal": "member(X, [1, 2, 3])",
  "all_solutions": true
}
```

**prolog.consult**:
```json
{
  "clauses": [
    "fact(one)",
    "rule(X) :- fact(X)"
  ]
}
```

**prolog.retract**:
```json
{
  "clause": "fact(_)",
  "all": true
}
```

---

## Integration with Clara Cerebellum

### EvaluateTool Wrapper

The `EvaluateTool` wraps DemonicVoice for use in the toolbox system:

```rust
use demonic_voice::DemonicVoice;
use clara_toolbox::EvaluateTool;

let voice = Arc::new(DemonicVoice::new("http://localhost:8000"));
let tool = EvaluateTool::new(voice);

// Register with toolbox
manager.register_tool(Arc::new(tool));
```

### From CLIPS Rules

```clojure
; Simple form - uses default evaluator (evaluate tool)
(defrule weather-check
    (need-weather ?city)
=>
    (bind ?response (clara-evaluate
        (str-cat "{\"question\":\"weather in " ?city "\"}")))
    (printout t "Response: " ?response crlf))

; Explicit tool form
(defrule explicit-evaluate
    (need-evaluation ?data)
=>
    (bind ?response (clara-evaluate
        "{\"tool\":\"evaluate\",\"arguments\":{\"data\":\"test\"}}"))
    (printout t "Response: " ?response crlf))
```

---

## Request/Response Patterns

### Question Answering

**Request**:
```json
{
  "question": "What is the capital of France?",
  "context": "geography"
}
```

**Response**:
```json
{
  "answer": "Paris",
  "confidence": 0.99,
  "reasoning": "Paris is the capital and largest city of France"
}
```

### Data Processing

**Request**:
```json
{
  "operation": "summarize",
  "text": "Long text content...",
  "max_length": 100
}
```

**Response**:
```json
{
  "summary": "Brief summary of content",
  "word_count": 15
}
```

### Rule Evaluation

**Request**:
```json
{
  "facts": [
    {"type": "person", "name": "Alice", "age": 30},
    {"type": "rule", "if": "age > 18", "then": "adult"}
  ],
  "query": "is Alice an adult?"
}
```

**Response**:
```json
{
  "result": true,
  "matched_rules": ["age > 18 → adult"],
  "explanation": "Alice is 30 years old, which is greater than 18"
}
```

---

## Configuration

### Timeouts

DemonicVoice uses `reqwest::blocking::Client` with default timeouts:

- **Connection timeout**: 30 seconds (default)
- **Read timeout**: 30 seconds (default)

To customize (future enhancement):
```rust
// Future API
let voice = DemonicVoice::builder()
    .base_url("http://localhost:8000")
    .timeout(Duration::from_secs(60))
    .build()?;
```

### TLS/HTTPS

When using HTTPS URLs, reqwest automatically handles TLS:

```rust
let voice = DemonicVoice::new("https://secure-daemon.example.com");
```

Certificates are validated by default. For self-signed certs (not recommended):
```rust
// Future: Custom certificate handling
```

---

## Testing

### Mock lil-daemon for Testing

Use the `echo` evaluator tool for testing without a real lil-daemon:

```bash
# Start REPL with echo evaluator (no network calls)
clips-repl --evaluator echo
```

```clojure
CLIPS[0]> (clara-evaluate "{\"test\":\"data\"}")
"{"status":"success","echoed":{"test":"data"},"message":"Echo tool received..."}"
```

### Integration Tests

```rust
#[test]
fn test_demonic_voice_call() {
    // Requires lil-daemon running on localhost:8000
    let voice = DemonicVoice::new("http://localhost:8000");
    let payload = json!({"question": "test"});

    let result = voice.evaluate(payload);
    assert!(result.is_ok());
}
```

---

## Performance Considerations

### Latency

Typical latency for lil-daemon calls:

- **Local LLM**: 100-500ms
- **OpenAI API**: 500-2000ms
- **Anthropic API**: 300-1500ms
- **Rule engine**: <50ms

### Throughput

DemonicVoice is synchronous and blocking:

- Single request per client instance at a time
- For concurrent requests, create multiple instances
- Future: Async version using `tokio`

### Connection Reuse

The underlying `reqwest::Client` maintains a connection pool:
- HTTP/1.1: Keep-alive enabled
- HTTP/2: Multiplexing supported

---

## Security

### Authentication

**Current**: No authentication implemented

**Future**: Bearer token support
```rust
let voice = DemonicVoice::builder()
    .base_url("https://api.example.com")
    .auth_token("bearer_token_here")
    .build()?;
```

### Input Validation

DemonicVoice accepts arbitrary JSON. The lil-daemon is responsible for:
- Input sanitization
- Rate limiting
- Content filtering

### Network Security

- Always use HTTPS in production
- Validate TLS certificates
- Consider API gateway for lil-daemon instances

---

## Troubleshooting

### Connection Refused

**Symptom**: `Http(Connect)` error

**Solutions**:
1. Verify lil-daemon is running: `curl http://localhost:8000/evaluate`
2. Check port number in base URL
3. Verify firewall settings

### Timeout Errors

**Symptom**: `Http(Timeout)` error

**Solutions**:
1. Increase timeout (future feature)
2. Optimize lil-daemon performance
3. Check network latency

### Invalid JSON Response

**Symptom**: Deserialization errors

**Solutions**:
1. Verify lil-daemon returns valid JSON
2. Check response Content-Type header
3. Inspect raw response: `curl -v http://localhost:8000/evaluate`

### HTTP 500 Errors

**Symptom**: `Status(INTERNAL_SERVER_ERROR, ...)`

**Solutions**:
1. Check lil-daemon logs
2. Verify input payload format
3. Check LLM API status (if applicable)

---

## Future Enhancements

### Planned Features

- **Async version** - Tokio-based non-blocking client
- **Retry logic** - Exponential backoff for transient failures
- **Circuit breaker** - Fail fast when lil-daemon is down
- **Request tracing** - Distributed tracing with OpenTelemetry
- **Connection pooling** - Explicit pool size configuration
- **Authentication** - JWT, API keys, OAuth2
- **Compression** - Gzip request/response bodies
- **Streaming** - Server-sent events for long-running evaluations

### Proposed Extensions

**Batch Evaluation**:
```rust
let responses = voice.evaluate_batch(vec![payload1, payload2, payload3])?;
```

**Streaming Responses**:
```rust
let stream = voice.evaluate_stream(payload)?;
for chunk in stream {
    println!("Partial result: {}", chunk?);
}
```

---

## Reference

### Related Documentation

- `ARCHITECTURE.md` - Overall system architecture
- `CLIPS_CALLBACKS.md` - How CLIPS rules call DemonicVoice
- `TOOLBOX_SYSTEM.md` - EvaluateTool integration

### Source Code

- Implementation: `demonic-voice/src/lib.rs`
- Tests: `demonic-voice/tests/`
- Integration: `clara-toolbox/src/tools/evaluate.rs`
