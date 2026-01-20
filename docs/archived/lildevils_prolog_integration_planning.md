> **Status: IMPLEMENTED** ✅
>
> This planning document has been fully implemented. See:
> - `clara-prolog/` - Rust FFI bindings for SWI-Prolog
> - `clara-session/` - Session management with Prolog support
> - `clara-api/` - REST API endpoints at `/devils/*`
> - `prolog-mcp-adapter/` - MCP adapter for Claude Desktop integration
>
> For API documentation, see `DEMONIC_VOICE_PROTOCOL.md`.

---

# Rust ↔ SWI‑Prolog Integration Overview

This document summarizes the proposed architecture for embedding SWI‑Prolog inside a Rust server, exposing Rust callbacks to Prolog, and providing a REST interface for managing Prolog sessions.

## 1. High‑Level Architecture
- **Rust host server**: owns lifecycle, REST API, session registry, callback dispatch.  
- **Embedded SWI‑Prolog engine**: initialized inside Rust using the SWI C API.  
- **Foreign predicates**: Rust functions exposed to Prolog via C shims.  
- **Rust → Prolog invocation layer**: synchronous goal execution and result extraction.  
- **Session manager**: isolates per‑session state and engine resources.  

## 2. Embedding Prolog in Rust
- **Initialize SWI‑Prolog runtime** using `PL_initialise`.  
- **Choose engine model**: global engine, per‑session engine, or Prolog threads.  
- **Load Prolog modules** at startup.  
- **Define Rust wrappers** for constructing terms and calling goals.  

## 3. Prolog → Rust Callbacks (Foreign Predicates)
- **C shims callable from Prolog** that forward to Rust via FFI.  
- **Extern "C" Rust functions** returning atoms, lists, dicts, or JSON.  
- **Register predicates** with `PL_register_foreign`.  
- **Term conversion layer** for atoms, strings, lists, dicts.  
- **Memory‑safety boundaries** between Rust and Prolog.  

## 4. Rust → Prolog Calls
- **Construct terms programmatically** using SWI term APIs.  
- **Invoke goals** via `PL_call` or query iteration.  
- **Extract results** into Rust types or JSON.  
- **Capture exceptions** and convert to Rust error types.  

## 5. Session Model
- **Session registry** mapping session IDs to engine state.  
- **Per‑session dynamic predicates** or dicts for state isolation.  
- **Optional per‑session modules** for rule isolation.  
- **Cleanup hooks** for teardown and resource reclamation.  

## 6. REST API Layer
- **Create session endpoint** for allocating engines or modules.  
- **Run query endpoint** for JSON → Prolog → JSON round‑trips.  
- **Callback registration endpoint** if dynamic foreign predicates are needed.  
- **Session teardown endpoint** for cleanup.  
- **Streaming/incremental results** for long‑running queries.  

## 7. Data Exchange Format
- **JSON as the interchange format** between Rust and Prolog.  
- **Term ↔ JSON mapping rules** for atoms, lists, dicts, numbers.  
- **Opaque handles** for Rust‑owned objects referenced from Prolog.  

## 8. Concurrency & Threading
- **SWI‑Prolog thread model** for multi‑engine setups.  
- **Rust async integration** ensuring synchronous Prolog calls per engine.  
- **Callback safety** to avoid blocking Prolog’s scheduler.  

## 9. Build & Deployment
- **Linking SWI‑Prolog** via dynamic or static linking.  
- **Cross‑platform considerations** for macOS/Linux.  
- **Module search paths** for `.pl` files.  

## 10. Testing & Debugging
- **Unit tests for foreign predicates** executed from Prolog.  
- **Integration tests** for REST → Rust → Prolog flows.  
- **Tracing/logging hooks** across FFI boundaries.  
- **Backtracking behavior tests** for Rust callbacks.  

---
