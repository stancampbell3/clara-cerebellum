# clara-pitsnake

A Rust MCP (Model Context Protocol) server that bridges MCP tool calls to any
LSP (Language Server Protocol) compatible language server running as a managed
subprocess. It is the Rust counterpart to
[PitSnake](../../lildaemon/pitsnake/README.md) in the lildaemon project.

Where PitSnake uses `jedi` for Python-specific static analysis, `clara-pitsnake`
speaks the standard LSP protocol, making it compatible with any language server:
`rust-analyzer`, `pyright`, `clangd`, `gopls`, `typescript-language-server`, etc.

---

## Tools

Six MCP tools are exposed, mirroring the LSP-style interface of the Python
PitSnake:

| Tool | LSP Method | Description |
|------|-----------|-------------|
| `lsp_goto_definition` | `textDocument/definition` | Jump to the definition of the symbol at a position |
| `lsp_find_references` | `textDocument/references` | Find all usages of a symbol across the workspace |
| `lsp_hover` | `textDocument/hover` | Get type information and documentation for a symbol |
| `lsp_get_completions` | `textDocument/completion` | Get code completion suggestions at a position |
| `lsp_search_symbols` | `workspace/symbol` | Search for symbols by name across the workspace |
| `lsp_get_diagnostics` | `didOpen` → `publishDiagnostics` | Get errors, warnings, and hints for a file |

All position parameters use the **LSP convention: 0-indexed lines and columns**.

---

## Transports

### stdio (default)

The server reads newline-delimited JSON-RPC from stdin and writes responses to
stdout. This is the standard MCP subprocess transport, suitable for use with
Claude Code and lildaemon evaluators.

```bash
PITSNAKE_LSP_COMMAND=rust-analyzer \
PITSNAKE_WORKSPACE=/path/to/project \
clara-pitsnake
```

### HTTP

An Axum HTTP server is started with three endpoints:

- `POST /mcp` — synchronous JSON-RPC request/response
- `GET /sse` — SSE stream; sends an `endpoint` event with your session's POST URL, then forwards responses as `message` events
- `POST /message?sessionId=<id>` — send a request for an SSE session
- `GET /health` — returns `ok`

```bash
PITSNAKE_LSP_COMMAND=rust-analyzer \
PITSNAKE_WORKSPACE=/path/to/project \
TRANSPORT=http \
HTTP_HOST=127.0.0.1 \
HTTP_PORT=8765 \
clara-pitsnake
```

---

## Configuration

All configuration is via environment variables:

| Variable | Default | Description |
|----------|---------|-------------|
| `PITSNAKE_LSP_COMMAND` | `rust-analyzer` | Language server executable |
| `PITSNAKE_LSP_ARGS` | _(empty)_ | Space-separated extra arguments passed to the language server |
| `PITSNAKE_WORKSPACE` | current directory | Workspace root directory (sent as `rootUri` during LSP initialization) |
| `TRANSPORT` | `stdio` | Transport mode: `stdio` or `http` |
| `HTTP_HOST` | `127.0.0.1` | Bind address for HTTP transport |
| `HTTP_PORT` | `8765` | Bind port for HTTP transport |
| `PITSNAKE_LSP_TIMEOUT` | `30` | LSP request timeout in seconds |

Logging level is controlled by `RUST_LOG` (e.g. `RUST_LOG=debug`).

