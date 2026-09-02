# FieryPit Registration & Remote Deployment — Verification Worksheet

Practical, step-by-step checklist for anyone on the team to confirm the
FieryPit registration/discovery feature and the standalone remote-FieryPit
deployment actually work, end to end. Background/design:
`docs/fierypit_registration_plan.md` and `docs/remote_fierypit_deployment.md`
— this worksheet doesn't re-explain the design, just verifies it.

Run Part 1 any time (regression check, no remote host needed). Run
Parts 2–4 once per new remote host you bring online (pineal is the first).

---

## Prerequisites

- [ ] `docker/.env` on the Dis host (e.g. limbic) has `DIS_DOMAIN_PEER_TOKEN`
      set (not blank/commented out).
- [ ] You know the Dis host's LAN address (e.g. `limbic` / `10.0.0.192`).
- [ ] For Parts 2–4: SSH access to the remote host (e.g. `ssh pineal` works
      non-interactively), and its checkout is up to date (`git pull` in
      both `lildaemon/` and `clara-cerebellum/`).

---

## Part 1 — Local (Dis host) sanity check

Run from the Dis host, in `clara-cerebellum/docker/`.

1. **Stack is up and healthy.**
   ```bash
   docker compose ps
   ```
   - [ ] `kafka`, `clara-api`, `lildaemon` all show `healthy`.

2. **Local FieryPit registered itself within one heartbeat interval.**
   ```bash
   docker logs docker-lildaemon-1 --since 1m | grep fierypit_registration
   ```
   - [ ] See `heartbeat ok (N evaluators)` at least once, no `heartbeat to
         Dis failed` warnings.

3. **Discovery works with the right token.**
   ```bash
   source .env
   curl -s -H "Authorization: Bearer $DIS_DOMAIN_PEER_TOKEN" \
     http://localhost:8080/fierypits | python3 -m json.tool
   ```
   - [ ] Returns a `fierypits` array with one entry, `base_url` like
         `http://lildaemon:6666`, a non-empty `evaluators` list.

4. **Auth actually gates it.**
   ```bash
   curl -s -o /dev/null -w "%{http_code}\n" http://localhost:8080/fierypits
   curl -s -o /dev/null -w "%{http_code}\n" \
     -H "Authorization: Bearer wrong-token" http://localhost:8080/fierypits
   ```
   - [ ] Both print `401`.

5. **Graceful shutdown deregisters immediately (not after the TTL).**
   ```bash
   docker compose stop lildaemon
   curl -s -H "Authorization: Bearer $DIS_DOMAIN_PEER_TOKEN" \
     http://localhost:8080/fierypits | python3 -m json.tool
   # expect: {"fierypits": []}
   docker compose up -d lildaemon
   ```
   - [ ] Registry is empty right after `stop`, before restarting.
   - [ ] After `up -d`, it reappears within one heartbeat interval.

6. **Kafka's external listener is LAN-bound, not loopback-only.**
   ```bash
   docker port docker-kafka-1
   ```
   - [ ] Shows `9094/tcp -> 0.0.0.0:9094` (not `127.0.0.1:9094`).

**Part 1 result:** Pass / Fail (circle one) — Run by: __________ Date: __________
Notes:

---

## Part 2 — Deploy the remote host (e.g. pineal)

Run **on the remote host itself** (SSH in first).

1. **Repo up to date.**
   ```bash
   cd ~/widebody/Development/lildaemon && git pull --ff-only
   cd ~/widebody/Development/clara-cerebellum && git pull --ff-only
   ```
   - [ ] Both pull cleanly, no local changes in the way.

2. **Firewall allows the Dis host to reach this machine's FieryPit port.**
   ```bash
   sudo ufw allow from <dis-host-LAN-IP> to any port 6666 proto tcp
   sudo ufw status
   ```
   - [ ] Rule present.

3. **Configure and start.**
   ```bash
   cd ~/widebody/Development/clara-cerebellum/docker
   cp remote-fierypit.env.example remote-fierypit.env
   # Edit remote-fierypit.env:
   #   LILDAEMON_BASE_URL=http://<this-host>:6666
   #   DIS_BASE_URL=http://<dis-host>:8080
   #   LILDAEMON_JWT_SECRET / LILDAEMON_SERVICE_SECRET / DIS_DOMAIN_PEER_TOKEN
   #     copied VERBATIM from the Dis host's own docker/.env
   docker compose -f docker-compose.remote-fierypit.yml \
     --env-file remote-fierypit.env up -d
   ```
   - [ ] Build succeeds, container starts.

4. **It's healthy and can reach Dis.**
   ```bash
   docker compose -f docker-compose.remote-fierypit.yml --env-file remote-fierypit.env ps
   docker logs $(docker compose -f docker-compose.remote-fierypit.yml --env-file remote-fierypit.env ps -q lildaemon) --since 1m | grep fierypit_registration
   ```
   - [ ] Container shows `healthy`.
   - [ ] Log shows `heartbeat ok (N evaluators)`, not `heartbeat to Dis
         failed`. If it failed: check `DIS_BASE_URL` is reachable from
         this host (`curl $DIS_BASE_URL/health`) and the firewall rule in
         Part 3 below is already in place on the Dis host side.

5. **Ollama is actually reachable from inside the container** (needed for
   any Ollama-backed evaluator to actually work, not just register).
   ```bash
   docker exec $(docker compose -f docker-compose.remote-fierypit.yml --env-file remote-fierypit.env ps -q lildaemon) \
     python3 -c "import urllib.request; print(urllib.request.urlopen('http://host.docker.internal:11434/api/tags', timeout=5).read()[:200])"
   ```
   - [ ] Returns a model list, not a connection error.

**Part 2 result:** Pass / Fail — Run by: __________ Date: __________ Host: __________
Notes:

---

## Part 3 — Dis host: open the firewall to the new remote host

Run **on the Dis host** (e.g. limbic).

```bash
sudo ufw allow from <remote-host-LAN-IP> to any port 8080 proto tcp   # Dis HTTP
sudo ufw allow from <remote-host-LAN-IP> to any port 9094 proto tcp   # Kafka external listener
sudo ufw status
```
- [ ] Both rules present.

**Part 3 result:** Pass / Fail — Run by: __________ Date: __________

---

## Part 4 — Cross-host confirmation

Run from the Dis host (or anywhere on the LAN that can reach it).

```bash
cd lildaemon
./scripts/check_remote_fierypit.sh <remote-host-hostname> http://<dis-host>:8080
```

- [ ] Exits 0, prints a registration whose `base_url` matches the remote
      host and whose `evaluators` list is non-empty.

If it fails, the script prints the full current registry — check whether
the remote host is simply missing (Part 2/3 firewall or `DIS_BASE_URL`
issue) or present with a stale timestamp (heartbeat not reaching Dis).

**Part 4 result:** Pass / Fail — Run by: __________ Date: __________
Notes:

---

## Not covered by this worksheet (yet)

Actually performing a Ritual with a seat on the remote host — this
worksheet only confirms registration/discovery and basic reachability
(Ollama, Dis, firewall). That needs the standalone example script
demonstrating a local-only Ritual vs. one with a discovered remote seat
(queued, not built as of this worksheet's writing) — update this section
with a Part 5 once that exists.

## Sign-off

| Part | Result | Run by | Date | Host(s) |
|------|--------|--------|------|---------|
| 1 (local sanity) | Pass | Claude (this session) | 2026-09-02 | limbic |
| 2 (remote deploy) | | | | |
| 3 (Dis firewall) | | | | |
| 4 (cross-host confirm) | | | | |
