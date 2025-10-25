# CLIPS MCP Adapter

An MCP (Model Context Protocol) adapter that exposes CLIPS expert system functionality for use by Claude and other LLM clients.

## Overview

This adapter bridges the Clara REST API with the MCP standard, allowing LLMs to:
- Evaluate CLIPS expressions
- Query facts from the knowledge base
- Assert new facts
- Reset the engine
- Check engine status

## Architecture

```
┌──────────────────┐
│ Claude/LLM       │
└────────┬─────────┘
         │ MCP JSON-RPC (stdin/stdout)
         ▼
┌──────────────────────────┐
│ CLIPS MCP Adapter        │
│ (this binary)            │
└────────┬─────────────────┘
         │ HTTP REST API calls
         ▼
┌──────────────────────────┐
│ Clara REST API           │
│ (localhost:8080)         │
└────────┬─────────────────┘
         │ Subprocess management
         ▼
┌──────────────────────────┐
│ CLIPS Binary             │
│ (fresh process per eval) │
└──────────────────────────┘
```

## Building

```bash
cargo build -p clips-mcp-adapter --release
```

The binary will be at: `target/release/clips-mcp-adapter`

## Running

### Standalone Mode (with running Clara API)

```bash
# Ensure clara-api is running
cargo run --bin clara-api

# In another terminal, run the adapter
export REST_API_URL=http://localhost:8080
./target/release/clips-mcp-adapter
```

### Environment Variables

- `REST_API_URL` - Base URL of Clara REST API (default: `http://localhost:8080`)
- `RUST_LOG` - Log level (default: `info`)

## MCP Tools

### 1. clips.eval

Evaluate CLIPS expressions.

**Parameters:**
- `expression` (string, required): CLIPS expression to evaluate

**Example:**
```json
{
  "name": "clips.eval",
  "arguments": {
    "expression": "(+ 1 2)"
  }
}
```

**Response:**
```json
{
  "success": true,
  "stdout": "         CLIPS (6.4.2 1/14/25)\nCLIPS> (+ 1 2)\n3\nCLIPS> ",
  "stderr": "",
  "exit_code": 0,
  "metrics": {
    "elapsed_ms": 4
  }
}
```

### 2. clips.query

Query facts from the knowledge base.

**Parameters:**
- `template` (string, required): CLIPS template pattern
- `limit` (integer, optional): Max results (default: 100)

**Example:**
```json
{
  "name": "clips.query",
  "arguments": {
    "template": "(myfact ?x ?y)"
  }
}
```

### 3. clips.assert

Assert facts into the knowledge base.

**Parameters:**
- `facts` (array, required): List of fact templates

**Example:**
```json
{
  "name": "clips.assert",
  "arguments": {
    "facts": ["(myfact 1 \"value\")", "(myfact 2 \"other\")"]
  }
}
```

### 4. clips.reset

Reset the CLIPS engine.

**Parameters:**
- `preserve_globals` (boolean, optional): Keep global variables

**Example:**
```json
{
  "name": "clips.reset",
  "arguments": {}
}
```

### 5. clips.status

Get engine status and session information.

**Parameters:** None

**Example:**
```json
{
  "name": "clips.status",
  "arguments": {}
}
```

## MCP JSON-RPC Protocol

### Request Format

```json
{
  "jsonrpc": "2.0",
  "id": "request-1",
  "method": "tools/call",
  "params": {
    "name": "clips.eval",
    "arguments": {
      "expression": "(+ 1 2)"
    }
  }
}
```

### Response Format

```json
{
  "jsonrpc": "2.0",
  "id": "request-1",
  "result": {
    "success": true,
    "stdout": "3"
  }
}
```

### Special Methods

- `initialize` - Handshake, returns capability info
- `tools/list` - List available tools with schemas
- `tools/call` - Execute a tool with parameters

## Session Management

The adapter creates a single persistent Clara session on startup (with ID like `mcp-<uuid>`).
All tool calls use this session, maintaining state across interactions.

### Future Improvements

- Multi-session support (one session per LLM context)
- Session lifecycle hooks
- Session persistence/recovery

## Testing

### Manual Test with curl

Start the adapter and test with stdin/stdout:

```bash
# Terminal 1: Start API
cargo run --bin clara-api

# Terminal 2: Start adapter
./target/release/clips-mcp-adapter

# Terminal 3: Send requests
echo '{"jsonrpc":"2.0","id":"1","method":"initialize"}' | nc localhost 9000
```

### With MCP Client SDK

Using the official MCP client:

```python
from mcp_client import McpClient

async with McpClient("clips-mcp-adapter") as client:
    result = await client.call_tool("clips.eval", {"expression": "(+ 1 2)"})
    print(result)
```

## Architecture Decisions

### Transactional CLIPS Processing

Each eval call spawns a fresh CLIPS subprocess:
- ✓ Simple, stateless
- ✓ No process leaks
- ✓ Memory efficient
- ✓ Fast startup (< 5ms)

**Known Limitation:** Facts asserted in one eval call do not persist to subsequent eval calls within the same session. Each eval runs against a fresh CLIPS engine. This is expected behavior in the current transactional model. To maintain state across evals, use a single eval with multiple CLIPS commands, or pre-load facts using the `clips.assert` tool before querying.

### Single MCP Session

The adapter maintains one Clara session:
- ✓ Persistent knowledge base across tool calls
- ✓ Shared state maintained
- ✓ Simple implementation

### JSON-RPC over stdio

- ✓ Standard MCP transport
- ✓ Works with Claude via stdio
- ✓ No external dependencies
- ✓ Secure (local only)

## Debugging

Enable debug logging:

```bash
RUST_LOG=debug ./target/release/clips-mcp-adapter
```

## Performance

- Tool call latency: ~5-10ms (mostly CLIPS startup time)
- Session creation: ~100ms
- Memory per session: ~50MB

## Security Considerations

1. **No input validation** - Assumes trusted LLM input
2. **No CLIPS sandboxing** - Arbitrary CLIPS code execution
3. **Local only** - Only listens on stdin/stdout
4. **No auth** - Assumes local deployment

For production use:
- Add input validation and sanitization
- Consider CLIPS sandboxing
- Implement authentication if exposed
- Rate limiting on tool calls

## Future Roadmap

- [ ] WebSocket transport option
- [ ] Multi-session support
- [ ] Resource limits (fact count, memory)
- [ ] Persistent session storage
- [ ] Rule execution tracing
- [ ] Integration with Claude templates
- [ ] Performance monitoring
