# Clara MCP Adapters

MCP (Model Context Protocol) adapters that expose the Clara reasoning engines as tools to LLM clients such as Zed, Claude Desktop, and any other MCP-compatible host.

Both adapters connect to **lildaemon** (the clara-api REST backend) on port 8080 and support two transports:

| Transport | How it works | When to use |
|-----------|-------------|-------------|
| `stdio`   | JSON-RPC over stdin/stdout — the host spawns the adapter as a subprocess | Local Zed "command" config, Claude Desktop |
| `http`    | JSON-RPC over `POST /mcp` — the adapter binds a TCP port and the host connects via URL | Remote Zed "url" config, shared dev machines |

---

## Prerequisites

clara-api must be running and reachable. The adapters connect to it at startup to create a session; if the connection fails the adapter exits.

```bash
# default lildaemon endpoint the adapters expect
http://localhost:8080   # when running locally
http://pineal:8080      # when connecting across the network
```

---

## Building

```bash
# from workspace root
cargo build -p prolog-mcp-adapter -p clips-mcp-adapter

# release build for deployment
cargo build -p prolog-mcp-adapter -p clips-mcp-adapter --release
```

Binaries land in `target/debug/` or `target/release/`.

---

## Environment Variables

Both adapters share the same env interface:

| Variable | Default | Description |
|----------|---------|-------------|
| `REST_API_URL` | `http://localhost:8080` | URL of the clara-api (lildaemon) backend |
| `TRANSPORT` | `stdio` | Transport mode: `stdio` or `http` |
| `HTTP_PORT` | `1968` (prolog) / `1951` (clips) | TCP port to bind in HTTP mode |

---

## prolog-mcp-adapter

Exposes SWI-Prolog via four tools.

**Default HTTP port:** `1968` *(Do Androids Dream of Electric Sheep? — Philip K. Dick, 1968)*

### Starting locally

```bash
# stdio mode (default) — used when Zed spawns the process directly
REST_API_URL=http://localhost:8080 ./target/debug/prolog-mcp-adapter

# HTTP mode — binds 0.0.0.0:1968
TRANSPORT=http REST_API_URL=http://localhost:8080 ./target/debug/prolog-mcp-adapter

# HTTP mode on a custom port
TRANSPORT=http HTTP_PORT=19680 REST_API_URL=http://localhost:8080 ./target/debug/prolog-mcp-adapter
```

### Zed configuration

Add one of these blocks to `~/.config/zed/settings.json` under `"context_servers"`:

**Spawn subprocess (stdio) — local only:**
```json
"prolog-local": {
  "command": {
    "path": "/path/to/prolog-mcp-adapter",
    "args": [],
    "env": {
      "REST_API_URL": "http://localhost:8080"
    }
  }
}
```

**Connect to running HTTP server — local or remote:**
```json
"prolog-remote": {
  "url": "http://pineal:1968"
}
```
*(See `prolog-mcp-adapter/config/prolog-mcp-adapter.zed.json` for a copy of the remote snippet.)*

### Tools

#### `prolog.query`

Execute a Prolog goal and return variable bindings.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `goal` | string | yes | Prolog goal (e.g. `member(X, [1,2,3])`) |
| `all_solutions` | boolean | no | Return all solutions instead of just the first (default: `false`) |

Response fields: `success`, `result`, `runtime_ms`

```json
{
  "name": "prolog.query",
  "arguments": {
    "goal": "member(X, [a, b, c])",
    "all_solutions": true
  }
}
```

---

#### `prolog.consult`

Load facts and rules into the session knowledge base via `assertz`.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `clauses` | string[] | yes | Array of Prolog clauses |

Response fields: `success`, `status`, `count`

```json
{
  "name": "prolog.consult",
  "arguments": {
    "clauses": [
      "parent(tom, mary)",
      "parent(tom, bob)",
      "grandparent(X, Z) :- parent(X, Y), parent(Y, Z)"
    ]
  }
}
```

---

#### `prolog.retract`

Remove clauses from the knowledge base.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `clause` | string | yes | Clause pattern to retract (e.g. `parent(tom, _)`) |
| `all` | boolean | no | Use `retractall` to remove every match (default: `false`) |

Response fields: `success`, `result`, `runtime_ms`

```json
{
  "name": "prolog.retract",
  "arguments": {
    "clause": "parent(tom, _)",
    "all": true
  }
}
```

---

#### `prolog.status`

Return session info from lildaemon. Takes no parameters.

```json
{ "name": "prolog.status", "arguments": {} }
```

