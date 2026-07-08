# Ritual CRUD — Phased Implementation Plan
# last claude session resume: claude --resume 6458bb9b-1994-48c2-b15b-ff4547622da5

Companion to `rituals_101.md`. This document breaks down the joint Dis + FieryPit
+ Cobbler Ritual persistence project into self-contained delivery phases.

Each phase ends with a working, testable state. No phase leaves the system in a
broken intermediate state.

---

## Phase 1 — Ritual Config persistence in lildaemon (no Dis, no Kafka) ✅ DONE

**Goal:** Users can create, read, update, and delete Ritual configs via the
FieryPit API. Configs are durable across restarts. No activation yet.

### What shipped

**Schema** — two tables added to `lildaemon.duc` via `RitualConfigStore.__init__`:

```sql
CREATE TABLE IF NOT EXISTS ritual_configs (
    ritual_config_id  VARCHAR PRIMARY KEY,
    user_id           VARCHAR NOT NULL,
    name              VARCHAR NOT NULL,
    status            VARCHAR NOT NULL DEFAULT 'draft'
                        CHECK (status IN ('draft', 'active', 'terminated')),
    ritual_id         VARCHAR,          -- Dis UUID; NULL until activated
    evaluator         VARCHAR,
    eval_timeout_s    DOUBLE  NOT NULL DEFAULT 30.0,
    kafka_bootstrap   VARCHAR NOT NULL,
    dis_url           VARCHAR NOT NULL,
    created_at        BIGINT  NOT NULL,
    updated_at        BIGINT  NOT NULL
);

CREATE TABLE IF NOT EXISTS ritual_participants (
    participant_id    VARCHAR PRIMARY KEY,
    ritual_config_id  VARCHAR NOT NULL,
    url               VARCHAR NOT NULL,
    role              VARCHAR
);
```

**Files created:**
- `goat/app/ritual_configs/__init__.py`
- `goat/app/ritual_configs/models.py` — `RitualConfigCreate`, `RitualConfigUpdate`,
  `RitualConfigResponse`, `ParticipantCreate`, `ParticipantResponse`
- `goat/app/ritual_configs/store.py` — `RitualConfigStore` (sync DuckDB, shares
  `get_conn()` connection; includes `list_by_status()` for Phase 5)
- `goat/app/ritual_configs/router.py` — `ritual_configs_router`

**Files modified:**
- `goat/app/main.py` — `ritual_configs_router` registered

**Endpoints (all require Bearer JWT):**

| Method | Path | Description |
|---|---|---|
| `POST` | `/ritual-configs` | Create a new draft config |
| `GET` | `/ritual-configs` | List configs for authenticated user |
| `GET` | `/ritual-configs/{config_id}` | Get a single config (with participants) |
| `PUT` | `/ritual-configs/{config_id}` | Update editable fields (draft only) |
| `DELETE` | `/ritual-configs/{config_id}` | Delete a draft config (draft only) |
| `POST` | `/ritual-configs/{config_id}/participants` | Add a participant URL (draft only) |
| `DELETE` | `/ritual-configs/{config_id}/participants/{participant_id}` | Remove a participant (draft only) |

**Tests:** 17 store unit tests + 19 router integration tests (36 total, all pass).

---

## Phase 2 — Service-to-service auth (Option A: pre-shared API key) ✅ DONE

**Goal:** lildaemon can call Dis endpoints authenticated. FieryPitClient in
clara-cerebrum can call lildaemon endpoints authenticated. Unblocks Phase 3.

### What shipped

**lildaemon — `goat/app/dis_client.py` (new)**

Async httpx-based `DisClient` with:
- Constructor accepts `base_url`, optional `service_key`, and optional `transport`
  (injectable for testing — no external mock library required)
- `Authorization: Bearer <key>` attached on every request when key is set; omitted
  when not set (forward-compatible — works against a Dis that has no auth yet)
- Four async methods: `create_ritual`, `join_ritual`, `delete_ritual`,
  `get_ritual_status`
- Typed exception hierarchy:

  | Exception | HTTP trigger |
  |---|---|
  | `DisRitualNotFound` | 404 |
  | `DisRitualTerminated` | 409 |
  | `DisAuthError` | 401 / 403 |
  | `DisUnavailable` | 5xx |
  | `DisError` (base) | other 4xx |

- `get_dis_client()` FastAPI Depends factory — override via
  `app.dependency_overrides[get_dis_client]` in tests

**clara-cerebrum — `fiery-pit-client/src/lib.rs` (modified)**

