 CLIPS Rust REST API Service Wrapper Plan

## 1. Architecture Overview

### Goals and scope
- Primary goal: Expose CLIPS via a Rust REST API with both ephemeral and persistent execution modes.
- Initial integration: Subprocess REPL wrapper with robust protocol handling and sandboxing.
- Target integration: FFI-based shared library for lower latency, structured data exchange, and finer resource control.
- Service style: Stateless endpoints by default with optional session persistence.

### Session lifecycle stages
- Create: Allocate a session, initialize environment, apply base configuration and preload rules/facts from allow-listed sources.
- Load: Load additional rules/facts (optional), validating paths and content, and record provenance.
- Evaluate: Execute commands or scripts with framed output and error capture; enforce timeouts and resource limits.
- Inspect: Retrieve status/metadata (uptime, last eval, memory usage, loaded modules) for observability and control.
- Persist: Save session state (facts, agenda, modules) to a durable format; record version and compatibility metadata.
- Reload: Restore session state into the same or a new process; verify integrity and compatibility.
- Shutdown: Graceful exit, cleanup temp files and resources; emit final metrics and logs.

### Transition criteria to FFI
- Lifecycle reliability: All lifecycle stages above are implemented and pass deterministic tests.
- Persistence fidelity: Round-trip persistence and reload produce equivalent inference results.
- Inference correctness: A baseline corpus of test cases passes consistently under subprocess.
- Performance thresholds: Subprocess mode meets minimum throughput/latency targets and shows clear upside for FFI.
- Security hardening: Deny/allow lists enforced, sandboxing in place, audit logging available.
- Operational stability: No zombie processes, bounded resource usage, structured error handling.

### Evolution path and compatibility
- Subprocess → FFI: Replace process boundary with shared library while preserving REST contract.
- Dual-stack phase: Support both backends selectable via config for rollout and validation.
- Structured API additions: Introduce typed endpoints once FFI layer is stable; version API for compatibility.

### Architecture decision records (ADRs)
- ADR-001: Backend selection (subprocess vs FFI).
- ADR-002: Session persistence format.
- ADR-003: Security model.

---

## 2. Session and Lifecycle Management

### Session types
- Ephemeral Sessions: Spawn CLIPS per request, run script, return output, terminate. No persistence.
- Persistent Sessions: Maintain live CLIPS processes keyed by session ID. Can be explicitly saved and reloaded.

### Persistence strategy
- Default: Sessions are not persisted automatically.
- Save on demand: API supports explicit save operations (mid-session or on logout).
- Reload: Saved sessions can be restored into a new or existing process.

### Session metadata
- userId: Identifier for the owning user.
- sessionId: Unique identifier.
- started: Timestamp of creation.
- touched: Last activity timestamp.
- resources: Facts, rules, objects, modules, agenda state, file references, usage metrics.

### Resource limits
- Configurable quotas: Max facts, rules, objects, memory, eval time.
- Metadata integration: Current usage and limits exposed in metadata.
- Enforcement: Hard caps enforced at runtime; violations trigger errors/shutdown.

### Lifecycle operations
- Initialization: Preload rules/facts, validate paths, sandbox environment.
- Evaluation: Execute commands with framed output, enforce timeouts/quotas.
- Persistence: Save state to durable storage (format TBD).
- Reload: Restore state with integrity checks.
- Shutdown: Graceful exit, cleanup, enforce timeouts.

---

## 3. API Design

All endpoints use JSON request/response bodies. Errors return structured JSON with "error" and "details" fields.

### Core Endpoints

POST /eval  
Request:
  {
    "script": "(defrule hello (initial-fact) => (printout t \"Hello\" crlf))",
    "timeout_ms": 2000
  }  
Response:
  {
    "stdout": "Hello\n",
    "stderr": "",
    "exit_code": 0,
    "metrics": { "elapsed_ms": 12 }
  }

POST /sessions  
Request:
  {
    "userId": "user-123",
    "preload": ["base_rules.clp"],
    "metadata": { "description": "demo session" }
  }  
Response:
  {
    "sessionId": "sess-abc123",
    "userId": "user-123",
    "started": "2025-10-23T17:03:00Z",
    "touched": "2025-10-23T17:03:00Z",
    "resources": { "facts": 0, "rules": 10, "objects": 0 }
  }

