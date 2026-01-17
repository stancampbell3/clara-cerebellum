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
````

## 🛠 Development scripts & helpers

This repository includes a small set of scripts under `./scripts/` to help bootstrapping, building, testing, and exercising the REST API. They are lightweight helpers intended for developer convenience.

Key scripts

- `./scripts/setup-dev.sh`
  - Bootstraps a developer environment: checks for `rustup`, ensures the `stable` toolchain, and runs `cargo build --workspace`.
  - Run:

    ```bash
    ./scripts/setup-dev.sh
    ```

- `./scripts/run-tests.sh`
  - Runs `cargo test --workspace`. Pass `--rest` to run the REST orchestrator after tests (service must be running at `BASE_URL`).
  - Examples:

    ```bash
    # run unit/integration tests only
    ./scripts/run-tests.sh

    # run tests and then the REST happy-path (service must be running)
    BASE_URL=http://localhost:8080 ./scripts/run-tests.sh --rest
    ```

- `./scripts/docker-build.sh`
  - Builds docker artifacts. Use `--compose` to build via `docker/docker-compose.yml` or run a local `docker/Dockerfile` build.
  - Example:

    ```bash
    # build via docker-compose
    ./scripts/docker-build.sh --compose
    ```

- `./scripts/benchmark.sh`
  - Wrapper for `cargo bench`. Run all benches or a single named benchmark.
  - Examples:

    ```bash
    # run all benches
    ./scripts/benchmark.sh

    # run a single benchmark
    ./scripts/benchmark.sh --bench eval_throughput
    ```

REST test orchestrator

A small REST test suite lives in `./scripts/rest_tests/` and includes an orchestrator `all_tests.sh` which walks a happy-path (health -> ephemeral eval -> create session -> session eval -> save -> delete).

- Run the orchestrator (from repo root):

```bash
./scripts/rest_tests/all_tests.sh
```

- With a bearer token and a custom base URL:

```bash
AUTH="your-token" BASE_URL=http://0.0.0.0:8080 ./scripts/rest_tests/all_tests.sh
```

Prerequisites

- `bash`, `curl`, and `jq` (used by the REST test scripts)
- `rustup` and a Rust toolchain (the scripts will help ensure `stable` is installed)
- `docker`/`docker-compose` if you plan to use the docker helpers

If you moved or refactored the location of the test scripts, update references or tell me the new path and I will update `./scripts/run-tests.sh` and the README accordingly.
