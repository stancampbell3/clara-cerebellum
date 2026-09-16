# Clara — OpenAI-Compatible API Facade: API Surface
# Brainstorming with Clara about exposing an API for integration with existing agent frameworks

## 1. Endpoints

| Method | Path | Purpose |
|--------|------|---------|
| `GET` | `/v1/models` | Model discovery (frameworks call this at startup) |
| `GET` | `/v1/models/{model_id}` | Single model metadata |
| `POST` | `/v1/chat/completions` | **Primary** — all agent frameworks hit this |
| `POST` | `/v1/responses` | *(Phase 2)* Newer API for OpenAI Agents SDK |
| `GET` | `/health` | Liveness/readiness (not OpenAI, but ops-essential) |

Auth: `Authorization: Bearer <key>`. Accept any non-empty token (or map to internal API keys). Return `401` with the standard error envelope on failure.

---

## 2. `GET /v1/models`

```json
// Response 200
{
  "object": "list",
  "data": [
    {
      "id": "clara-reasoner",
      "object": "model",
      "created": 1717000000,
      "owned_by": "clara"
    },
    {
      "id": "clara-fast",
      "object": "model",
      "created": 1717000000,
      "owned_by": "clara"
    }
  ]
}
```

> **Tip:** Expose 2–3 "models" that map to different Clara configurations (e.g. `clara-fast` = single-node, `clara-reasoner` = full distributed multi-step, `clara-deep` = extended chain-of-thought). This lets frameworks pick via the `model` field without any custom logic.

---

## 3. `POST /v1/chat/completions` — Request

```jsonc
{
  "model": "clara-reasoner",          // maps to internal Clara config
  "messages": [
    { "role": "system",    "content": "You are Clara…" },
    { "role": "user",      "content": "Explain distributed consensus…" },
    { "role": "assistant", "content": "Certainly…" },
    { "role": "user",      "content": "Now compare Raft vs Paxos." }
  ],

  // --- Sampling / generation params (all optional) ---
  "temperature": 0.7,
  "top_p": 0.9,
  "max_tokens": 4096,                  // alias: "max_completion_tokens"
  "n": 1,
  "stop": null,                        // string | string[]
  "presence_penalty": 0.0,
  "frequency_penalty": 0.0,
  "seed": null,

  // --- Structured output ---
  "response_format": { "type": "json_object" },  // or "text"

  // --- Tool / function calling (Phase 2) ---
  "tools": [
    {
      "type": "function",
      "function": {
        "name": "search_knowledge_base",
        "description": "Query Clara's knowledge graph",
        "parameters": {
          "type": "object",
          "properties": {
            "query": { "type": "string" }
          },
          "required": ["query"]
        }
      }
    }
  ],
  "tool_choice": "auto",               // "auto" | "none" | {type:"function",function:{name:...}}

  // --- Streaming ---
  "stream": true,
  "stream_options": { "include_usage": true },

  // --- Clara-specific (extension, ignored by vanilla OpenAI clients) ---
  "clara": {
    "reasoning_depth": 3,             // number of internal reasoning steps
    "parallel_agents": 2,             // fan-out width
    "cite_sources": true
  },

  "user": "agent-framework-xyz"       // opaque, for logging
}
```

> **Design note:** The `clara` block is a *superset* extension. OpenAI-compatible clients will simply not send it; native Clara clients can. This keeps the facade spec-clean while letting you expose distributed-systems knobs.

---

## 4. `POST /v1/chat/completions` — Response (non-streaming)

```jsonc
{
  "id": "chatcmpl-abc123",
  "object": "chat.completion",
  "created": 1717000123,
  "model": "clara-reasoner",

  "choices": [
    {
      "index": 0,
      "message": {
        "role": "assistant",
        "content": "Raft and Paxos are both consensus algorithms…",
        "tool_calls": null,
        "refusal": null
      },
      "finish_reason": "stop",        // "stop" | "length" | "tool_calls" | "content_filter"
      "logprobs": null
    }
  ],

  "usage": {
    "prompt_tokens": 142,
    "completion_tokens": 387,
    "total_tokens": 529
  },

  "system_fingerprint": "clara-v2.3.1"
}
```

### Tool-call response variant (when `tools` is set and Clara decides to call one):