POST /sessions/{id}/eval  
Request:
  {
    "commands": ["(assert (foo bar))", "(run)"],
    "timeout_ms": 3000
  }  
Response:
  {
    "stdout": "==> f-1 (foo bar)\n",
    "stderr": "",
    "exit_code": 0,
    "metrics": { "elapsed_ms": 8 },
    "session": {
      "sessionId": "sess-abc123",
      "touched": "2025-10-23T17:05:00Z",
      "resources": { "facts": 1, "rules": 10, "objects": 0 }
    }
  }

DELETE /sessions/{id}  
Response:
  { "sessionId": "sess-abc123", "status": "terminated", "saved": false }

### Optional Endpoints

POST /sessions/{id}/load  
Request:
  { "files": ["extra_rules.clp"] }  
Response:
  {
    "sessionId": "sess-abc123",
    "loaded": ["extra_rules.clp"],
    "resources": { "facts": 1, "rules": 15, "objects": 0 }
  }

GET /sessions/{id}/status  
Response:
  {
    "sessionId": "sess-abc123",
    "userId": "user-123",
    "started": "2025-10-23T17:03:00Z",
    "touched": "2025-10-23T17:05:00Z",
    "resources": { "facts": 1, "rules": 15, "objects": 0 },
    "limits": { "maxFacts": 1000, "maxRules": 500, "maxMemoryMb": 128 },
    "health": "ok"
  }

POST /sessions/{id}/save  
Request:
  { "label": "checkpoint-1" }  
Response:
  {
    "sessionId": "sess-abc123",
    "savedAs": "checkpoint-1",
    "timestamp": "2025-10-23T17:06:00Z"
  }

POST /sessions/{id}/reload  
Request:
  { "label": "checkpoint-1" }  
Response:
  {
    "sessionId": "sess-abc123",
    "status": "reloaded",
    "resources": { "facts": 1, "rules": 15, "objects": 0 }
  }

Error schema example:  
  { "error": "ValidationError", "details": "File path not in allow-list", "code": 400 }

---

## 4. REPL Protocol Specifics

### Prompt detection
- Wait for the canonical "CLIPS>" prompt before considering the interpreter ready.
- On startup, inject a handshake command (e.g., (printout t "__READY__" crlf)) to confirm readiness.

### Command framing
- Each command terminated with newline.
- Append sentinel marker after each evaluation, e.g. (printout t "__END__" crlf).
- Read stdout until sentinel marker observed, then strip it.

### Output capture
- stdout: All normal REPL output up to sentinel.
- stderr: Any error messages.
- exit_code: Nonzero if process terminates unexpectedly.

### Error handling
- Errors normalized into schema:
  {
    "type": "SyntaxError" | "RuntimeError" | "ResourceLimit" | "Unknown",
    "message": "string",
    "line": number (optional),
    "severity": "warning" | "error" | "fatal"
  }

### Session safety
- Deny-list enforced (system, unrestricted load, unsafe file I/O).
- Allow-list may be maintained for stricter deployments.
- Blocked commands return error with type "SecurityViolation".

### Timeout enforcement
- Each eval request has configurable timeout (default e.g. 2000 ms).
- If exceeded, subprocess terminated and error returned:
  {
    "type": "Timeout",
    "message": "Evaluation exceeded 2000 ms",
    "severity": "fatal"
  }

### Logging and tracing
- All commands logged with sessionId and timestamp.
- Output/errors logged with correlation IDs.
- Sensitive data redacted.

### Extensibility
- Sentinel marker configurable to avoid collisions.
- Future FFI integration will bypass string parsing and return structured results directly, but REPL protocol remains baseline.

---

## 5. Concurrency and Scaling

### Execution model
- Service runs on async runtime (Tokio) to multiplex I/O with CLIPS subprocesses or FFI calls.
- Each session bound to a lightweight task that manages stdin/stdout and enforces timeouts.
- Requests queued per session to ensure ordered evaluation and avoid interleaving.

### Concurrency limits
- Global maximum concurrent sessions (configurable, e.g. 100).
- Per-session evaluation queue depth (configurable, e.g. 10 pending evals).
- Global maximum concurrent evals across all sessions (configurable, e.g. 500).
- Hard caps enforced; excess requests return structured error:
  { "error": "ConcurrencyLimit", "details": "Too many concurrent sessions", "code": 429 }

