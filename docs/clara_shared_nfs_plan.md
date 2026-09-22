# Shared NFS filesystem: Ritual collaboration, Ego continuity, and generated files

Plan approved 2026-09-22, following a Clara-assisted brainstorm (`lildaemon/docs/
brainstorm_clara_nfs_hermes_layout.md`). Goal: one shared filesystem, mounted on FieryPit hosts
only (never into Hermes seat containers — see "Decisions" below), backing three things: Ritual
collaboration (`goat/ritual_space/`), generated/published files (the Ego gate's outbox), and,
later, Hermes memory/continuity. This doc is the durable record; the original plan-mode document
lived only in `~/.claude/plans/` and is not repeated here verbatim.

## Decisions (asked of the user before building anything)

1. **Gate-mediated only, no direct NFS mount into Hermes seat containers.** The mount lives on the
   FieryPit host (limbic, pineal, ...); a seat's only mount stays its own ephemeral home. Preserves
   every existing "models never see filesystem paths, only tool calls" decision.
2. **Scope this phase to ritual continuity + collaboration.** Per-user and global Hermes memory
   tiers, and folding graduated memories into Edgequake, are deferred (see "Deferred" below) —
   real hooks for both were found in Hermes' own source and are recorded there for later.
3. When Deliverable A's live gate found a real bug, and the first fix didn't fully resolve it: **accept the
   residual cross-host limitation as documented, continue to Deliverable B**, rather than stopping
   for a bigger service-hosted-backend redesign. Rationale below.

## Architecture

```
/mnt/clara-shared/            (real directory on limbic; NFSv4.2 export, mounted at the same path
  ritual-spaces/<ritual_id>/   on every other FieryPit host — pineal today)
  outbox/<ritual_id>/
  hermes-continuity/<ritual_id>/    (built + verified live — Deliverable D, below)
```

limbic is the NFS **server** (`nfs-kernel-server`, already running — it also serves the unrelated
`/mnt/moonpool` dev-tree mount, see [[moonpool_mount_shared_hazard]], which this is deliberately
separate from) and uses `/mnt/clara-shared` as a plain local directory, no self-mount. Every other
FieryPit host mounts it over NFSv4.2. Setup is scripted and idempotent:
`lildaemon/scripts/setup_clara_shared_nfs_server.sh <client-ip>` (limbic) and
`setup_clara_shared_nfs_client.sh [server-host]` (pineal, or a future host).

Export is scoped per-client-IP with `root_squash` (`/32` ACL, not the whole LAN; squash, not
`no_root_squash`) — a deliberate hardening difference from `/mnt/moonpool`'s export, since this is
reached from Docker containers. **Operational consequence of `root_squash`:** the lildaemon/Ego
container's own process runs as root inside its container (confirmed via `docker top`), so it is
squashed to `nobody` for every write here. `ritual-spaces/` and `outbox/` are therefore `0777` at
the top level (so `nobody` can create the per-ritual subdirectory), while everything a FieryPit
creates below that stays owner-only (`0755`, owned by `nobody`) — fine, since every writer of a
given ritual is that same squashed identity. **A human operator cleaning up stray data under
either directory needs `sudo`** (plain `stanc` cannot delete `nobody`-owned files there);
`goat/app/ritual_space_reaper.py`, running as part of a FieryPit, is unaffected.

## Deliverable A: the export, and what its live gate found

Infra came up clean on the first try (see the scripts above); the real work was the gate this
phase requires before anything depends on the mount: a genuine two-host `fcntl.flock` hammer test
(4 processes, 150 appends each, split 2-on-limbic/2-on-pineal, against one `goat.ritual_space`
document), because [[flock_over_nfs_unverified]] had never actually been checked.

**First run (unpatched code): failed.** Lock exclusivity itself was fine (the journal's `seq`
numbers came out perfectly unique and sequential, 1..600, across all 4 writers) — but the
committed document content was short (510 of 600 lines), and `_commit()`'s journal write
(`goat/ritual_space/directory.py`), the only write in that function still using raw
`open(path, "ab")` rather than the atomic temp-file-plus-rename pattern already used for
`docs/`/`meta/`, is not safe for multi-host NFS: append mode's "always at the true current
end of file" guarantee does not hold across different client machines. **Fixed**: the journal now
builds its new content in memory and writes it through the same atomic-rename helper
(`_atomic_write`) as `docs/` and `meta/`. Regression test:
`tests/test_ritual_space.py::test_journal_write_is_atomic_rename_not_append`. Full suite: 2 failed
(pre-existing, unrelated — `TestGetFocusedEvaluatorInstance`) / 1787 passed / 26 skipped;
black/mypy clean.

**Rerunning the identical gate against the fix: still failed**, differently (489 of 600 journal
entries, again no duplicates, again unique/sequential up to that point — a contiguous block at the
tail simply stopped growing). Trying `noac` (disables NFS attribute/data caching) did not fix it
either, and produced a *worse* failure mode on the first (unpatched-code) attempt (NUL-byte-filled
gaps the exact length of a real record) before the code fix existed. **Isolating further**: the
same test with all 4 writers on pineal alone (no cross-host contention at all) passed perfectly,
600/600, every time. So the bug is specifically about two *different client machines* racing on
the same file — a genuine NFS cross-client cache-coherency gap: a process can hold the lock
correctly and still read a stale copy of a file another host just wrote and closed, then overwrite
it, silently discarding the other host's committed work. This is not fixed by changing the write
mode, and is not (currently) understood well enough to fix via mount options.

**Decision (the user, given this evidence):** accept it as a documented limitation rather than
design a service-hosted backend now, because the actual deployment never has two different hosts
writing the *same ritual_id* concurrently — a ritual's space is always owned by exactly one Ego
FieryPit at a time, for its whole life. This finding would need revisiting only if that assumption
ever changes (e.g. a ritual's Ego seat migrating between hosts mid-ritual, or anything other than
the owning FieryPit ever writing into a ritual's space directly).

**Not verified**: kill-mid-lock behavior over the real export (does a lock get released promptly if
the holding host dies, not just the process). Built as `scripts/flock_nfs_hold_and_kill.py` but the
live attempt was confounded by an SSH detachment quirk (the holder ran to a clean, un-killed
completion instead of being killed mid-hold) and was not repeated. Low priority given the
single-writer-per-ritual model above and the existing, separate seat-lease dead-man-switch
mechanism this doesn't interact with.

## Deliverable B: ritual space and outbox on the shared mount — done, verified live

`RITUAL_SPACE_ROOT`/`EGO_OUTBOX_ROOT` (container-side paths, unchanged) now bind-mount from
`${CLARA_SHARED_ROOT:-/mnt/clara-shared}/{ritual-spaces,outbox}` in both `docker-compose.ego.yml`
and `docker-compose.ego-remote.yml`, replacing the old per-host-local `$EGO_STATE_DIR` paths.
`$EGO_STATE_DIR`'s `data/`, `output/`, `workspace/` are untouched (per-process scratch, never
shared). Pineal's live Ego FieryPit was redeployed with this change (`docker compose ... up -d
--force-recreate`) and verified with a full live run of `examples_ritual_hermes_ego.py --strict`
against `pineal:6667`: `export_document` approved and executed, the outbox file and every ledger
record readable and byte-identical from *both* limbic and pineal, `plan.md` visible in the ritual
space from both hosts. (The run's own `--strict` exit reported two unrelated findings: the
reviewer approving a free-form `send_email` — known LLM-judgment variance, not new — and the
script's docker-evidence check being local-only, so it can't see a remote host's seat container;
neither is a regression from this work.)

This effectively resolves the paused "pickup story" for generated files
([[ego_pickup_story_paused]]): a published file is now a plain file under
`/mnt/clara-shared/outbox/<ritual_id>/...`, reachable from any host with the mount — no
bespoke pull-proxy needed. Revisit only if a host without the mount ever needs access.

## Deliverable D: ritual-keyed Hermes session continuity — done, verified live

The actual gap that partly motivated this plan: an idle-released Ego seat used to start a
genuinely blank Hermes home even within the same ritual. New module
`seat_launcher/continuity.py`: `session_id_for(ritual_id)` gives a stable, ritual-derived Hermes
session id; `start_run` now sends it in the `/v1/runs` body (Hermes' `api_server_runs.py` resumes a
session by this mechanism); `_build_home` calls `restore_into_home` before the container starts
(copies a checkpoint in, if one exists for this `ritual_id`); `_cleanup` calls
`checkpoint_from_home` *after* `docker rm -f` confirms the container gone (never before — copying a
file the container might still be writing risks a torn one) and *before* the seat directory is
deleted. New config field `continuity_root` (`None` by default: identical to today's behavior,
every seat starts blank). Deliberately not a Hermes `MemoryProvider` plugin — no new tool exposed to
the model, no new attack surface, `memory.memory_enabled` stays `false`; it rides entirely on
Hermes' own built-in session/resume mechanism.

**What actually needed checkpointing, found by inspecting a live seat's home mid-run (not
assumed):** `state.db` is SQLite in WAL mode — the most recent activity sits in `state.db-wal`
(in one real seat, *larger* than `state.db` itself) and is not yet checkpointed into the main file.
A first attempt that copied only `state.db` "succeeded" (no error) but silently lost the whole
conversation on restore. Fixed by checkpointing `state.db`, `state.db-wal`, `state.db-shm` and
`sessions/` together (the last confirmed empty in this Hermes version/config — kept anyway, since
Hermes' own source names it as session storage and harmless if unused). 12 tests in
`tests/test_seat_launcher_continuity.py`, plus integration tests in
`tests/test_seat_launcher_manager.py` covering the full create/checkpoint/delete/restore
lifecycle, the `session_id` sent on `start_run`, and that checkpoint only ever runs after the
container is confirmed removed. Full suite: 2 failed (same pre-existing) / 1805 passed / 26
skipped; black/mypy clean; mutation-tested (7/7 killed).

**Live proof** (pineal, `EGO_SEAT_IDLE_SECONDS` lowered temporarily to 30 for a fast, deterministic
test): told the Ego a secret code, waited for a real idle release (confirmed by a different
container name before/after), asked a follow-up in the same session — the rebuilt seat correctly
recalled the code. Without this fix (confirmed as the explicit baseline first): forgotten, every
time.

## Deferred (not built this phase)

- **Per-user memory tier.** Real hook found in Hermes' source: `gateway/platforms/
  api_server_runs.py` accepts a `turn_author` (`{id, name, is_bot}`) per run, which
  `MemoryProvider.sync_turn` already receives as writer identity. Needs `user_id` threaded from
  the assistant router down through the Offering into the launcher call — none of that plumbing
  exists yet. Build once there's a second real user to design against.
- **Global skill tier**, and whether a graduating skill needs Superego review before every Ego
  trusts it — open question.
- **A real Hermes `MemoryProvider` plugin** for the deferred tiers: files on the shared mount,
  served through a small gate-mediated HTTP surface (never a raw mount into the seat container),
  dropped into `$HERMES_HOME/plugins/<name>/` (a supported Hermes discovery path).
- **Async fold-in to Edgequake** for durable, searchable long-term recall — reuses the existing
  tag-and-reap pattern ([[edgequake_document_tagging_reap]]), deliberately never on Hermes'
  synchronous `prefetch()` path (Edgequake's `query()` has no metadata filter and can take 10s+).
- Whether Hermes' native memory-write tools, if ever enabled, are already excluded from the
  API-server platform by the same `platform_toolsets.api_server: []` gate — needs a real check,
  not assumed, before that phase.

## Related

[[ritual_space_design]], [[flock_over_nfs_unverified]] (superseded by this doc's findings — kept
for history), [[hermes_ego_pineal_deployment]], [[ego_pickup_story_paused]] (resolved by
Deliverable B), [[shared_durable_store_fierypit_dis_future_work]].