```jsonc
"message": {
  "role": "assistant",
  "content": null,
  "tool_calls": [
    {
      "id": "call_xyz789",
      "type": "function",
      "function": {
        "name": "search_knowledge_base",
        "arguments": "{\"query\": \"Raft vs Paxos comparison\"}"
      }
    }
  ]
}
// finish_reason: "tool_calls"
```

---

## 5. Streaming (SSE) Format

Content-Type: `text/event-stream`

Each event is `data: <json>\n\n`. The stream terminates with `data: [DONE]\n\n`.

```
data: {"id":"chatcmpl-abc123","object":"chat.completion.chunk","created":1717000123,"model":"clara-reasoner","choices":[{"index":0,"delta":{"role":"assistant","content":""},"finish_reason":null}]}

data: {"id":"chatcmpl-abc123","object":"chat.completion.chunk","created":1717000123,"model":"clara-reasoner","choices":[{"index":0,"delta":{"content":"Raft"},"finish_reason":null}]}

data: {"id":"chatcmpl-abc123","object":"chat.completion.chunk","created":1717000123,"model":"clara-reasoner","choices":[{"index":0,"delta":{"content":" and"},"finish_reason":null}]}

data: {"id":"chatcmpl-abc123","object":"chat.completion.chunk","created":1717000123,"model":"clara-reasoner","choices":[{"index":0,"delta":{"content":" Paxos"},"finish_reason":null}]}

data: {"id":"chatcmpl-abc123","object":"chat.completion.chunk","created":1717000123,"model":"clara-reasoner","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}

data: {"id":"chatcmpl-abc123","object":"chat.completion.chunk","created":1717000123,"model":"clara-reasoner","choices":[],"usage":{"prompt_tokens":142,"completion_tokens":387,"total_tokens":529}}

data: [DONE]
```

