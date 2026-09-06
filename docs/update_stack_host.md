# Updating a peer FieryPit host (e.g. pineal)

**Script:** `scripts/update_stack_host.sh`
**Metadata:** `docker/dis_domain_hosts.json` — describes every host in the
Dis domain (which repos it needs, which compose/env file, its role) so
the same script works for any peer host without editing script logic.

## Usage

Run **on the target host itself** (e.g. pineal), from its
`clara-cerebellum` checkout:

```bash
cd clara-cerebellum
./scripts/update_stack_host.sh --dry-run   # preview, no changes
./scripts/update_stack_host.sh             # pull, rebuild, restart, verify
./scripts/update_stack_host.sh --yes       # non-interactive (for cron/CI)
```

Defaults to `$(hostname)` looked up in `dis_domain_hosts.json`; pass a
different name as the first argument to target another entry.

## What it does

1. For each repo listed for this host's role (`lildaemon`,
   `clara-cerebellum` for the `remote-fierypit` role): fetches, refuses
   to touch anything if there are local modifications to *tracked*
   files, then fast-forwards only (`git merge --ff-only`) — never force,
   never stash, never discard. If nothing's new, it says so and moves on.
2. Diffs the host's real, gitignored env file (e.g.
   `remote-fierypit.env`) against its `.example` counterpart's *required*
   keys. Anything newly required gets prompted for interactively (and
   saved) unless `--yes`, in which case the script aborts and lists what's
   missing rather than guessing. This is the "preserve local settings, or
   ask" behavior — the env file itself is never touched except to append
   a genuinely new key.
3. If anything actually changed (code or env), rebuilds and restarts via
   the same `docker compose ... up -d --build` command from the
   deployment doc — otherwise skips the rebuild entirely (safe to run
   repeatedly, e.g. on a schedule).
4. Confirms the restart actually rejoined Dis by calling the existing
   `lildaemon/scripts/check_remote_fierypit.sh` against the host's
   configured `DIS_BASE_URL`.

## Adding a new peer host

Add a block to `docker/dis_domain_hosts.json`:
```json
"newhost": {
  "role": "remote-fierypit",
  "repos": ["lildaemon", "clara-cerebellum"],
  "compose_file": "docker-compose.remote-fierypit.yml",
  "env_file": "remote-fierypit.env",
  "env_example_file": "remote-fierypit.env.example",
  "dis_base_url": "http://limbic:8080"
}
```
No script changes needed. The new host still needs to be bootstrapped
once by hand first (repos cloned, `remote-fierypit.env` created from the
`.example` — see `docs/remote_fierypit_deployment.md`) — this script
updates an already-running host, it doesn't provision a new one.

## Real findings from the first live run (2026-09-06, against pineal)

- **`hostname` can return a differently-cased string than everything
  else expects.** Confirmed live: pineal's own `hostname` reports
  `Pineal` (capitalized), while `/etc/hosts`, this metadata file, and
  every FieryPit's own registered `base_url` are lowercase. Fixed by
  normalizing the resolved host name to lowercase before doing anything
  with it — a case mismatch here would have silently broken both the
  metadata lookup and the final rejoin check's substring match.
- **First real run against pineal, end to end**: found it genuinely 8
  commits behind (the reset-tooling work from the prior session), fast-
  forwarded both repos cleanly, rebuilt the image (~90s, mostly cached),
  recreated the container, and — once limbic's own stack was confirmed
  up — verified pineal re-registered with Dis with its full evaluator
  list intact.
- The Dis-rejoin check will correctly fail if Dis (limbic's own
  `clara-api`) isn't running — this is not a bug in the script; it's an
  accurate signal that the *other* end of the loop is down, not that the
  update itself failed. Check `curl limbic:8080/health` if this happens.

## Explicitly out of scope

- Resetting any data — see `docs/reset_clara_stack.md`, a separate,
  earlier step at a release/dev checkpoint.
- Provisioning a brand-new host from scratch (cloning repos for the
  first time, creating the initial env file, opening firewall ports —
  see `docs/remote_fierypit_deployment.md`).
- Updating limbic itself — limbic is where commits originate during
  normal development, not somewhere code gets pulled *to* today.
