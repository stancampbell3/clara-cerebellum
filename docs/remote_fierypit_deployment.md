# Deploying a remote FieryPit

**Status:** Implemented and live-verified end to end, including a real
cross-host Ritual performance against pineal (2026-09-02) — see
`lildaemon/docs/ritual_remote_fierypit_example.md`'s "Live verification
status" for the actual run.
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

**On the Dis host first**, if not already done: `docker/.env` needs
`KAFKA_EXTERNAL_ADVERTISED_HOST` set to this host's real LAN name (not
left at the default `localhost`) — see "Real bugs found" below for why
this one is easy to skip and hard to diagnose. Recreate the `kafka`
container after setting it (`docker compose up -d --force-recreate
kafka`).

**On the new host**, from a checkout whose directory layout matches this
one (`Development/{lildaemon,clara-cerebellum}/`):

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

The `lildaemon` service runs with `network_mode: host` (see the compose
file's own comment for why — a real Docker/ufw interaction forced this,
not a stylistic choice) — it binds directly to this host's port 6666,
LAN-open, firewall-only. **Three separate firewall rules are needed
across both hosts** before a Ritual can actually be performed across
them (registration alone only needs the first):

```bash
# On the NEW host: let the Dis host reach this FieryPit's own API.
sudo ufw allow from <dis-host-LAN-IP> to any port 6666 proto tcp

# On the Dis host: let the new FieryPit reach Dis's HTTP API and Kafka.
sudo ufw allow from <new-host-LAN-IP> to any port 8080 proto tcp
sudo ufw allow from <new-host-LAN-IP> to any port 9094 proto tcp
```