Response fields: `success`, `session`

---

## clips-mcp-adapter

Exposes the CLIPS expert system engine via five tools.

**Default HTTP port:** `1951` *(Foundation — Isaac Asimov, 1951)*

### Starting locally

```bash
# stdio mode (default)
REST_API_URL=http://localhost:8080 ./target/debug/clips-mcp-adapter

# HTTP mode — binds 0.0.0.0:1951
TRANSPORT=http REST_API_URL=http://localhost:8080 ./target/debug/clips-mcp-adapter

# HTTP mode on a custom port
TRANSPORT=http HTTP_PORT=19510 REST_API_URL=http://localhost:8080 ./target/debug/clips-mcp-adapter
```

### Zed configuration

**Spawn subprocess (stdio) — local only:**
```json
"clips-local": {
  "command": {
    "path": "/path/to/clips-mcp-adapter",
    "args": [],
    "env": {
      "REST_API_URL": "http://localhost:8080"
    }
  }
}
```

**Connect to running HTTP server — local or remote:**
```json
"clips-remote": {
  "url": "http://pineal:1951"
}
```
*(See `clips-mcp-adapter/config/clips-mcp-adapter.zed.json` for a copy of the remote snippet.)*

### Tools

#### `clips.eval`

Evaluate any CLIPS expression and return stdout/stderr.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `expression` | string | yes | CLIPS expression (e.g. `(+ 1 2)`, `(run)`) |

Response fields: `success`, `stdout`, `stderr`, `exit_code`, `metrics`

```json
{
  "name": "clips.eval",
  "arguments": {
    "expression": "(defrule hello => (printout t \"Hello world\" crlf))"
  }
}
```

---

#### `clips.query`

Query facts matching a template pattern using `find-all-facts`.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `template` | string | yes | Fact template/pattern (e.g. `(myfact ?x ?y)`) |
| `limit` | integer | no | Max results to return (default: 100) |

Response fields: `success`, `results`, `metrics`

```json
{
  "name": "clips.query",
  "arguments": {
    "template": "(person ?name ?age)"
  }
}
```

---

#### `clips.assert`

Assert one or more facts into the engine.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `facts` | string[] | yes | Facts to assert (each as a CLIPS fact string) |

Response fields: `success`, `count`, `stdout`, `metrics`

```json
{
  "name": "clips.assert",
  "arguments": {
    "facts": [
      "(person \"Alice\" 30)",
      "(person \"Bob\" 25)"
    ]
  }
}
```

---

#### `clips.reset`

Reset the CLIPS engine to its initial state (`(reset)`). Removes all facts, resets agenda.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `preserve_globals` | boolean | no | Reserved; not yet implemented (default: `false`) |

Response fields: `success`, `stdout`, `metrics`

```json
{ "name": "clips.reset", "arguments": {} }
```

---

#### `clips.status`

Return current facts (`(facts)`) and session info. Takes no parameters.

```json
{ "name": "clips.status", "arguments": {} }
```

Response fields: `success`, `facts`, `session`, `metrics`

---

## Smoke-testing the HTTP endpoints

With clara-api running and an adapter started in HTTP mode:

```bash
# health check
curl http://localhost:1968/health
# → ok

# initialize handshake
curl -s -X POST http://localhost:1968/mcp \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' | jq .

# list tools
curl -s -X POST http://localhost:1968/mcp \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' | jq .

# call a tool
curl -s -X POST http://localhost:1968/mcp \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "id": 3,
    "method": "tools/call",
    "params": {
      "name": "prolog.query",
      "arguments": { "goal": "member(X,[1,2,3])", "all_solutions": true }
    }
  }' | jq .
```

Same pattern for CLIPS on port 1951.

---

## Full Zed settings.json example

```json
{
  "context_servers": {
    "prolog-remote": {
      "url": "http://pineal:1968"
    },
    "clips-remote": {
      "url": "http://pineal:1951"
    }
  }
}
```

Or with both engines running locally as subprocesses:

```json
{
  "context_servers": {
    "prolog-local": {
      "command": {
        "path": "/mnt/vastness/home/stanc/Development/clara-cerebrum/target/release/prolog-mcp-adapter",
        "args": [],
        "env": {
          "REST_API_URL": "http://localhost:8080"
        }
      }
    },
    "clips-local": {
      "command": {
        "path": "/mnt/vastness/home/stanc/Development/clara-cerebrum/target/release/clips-mcp-adapter",
        "args": [],
        "env": {
          "REST_API_URL": "http://localhost:8080"
        }
      }
    }
  }
}
```