- Added `service_key: Option<Arc<String>>` field to `FieryPitClient`
- Added `.with_service_key(key)` builder method for chaining
- Updated `from_env()` to read `FIERYPIT_SERVICE_KEY` env var
- Updated `get`, `post`, and `delete` helpers to call `.bearer_auth(key)` when key
  is present — no change to callers that don't need auth
- Added 4 new unit tests covering key presence, absence, and env-var reading

**clara-cerebrum — `clara-api/src/handlers/ritual_handler.rs` (modified)**

- Auto-bootstrap path now reads `FIERYPIT_SERVICE_KEY` and passes it to every
  `FieryPitClient` constructed during participant bootstrap, fixing the `401
  Unauthorized` failure documented in `rituals_101.md`

**Tests:** 16 Python tests in `tests/test_dis_client.py` (all pass). 9 Rust unit
tests + 1 doctest in `fiery-pit-client` (all pass). `clara-api` compiles clean.

### Deployment for Phase 2

**Step 1 — Generate a lildaemon service JWT for Dis**

Dis needs a long-lived JWT to authenticate its calls to lildaemon. The
`POST /auth/service-token` endpoint upserts the service account and mints the
token in one call, authenticated by `LILDAEMON_SERVICE_SECRET`:

```bash
# Ensure LILDAEMON_SERVICE_SECRET is set in lildaemon's environment, then:
curl -s -X POST http://localhost:6666/auth/service-token \
  -H "Content-Type: application/json" \
  -d '{
    "service_name": "dis",
    "service_secret": "<LILDAEMON_SERVICE_SECRET value>"
  }' | jq -r '.access_token'
```

This is idempotent — safe to re-run. The returned `access_token` is the value
to set as `FIERYPIT_SERVICE_KEY` in Dis's environment. Default TTL is 30 days
(override with `LILDAEMON_SERVICE_TOKEN_TTL_DAYS`).

**Step 2 — Set environment variables**

```bash
# lildaemon (.env or systemd environment)
DIS_BASE_URL=http://localhost:8080      # or wherever Dis is running
DIS_SERVICE_KEY=<token-if-dis-adds-auth>  # leave unset until Phase 6

# clara-cerebrum (.env or systemd environment)
FIERYPIT_SERVICE_KEY=<token-from-step-1>  # the JWT minted above
```

> **Note:** `DIS_SERVICE_KEY` is reserved for when Dis adds its own auth
> middleware (Phase 6). Dis currently has no auth, so this variable can be
> left unset. `FIERYPIT_SERVICE_KEY` is the operative credential right now —
> without it, Dis's auto-bootstrap calls to lildaemon will receive `401
> Unauthorized`.

**Step 3 — Restart both services**

```bash
# Restart Dis so it picks up FIERYPIT_SERVICE_KEY
systemctl restart dis   # or: cargo run / ./Dis

# Restart lildaemon so it picks up DIS_BASE_URL
systemctl restart lildaemon  # or: uvicorn goat.app.main:app
```

**Step 4 — Verify**

```bash
# Confirm lildaemon accepts the Dis service token
curl -s http://localhost:6666/ritual \
  -H "Authorization: Bearer <FIERYPIT_SERVICE_KEY>" | jq .

