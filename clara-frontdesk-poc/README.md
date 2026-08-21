# Clara — general-purpose learning assistant

A Rust/actix-web WebSocket frontend for `lildaemon`'s
`goat/app/assistant/` API — a general-purpose assistant that researches
what it doesn't already know (via a standing Snek + Edgequake-ingest
Ritual) and gets visibly better-grounded over time as more topics get
asked about. Successor to the earlier "City of Dis" visitor-intake demo,
which this crate used to be; same Rust service, WS protocol, and Docker
deploy shape, entirely different backend.

## What it does

A user chats with **Clara** via a browser-based WebSocket UI. Each message
triggers one call to lildaemon's `POST /assistant/sessions/{id}/send`,
which:

1. **Classifies** the message as `chat` or `knowledge_query`, via a
   Prolog predicate the active *ruleset* defines (see below) — a
   deterministic policy, not an LLM call (see
   `lildaemon/docs/assistant_demo.md` for why).
2. For `chat`: answers directly from Clara's own knowledge.
3. For `knowledge_query`: checks whether this topic (`slugify(query)`) has
   already been researched — researching it first (a real Snek crawl +
   Edgequake ingest, via a **standing Ritual** joined once and reused
   across every turn, not re-created per query) only if it hasn't — then
   answers using Clara's own knowledge *and* Edgequake grounding,
   reconciled into one response. Every ingested page is tagged (topic,
   query, source URL) and lands in one shared, ever-growing Edgequake
   workspace (`assistant.general`), so grounding keeps improving across
   *unrelated* topics over the session's lifetime, not just repeated
   questions on the same topic. A document's effective lifetime resets
   every time any answer cites it, regardless of which topic first
   crawled it (see `lildaemon/docs/assistant_demo.md`'s "Document tagging
   redesign").

This frontend itself is thin: each browser tab logs in with its own
username (no password — see "Known constraints" below), creates one
assistant session per WebSocket connection, and relays each message to
`/send`. No admit/deny/redirect terminal states — this is continuous
chat, not a gated interaction.

## Architecture

```
Browser (WS) ──► clara-frontdesk (8088)
                      │
                      └─► lildaemon /assistant/* (6666)
                              ├─► standing Ritual (Snek + EdgequakeIngest,
                              │     joined once, reused every turn)
                              ├─► clara-api /deduce (8080)
                              │     └─► clara-cycle: Prolog + CLIPS
                              └─► Edgequake (graph RAG) — one shared
                                    workspace, documents tagged per topic
```

## Rulesets: how behavior is swapped

The frontend never changes — a ruleset is a plain Prolog file, selected
entirely **server-side** (lildaemon's `ASSISTANT_RULESET_PATH` env var,
or the default `goat/app/assistant/rulesets/general_assistant.pl`).
Every ruleset must implement this contract:

```prolog
assistant_turn(+Query, -Action, -Reply) is semidet.
%   Action ∈ {chat, knowledge_query}.
%   chat: also bind Reply — sent straight to the user.
%   knowledge_query: bind Reply = none — the platform runs its own fixed
%   research_step/8 + answer_step/9 pipeline and generates the reply.

research_step/8, answer_step/9, extract_hohi_response/2
%   Fixed platform predicates every ruleset must copy verbatim (see
%   general_assistant.pl for the reference implementation) — only
%   assistant_turn/3 is meant to actually vary between rulesets.
```

Two rulesets exist today:

| Ruleset | Classification policy | Chat tone |
|---|---|---|
| `general_assistant.pl` (default) | Research everything except obvious small talk | Whatever Clara's model returns, unmodified |
| `terse_analyst.pl` | Only research when explicitly asked ("research", "look up", "latest", ...) | Forced one-sentence, no-pleasantries |

Swap by setting `ASSISTANT_RULESET_PATH` on the lildaemon process — no
`clara-api` restart needed (confirmed live, 10/10 alternating calls with
no restart in between).

## File layout

```
clara-frontdesk-poc/
├── Cargo.toml
├── config/
│   ├── city_of_dis.toml       # Docker deploy: persona copy + service URLs
│   └── localnet_dis.toml      # local dev variant
├── src/
│   ├── main.rs               # server init, POST /login route
│   ├── config.rs              # TOML config structs
│   ├── state.rs                # AppState shared across WS connections
│   ├── assistant_client.rs    # blocking REST client for /assistant/*
│   └── ws.rs                  # WebSocket actor: one session per connection
└── static/
    └── index.html              # single-file chat UI
```

(`lildaemon/goat/app/assistant/rulesets/` — not in this crate — is where
the actual ruleset `.pl` files live, since ruleset selection is entirely
lildaemon-side.)

## Running it

### Docker (normal path)

Part of the standard `./clara.sh up -d` stack (see the workspace root) —
builds from `docker/Dockerfile`'s `frontdesk` stage, config baked in from
`config/city_of_dis.toml`. Open `http://localhost:8088`.

### Local dev

```bash
cargo build -p clara-frontdesk-poc
FRONTDESK_CONFIG=clara-frontdesk-poc/config/localnet_dis.toml \
    RUST_LOG=clara_frontdesk=debug \
    cargo run -p clara-frontdesk-poc
```

Requires lildaemon and clara-api already running and reachable at the
URLs in the config file (`fiery_pit_url`).

## Configuration reference

```toml
[company]
name       = "Clara"
agent_name = "Clara"
greeting   = "..."   # sent as the first WS message on connect

[server]
port = 8088

[paths]
fiery_pit_url     = "http://lildaemon:6666"  # hosts both FieryPit and /assistant/*
static_path       = "/app/static"
```

No login credentials belong in config anymore — each browser tab logs in
with its own username via the demo login screen (`POST /login` →
lildaemon's `POST /auth/login-demo`, no password).

Config file location is read from the `FRONTDESK_CONFIG` environment
variable; defaults to `clara-frontdesk-poc/config/city_of_dis.toml`
(relative to workspace root).

## Known constraints

- **Per-visitor identity, per-tab.** Each browser tab logs in independently
  (username only, no password) via `POST /login`, which proxies to
  lildaemon's `POST /auth/login-demo` and upserts a "service"-role account
  by username. The resulting JWT and the tab's assistant `session_id` are
  both stored in `sessionStorage` — a new tab always re-prompts; the same
  tab's reload reuses its existing login. Two tabs logging in with the
  same username share that one lildaemon identity, but get independent
  assistant sessions.
- **`reqwest` client needs a generous timeout.** A `knowledge_query` turn
  can legitimately take minutes (research + answer legs chained
  sequentially) — `main.rs` sets an explicit 420s timeout; don't remove it.
- **Reap scheduler runs automatically.** `goat/models/ReapScheduler.py`'s
  `PeriodicReaper` reaps stale assistant documents and expired REPL
  sessions on a timer (`ASSISTANT_REAP_*`/`REPL_SESSION_REAP_*` env vars);
  `POST /assistant/documents/reap` still exists for a manual trigger.

## Related reading

- `lildaemon/docs/assistant_demo.md` — the full design, verified-live
  results, and every bug found building this, including the same-day
  "Document tagging redesign" that replaced the per-topic-workspace +
  dual-write design this README used to describe.
- `clara-cerebellum/docs/edgequake_workspace_capabilities.md` — why
  Edgequake has neither real cross-workspace set operations nor a
  server-side TTL, which is what motivated tagging/reap in the first place.
- `clara-cerebellum/docs/rituals_101.md` — general Ritual mechanics.