### Resource isolation
- Each session has its own process (subprocess mode) or environment (FFI mode).
- CPU and memory quotas enforced via cgroups or OS limits.
- Timeouts applied per eval; runaway sessions terminated and logged.

### Scaling strategies
- Ephemeral mode: Spawn rate limited (configurable, e.g. 20/sec) to avoid fork/exec storms.
- Persistent mode: Sessions tracked in LRU cache; least recently used sessions evicted when limits reached.
- Horizontal scaling: Multiple service instances can run behind a load balancer.
  - Stateless endpoints (/eval) can be routed arbitrarily.
  - Persistent sessions require sticky routing (session affinity) to ensure continuity.

### Session eviction policy
- Eviction triggered when max sessions exceeded.
- Candidate selection: least recently touched session.
- Eviction process:
  - Attempt graceful shutdown (save if configured).
  - If shutdown fails within timeout, force terminate.
  - Emit eviction event to logs/metrics.

### Metrics and observability
- Counters:
  - active_sessions
  - eval_requests_total
  - eval_timeouts_total
  - evictions_total
- Histograms:
  - eval_latency_ms
  - session_lifetime_seconds
- Gauges:
  - current_sessions
  - queued_requests_per_session
- Metrics tagged with userId/sessionId where appropriate (with privacy safeguards).

### Future considerations
- Dynamic scaling: auto‑scale service replicas based on CPU/memory/queue depth.
- Priority scheduling: allow high‑priority sessions to preempt lower‑priority ones.
- Distributed session store: enable session migration between nodes if needed.

## 6. Security Considerations

### Authentication and authorization
- Token-based authentication (e.g., JWT or opaque tokens) with short lifetimes and refresh flow.
- Role-based access control (RBAC): admin, standard, read-only.
- Scoped tokens: "session:create", "session:eval", "session:save", "session:load", "session:status", "session:delete".
- Per-endpoint authorization checks; deny by default with explicit allow.
- Audit every auth decision (userId, scope, endpoint, outcome) with correlation IDs.

### Input validation and command filtering
- Strict validation for request bodies: types, lengths, allowed characters; reject unknown fields.
- Deny-list of dangerous CLIPS commands (system, unrestricted load, unsafe file IO).
- Allow-list mode optional for hardened deployments; only permit explicitly listed commands.
- File path validation: canonicalize paths; enforce allow-listed directories; reject symlinks escaping allow-list.
- Reject embedded binary data in scripts unless explicitly enabled via configuration.

### Process and environment sandboxing
- Subprocess mode: run CLIPS under a restricted user with minimal privileges.
- Apply resource limits: CPU, memory, file handles; enforce via OS limits/cgroups.
- Seccomp/apparmor profiles (where available) to limit syscalls.
- Isolate temp directories per session; auto-clean on shutdown; set restrictive permissions.

### Network and transport security
- TLS required for all external traffic; modern cipher suites; HSTS enabled.
- Optional mutual TLS for internal service-to-service communication.
- Disable insecure protocols; redirect HTTP to HTTPS.
- CORS configured for specific origins; preflight checks; reject wildcards in production.