# Should return: {"ritual_ids": [], "count": 0}
```

**Token rotation**

Service JWTs are signed with `LILDAEMON_JWT_SECRET`. To rotate:
1. Mint a new token via `POST /auth/service-token`
2. Update `FIERYPIT_SERVICE_KEY` in Dis's environment
3. Restart Dis — no lildaemon restart needed

---

## Phase 3 — Activation and deactivation (lildaemon ↔ Dis integration) ✅ DONE

**Goal:** Users can activate a draft config, which provisions a Dis Ritual, joins
Kafka, and tracks the live state. Deactivation tears it down cleanly.

### What shipped

**New endpoints:**

| Method | Path | Description |
|---|---|---|
| `POST` | `/ritual-configs/{config_id}/activate` | Draft → active |
| `POST` | `/ritual-configs/{config_id}/deactivate` | Active → terminated |
| `GET` | `/ritual-configs/{config_id}/status` | Proxy Dis status for active configs |

**Activate flow** (`POST /ritual-configs/{config_id}/activate`):

```
1. Load config; assert status == 'draft'
2. DisClient.create_ritual(name, participant_urls)  → { ritual_id }
3. Store ritual_id; set status = 'active'; set updated_at
4. DisClient.join_ritual(ritual_id, participant_key=LILDAEMON_BASE_URL) → { topic, dis_domain }
5. RitualManager.join(ritual_id, topic, kafka_bootstrap, dis_domain, ...)
6. Return updated config
```

If steps 4-5 fail: `DisClient.delete_ritual(ritual_id)` cleans up Dis,
status reverts to `'draft'` with `ritual_id` reset to `NULL`.

**Deactivate flow** — resilient: if the consumer isn't in memory (e.g. after
restart) the `RitualManager.leave` call is skipped; if Dis returns 404 for the
ritual it is treated as already gone. Either way, status is set to `'terminated'`.

**`GET /ritual-configs/{config_id}/status`** — returns
`{ ritual_config_id, status, ritual_id, dis_state }`. `dis_state` is populated
only for `active` configs and only when Dis responds; if Dis is unreachable the
field is `null` and the endpoint still returns 200 with local state.

**`RitualConfigStatusResponse`** — new Pydantic model in `models.py`.

**`RitualConfigStore.set_status` sentinel** — the `ritual_id` parameter now uses
an `_UNSET` sentinel so callers can explicitly pass `None` to clear the column
to `NULL` (needed for rollback). Omitting the argument still leaves the column
unchanged. All existing tests continue to pass without modification.

**`LILDAEMON_BASE_URL` env var** — used as `participant_key` in `join_ritual` so
Dis knows where to reach back. Defaults to `http://localhost:6666`.

**Tests:** 13 new router tests (64 total, all pass). Covers:
- Happy path activate: ritual_id stored, manager.join called
- Double activate returns 409
- Rollback: join failure → Dis cleaned up, config stays draft, ritual_id NULL
- Deactivate happy path: manager.leave + Dis delete, status = terminated
- Deactivate when consumer not in memory: manager.leave skipped, still succeeds
- Deactivate when Dis 404: still marks terminated
- Status for active config: dis_state populated
- Status for draft config: dis_state null
- Status when Dis unreachable: returns local state, no error
- Auth guards on activate, status

---

## Phase 4 — Cobbler Ritual Editor page

**Goal:** Users can manage Ritual configs from the Cobbler browser UI.

### clara-dagda (Cobbler backend) work

Add proxy routes at port 5001 (same pattern as existing REPL proxy):

```
GET    /api/ritual-configs           → lildaemon GET  /ritual-configs
POST   /api/ritual-configs           → lildaemon POST /ritual-configs
GET    /api/ritual-configs/:id       → lildaemon GET  /ritual-configs/{id}
PUT    /api/ritual-configs/:id       → lildaemon PUT  /ritual-configs/{id}
DELETE /api/ritual-configs/:id       → lildaemon DELETE /ritual-configs/{id}
POST   /api/ritual-configs/:id/activate    → lildaemon POST /ritual-configs/{id}/activate
POST   /api/ritual-configs/:id/deactivate  → lildaemon POST /ritual-configs/{id}/deactivate
GET    /api/ritual-configs/:id/status      → lildaemon GET  /ritual-configs/{id}/status
POST   /api/ritual-configs/:id/participants        → lildaemon ...
DELETE /api/ritual-configs/:id/participants/:pid   → lildaemon ...
```

Cobbler forwards the user's Bearer JWT (already in session) on every proxied
request.

### Cobbler frontend work

New **Ritual Editor** page/tab (separate from the REPL page):

**Components:**

- `RitualConfigList` — table of user's configs with status badges
  (`draft` / `active` / `terminated`), activate/deactivate/delete action buttons
- `RitualConfigForm` — create/edit form: name, evaluator (dropdown from available
  evaluators), `eval_timeout_s`, `kafka_bootstrap`, `dis_url`, participant list
- `ParticipantList` — inline editable list of participant URLs within the form
- `RitualStatusBadge` — polls `GET /status` every 10s for active configs

**Page layout:**
- Left panel: list of configs
- Right panel: selected config detail / edit form
- Top bar: "New Ritual Config" button, status summary

**REPL page relationship:**

The existing REPL page remains unchanged — it is for sending Offerings manually
to individual evaluators and watching Hohi/Tabu responses live. The new Ritual
Editor page is for configuration and lifecycle management only. A future "Run"
button on the editor page could link to a REPL session pre-configured for that
Ritual.

### Deliverable

A logged-in Cobbler user can create a draft Ritual config, add participant URLs,
activate it, see the live Dis status, and deactivate it — all without touching
the CLI or curl.

---

## Phase 5 — Restart resilience

