# Spawning the Edgequake MCP Server (Node) from Dis (Rust)

> **Context:** The Edgequake MCP server is installed as a Node dependency in the Clara-stack Edgequake fork. Dis (Rust, long-lived) spawns it as a managed stdio subprocess for Evaluator instances. This document catalogues the issues and mitigations for that integration.

## Architecture

```
Dis (Rust, long-lived)
  ├── MCP server A (edgequake, Node)   ← stdio child
  ├── MCP server B (other capability)  ← stdio child
  ├── Evaluator 1 (LLM call)           ← uses A, B
  ├── Evaluator 2 (LLM call)           ← uses A, B
  └── ...
```

Dis spawns Node, talks JSON-RPC over stdin/stdout, and acts as the process supervisor.

---

## 1. Which `node` binary, and where's the package?

`npx @edgequake/mcp-server` is a **network operation** on first invocation. In a production Dis domain that's a non-starter.

```rust
// NOT this:
Command::new("npx").arg("@edgequake/mcp-server")

// This:
Command::new("/opt/edgequake/node_modules/.bin/edgequake-mcp")
```

- Pin the binary path. Bake `npm ci` into the Dis image build.
- The `node` binary itself should be a known absolute path (`/usr/local/bin/node` or a version-pinned `/opt/node-22/bin/node`), not a PATH lookup.
- If you have **multiple** MCP servers as Node deps, use a single `node_modules` tree (one `package.json` in the Dis workspace, `npm ci`, done).

---

## 2. stdio is your IPC channel — treat it like a socket

MCP over stdio means:

- **stdin** → Dis writes JSON-RPC requests (newline-delimited)
- **stdout** → Node writes JSON-RPC responses + notifications
- **stderr** → Node's logs (keep this out of the protocol stream)

```rust
use tokio::process::Command;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

let mut child = Command::new("/opt/edgequake/node_modules/.bin/edgequake-mcp")
    .env("EDGEQUAKE_API", "http://127.0.0.1:8000")
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())  // ← critical, see below
    .kill_on_drop(true)      // ← critical, see below
    .spawn()?;
```

### Pitfalls

- **stderr backpressure:** If you don't drain `stderr` into a pipe or a log file, the OS pipe buffer (~64 KB on Linux) fills up, Node's stderr write **blocks**, and your MCP server hangs mid-response. You *must* read stderr in a dedicated task and forward it to your logger.
- **stdout framing:** MCP uses **newline-delimited JSON** (one JSON-RPC message per line). Don't assume Content-Length headers like LSP. Read line-by-line:

```rust
let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
```

---

## 3. Lifecycle: spawn, readiness, death

### Readiness

Node + `@modelcontextprotocol/sdk` takes ~200–500 ms to boot. If Dis fires the first `tools/list` before the server is ready, you'll get an empty response or a parse error.

**Solution:** Use the MCP `initialize` handshake. Send `initialize`, wait for `initialized` notification. That's your readiness gate. It's in the spec.

### Death / crash

- Dis's `wait()` future resolves. You get an `ExitStatus`.
- You need a **supervisor loop**: detect death → log → respawn (with backoff) → re-handshake → notify active Evaluators that their tool calls just died.
- Active in-flight JSON-RPC requests will hang (the pipe closes, `read` returns 0 bytes / EOF). Your MCP client layer in Rust must treat EOF as a fatal error, not "waiting for more data."

```rust
// Pseudocode for the supervisor
loop {
    let mut child = spawn_mcp_server()?;
    handshake(&mut child).await?;
    // ... serve requests ...
    let status = child.wait().await?;
    if !status.success() {
        log::error!("MCP server died: {status}");
    }
    backoff.sleep().await;
    // loop restarts
}
```

### Multiple Evaluators, one server?

| | Shared server | Per-Evaluator server |
|---|---|---|
| **Process count** | 1 Node process for all Evaluators | N Node processes |
| **Concurrency** | Node's event loop multiplexes (fine, I/O-bound HTTP calls) | Each gets its own event loop |
| **Isolation** | One bad request can affect others | Full isolation |
| **Resource cost** | ~50 MB RSS per Node process × N | Higher |
| **MCP spec** | Protocol is **connection-oriented**: one stdio pipe = one client session. Requires multiplexing at the Dis layer. | One pipe per Evaluator. Clean. |

