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
- [ ] For Parts 2–4: **non-interactive SSH from the Dis host to the remote
      host** — `ssh <remote-host> true` must succeed with no password/
      passphrase/host-key prompt. This is a real requirement, not
      optional: Part 2's commands run over SSH with no human at the
      keyboard, and any prompt just hangs. Set this up once per Dis-host-
      to-remote-host pair:
      1. On the Dis host: `ssh-keygen -t ed25519 -f ~/.ssh/<name>_deploy_ed25519 -N ""`
         (empty passphrase — it must be usable non-interactively).
      2. Add a `Host <remote-host>` block to `~/.ssh/config` on the Dis
         host pointing `IdentityFile` at that key, with `BatchMode yes`
         (fails fast instead of prompting if the key isn't accepted yet)
         and `StrictHostKeyChecking accept-new` (accepts the host key on
         first connect without prompting).
      3. Append the generated `.pub` file's contents to
         `~/.ssh/authorized_keys` **on the remote host** (paste the
         `ssh-keygen -y` output or the `.pub` file's own contents as ONE
         single line — pasting a multi-line block here is a common
         failure mode: it can mangle the `>>` redirect and produce a
         confusing "Permission denied" that's actually a corrupted
         command, not an actual permissions problem).
      4. Confirm: `ssh <remote-host> true` from the Dis host, no prompt,
         exit code 0.
      This key lives outside both repos (`~/.ssh/`, not under
      `Development/`) — never commit it; `docker/.env`-style secrets are
      gitignored but a private key checked in by accident would not be
      caught by that same pattern.
- [ ] The remote host's checkout is up to date (`git pull` in both
      `lildaemon/` and `clara-cerebellum/`).

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
   The `lildaemon` service runs with `network_mode: host` (see the
   compose file's own comment — a real Docker/ufw interaction forced
   this), so this is a genuine, load-bearing rule, not a nice-to-have:
   confirmed live against pineal that registration and `/ritual/join`
   both silently work WITHOUT this rule present as long as the service
   is bridge-networked with a published port (Docker's own port-publish
   bypasses ufw's INPUT filtering) — but break the moment you're on
   `network_mode: host`, where that bypass no longer applies.
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
   - [ ] **Dis host's own `docker/.env` has `KAFKA_EXTERNAL_ADVERTISED_
         HOST` set to its real LAN name**, not left at the `localhost`
         default — confirmed live this is easy to skip and produces a
         confusing failure much later (Part 5, not here): registration
         succeeds fine regardless, but every `caws_offer`/`caws_await`
         to this seat silently hangs until patience runs out, because
         the remote FieryPit's Kafka client gets told to reconnect to
         its OWN loopback instead of the real broker. If unset, fix it
         on the Dis host and `docker compose up -d --force-recreate
         kafka` there before continuing to Part 5.

5. **Ollama is actually reachable from inside the container** (needed for
   any Ollama-backed evaluator to actually work, not just register).
   With `network_mode: host`, this is the SAME loopback the host itself
   uses — if this fails, Ollama itself is the problem (not reachable
   even from the host — check `curl localhost:11434/api/tags` on the
   host directly first), not a container-networking issue.
   ```bash
   docker exec $(docker compose -f docker-compose.remote-fierypit.yml --env-file remote-fierypit.env ps -q lildaemon) \
     python3 -c "import urllib.request; print(urllib.request.urlopen('http://localhost:11434/api/tags', timeout=5).read()[:200])"
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
- [ ] Both rules present. (Against pineal, both ports turned out to
      already be reachable without these — limbic's existing `ufw`
      config was already permissive enough for this LAN. Don't assume
      that generalizes to a new Dis host; add them explicitly and treat
      "already worked" as a bonus, not a reason to skip this step.)

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

## Part 5 — Actually perform a cross-host Ritual

Run from the Dis host, after Parts 2–4 all pass. Uses
`lildaemon/examples_ritual_remote_fierypit.py` (see `lildaemon/docs/
ritual_remote_fierypit_example.md` for the full design and the bugs
found building it).

```bash
cd lildaemon
python examples_ritual_remote_fierypit.py \
  "Name one interesting fact about octopuses, briefly." \
  --peer-token "$DIS_DOMAIN_PEER_TOKEN" \
  --remote \
  --remote-kafka-bootstrap <dis-host-LAN-address>:9094 \
  --poll-max-wait-s 280 --max-cycles 600 --patience-cycles 500
```

(Larger budgets than the script's own defaults, deliberately: `demo_local`
and `demo_remote` run as one sequential conjunction, so the shared cycle
budget has to cover TWO real LLM round trips back to back, not one —
confirmed live this matters, the defaults occasionally weren't enough
even just for the local leg alone.)

- [ ] Prints a `=== Local seat ===` section with an answer.
- [ ] Prints a `=== Remote seat ===` section with a genuinely different
      answer (confirms it actually came from the remote host's own
      model, not a repeated local response).
- [ ] Container logs on both hosts show the seat joining and leaving
      cleanly (`RitualParticipant: stopped`, `RitualManager: left
      ritual`) — no orphaned consumers left running after the script
      exits.

**Part 5 result:** Pass / Fail — Run by: __________ Date: __________
Notes:

## Sign-off

| Part | Result | Run by | Date | Host(s) |
|------|--------|--------|------|---------|
| 1 (local sanity) | Pass | Claude (this session) | 2026-09-02 | limbic |
| 2 (remote deploy) | Pass | Claude (this session) + user (firewall/ufw steps) | 2026-09-02 | pineal |
| 3 (Dis firewall) | Pass (already open) | Claude (this session) | 2026-09-02 | limbic |
| 4 (cross-host confirm) | Pass | Claude (this session) | 2026-09-02 | limbic + pineal |
| 5 (cross-host Ritual) | Pass (x2) | Claude (this session) | 2026-09-02 | limbic + pineal |
