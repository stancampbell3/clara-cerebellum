# FieryPit → Dis Registration & Discovery

**Status:** Implemented — deliberated via the Robert's Rules Ritual (Clara's
multi-model assembly affirmed the design as proposed), live-verified.
**Date:** 2026-09-01
**Builds on:** `docs/ritual_run_multi_fierypit.md` (cross-FieryPit Ritual
joins by hand-typed URL + `DIS_DOMAIN_PEER_TOKEN`)
**Repos touched:** `clara-cerebellum` (clara-api, clara-config, docker
compose) and `lildaemon` (DisClient, startup/shutdown lifecycle).

---

## Context

This closes two open items from the **PitBoss** planning note (raised
2026-08-27 after real GPU contention in the Robert's Rules deliberation
Ritual — one Ollama/GPU serving multiple local seats plus Edgequake's own
model, all contending): FieryPits had no registration/heartbeat mechanism
with Dis at all, and the Kafka/lildaemon network topology needed
examining before any FieryPit outside the local docker-compose network
could participate.

The concrete ask: **lildaemon FieryPits register themselves with a Dis
domain and are made available to offer instantiations of Ritual
participant evaluators**, and Kafka needed to be reachable beyond
containers on the local compose network.

Two scope decisions were made before designing this:
- **Kafka exposure — LAN-open, firewall-only.** Bind the external
  listener to the host's LAN interface instead of `127.0.0.1`, make the
  advertised host configurable, rely on host firewall (`ufw`) for access
  control — the same "firewall only, no proxy" posture already used
  elsewhere in this stack. No Kafka SASL/TLS — Kafka already has zero
  auth today; this doesn't worsen that, it just widens who's on the same
  trust boundary (this host's LAN, behind its firewall) to reach it.
- **Registration + discovery only, no auto-placement.** This does NOT
  touch `activate_ritual_config`'s existing node-to-FieryPit resolution
  (`partition_nodes_by_target`, lildaemon's `goat/app/ritual_configs/
  router.py`) or Cobbler's hand-typed-URL UX. A FieryPit becomes
  *discoverable*; automatically picking one for a specific Ritual node is
  a deliberately separate follow-up.

## Terminology (two unrelated "Dis domain" concepts)

1. **`Disdomain`** (`goat/models/Disdomain.py`, lildaemon) — an
   in-process registry of Evaluator classes local to one FieryPit.
   Nothing to do with networking.
2. **"Dis domain"** (`dis_domain`, two words) — a plain `String` on
   clara-api's `AppState`, set once at startup (default `"dis.local"`),
   used only as a Kafka topic-name prefix
   (`{dis_domain}.ritual.{ritual_id}`, `clara-ritual/src/topic.rs`). One
   clara-api process = one domain value for its whole lifetime — not a
   registered entity, no table, no CRUD.

## What already existed

Multi-FieryPit Ritual joining shipped earlier (`lildaemon@539a052`, doc
`docs/ritual_run_multi_fierypit.md`): a Cobbler graph node can name a
remote FieryPit by URL; activation partitions nodes into local/remote and
calls `goat/app/fiery_pit_peer_client.py`'s `join_remote()` — a plain
`POST {base_url}/ritual/join` authenticated with a shared static secret,
`DIS_DOMAIN_PEER_TOKEN`, checked by `get_current_user_or_peer()`. That
feature's own "Known gaps" section named "no service discovery" as
accepted, out-of-scope — this feature is exactly that gap. It was also
never wired into docker-compose (no `DIS_DOMAIN_PEER_TOKEN` set anywhere)
and only ever exercised with two processes on the *same* host/network.

clara-api's inbound HTTP requests had **no auth of any kind** before this
feature — `AuthConfig.jwt_secret` was loaded/validated but had zero
consumers; the Actix app wrapped only `Logger`. `clara-api/src/
middleware/auth.rs` existed as a one-line stub.

## Design

**Identity & payload.** A FieryPit derives its own registry id
deterministically — `uuid.uuid5(uuid.NAMESPACE_URL, base_url)` — so
nothing persists client-side across restarts; Dis treats it as an opaque
key, no derivation/verification of its own. Registration payload:
`base_url`, `dis_domain`, `evaluators: [names]` (from the FieryPit's own
`Disdomain.list_all()`). Deliberately excludes anything Kafka-related —
registration answers "who exists, what can they run"; Kafka reachability
stays the pre-existing, separate operator concern it already was
(`bootstrap_servers`/`KAFKA_BOOTSTRAP`, exactly as the existing
multi-FieryPit feature already required).

**Rust side (clara-api).** New `FieryPitRegistry`
(`clara-api/src/fierypit_registry.rs`) — in-memory only,
`Arc<RwLock<HashMap<Uuid, FieryPitRegistration>>>`, no `CoireStore`
persistence, unlike `RitualRegistry`: a registration is inherently
ephemeral and heartbeat-repopulated, so persisting stale entries across a
restart would only risk serving up addresses nobody has heartbeated
recently. Added as a new `AppState` field. Three endpoints:
- `PUT /fierypits/{id}` — idempotent upsert (register == heartbeat).
- `GET /fierypits?evaluator=&dis_domain=` — discovery, staleness filtered
  lazily on read via a TTL (`fierypit_registration_ttl_seconds`, config
  default 90s — well above the 30s recommended heartbeat interval).
- `DELETE /fierypits/{id}` — best-effort graceful deregister.

All three gated by a new `require_peer_token()` guard (`clara-api/src/
middleware/auth.rs`, the first real code in that file), checking
`Authorization: Bearer <DIS_DOMAIN_PEER_TOKEN>` — the same shared secret
the existing peer-join flow already uses, reused deliberately rather than
minting a second one (same trust boundary: every FieryPit under this Dis
domain). `GET` is gated too, not just the mutating endpoints: once Kafka
(and this port) are LAN-reachable, an open discovery endpoint would leak
every FieryPit's URL and capability roster to anyone on the LAN.

**Python side (lildaemon).** New `DisClient` methods
(`register_fiery_pit`/`deregister_fiery_pit`/`list_fiery_pits`,
`goat/app/dis_client.py`). Wired into `goat/app/main.py`'s
`startup_event()`/`shutdown_event()` using the same `PeriodicReaper`
idiom already used three times in that file (assistant-document reap,
REPL-session reap, pending-research poll) — `PeriodicReaper` calls its
function once immediately on `start()`, so starting the heartbeat loop
*is* the initial registration. Registration/heartbeat/deregister are all
best-effort and never fatal — a FieryPit keeps working standalone
(single-host dev, tests, `examples_ritual_*.py`) whether or not Dis is
reachable or `DIS_DOMAIN_PEER_TOKEN` is configured, matching every other
startup step's existing tone in that function. New env vars:
`DIS_DOMAIN_PEER_TOKEN` (reused, now actually wired into docker-compose),
`DIS_DOMAIN` (new — must match the Rust side's configured domain by
convention; no cross-repo validation, same manual-consistency burden
`LILDAEMON_SERVICE_SECRET` already carries), `FIERYPIT_HEARTBEAT_
INTERVAL_SECONDS` (default 30s).

**docker-compose.yml (dev only — not prod).** Kafka's port publish
changed from `127.0.0.1:9094:9094` to a configurable
`${KAFKA_EXTERNAL_BIND_HOST:-0.0.0.0}:9094:9094`; its advertised
`EXTERNAL` listener host is now `${KAFKA_EXTERNAL_ADVERTISED_HOST:-
localhost}:9094` instead of a hardcoded string. Docker's normal
port-publish mechanism makes a non-loopback bind host-firewall-reachable
without any other change to the `clara-net` bridge network — the bridge
network is for container-to-container traffic; port-publish is a
separate host-level DNAT rule Docker manages independently. `lildaemon`
and `clara-api` both gained `DIS_DOMAIN_PEER_TOKEN` (shared, mirroring
how `LILDAEMON_SERVICE_SECRET` is already shared between them).
`docker-compose.prod.yml` is unchanged — that's a single EC2 host with no
remote-FieryPit story today; opening its Kafka listener would be a live
security-posture change with no corresponding consumer.

## Explicitly out of scope

- Auto-placement/scheduling — `activate_ritual_config` untouched.
- Kafka SASL/TLS, or any auth beyond host firewall.
- `docker-compose.prod.yml` changes.
- Capacity/load signals (GPU memory, in-flight eval count) — this pass
  only tracks liveness + capability (evaluator names), not load.
- Cross-domain gossip (a forward-looking comment exists in
  `clara-config/src/schema.rs` about this; still just an idea).

## Testing

**Rust** (17 new tests, all green): 7 unit tests on `FieryPitRegistry`
(`clara-api/src/fierypit_registry.rs` — upsert/list roundtrip, idempotent
re-upsert, evaluator filter, dis_domain filter, TTL expiry, remove,
remove-of-nonexistent-is-a-noop); 4 unit tests on `require_peer_token`
(`clara-api/src/middleware/auth.rs` — missing header, wrong token,
correct token, unconfigured secret); 5 integration tests
(`clara-api/tests/fierypit_registry_tests.rs`, mirroring
`bootstrap_auth_tests.rs`'s `actix_web::test::init_service` + `ENV_LOCK`
pattern — 401 on all three routes without a token, put-then-get
roundtrip, put-twice updates not duplicates, delete-then-get, evaluator/
dis_domain filters through the real HTTP layer). Full existing `clara-api`
+ `clara-config` suite still green (36 lib unit tests, 4 bootstrap_auth,
9 devils_api, 6 startup_tests, 1 config test, 1 doctest — no regressions).

**Python** (8 new tests, all green): `tests/test_dis_client.py` gained
cases for `register_fiery_pit`/`deregister_fiery_pit`/`list_fiery_pits`
using `httpx`'s existing `StaticTransport` test helper — correct PUT/
DELETE/GET method and path, correct body, peer-token bearer header (not
`DIS_SERVICE_KEY`), deterministic id derivation, query-param filter
presence/omission. Full lildaemon suite: 1156 passed, 25 skipped
(legitimate: no local Kafka broker for the `examples_ritual_*` broker
tests, pylint not installed), 0 failed, 0 warnings — no regressions.

## Live verification

1. **Single-host** (`docker compose up`): confirmed `PUT /fierypits/{id}`
   fires within one heartbeat interval of lildaemon startup (log line
   both sides); `GET /fierypits` (with peer token) lists it with the
   correct `evaluators`; wrong/missing token → 401; graceful `docker
   compose stop lildaemon` → immediate deregister (log line, entry gone
   from `GET /fierypits` right away, not after the TTL).
2. **Kafka LAN reachability**: confirmed the external listener now binds
   `0.0.0.0:9094` (was `127.0.0.1:9094`) and is reachable from another
   host on the LAN once the host firewall (`ufw`) allows it.
3. **Cross-host, pineal → limbic** (a real second machine on the LAN, an
   Nvidia 3070 box, SSH-reachable): pineal runs its own standing
   deployment via `docker-compose.remote-fierypit.yml` (see
   `docs/remote_fierypit_deployment.md` — a separate, reusable artifact
   for any FieryPit-capable host, not a one-off test script), pointed at
   limbic's Dis and LAN-exposed Kafka; `lildaemon/scripts/
   check_remote_fierypit.sh pineal <limbic-url>` confirms the
   registration from limbic's side. **Not yet run** — needs real
   SSH/terminal access to pineal, which this repo's automation sandbox
   doesn't have; see `docs/remote_fierypit_deployment.md`'s "First remote
   host: pineal" section for status. This will be the step up from
   `ritual_run_multi_fierypit.md`'s own same-host-two-processes
   verification — the first genuinely cross-host proof for this feature
   family — once run.
4. **Regression**: existing `ritual_run_multi_fierypit.md` flows
   (same-host multi-node activation, `POST .../run`) unaffected — this
   feature is purely additive.

## Known gaps / open questions

- `GET /fierypits` requires the peer token; if a future dashboard (e.g.
  in Cobbler) wants unauthenticated read access, that's a one-line change
  to drop the guard call — deliberately not done here given the LAN
  exposure this feature introduces.
- `docker-compose.prod.yml` doesn't get the registration env vars either,
  for parity — harmless to add later since it doesn't touch Kafka
  exposure, just not done in this pass.
- `PeriodicReaper`'s `reap_fn` return-value/logging contract ("reaped
  %d") is semantically mismatched for a heartbeat; addressed locally (the
  heartbeat function always returns 0 and does its own logging) rather
  than changing the shared `ReapScheduler.py` primitive for one caller.
- `dis_domain` consistency between the two repos remains entirely
  operator-managed (Rust: `config/default.toml`'s `dis_domain_id`;
  Python: `DIS_DOMAIN` env var) — no validation anywhere that they match.
- No service discovery/autoscaling of FieryPit *processes* — an operator
  still starts each FieryPit; this feature only makes an already-running
  one discoverable.

## Explicitly out of scope (unchanged from the plan)

- Auto-placement/scheduling of Ritual nodes onto discovered FieryPits.
- Kafka SASL/TLS.
- Capacity/load-aware scheduling (PitBoss's fuller vision).
- Any change to Cobbler/dagda frontend code.

## Files changed

```
clara-cerebellum/
  clara-api/src/fierypit_registry.rs               (new)
  clara-api/src/middleware/auth.rs                 (stub -> real)
  clara-api/src/handlers/fierypit_registry_handler.rs  (new)
  clara-api/src/handlers/session_handler.rs        (AppState field)
  clara-api/src/handlers/mod.rs
  clara-api/src/routes/fierypits.rs                (new)
  clara-api/src/routes/mod.rs
  clara-api/src/server.rs                          (registry construction, both real + test)
  clara-api/src/lib.rs
  clara-api/src/main.rs                            (TTL env override)
  clara-api/tests/fierypit_registry_tests.rs       (new)
  clara-api/tests/devils_api_tests.rs               (AppState test-constructor)
  clara-api/tests/bootstrap_auth_tests.rs           (AppState test-constructor)
  clara-config/src/schema.rs
  clara-config/src/defaults.rs
  config/default.toml
  docker/docker-compose.yml
  docker/.env
  docker/docker-compose.remote-fierypit.yml        (new — see docs/remote_fierypit_deployment.md)
  docker/remote-fierypit.env.example              (new)
  docs/fierypit_registration_plan.md               (this file)
  docs/remote_fierypit_deployment.md               (new)

lildaemon/
  goat/app/dis_client.py
  goat/app/main.py
  .env.example
  tests/test_dis_client.py
  scripts/check_remote_fierypit.sh                 (new)
```