**Recommendation:** Spawn one Node process **per Evaluator instance** (or per conversation). Node is cheap. 50 MB × 10 Evaluators is 500 MB, which is fine. Each gets its own clean stdio pipe. Dis is the supervisor for all of them. When an Evaluator finishes, `kill` its Node child.

If you go shared, you're implementing a JSON-RPC session multiplexer. Doable, but it's *your* code now.

---

## 4. Signal handling and shutdown ordering

When Dis gets `SIGTERM` (container orchestrator, manual stop, etc.):

1. Stop accepting new Evaluator sessions.
2. Send MCP `shutdown` notification to each Node child (graceful).
3. Wait up to N seconds for them to ack.
4. `SIGKILL` any stragglers.
5. Exit 0.

In Rust, use `tokio::signal::unix::signal(SignalKind::terminate())` to trigger this sequence. **Don't** just let the process die — you'll orphan the Node children.

`kill_on_drop(true)` is your safety net, but it fires on *drop*, not on *signal*. Make sure your shutdown path actually drops the `Child` handles (or explicitly calls `.kill().await`).

---

## 5. Environment and secrets

Pass config via **env vars**, not CLI args. They show up in `ps aux` otherwise.

```rust
.env("EDGEQUAKE_API", "http://127.0.0.1:8000")
.env("EDGEQUAKE_TOKEN", &secret_token)  // from your secret store
```

Make sure the Node process **doesn't** inherit the full Dis environment. A leaked `DATABASE_URL` or `AWS_SECRET_KEY` in the Node process's `env` is a lateral-movement vector.

```rust
// Minimal env:
.env("EDGEQUAKE_API", "...")
.env("EDGEQUAKE_TOKEN", "...")
.env("PATH", "/usr/local/bin:/usr/bin:/bin")  // minimal
```

---

## 6. File descriptor and process limits

Each Node child uses:
- 3 fds for stdio (in, out, err)
- 1+ for the TCP connection to Edgequake
- Internal Node bookkeeping (event loop, etc.)

If you're spawning 50 Evaluators, that's ~200+ fds in Dis. Linux default `ulimit -n` is often 1024 (fine), but check. In a container, the cgroup `pids.max` limit can be lower than expected. Set it explicitly in your container spec if running many Evaluators.

---

## 7. The Rust ↔ Node impedance mismatch

- **JSON serialization:** Use `serde_json` in Rust, `JSON.stringify`/`parse` in Node. Both sides must agree on UTF-8, no BOM, no trailing commas.
- **Async model:** Rust's `tokio` and Node's libuv are both event loops, but in **separate processes**. A Node event-loop stall doesn't block Dis (good). Backpressure is handled by the pipe buffer. If Node is slow, Dis's write to stdin will eventually block on a full pipe — handle with read-side timeouts.
- **Timeouts:** Wrap every `read_line()` in a `tokio::time::timeout`. A hung Node process should not wedge Dis forever. 30 seconds is reasonable for a GraphRAG query.

---

## TL;DR Checklist

| Concern | Action |
|---|---|
| Binary resolution | Absolute path to `node_modules/.bin/edgequake-mcp`, baked at image build |
| Spawn | `tokio::process::Command`, `kill_on_drop(true)`, `stderr` piped |
| Readiness | MCP `initialize` handshake before first tool call |
| stderr | Dedicated `tokio::task` draining it to your logger |
| stdout | Line-delimited JSON, `BufReader::lines()`, per-read timeout |
| Crash | `wait()` in a supervisor loop, backoff + respawn, notify Evaluators |
| Shutdown | `SIGTERM` handler → MCP `shutdown` → grace period → `SIGKILL` → exit |
| Env | Minimal, explicit. No inherited secrets. |
| Concurrency | One Node process per Evaluator (simplest), or multiplex (more work) |
| Limits | Check `ulimit -n` and cgroup `pids.max` in the container spec |

---

*Prepared for Clara-stack team review. Questions → #edgequake-integration.*