(In practice, against pineal, ports 8080 and 9094 on limbic turned out
to already be reachable without adding those two rules explicitly —
limbic's existing `ufw` configuration was already permissive enough for
this LAN. Don't assume that generalizes; add them explicitly on a new
Dis host unless you've confirmed otherwise.)

## Confirming it worked

From the Dis host (or anywhere that can reach it):

```bash
lildaemon/scripts/check_remote_fierypit.sh <new-host-hostname> http://<dis-host>:8080
```

Exits 0 and prints the registration (including its advertised evaluator
list) if found; exits 1 with the current full registry printed otherwise.
Does not start or manage anything — purely a discovery check against
whatever's already registered. Passing this confirms registration/
discovery works; it does NOT confirm the new FieryPit can actually serve
a Ritual seat (Kafka reachability and the new host's own inbound firewall
rule are separate, unverified by this check — see `lildaemon/examples_
ritual_remote_fierypit.py` for that fuller proof).

## Real bugs found deploying this (all fixed, against pineal, 2026-09-02)

1. **Kafka's advertised listener defaulted to `localhost`.**
   `fierypit_registration_plan.md`'s `KAFKA_EXTERNAL_ADVERTISED_HOST`
   env var existed in `docker-compose.yml` but was never actually set in
   `docker/.env` — so it silently used its `localhost` default. Pineal's
   rdkafka client connected to the real bootstrap address (`limbic:9094`)
   fine, received Kafka's metadata response telling it "the broker is
   actually at `localhost:9094`", and then failed reconnecting to *its
   own* loopback (`rdkafka#consumer: ... 127.0.0.1:9094/1 ... Connection
   refused`) — every `caws_offer`/`caws_await` to that seat hung until
   patience ran out. Fixed by setting `KAFKA_EXTERNAL_ADVERTISED_HOST` to
   the Dis host's real LAN name and recreating the `kafka` container.
2. **Docker's `DOCKER-USER` iptables chain silently blocked bridge→host
   traffic, ahead of ufw.** The remote FieryPit container (bridge-
   networked at the time) couldn't reach pineal's own Ollama via
   `host.docker.internal` (the docker0 bridge gateway) — confirmed via
   direct TCP connect from inside the container: a timeout, not
   "connection refused" (a silent drop, not a rejection). Isolated with
   `curl` from pineal's own host shell: the HOST reached its own bridge
   gateway IP on Ollama's port fine; only container-originated traffic
   through the bridge was blocked — even after adding an explicit `ufw
   allow from 172.17.0.0/16 to any port 11434 proto tcp` rule and
   reloading. Root cause: Docker (confirmed 29.6.1) manages its own
   `DOCKER-USER` chain, which intercepts bridge/forward traffic before
   ufw's INPUT-chain rules are ever evaluated — the ufw rule was never
   wrong, it just wasn't the chain actually dropping the packets. Fixed
   by switching the compose service to `network_mode: host` entirely,
   sidestepping the bridge (and this whole class of interaction) rather
   than chasing iptables-chain-ordering fixes on every future host.
3. **Switching to host networking silently changed which firewall rule
   mattered.** Before the fix above, Docker's own port-publish DNAT
   rules (`-p 0.0.0.0:6666:6666`) had been bypassing ufw's INPUT
   filtering entirely — a separate, well-known Docker/ufw interaction,
   in the opposite direction from #2 — which is why registration and
   `/ritual/join` worked against port 6666 *before* an explicit `ufw
   allow ... port 6666` rule existed on pineal. Once on `network_mode:
   host`, the app binds as a normal host socket with no Docker DNAT
   bypass, and ufw's ordinary rules apply for real — the previously-
   silent gap (no rule for 6666) became a real block. Fixed by adding
   the rule for real (`sudo ufw allow from <dis-host> to any port 6666
   proto tcp` on pineal) — now reflected as a required step above,
   rather than an accidentally-unnecessary one.
4. **Non-interactive SSH had to be set up first** (a prerequisite, not a
   bug in this feature, but a real blocker until solved) — see the
   verification worksheet's Prerequisites section for the general setup,
   and the note below on what was actually done for pineal. Includes one
   genuine gotcha: pasting a multi-line `authorized_keys` append as one
   block can mangle the `>>` redirect and produce a "Permission denied"
   that's actually bash trying to execute `authorized_keys` as a
   program, not a real permissions problem — paste as one line.

## First remote host: pineal

Pineal (a second LAN host, Nvidia RTX 5070/12GB, Ubuntu, checkout at
`~stanc/widebody/Development` mirroring this layout) is the first host
this was actually deployed and verified against, 2026-09-02.

**Non-interactive SSH**: `sudo ufw allow OpenSSH` was run on pineal, then
a dedicated key (`~/.ssh/pineal_deploy_ed25519` on limbic, no
passphrase) was generated and its public half appended to pineal's
`~/.ssh/authorized_keys`, with a matching `Host pineal` block in
limbic's `~/.ssh/config` (`BatchMode yes`, `StrictHostKeyChecking
accept-new`). `ssh pineal true` succeeds with no prompt from limbic.

**Deployment**: both checkouts pulled up to date, `remote-fierypit.env`
built from limbic's own `docker/.env` secrets, `docker compose -f
docker-compose.remote-fierypit.yml --env-file remote-fierypit.env build
&& up -d` run on pineal. Registration succeeded immediately
(`check_remote_fierypit.sh pineal http://limbic:8080` finds it,
advertising the full evaluator list).

**Full cross-host Ritual**: after the three bugs above were found and
fixed (Kafka advertised listener, Docker bridge/ufw chain → host
networking, the now-real port-6666 firewall rule), `lildaemon/
examples_ritual_remote_fierypit.py --remote` against pineal succeeded
twice in a row — see its own doc for the transcript. This is the first
genuinely cross-host Ritual performance in this system's history.

## Known gaps

- No automated health monitoring of a remote FieryPit beyond Dis's own
  registration TTL (an unreachable/crashed remote FieryPit simply stops
  appearing in `GET /fierypits` after `fierypit_registration_ttl_seconds`
  — no alerting).
- `DIS_DOMAIN_PEER_TOKEN`/`LILDAEMON_SERVICE_SECRET` are copy-pasted
  between hosts' `.env` files by hand — no secret-distribution mechanism.
- Firewall rules on both sides are manual (`ufw`), per host, per
  direction — nothing automates opening the right ports for a new host,
  and (per bug #3 above) which rules actually matter can depend on
  networking-mode details that aren't obvious until something fails.