> **Clara-specific:** If Clara runs multi-step reasoning internally, you can emit a *reasoning* delta before the final answer (similar to OpenAI's `reasoning` content part in o-series models):
> ```json
> {"choices":[{"index":0,"delta":{"reasoning":"Step 1: Identify consensus properties…"},"finish_reason":null}]}
> ```
> This is a superset extension; vanilla clients ignore unknown delta fields.

---

## 6. Error Envelope (all endpoints)

```json
{
  "error": {
    "message": "Model 'clara-ultra' not found. Available: clara-fast, clara-reasoner, clara-deep.",
    "type": "invalid_request_error",
    "param": "model",
    "code": "model_not_found"
  }
}
```

| HTTP | `type` | When |
|------|--------|------|
| 400 | `invalid_request_error` | Malformed body, bad params |
| 401 | `authentication_error` | Missing/invalid Bearer token |
| 403 | `permission_error` | Key lacks access to that model |
| 404 | `invalid_request_error` | Unknown model ID |
| 429 | `rate_limit_error` | Token/burst limit exceeded |
| 500 | `server_error` | Internal Clara node failure |
| 503 | `server_error` | Clara cluster degraded, retry later |

---

## 7. Headers to Respect / Emit

| Header | Direction | Notes |
|--------|-----------|-------|
| `Authorization: Bearer …` | In | Auth |
| `Content-Type: application/json` | In | (or `text/event-stream` out) |
| `X-Request-ID` | In → echo out | Correlate logs across distributed nodes |
| `Retry-After` | Out (429) | Seconds until retry |
| `X-RateLimit-Limit` / `X-RateLimit-Remaining` | Out | Polite rate-limit signalling |

---

## 8. Implementation Checklist (suggested stack)

```
┌─────────────────────────────────────────────────────────────────────────┐
│  Agent Framework (LangGraph, CrewAI, Agents SDK…)                      │
└───────────────────────────────┬─────────────────────────────────────────┘
                                │  HTTPS / Bearer
                                ▼
┌─────────────────────────────────────────────────────────────────────────┐
│  API Gateway / Reverse Proxy (Caddy, Envoy, or NGINX)                  │
│  • TLS termination                                                     │
│  • Rate limiting (token bucket)                                        │
│  • Request ID injection                                                │
└───────────────────────────────┬─────────────────────────────────────────┘
                                ▼
┌─────────────────────────────────────────────────────────────────────────┐
│  Facade Service (FastAPI / Starlette + Uvicorn)                        │
│  • /v1/models                                                          │
│  • /v1/chat/completions  (sync + SSE stream)                           │
│  • Request validation (Pydantic schemas)                               │
│  • Model-name → Clara config mapping                                   │
│  • Token accounting                                                    │
└───────────────────────────────┬─────────────────────────────────────────┘
                                ▼
┌─────────────────────────────────────────────────────────────────────────┐
│  Clara Distributed Reasoning Core                                      │
│  • Planner → Executor → Aggregator                                     │
│  • Internal message bus (gRPC / Redis Streams)                         │
│  • Streaming: push deltas back via async generator                     │
└─────────────────────────────────────────────────────────────────────────┘
```

**Key implementation details:**

1. **Streaming bridge:** Clara's internal async event stream → Python `async generator` → `StreamingResponse` (Starlette) → SSE. Use `yield` per token/chunk; flush per event.
2. **Timeouts:** Set a generous upstream timeout (Clara can take 30 s+ for deep reasoning). Use a *heartbeat* SSE comment (`: keep-alive\n\n`) every 15 s to prevent proxy idle-timeout kills.
3. **Idempotency:** If the client retries, Clara should deduplicate on `X-Request-ID` or a client-supplied `idempotency_key` (extension field) to avoid double-execution.
4. **Backpressure:** If Clara's internal queue is full, return `429` with `Retry-After` rather than buffering indefinitely.

---

## 9. Conformance Test Matrix (smoke tests)

| Test | What it verifies |
|------|-----------------|
| `GET /v1/models` returns ≥ 1 model | Discovery |
| `POST /v1/chat/completions` (non-stream) returns valid schema | Basic round-trip |
| `POST /v1/chat/completions` (`stream: true`) yields valid SSE chunks ending in `[DONE]` | Streaming |
| `POST` with `max_tokens: 1` → `finish_reason: "length"` | Truncation |
| `POST` with unknown model → `404` + error envelope | Error handling |
| `POST` with `response_format: {type: "json_object"}` → content is valid JSON | Structured output |
| `POST` with `tools` → response contains `tool_calls` or ignores gracefully | Tool-calling contract |
| No `Authorization` header → `401` | Auth |
| 100 concurrent requests → no 5xx, p99 < SLO | Load / distributed correctness |

Write these as a `pytest` suite hitting a live facade instance. Run against LangGraph, CrewAI, and the OpenAI Agents SDK as *integration* tests (they'll catch schema drift you'd miss with raw HTTP tests).

---

## 10. Candidate Agent Frameworks to Investigate

### Tier 1 — Highest-traffic, most-adopted

| Framework | Why it matters |
|---|---|
| **LangChain / LangGraph** | De-facto standard. `base_url` + `api_key` is the entire integration surface. LangGraph adds stateful multi-step reasoning. |
| **OpenAI Agents SDK** (ex-Swarm) | OpenAI's own framework. Natively speaks the Responses/Chat API. |
| **CrewAI** | Very popular multi-agent orchestration. Plugs in via `OpenAI` LLM class with a custom `base_url`. |
| **AutoGen / AG2 (Microsoft)** | Enterprise multi-agent. Supports OpenAI-compatible endpoints. |

### Tier 2 — Strong, growing, or niche-relevant

| Framework | Why it matters |
|---|---|
| **Pydantic AI** | Explicitly designed around OpenAI-compatible APIs. Type-safe, Pythonic. |
| **Semantic Kernel (Microsoft)** | Enterprise/C#-heavy. Relevant for .NET shops. |
| **LlamaIndex** | RAG-centric with a full agent layer. Supports `base_url` override. |
| **Smolagents (Hugging Face)** | Lightweight, code-generating agents. Trivial integration. |

### Tier 3 — Platform / workflow / IDE integrations

| Framework | Why it matters |
|---|---|
| **Dify / Flowise / n8n** | Visual workflow builders. All support OpenAI-compatible endpoints. |
| **Continue.dev / VS Code Copilot** | IDE-embedded agents. Both accept a custom `base_url`. |
| **Haystack (deepset)** | NLP pipeline framework with agent support. European enterprise footprint. |

### Practical Recommendations

1. **Implement the Chat Completions API first** (`/v1/chat/completions`) — that's the 95% case.
2. **Add the Responses API** (`/v1/responses`) if targeting OpenAI Agents SDK.
3. **Expose `/v1/models`** — most frameworks call this at startup.
4. **Support streaming (SSE)** — non-negotiable for UX.
5. **Pick 2–3 from Tier 1 for a smoke-test matrix** (LangGraph, OpenAI Agents SDK, CrewAI) and write a conformance test suite.
