# Hermes Ego seat: limbic runbook (slice 4c)

Runs a **second FieryPit** (`lildaemon-ego`) that hosts Hermes Ego seats, next to the live stack, without touching it. Plan and
design: `hermes_agent_evaluator_plan_v3.md`. Every step is additive and reversible; the live `lildaemon`, `lildaemon:latest` and
`docker-compose.yml` are never modified.

```
host:  seat_launcher daemon (system Python) ── unix socket ──┐
                                                             ▼ bind-mounted dir
docker_clara-net ─── lildaemon-ego (FieryPit + ego_gate :8765) ─── clara-seats bridge ─── Hermes seat containers
   (Dis, Kafka, ...)        │  registers with Dis by URL                (one per Ego seat, created on demand)
                            └── ritual_spaces/  (documents shared by a ritual's participants)
```

Seats live on the dedicated `clara-seats` bridge, so a seat cannot reach the main stack's services **by name** on the docker
networks (see "What the isolation does and does not give you"). No code assumes a docker network name: FieryPits can be remote and are identified by URL. The advertised gate URL plus each host's launcher `network`
setting is all that ties a seat to its FieryPit.

> **Phase 2 (2026-09-21, branch `hermes-ego-phase2`, not merged):** the gate can be real (`EGO_GATE_MODE=superego`, below). Build `lildaemon:ego` from that branch to use it.
>
> **Status (2026-09-21):** the Hermes Ego work is merged onto lildaemon `master` (local; not pushed) and the live `lildaemon:latest` was rebuilt from it, without the baked
> `.env`. The commands below still build `lildaemon:ego` from whatever is checked out in `lildaemon`, which is now `master`.

## Prerequisites

- The main stack is up (Dis = `clara-api`, Kafka) and `clara-cerebellum/docker/.env` holds its secrets.
- Ollama on the host with `qwen-clara-hermes:latest` (a 64K-context tag; Hermes refuses smaller). ComfyUI stopped, GPU free.
- The `lildaemon` repo on the branch you want to run (the image is built from the working tree).

## One-time setup

    mkdir -p /tmp/sl-ego /tmp/ego-launcher-state /tmp/ego-fierypit/{data,output,workspace,ritual_spaces,outbox}

Create these yourself first so they are owned by you; docker would otherwise create them as root and the launcher could not use them.
For a lasting deployment choose persistent paths and set `SEAT_LAUNCHER_DIR` and `EGO_STATE_DIR` accordingly.

Launcher config, e.g. `/tmp/sl-ego/launcher.toml`:

    socket_path = "/tmp/sl-ego/l.sock"            # keep under 107 bytes
    state_dir   = "/tmp/ego-launcher-state"
    image       = "nousresearch/hermes-agent:latest"
    model_name     = "qwen-clara-hermes:latest"
    model_base_url = "http://host.docker.internal:11434/v1"
    extra_hosts    = ["host.docker.internal:host-gateway"]
    network        = "clara-seats"                # created by the compose file below
    allowed_gate_url_prefixes = ["http://lildaemon-ego:8765/"]
    max_seats = 3
    startup_timeout_seconds = 180
    # hardening: the secure default applies (no-new-privileges, cap-drop ALL + six capabilities)

## Bring up

    # 1. the launcher (system Python, no venv)
    cd /path/to/lildaemon && python3 -m seat_launcher --config /tmp/sl-ego/launcher.toml &

    # 2. the ego FieryPit (builds lildaemon:ego from the working tree; does NOT retag lildaemon:latest)
    cd /path/to/clara-cerebellum/docker
    docker compose -f docker-compose.ego.yml up -d --build

Check: `docker ps` shows `ego-lildaemon-ego-1` healthy; `curl -s http://127.0.0.1:6667/health`; Dis lists it (`GET /fierypits` with the
peer token); the container log shows `ego_gate: listening on http://0.0.0.0:8765/mcp`.

## Enable the real gate (Phase 2)