---

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                      clara-pitsnake                     │
│                                                         │
│  MCP client (Claude / lildaemon)                        │
│       │ JSON-RPC (stdio or HTTP)                        │
│       ▼                                                 │
│  ┌─────────────┐     ┌──────────────┐                   │
│  │  server.rs  │────▶│   tools.rs   │                   │
│  │ (MCP layer) │     │ (6 tool fns) │                   │
│  └─────────────┘     └──────┬───────┘                   │
│                             │                           │
│                      ┌──────▼────────┐                  │
│                      │ lsp_client.rs │                  │
│                      │               │                  │
│                      │  writer task  │──── stdin ──▶    │
│                      │  reader task  │◀─── stdout ──    │
│                      └───────────────┘                  │
│                             │                           │
│                    Language server subprocess            │
│               (rust-analyzer / pyright / clangd / …)   │
└─────────────────────────────────────────────────────────┘
```

### LSP client internals (`lsp_client.rs`)

The LSP client manages the language server subprocess and the LSP protocol
framing (`Content-Length: N\r\n\r\n<body>`).

**Writer task** — owns `ChildStdin`. Receives serialised JSON bodies from a
`tokio::sync::mpsc` channel and writes them with the correct framing. All
outgoing bytes are serialised through a single task so no locking is required
on the pipe.

**Reader task** — owns `ChildStdout`. Uses `read_line` on a `BufReader` to
parse headers safely (binary-safe; no `BufReader::lines()` which assumes UTF-8
text) then `read_exact` for the body. Dispatches:

- **Responses** (messages with an `id` and no `method`): looked up in the
  pending-request map by id and forwarded to the caller's `oneshot` channel.
- **`textDocument/publishDiagnostics` notifications**: stored in a URI-keyed
  diagnostics cache.
- All other notifications are trace-logged and discarded.

**Concurrent request handling**: each call to `LspClient::request()` inserts a
`oneshot::Sender` into the shared pending map *before* enqueuing bytes, keyed
by an atomically-incremented request id. Callers then await their own
`oneshot::Receiver` independently — 50 concurrent tool calls never block each
other.

**Diagnostics**: LSP servers push diagnostics as notifications rather than
responding to a pull request (LSP ≤ 3.16). `fetch_diagnostics()` sends
`textDocument/didOpen`, polls the diagnostics cache every 50 ms until an entry
appears or the timeout elapses, then sends `textDocument/didClose`. A timeout
is treated as an empty diagnostic list rather than an error.

---

## Tool Details

### `lsp_goto_definition`

```json
{ "file_path": "/abs/path/to/file.rs", "line": 42, "column": 8 }
```

Returns:
```json
{ "definitions": [{ "uri": "file:///...", "range": { "start": {...}, "end": {...} } }] }
```

Normalises `Location`, `Location[]`, and `LocationLink[]` responses to a
uniform array of `{ uri, range }` objects.

### `lsp_find_references`

```json
{ "file_path": "...", "line": 42, "column": 8, "include_declaration": true }
```

Returns:
```json
{ "references": [{ "uri": "...", "range": {...} }] }
```

### `lsp_hover`

```json
{ "file_path": "...", "line": 42, "column": 8 }
```

Returns:
```json
{ "contents": "```rust\nfn foo(x: u32) -> u32\n```\nDoes the thing.", "range": {...} }
```

`Hover.contents` is normalised from any of its three possible LSP shapes
(`MarkupContent`, `MarkedString`, `MarkedString[]`) to a plain markdown string.

### `lsp_get_completions`

```json
{ "file_path": "...", "line": 42, "column": 8 }
```

Returns:
```json
{
  "completions": [
    { "label": "println!", "kind": 3, "detail": "macro", "documentation": "..." }
  ]
}
```

Handles both `CompletionList` and `CompletionItem[]` response shapes. `kind`
values follow the [LSP CompletionItemKind](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#completionItemKind)
enum (1=Text, 2=Method, 3=Function, …).

### `lsp_search_symbols`

```json
{ "query": "DeductionSession" }
```

Returns:
```json
{
  "symbols": [
    { "name": "DeductionSession", "kind": 5, "location": {...}, "container_name": "clara_cycle" }
  ]
}
```

Compatible with both `SymbolInformation` (LSP 3.16) and `WorkspaceSymbol`
(LSP 3.17) response shapes.

### `lsp_get_diagnostics`

```json
{ "file_path": "/abs/path/to/file.rs" }
```

Returns:
```json
{
  "file_path": "/abs/path/to/file.rs",
  "diagnostics": [
    {
      "severity": "error",
      "range": { "start": { "line": 5, "character": 3 }, "end": { "line": 5, "character": 10 } },
      "message": "cannot borrow `x` as mutable because it is also borrowed as immutable",
      "source": "rust-analyzer",
      "code": "E0502"
    }
  ]
}
```

`severity` is mapped from the LSP integer (1–4) to a human-readable label:
`error`, `warning`, `information`, `hint`.

The `languageId` sent in `didOpen` is inferred from the file extension
(`.rs` → `rust`, `.py` → `python`, `.ts`/`.tsx` → `typescript`, etc.).
Unknown extensions default to `plaintext`.

---

## Integration with lildaemon

Register `clara-pitsnake` as an MCP server in lildaemon's `evaluators.yaml`,
pointed at the clara-cerebrum workspace:

```yaml
- name: pitsnake-rs
  command: clara-pitsnake
  env:
    PITSNAKE_LSP_COMMAND: rust-analyzer
    PITSNAKE_WORKSPACE: /path/to/clara-cerebrum
```

For stdio transport no additional configuration is needed — lildaemon manages
the subprocess lifecycle.

---

## Building

```bash
# From the clara-cerebrum workspace root:
cargo build -p clara-pitsnake --release

# The binary is at:
./target/release/clara-pitsnake
```
