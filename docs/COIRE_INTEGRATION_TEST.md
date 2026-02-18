# Coire Integration Test Plan

## What Is Under Test

The engine-to-Coire bindings: foreign predicates in SWI-Prolog and user-defined functions in CLIPS that allow both engines to emit events to and poll events from the shared in-memory Coire (DuckDB event mailbox).

### Components

| Component | Artifact | Role |
|-----------|----------|------|
| `clara-coire` | Global singleton (`OnceLock<Coire>`) | Shared event store |
| `clara-prolog` | `coire_emit/3`, `coire_poll/2`, `coire_mark/1`, `coire_count/2` | Prolog foreign predicates |
| `clara-clips` | `(coire-emit ...)`, `(coire-poll ...)`, `(coire-mark ...)`, `(coire-count ...)` | CLIPS UDFs |
| Startup wiring | `clara_coire::init_global()` in API, Prolog REPL, CLIPS REPL | Singleton lifecycle |

## Test Procedure

### 1. Unit tests (already passing)

```bash
cargo test -p clara-coire -- --nocapture
```

Validates: `poll_pending` atomicity, session isolation, event ordering, mark/drain/clear semantics.

### 2. Prolog REPL round-trip

Start the Prolog REPL and exercise coire predicates directly.

```bash
RUST_LOG=info cargo run --bin prolog-repl
```

```prolog
`%% Pick a session UUID (use any valid v4 UUID)
?- coire_emit('aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee', 'prolog', '{"msg":"hello from prolog"}').
%% Expected: true

?- coire_count('aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee', N).
%% Expected: N = 1

?- coire_poll('aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee', Events).
%% Expected: Events = JSON array string with one event, status "Processed"

?- coire_count('aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee', N).
%% Expected: N = 0 (poll marked it processed)`
```

### 3. CLIPS REPL round-trip

Start the CLIPS REPL and exercise coire UDFs.

```bash
RUST_LOG=info cargo run --bin clips-repl
```

```clips
(coire-emit "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee" "clips" "{\"msg\":\"hello from clips\"}")
;; Expected: "ok"

(coire-count "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee")
;; Expected: 1

(coire-poll "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee")
;; Expected: JSON array string with one event

(coire-count "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee")
;; Expected: 0
```

### 4. Cross-engine round-trip (API server)

This is the key integration test. Both engines share one Coire instance in the API server process.

```bash
RUST_LOG=info cargo run --bin clara-api
```

From a Prolog session (via REST or WebSocket), emit an event:

```prolog
?- coire_emit('shared-session-uuid', 'prolog', '{"intent":"greet"}').
```

From a CLIPS session in the same process, poll for it:

```clips
(coire-poll "shared-session-uuid")
;; Expected: JSON array containing the event emitted by Prolog
```

And vice versa: emit from CLIPS, poll from Prolog.

## What to Monitor

### Logs

Run with `RUST_LOG=info` (or `debug` for more detail). Watch for:

- `"Global Coire initialized"` at startup
- `"All coire predicates registered"` (Prolog predicate registration)
- Any `ERROR` lines from `coire_emit`, `coire_poll`, `coire_mark`, `coire_count`

### Failure signatures

| Symptom | Likely cause |
|---------|-------------|
| Panic: `"Global Coire not initialized"` | `init_global()` not called before engine use |
| Prolog predicate returns `false` | Bad UUID format, invalid JSON payload, or Coire not initialized |
| CLIPS returns `{"error":"..."}` | Same as above; the error message will say which argument is wrong |
| Linker errors on build | Missing `pub use` re-export for FFI symbols in `clara-clips/src/lib.rs` |
| `EventNotFound` from `coire_mark` | Event already processed or UUID typo |

## Success Criteria

1. `cargo test -p clara-coire` passes all 10 tests
2. Prolog REPL: emit/count/poll/count cycle works as described above
3. CLIPS REPL: emit/count/poll/count cycle works as described above
4. Cross-engine: event emitted by one engine is visible to the other via `poll` in the same process
5. No panics, no undefined-symbol errors, no leaked memory (poll frees returned strings)