By default the gate is `deny_all`. To use the Prolog Superego, bring the FieryPit up with the gate mode set (it needs the main stack's Dis, and Ollama for the reviewer):

    EGO_GATE_MODE=superego docker compose -f docker-compose.ego.yml up -d --build

Then the container log shows `ego_gate: gate mode superego`. Other knobs, all optional: `EGO_GATE_SUPEREGO_NODE` (default `superego`), `EGO_GATE_SUPEREGO_MODEL` (default `qwen-clara-hermes:latest`, the seat's own model, so
Ollama does not swap models), `EGO_GATE_DIS_POLL_SECONDS`. Approved `export_document` / `record_note` actions write only under `$EGO_STATE_DIR/outbox/<ritual_id>/`. Every decision is appended to
`$EGO_STATE_DIR/ritual_spaces/<ritual_id>/actions.jsonl`, which the model has no tool to read or write.

## Escalations and the frontdesk Ego (Phase 3)

The compose file also sets `EGO_FIERYPIT_URL` (default `http://lildaemon-ego:6666`), `EGO_REMOTE_KAFKA_BOOTSTRAP` (default `kafka:9092`), and `EGO_SEAT_IDLE_SECONDS` (default 900). An assistant runtime with the `ego` ruleset
(here this FieryPit itself; in a real deployment the MAIN lildaemon, pointed at this FieryPit by URL and needing the same `DIS_DOMAIN_PEER_TOKEN`) creates one ritual per session with `ego-<id>`/`superego-<id>` nodes hosted here.
**Main-hosted assistant.** The main stack's `docker-compose.yml` now passes `EGO_FIERYPIT_URL` and `EGO_REMOTE_KAFKA_BOOTSTRAP` (empty by default = the ego ruleset says the Ego is not configured). Set them in `docker/.env`
(e.g. `EGO_FIERYPIT_URL=http://lildaemon-ego:6666`, `EGO_REMOTE_KAFKA_BOOTSTRAP=kafka:9092` for the second FieryPit on the same host; for a real remote host use its LAN URL and Kafka's external listener, e.g. `limbic:9094`), then
`docker compose up -d --no-deps lildaemon`. Ledger, outbox and seats stay on the Ego FieryPit. Remember: from inside a container on this host, `limbic` resolves to 127.0.1.1 and the host's published ports are firewalled off from the bridges.

To try it: run a frontdesk pointed at the FieryPit (`fiery_pit_url` in its TOML, e.g. a local `cargo run -p clara-frontdesk-poc` on another port with `FRONTDESK_CONFIG`), log in, choose "Ego (acts through a gate)", and ask it to
publish a document (`publish_document {"name": ...}`) or do anything irreversible. It appears in the bell with Approve/Deny. The ledger and outbox are under `$EGO_STATE_DIR` as before.
`EGO_ESCALATION_TTL_SECONDS` (default 86400) sets how long an unanswered escalation stays open before it counts as denied. Seats are capped by the launcher's `max_seats`; a turn that cannot get a seat tells the user nothing was done.

## A remote host (pineal), verified 2026-09-21

The Ego FieryPit can run on another machine, reached by URL. Recipe used on pineal (no sudo except step 3; everything under `$HOME`, since snap docker only bind-mounts there):

1. **Model.** `ollama create qwen-clara-hermes:latest` from `FROM qwen-clara:latest` with **`RENDERER qwen3.8` and `PARSER qwen3.5`**, `num_ctx 65536`, `top_p 0.95`. The RENDERER/PARSER lines are not optional: pineal's stock `qwen-clara` used `qwen3-coder`, under which `think: false`
   is ignored and the model writes ~1,300 tokens of reasoning per answer (13 to 47 s). Check with a `think:false` chat: it should answer in under a second.
2. **Images.** `docker save lildaemon:ego | ssh <host> docker load` (built without `.env`), and `docker pull nousresearch/hermes-agent@sha256:6fd6f57...` (v0.21.3), tagged `hermes-agent:v0.21.3`.
3. **Network (needs sudo once).** `docker network create --subnet 172.30.0.0/24 -o com.docker.network.bridge.name=br-clara-seats clara-seats`, then `sudo ufw allow in on br-clara-seats to any port 11434 proto tcp`. Without the rule a custom bridge cannot reach the host's Ollama
   (only `docker0` is allowed by default), and both seats and the reviewer need it. Verify with a probe container on the bridge.
4. **Config.** `~/ego/.env` (0600: the domain's `DIS_DOMAIN_PEER_TOKEN`, fresh `LILDAEMON_JWT_SECRET` and `LILDAEMON_SERVICE_SECRET`), `~/ego/launcher.toml`, and `docker-compose.ego-remote.yml`. **Hardening override on this host:** its docker refuses the Hermes (s6-overlay) entrypoint under
   `--security-opt no-new-privileges` ("exec ...entrypoint-dispatch.sh: operation not permitted"; limbic's docker does not), so `launcher.toml` sets `hardening` to the default minus that one flag. The capability bounding set stays exactly the six granted caps (verified `CapBnd 0xeb`).
5. **Launcher as a user service** (`loginctl enable-linger $USER` worked without sudo), unit at `~/.config/systemd/user/seat-launcher.service` (see `seat_launcher/seat-launcher.service.example`), then `docker compose -f docker-compose.ego-remote.yml up -d` (restart policy `unless-stopped`).
6. **Main side.** In limbic's `docker/.env`: `EGO_FIERYPIT_URL=http://pineal:6667`, `EGO_REMOTE_KAFKA_BOOTSTRAP=limbic:9094` (Kafka's external listener as the Ego host resolves it; the remote compose maps `limbic` with `extra_hosts`), and **`EGO_SUPEREGO_LOCAL=1`**, then recreate only `lildaemon`.

**Why the reviewer runs on limbic, not beside the Ego.** A reviewer should not share a host with the agent it reviews, and pineal's 9B was a poor reviewer: thinking off it judged `send_email` reversible and contained, thinking on it took 14 to 55 s and approved a hostile-justified email.
`EGO_SUPEREGO_LOCAL=1` hosts the reviewer on the main FieryPit (27B, 1 to 5 s). The gate consults it by node id through Dis, so its location does not matter to the gate. `EGO_GATE_SUPEREGO_THINK=true` exists for a small reviewer that must reason (default off).

## Run a ritual with an Ego seat

    cd /path/to/lildaemon
    python examples_ritual_hermes_ego.py --strict \
        --ritual-space-root /tmp/ego-fierypit/ritual_spaces --outbox-root /tmp/ego-fierypit/outbox

(`DIS_DOMAIN_PEER_TOKEN` in the environment; the script reads it from there.) It creates a ritual, joins `hermes_ego` as node `ego` and a reviewer as node `superego`
(`--no-superego` skips it),
sends the Ego a three-part task through Prolog `caws_offer`/`caws_await` (export a document; email someone; a request with a hostile justification), then prints **evidence, not the model's account**: the shared documents
and their authors from disk, the gate's action ledger and the outbox, structural checks on them (no free-form action executed, the evaluator's report matches the ledger, the outbox holds exactly what the ledger
recorded), and the seat container's real isolation from `docker inspect`. It then leaves the ritual, which closes the
evaluator and deletes the seat, and confirms the container is gone. `--strict` exits non-zero if any check fails.

The first offering starts the seat (15 to 30 seconds), so allow for it; `eval_timeout_s` defaults to 240.

## What the isolation does and does not give you (measured live, from inside a running seat)

| Reachable from a seat | Result |
|---|---|
| the ego gate (`lildaemon-ego:8765`) and Ollama on the host | reachable, as intended |
| Dis, Kafka, Cobbler, Edgequake **by their compose names** | do not resolve: the seat is not on `clara-net` |
| host-published ports via the docker gateway (Dis `:8080`, Kafka external `:9094`, Cobbler `:5001`, ssh `:22`) | **reachable** |
| LAN names (`pineal`) | **resolvable** |

So the `clara-seats` bridge isolates seats from the compose networks but is **not egress filtering**. Hermes has no native network
tools with all toolsets off, so the *model* cannot open those connections; the residual risk is a compromised Hermes process. To close it,
add host firewall rules on the `clara-seats` bridge (for example in the `DOCKER-USER` chain) that allow only DNS, the gate, and Ollama.
Not done here; recorded as a follow-up.

The seat containers themselves: 8 GiB memory, 2 CPUs, 512 pids, not privileged, one bind mount (their own home), no `docker.sock`,
`no-new-privileges`, all capabilities dropped but six (`docker inspect` output printed by the example script).

## Tear down and roll back

    cd /path/to/clara-cerebellum/docker && docker compose -f docker-compose.ego.yml down
    docker network rm clara-seats            # only if `down` left it (it removes networks it created)
    kill %1                                   # the launcher: a clean stop removes every seat
    docker run --rm -v /tmp:/t alpine rm -rf /t/ego-fierypit /t/ego-launcher-state /t/sl-ego   # root-owned files
    docker rmi lildaemon:ego                  # optional

The live stack is unaffected throughout: compare `docker ps` start times and `docker image inspect lildaemon:latest` before and after.

## Troubleshooting (all found the hard way)

- **`AF_UNIX path too long`**: unix socket paths max 107 bytes. Use a short `socket_path`.
- **A seat is `failed`, "ego_gate MCP connection failed"**: the seat cannot reach the advertised gate URL. It must be reachable from the
  seat's own network (`clara-seats` here). A listener bound on the *host* is unreachable from containers (host firewall); the gate must
  run inside the FieryPit container.
- **Hermes never becomes healthy, log says "No messaging platforms enabled"**: Hermes silently skips its API when the API key is weak.
  The launcher generates a strong one; do not hand-edit a seat's `.env`.
- **The token must be in the seat's `.env`**, not the container environment: `${VAR}` in MCP headers resolves only from there, and an
  unresolved variable stays literal without error. The launcher does this for you.
- **Seat starts slowly**: 15 to 30 seconds is normal; about 80 seconds if several start at once.
- **A read-only root filesystem breaks Hermes** (its init needs a writable root); do not add `--read-only`.
- **Stopping processes**: `pgrep -f` and `pkill -f` match their own command line. Find the daemon with `ps -eo pid,args | grep seat_launcher`.
- **(Fixed for images built after 2026-09-21.) The FieryPit image used to bake in a copy of the repo `.env`** (API keys, tokens, JWT secrets), and `goat/__init__.py` loads it at import.
  `docker-compose.ego.yml` therefore sets `EDGEQUAKE_BASE_URL`, `EDGEQUAKE_API_KEY`, `GROQ_API_KEY`, `GITHUB_TOKEN` and
  `GOOGLE_CUSTOM_SEARCH_API_KEY` to explicit empty values (an empty variable wins over the baked file). Merely omitting them is not enough:
  found live, the assistant-documents reaper ran against `EDGEQUAKE_BASE_URL=http://localhost:8082` from the baked file. The main image has
  the same baked `.env`; `Dockerfile.lildaemon.dockerignore` does not exclude it.
- **Stale root-owned files** from container writes need `docker run --rm -v ... alpine rm -rf` to remove.
