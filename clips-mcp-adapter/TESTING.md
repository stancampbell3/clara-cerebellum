# MCP Adapter Testing Guide

## Quick Test

With Clara API running (`cargo run --bin clara-api`), test the MCP adapter:

```bash
# Terminal 1: Start the adapter (listens on stdin)
./target/release/clips-mcp-adapter

# Terminal 2: Send test requests
echo '{"jsonrpc":"2.0","id":"1","method":"initialize"}' | ./target/release/clips-mcp-adapter

# Or pipe multiple requests:
cat test_requests.txt | timeout 15 ./target/release/clips-mcp-adapter
```

## Test Request Examples

### 1. Initialize (Capability Handshake)
```json
{"jsonrpc":"2.0","id":"1","method":"initialize","params":{}}
```

**Response:**
```json
{
  "jsonrpc":"2.0",
  "id":"1",
  "result":{
    "name":"clara-clips",
    "version":"0.1.0",
    "capabilities":{"tools":true,"resources":false}
  }
}
```

### 2. List Tools (Discover Available Functions)
```json
{"jsonrpc":"2.0","id":"2","method":"tools/list","params":{}}
```

**Response:** Lists all 5 tools with input schemas

### 3. Evaluate CLIPS Expression
```json
{
  "jsonrpc":"2.0",
  "id":"3",
  "method":"tools/call",
  "params":{
    "name":"clips.eval",
    "arguments":{"expression":"(+ 1 2)"}
  }
}
```

**Response:**
```json
{
  "jsonrpc":"2.0",
  "id":"3",
  "result":{
    "success":true,
    "stdout":"         CLIPS (6.4.2 1/14/25)\nCLIPS> (+ 1 2)\n3\nCLIPS> (exit)\n",
    "exit_code":0,
    "metrics":{"elapsed_ms":1}
  }
}
```

### 4. Get Engine Status
```json
{
  "jsonrpc":"2.0",
  "id":"4",
  "method":"tools/call",
  "params":{
    "name":"clips.status",
    "arguments":{}
  }
}
```

**Response:** Engine status with facts count, session info

### 5. Assert Facts
```json
{
  "jsonrpc":"2.0",
  "id":"5",
  "method":"tools/call",
  "params":{
    "name":"clips.assert",
    "arguments":{
      "facts":["(myfact 1 \"value\")", "(myfact 2 \"other\")"]
    }
  }
}
```

### 6. Query Facts
```json
{
  "jsonrpc":"2.0",
  "id":"6",
  "method":"tools/call",
  "params":{
    "name":"clips.query",
    "arguments":{"template":"(myfact ?x ?y)"}
  }
}
```

### 7. Reset Engine
```json
{
  "jsonrpc":"2.0",
  "id":"7",
  "method":"tools/call",
  "params":{
    "name":"clips.reset",
    "arguments":{}
  }
}
```

## Batch Test File

Create `test_requests.txt`:
```
{"jsonrpc":"2.0","id":"1","method":"initialize","params":{}}
{"jsonrpc":"2.0","id":"2","method":"tools/list","params":{}}
{"jsonrpc":"2.0","id":"3","method":"tools/call","params":{"name":"clips.eval","arguments":{"expression":"(+ 1 2)"}}}
{"jsonrpc":"2.0","id":"4","method":"tools/call","params":{"name":"clips.status","arguments":{}}}
```

Then run:
```bash
timeout 15 ./target/release/clips-mcp-adapter < test_requests.txt | jq .
```

## Known Limitations

### Fact Persistence Across Evals

**Current behavior:** Facts asserted in one eval call do not persist to subsequent eval calls within the same session. This is because the transactional model spawns a fresh CLIPS process for each eval.

**Example of current behavior:**
```bash
# First eval: assert a fact
{"jsonrpc":"2.0","id":"1","method":"tools/call","params":{"name":"clips.eval","arguments":{"expression":"(assert (color red))"}}}

# Second eval: try to query the fact
{"jsonrpc":"2.0","id":"2","method":"tools/call","params":{"name":"clips.eval","arguments":{"expression":"(facts)"}}}
# Result: No facts returned (the asserted fact is gone)
```

**Workarounds:**

1. **Use a single eval with multiple commands:**
```json
{
  "name": "clips.eval",
  "arguments": {
    "expression": "(progn\n  (assert (color red))\n  (assert (color blue))\n  (facts)\n)"
  }
}
```

2. **Pre-load facts before queries:** Use `clips.assert` first, then query later in the same "session context" if needed.

3. **Future fix:** Will implement persistent session state management to maintain facts across eval calls.

## Debugging

Enable debug logging:
```bash
RUST_LOG=debug ./target/release/clips-mcp-adapter < test_requests.txt 2>&1 | grep -v "^\[20"
```

## Key Metrics

From successful test run:
- **Eval latency:** 1ms (CLIPS startup ~5-10ms in single process)
- **Session creation:** ~100ms
- **Tools count:** 5 (eval, query, assert, reset, status)
- **JSON-RPC version:** 2.0 compatible
- **Transport:** stdin/stdout (MCP stdio protocol)

## Integration with Claude

The adapter can be invoked by Claude as an MCP server:

```bash
# In Claude config (e.g., ~/.claude_config or settings)
{
  "mcp_servers": {
    "clips": {
      "command": "/path/to/target/release/clips-mcp-adapter",
      "env": {
        "REST_API_URL": "http://localhost:8080",
        "RUST_LOG": "info"
      }
    }
  }
}
```

Claude will:
1. Start the adapter as a subprocess
2. Send JSON-RPC requests via stdin
3. Parse JSON responses from stdout
4. Use discovered tools from `tools/list`
5. Call tools via `tools/call` RPC method
