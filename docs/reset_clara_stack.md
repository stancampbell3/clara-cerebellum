# Full stack reset

**Script:** `scripts/reset_clara_stack.sh`
**Why this exists:** the physical hardware hit a temperature limit
(2026-09), and until there's a more stable setup, it's not safe to assume
stale state — half-finished Rituals, orphaned research-task rows,
leftover Edgequake documents from an interrupted ingest, stale Kafka
consumer groups — is fine to carry forward across restarts. This gives a
reliable way back to a known-empty baseline across every component.

## Usage

```bash
cd clara-cerebellum
./scripts/reset_clara_stack.sh --dry-run   # see what would be wiped, no changes
./scripts/reset_clara_stack.sh             # interactive confirm, then reset
./scripts/reset_clara_stack.sh --yes       # skip the confirmation prompt
./scripts/reset_clara_stack.sh --start     # also bring the stack back up after
```

Must be run with `duckdb` and `jq` available on `PATH`, from a checkout
where `lildaemon/` is a sibling of `clara-cerebellum/` (the same layout
`./clara` itself assumes).

## What gets wiped

| Component | Mechanism |
|---|---|
| clara-api's CoireStore (`data/coire.duckdb`) — `rituals`, deductions, coire events, source registry | `scripts/flush_coire.sh` (row-level `DELETE`, schema stays) |
| lildaemon's DuckDB (`lildaemon/data/lildaemon.duc`) — users, sessions, ritual configs, research queue | `lildaemon/scripts/flush_lildaemon_db.sh` (new, mirrors `flush_coire.sh`) |
| lildaemon's `output/` and `workspace/` directories | `purge_output.sh` + `purge_workspace.sh` |
| Kafka — every topic, every Ritual's consumer-group offsets | container recreate (no volume mounted, so this is a complete wipe in one step) |
| Clara's own Edgequake workspace — documents, chunks, vectors, AGE graph nodes, PDFs/assets | `scripts/reset_edgequake_clara_workspace.sh`, Edgequake's scoped `DELETE /api/v1/documents` |
| Leftover `adhoc.*` demo workspaces from crashed `examples_ritual_*.py` runs | `scripts/reset_adhoc_workspaces.sh` |

Cobbler (dagda) needs **no step of its own** — it owns no persistent data;
its one write path (session bookmarks) lives inside lildaemon's own
`output/` tree, already covered above.

## What is deliberately left untouched

- **dagda's unrelated data-ingestion/training work** (`scripts/`, `test/`,
  `dataset/`, `db/`, `models/`, `output/` at the dagda repo root) — a
  separate concern sharing the repo pending a future split, explicitly
  out of scope per that repo's own `.gitlab-ci.yml`.
- **Any other tenant/workspace Edgequake hosts.** Edgequake is a genuinely
  multi-tenant service, running as its own separate docker-compose
  project (`~/moonpool/tools/edgequake`) — not something this stack owns
  outright. The reset only ever touches the tenant/workspace named by
  `EDGEQUAKE_DEFAULT_TENANT`/`EDGEQUAKE_DEFAULT_WORKSPACE` in
  `docker/.env` (Clara's own), via Edgequake's *scoped* wipe API.

  **Do not** reach for Edgequake's own `make db-reset` / `make db-clean`
  / `make db-clean-force` targets to reset "Clara's" Edgequake data —
  all three are whole-instance, cross-tenant operations that would erase
  every tenant/workspace the instance hosts, not just Clara's.

## A mount-path gotcha this script gets right (so you don't have to)

There are **two** `lildaemon.duc` files in a typical checkout, and only
one of them is what the running stack actually uses:

- `clara-cerebellum/lildaemon/data/lildaemon.duc` — gitignored inside this
  repo (`clara-cerebellum/.gitignore`), exists purely as the docker-compose
  bind-mount target (`../lildaemon/data:/app/data`, relative to `docker/`).
  **This is the real, live one** — confirmed via `docker compose ... config`
  against the actual `./clara` invocation.
- `Development/lildaemon/lildaemon.duc` (the sibling repo's own root) — only
  used if someone runs lildaemon natively (`uvicorn goat.app.main:app`)
  outside Docker. A separate, independent database from the one above.

`reset_clara_stack.sh` and `flush_lildaemon_db.sh` always operate on the
first one (`clara-cerebellum/lildaemon/data/lildaemon.duc`), matching what
`./clara` mounts. The native-mode file is out of scope for this tool, same
as dagda's unrelated data-ingestion work — it's dev-only, not part of the
docker-orchestrated stack this script manages.

## Real bugs found running this for real (all fixed, 2026-09-06)

1. **No standalone `duckdb` CLI on the host.** `flush_coire.sh` and
   `flush_lildaemon_db.sh` originally hard-required it — a real gap, since
   a deployed Clara node won't always have it installed even though it
   has Python `duckdb` available (lildaemon's own venv needs it anyway).
   Fixed: both scripts now fall back to `python3 -c "import duckdb; ..."`
   (system Python first, then the sibling repo's `.venv`) when the CLI
   isn't on `PATH`.
2. **Docker-created files are root-owned on the host bind mount.** A
   docker-run `lildaemon` writes `lildaemon.duc`, `output/context/`,
   `output/sessions/`, and `workspace/instances/*` as root — a host user's
   own `duckdb`/`python`/`rm` can't write or delete them (`Permission
   denied`), confirmed live. Fixed: `flush_lildaemon_db.sh`,
   `purge_output.sh`, and `purge_workspace.sh` now detect this and re-run
   the same operation inside a throwaway `lildaemon:latest` container
   instead (same root that owns the files) — no host `sudo` needed.
3. **`purge_output.sh` was missing its executable bit** (a pre-existing
   file, never actually run via `./scripts/reset_clara_stack.sh` before
   this). Fixed.

## Live verification (2026-09-06, real run against limbic)

Ran end-to-end for real (not `--dry-run`): CoireStore's 6 tables and
lildaemon's 8 tables confirmed empty by direct row-count query afterward;
Kafka recreated with no volume (all topics + consumer-group offsets
gone); Clara's Edgequake workspace went from 25 real documents to 0
(confirmed by re-querying `GET /api/v1/documents` after the wipe task
completed); the 6 real leftover `adhoc.*` demo workspaces found during
testing were swept. Edgequake's own containers (a separate compose
project) were started only for the duration of the wipe and stopped
again afterward, matching "leave everything down by default."

## Recovery after a reset

- `users` is fully wiped, but `POST /auth/register` requires no prior
  auth — the instance is immediately self-service-recoverable, no
  admin-reseed step needed.
- The stack is left **down** by default (matching the "careful, don't
  assume it's safe to just keep running" motivation for this tool) — run
  `./clara up -d` (or pass `--start`) when you're ready to bring it back.
