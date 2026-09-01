# Deploying a remote FieryPit

**Status:** Implemented, live-verified for registration on the Dis side;
a genuinely remote host's own deployment is pending a manual run (see
"First remote host: pineal" below).
**Builds on:** `docs/fierypit_registration_plan.md` (registration/
discovery design).

## What this is

A FieryPit-capable host — one with its own Ollama/GPU worth lending to
Rituals — runs `docker-compose.remote-fierypit.yml` (`clara-cerebellum/
docker/`) as its own **standing, independent deployment**. It registers
itself with an already-running Dis instance elsewhere on the LAN entirely
on its own, at startup, via the same registration/heartbeat code path any
FieryPit uses (`goat/app/main.py`'s `startup_event`) — there is no
separate "join" step, and nothing on the Dis side drives this host's
lifecycle. This deliberately mirrors how the *local* stack already works:
the `clara` script stands up limbic's own FieryPit persistently; nothing
external manages its lifecycle per-Ritual either.

This is **not** a test harness. `docker-compose.yml`'s own `lildaemon`
service remains how a Dis host runs its own local FieryPit (bundled with
Kafka + clara-api, `127.0.0.1`-bound); this file is for a *second, third,
...* host that only runs a FieryPit, pointed at a Dis instance elsewhere.

## Deploying on a new host

From a checkout whose directory layout matches this one
(`Development/{lildaemon,clara-cerebellum}/`):

```bash
cd clara-cerebellum/docker
cp remote-fierypit.env.example remote-fierypit.env
# Fill in: LILDAEMON_BASE_URL (this host's own address), DIS_BASE_URL
# (the Dis host's address), and the three shared secrets copied verbatim
# from the Dis host's own docker/.env (LILDAEMON_JWT_SECRET,
# LILDAEMON_SERVICE_SECRET, DIS_DOMAIN_PEER_TOKEN).
docker compose -f docker-compose.remote-fierypit.yml \
  --env-file remote-fierypit.env up -d
```

It publishes its own HTTP API on `0.0.0.0:6666` (configurable via
`LILDAEMON_PORT`) — LAN-open, firewall-only, the same posture already
chosen for Kafka's external listener. On the Dis host's firewall, the
reverse direction needs opening too: this new FieryPit's registration
calls are outbound-only (it calls Dis, not the other way around for
registration), but Kafka (for actually joining a Ritual) and Dis's own
HTTP port need to accept connections FROM the new host:

```bash
# On the Dis host (e.g. limbic), allow the new FieryPit's LAN address in:
sudo ufw allow from <new-host-LAN-IP> to any port 8080 proto tcp   # Dis HTTP
sudo ufw allow from <new-host-LAN-IP> to any port 9094 proto tcp   # Kafka external listener
```

## Confirming it worked

From the Dis host (or anywhere that can reach it):

```bash
lildaemon/scripts/check_remote_fierypit.sh <new-host-hostname> http://<dis-host>:8080
```

Exits 0 and prints the registration (including its advertised evaluator
list) if found; exits 1 with the current full registry printed otherwise.
Does not start or manage anything — purely a discovery check against
whatever's already registered.

## First remote host: pineal

Pineal (a second LAN host, Nvidia 3070, Ubuntu, checkout at
`~stanc/widebody/Development` mirroring this layout) is the intended
first host to run this. SSH access was opened (`sudo ufw allow OpenSSH`)
in preparation, but the actual `docker compose ... up -d` on pineal
itself, and the corresponding `check_remote_fierypit.sh pineal ...`
confirmation from limbic, have not yet been run — this needs to happen
from a session with real SSH/terminal access to pineal (this repo's
automation sandbox has none). Once run, note the outcome here.

## Known gaps

- No automated health monitoring of a remote FieryPit beyond Dis's own
  registration TTL (an unreachable/crashed remote FieryPit simply stops
  appearing in `GET /fierypits` after `fierypit_registration_ttl_seconds`
  — no alerting).
- `DIS_DOMAIN_PEER_TOKEN`/`LILDAEMON_SERVICE_SECRET` are copy-pasted
  between hosts' `.env` files by hand — no secret-distribution mechanism.
- Firewall rules on both sides are manual (`ufw`), per host, per
  direction — nothing automates opening the right ports for a new host.
