# Clara Cerebellum 🧠

Clara Cerebellum is a modular Rust workspace that exposes the CLIPS expert system via a secure, scalable REST API. It supports both ephemeral and persistent session modes, with robust lifecycle management, sandboxing, and observability. Designed for inference orchestration, rule-based automation, and structured decision support.

---

## 🧩 Architecture Overview

- **Subprocess-first, FFI-ready**: Initial integration uses a REPL subprocess with framed I/O and protocol enforcement. Future evolution targets FFI for lower latency and structured data exchange.
- **Session lifecycle**: Create → Load → Evaluate → Inspect → Persist → Reload → Shutdown.
- **Dual-mode execution**: Stateless ephemeral sessions or long-lived persistent sessions with save/reload support.
- **Security-first**: RBAC, scoped tokens, command filtering, sandboxing, and audit logging.
- **Observability**: Metrics, tracing, structured logging, and health endpoints.

---

## 📦 Workspace Crates

| Crate              | Purpose                                 |
|--------------------|------------------------------------------|
| `clara-api`        | REST API server (Axum-based)             |
| `clara-core`       | Session orchestration and lifecycle FSM  |
| `clara-clips`      | CLIPS subprocess/FFI integration         |
| `clara-session`    | Session store, manager, eviction logic   |
| `clara-persistence`| Save/load formats and storage backends   |
| `clara-security`   | Auth, RBAC, sandboxing, filtering        |
| `clara-metrics`    | Metrics, tracing, logging exporters      |
| `clara-config`     | Config loading, validation, env overrides|

---

## 🚀 Quick Start

```bash
# Clone the repo
git clone https://github.com/your-org/clara-cerebellum.git
cd clara-cerebellum

# Set up development environment
./scripts/setup-dev.sh

# Run the API server
cargo run -p clara-api