**Goal:** Active Ritual configs are automatically rejoined when lildaemon restarts.

### lildaemon work

In `goat/app/main.py` startup event (after `GoatWrangler` is initialised):

```python
async def _rejoin_active_rituals(store: RitualConfigStore, manager: RitualManager):
    active = store.list_by_status("active")
    for config in active:
        try:
            routing = await dis_client.join_ritual(config.ritual_id, self_url)
            await manager.join(
                ritual_id=config.ritual_id,
                topic=routing["topic"],
                bootstrap_servers=config.kafka_bootstrap,
                dis_domain=routing["dis_domain"],
                wrangler=goat_manager,
                evaluator_name=config.evaluator,
                eval_timeout_s=config.eval_timeout_s,
                ritual_config_id=config.ritual_config_id,
            )
        except DisRitualTerminated:
            store.set_status(config.ritual_config_id, "terminated")
        except Exception as e:
            logger.warning("Could not rejoin ritual %s on startup: %s", config.ritual_config_id, e)
```

If Dis returns 409 (already terminated) or 404, mark the config `terminated`
locally. Other errors are logged and skipped — the config stays `active` and can
be manually re-activated after the underlying problem is resolved.

**`RitualConfigStore.list_by_status(status)`** — new method needed in Phase 1
store (add it there; leave it unused until Phase 5).

### Tests

- Startup re-join: mock DisClient, pre-populate DB with an `active` config, verify
  `RitualManager.join()` is called on startup
- Dis-terminated detection: mock DisClient to raise `DisRitualTerminated`, verify
  config transitions to `terminated`

### Deliverable

Restart lildaemon with an active Ritual config in the DB → consumer is running
within seconds of boot, no manual `activate` call needed.

---

## Phase 6 — JWT service-to-service auth (Option B)

**Goal:** Replace the pre-shared API key with short-lived RS256 JWTs. No API
surface changes; purely a credential format upgrade.

### lildaemon work

- Generate an RS256 key pair on first boot; persist private key to
  `LILDAEMON_SERVICE_KEY_PATH` (default `./lildaemon_service.key`).
- `DisClient` mints a JWT per request: `iss=fierypit.local`, `aud=dis`,
  `exp=now+5min`, signed with the private key.
- Expose `GET /service/public-key` (no auth) returning the PEM public key so Dis
  can register it.

### clara-cerebrum work

- Dis's auth middleware: accept Bearer JWTs signed by a registered FieryPit public
  key (`FIERYPIT_PUBLIC_KEY` env var or fetched from `/service/public-key` on
  startup).
- `FieryPitClient`: mint JWT with Dis's key pair similarly, or continue using
  service API key depending on deployment needs. (FieryPitClient → lildaemon
  direction is less critical for Phase 6.)

### Migration path

1. Deploy Phase 6 lildaemon → `GET /service/public-key` is now available
2. Register public key in Dis config (`FIERYPIT_PUBLIC_KEY`)
3. Remove `DIS_SERVICE_KEY` from lildaemon env; remove `FIERYPIT_SERVICE_KEY`
   from Dis env
4. Both sides now use JWTs; no downtime required if done in order

### Deliverable

All DisClient calls use short-lived JWTs. Rotating credentials requires no shared
secret update — just key regeneration on one side.

---

## Cross-cutting concerns

### `DisClient` dependency injection

Inject `DisClient` via FastAPI `Depends` rather than importing a singleton. This
makes Phase 3 router tests trivially mockable without patching at the module level.

```python
def get_dis_client() -> DisClient:
    return DisClient(
        base_url=settings.dis_base_url,
        service_key=settings.dis_service_key,
    )
```

Override in tests:

```python
app.dependency_overrides[get_dis_client] = lambda: MockDisClient(...)
```

### Error taxonomy

`DisClient` should raise typed exceptions so callers don't inspect HTTP codes:

- `DisRitualNotFound` — 404
- `DisRitualTerminated` — 409 from a terminated ritual
- `DisAuthError` — 401/403 (misconfigured key)
- `DisUnavailable` — connection refused / 5xx

### Sequencing and dependencies

```
Phase 1 (CRUD)
    │
    └─► Phase 2 (auth) ──► Phase 3 (activation) ──► Phase 5 (restart)
                                │
                                └─► Phase 4 (Cobbler UI)

Phase 6 (JWT) is independent; can be done any time after Phase 2.
```

Phase 4 can be developed in parallel with Phase 3 once the Phase 1 API shape
is stable (mock responses are sufficient to build the UI against).