### API safety controls
- Rate limiting per userId and per IP; burst and sustained limits.
- Request size limits (headers, body); reject oversized payloads.
- Concurrency caps enforced (see section #5).
- Idempotency guidance: DELETE /sessions/{id} treated as idempotent.
- CSRF protection if a browser-based client is used (tokens or same-site cookies).

### Data handling, persistence, and privacy
- Session metadata contains only minimal PII (userId); avoid storing secrets.
- Persistence storage encrypted at rest; keys managed via KMS; rotation enforced.
- Integrity checks on saved session artifacts (hashes); verify on reload.
- Data retention policies: TTLs for session saves; lifecycle rules for deletion.
- Redact sensitive tokens/IDs in logs; structured logging with least information principle.

### Auditing and observability
- Security audit log channel: auth decisions, command filters, evictions, timeouts, save/reload operations.
- Tamper-evident logging (append-only, remote sink).
- Metrics for security signals: auth_failures_total, command_blocked_total, rate_limit_hits_total.

### Error handling and responses
- Do not leak internal details; return generic error messages with codes.
- Map internal errors to public types: ValidationError, SecurityViolation, Timeout, ConcurrencyLimit, InternalError.
- Correlation ID in response headers for traceability.

### Configuration hardening
- Secure defaults: persistence off by default; strict command filtering on; allow-list required for file loads.
- Configuration precedence: env vars override config file; CLI flags override env; all changes auditable.
- Validate configuration at startup; refuse to run with insecure combinations (e.g., no TLS in production).

### Dependency and supply chain security
- Pin dependencies; use checksums/signatures for CLIPS binary/artifacts.
- Regular vulnerability scanning (SCA); patch cadence defined.
- Reproducible builds; signed container images; provenance metadata.

### Incident response and recovery
- Defined detection, triage, containment, eradication, recovery, postmortem steps.
- Kill-switch configs: temporarily disable eval or file load globally.
- Forensic artifacts retained for a limited window (logs, config snapshots) respecting privacy policies.

### Multi-tenant isolation
- Per-user quotas (sessions, evals/sec, memory) to prevent noisy-neighbor impact.
- Session affinity with isolation; no cross-session data leakage.
- Optional tenant-scoped namespaces in persistence to prevent cross-access.

### Security testing
- Unit tests for filters/validators; fuzz testing for REPL inputs.
- Integration tests: auth flows, rate limits, file load boundaries.
- Chaos/security drills: timeout storms, eviction spikes, malformed payloads, prompt spoofing.

## 7. Observability and Testing

### Logging
- Structured logging with JSON format for easy ingestion by log aggregators.
- Each log entry includes: timestamp, level, sessionId, userId (if available), requestId/correlationId, event type, outcome, latency.
- Sensitive data (auth tokens, raw scripts) redacted or hashed.
- Log levels:
  - INFO: session lifecycle events, eval start/stop, save/reload.
  - WARN: timeouts, resource limit warnings, retries.
  - ERROR: failed evals, process crashes, security violations.
  - DEBUG (optional): raw REPL I/O for troubleshooting in non-production.

### Metrics
- Exposed via /metrics endpoint (Prometheus-compatible).
- Counters:
  - sessions_created_total
  - sessions_terminated_total
  - eval_requests_total
  - eval_failures_total
  - security_violations_total
- Histograms:
  - eval_latency_ms
  - session_duration_seconds
- Gauges:
  - active_sessions
  - queued_requests
  - memory_usage_mb
- Metrics tagged with service instance ID; userId/sessionId tags optional and privacy‑controlled.

### Health checks
- /healthz: returns 200 if service is alive and dependencies reachable.
- /readyz: returns 200 if service is ready to accept traffic (e.g., warmed up, CLIPS binary available).
- /livez: returns 200 if process is not deadlocked or OOM‑killed.
- Health endpoints lightweight, no heavy checks.

### Tracing
- Distributed tracing headers supported (e.g., W3C Trace Context).
- Each request traced through API layer, session manager, REPL/FFI boundary.
- Spans include eval latency, I/O wait, serialization/deserialization.
- Traces exported to OpenTelemetry collector.

### Testing strategy
- **Unit tests:**
  - Protocol parsing (sentinel detection, error schema).
  - Session metadata handling.
  - Config validation.
- **Integration tests:**
  - End‑to‑end eval with real CLIPS binary.
  - Session persistence and reload round‑trip.
  - Security filters (blocked commands, invalid file paths).
- **Load tests:**
  - High concurrency evals across ephemeral and persistent sessions.
  - Stress eviction policies and resource limits.
- **Fuzz tests:**
  - Randomized REPL input to detect parser crashes.
  - Malformed JSON payloads for API robustness.
- **Chaos tests:**
  - Kill CLIPS subprocess mid‑eval; verify recovery.
  - Induce timeouts and resource exhaustion; verify graceful handling.

### CI/CD integration
- Automated test suite runs on every commit.
- Coverage thresholds enforced (e.g., 80%+).
- Static analysis and linting included.
- Security scans (SCA, container image scanning) part of pipeline.
- Load/chaos tests run nightly or pre‑release.

### Observability goals
- Mean time to detect (MTTD) < 1 minute for critical failures.
- Mean time to recover (MTTR) < 5 minutes with auto‑restart and eviction.
- Logs and metrics sufficient to reconstruct session history and diagnose issues without raw REPL dumps.

