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

> **Status (2026-09-21):** the Hermes Ego work is merged onto lildaemon `master` (local; not pushed) and the live `lildaemon:latest` was rebuilt from it, without the baked
> `.env`. The commands below still build `lildaemon:ego` from whatever is checked out in `lildaemon`, which is now `master`.

## Prerequisites

- The main stack is up (Dis = `clara-api`, Kafka) and `clara-cerebellum/docker/.env` holds its secrets.
- Ollama on the host with `qwen-clara-hermes:latest` (a 64K-context tag; Hermes refuses smaller). ComfyUI stopped, GPU free.
- The `lildaemon` repo on the branch you want to run (the image is built from the working tree).

## One-time setup

    mkdir -p /tmp/sl-ego /tmp/ego-launcher-state /tmp/ego-fierypit/{data,output,workspace,ritual_spaces}

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

## Run a ritual with an Ego seat

    cd /path/to/lildaemon
    python examples_ritual_hermes_ego.py --strict --ritual-space-root /tmp/ego-fierypit/ritual_spaces

(`DIS_DOMAIN_PEER_TOKEN` in the environment; the script reads it from there.) It creates a ritual, joins `hermes_ego` as node `ego`,
sends the Ego a task through Prolog `caws_offer`/`caws_await`, then prints **evidence, not the model's account**: the shared documents
and their authors from disk, and the seat container's real isolation from `docker inspect`. It then leaves the ritual, which closes the
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
