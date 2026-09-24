#!/usr/bin/env python3
"""
Ground state for the Clara stack: capture a known baseline, reset to it, verify it. See docs/ground_state.md.

A baseline is ONE directory holding every layer, so that "the state we test against" is a thing you can name, copy and diff:

    baseline.json                    what/when/which commits
    manifest.json + documents/       Edgequake documents as text with content hashes         (re-ingestable: `seed`)
    pg/                              exact, workspace-scoped Postgres snapshot                (instant, no LLM: `restore`)
    assistant_*.jsonl                the assistant's tags, topics and research queue
    duckdb/                          lildaemon baseline tables (users, ritual configs, ...)   (owner-only)
    kafka.json                       expected topics; a baseline holds no ritual/coire topics
    canaries.json (optional)         questions the workspace must answer, with the sources they must cite

    ground_state.py status
    ground_state.py capture [--to DIR] [--allow-orphans]
    ground_state.py reset   [--to DIR | --no-archive] [--components edgequake,kafka,dis] [--all-kafka] [--dry-run] [--yes]
    ground_state.py restore --from DIR [--components pg,bookkeeping,kafka,duckdb] [--dry-run]
    ground_state.py seed    --from DIR
    ground_state.py verify  --baseline DIR
    ground_state.py curate  --baseline DIR --keep-users a,b --empty table1,table2
    ground_state.py queue   <status|expire-stuck|archive|export|reset-coupled|promote ...>

Everything destructive archives first (`--no-archive` must be explicit), names the workspace by slug (default assistant.general), and
touches only that workspace. DuckDB allows one process per file, so commands that read lildaemon's database stop the lildaemon
container for the duration and start it again.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import time
from pathlib import Path
from typing import Any, Dict, List, Optional, Sequence

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import ground_state_kafka as gk  # noqa: E402
import ground_state_pg as gp  # noqa: E402

REPO = HERE.parent
DEV = REPO.parent
LILDAEMON_SRC = DEV / "lildaemon"
ENV_FILE = REPO / "docker" / ".env"
LILDAEMON_CONTAINER = "docker-lildaemon-1"
CLARA_API_CONTAINER = "docker-clara-api-1"
DEFAULT_SLUG = "assistant.general"
HOST_ARCHIVE_ROOT = Path(os.environ.get("GROUND_STATE_HOST_ARCHIVE_ROOT", "/mnt/moonpool/clara-archives"))
CONTAINER_MOUNT = ("/mnt/moonpool", "/app/moonpool")
DIS_URL = os.environ.get("DIS_URL", "http://127.0.0.1:8080")
# Point the container tool at a different data directory (an isolated copy, for testing). The live lildaemon service is then left
# running, since nothing holds the file being opened.
DATA_DIR = Path(os.environ.get("GROUND_STATE_DATA_DIR", str(REPO / "lildaemon" / "data")))
LIVE_DATA_DIR = REPO / "lildaemon" / "data"


def pg_handle() -> "gp.Pg":
    """Edgequake's Postgres. GROUND_STATE_PG_CONTAINER points the tool at a scratch container when testing a restore."""
    return gp.Pg(os.environ.get("GROUND_STATE_PG_CONTAINER", gp.DEFAULT_CONTAINER))


def load_env(path: Path = ENV_FILE) -> Dict[str, str]:
    env: Dict[str, str] = {}
    if path.exists():
        for line in path.read_text().splitlines():
            line = line.strip()
            if line and not line.startswith("#") and "=" in line:
                k, v = line.split("=", 1)
                env[k.strip()] = v.strip().strip('"').strip("'")
    return env


def stamp() -> str:
    return time.strftime("%Y%m%dT%H%M%S", time.gmtime())


def to_container(path: Path) -> str:
    host, cont = CONTAINER_MOUNT
    p = str(Path(path).resolve())
    if not (p == host or p.startswith(host + "/")):
        raise SystemExit(f"{p} is not under {host}, which is the only archive location the lildaemon container can see")
    return cont + p[len(host) :]


def _run(cmd: Sequence[str], check: bool = True, **kw) -> subprocess.CompletedProcess:
    return subprocess.run(list(cmd), check=check, text=True, **kw)


def container_running(name: str) -> bool:
    r = subprocess.run(["docker", "inspect", "-f", "{{.State.Running}}", name], capture_output=True, text=True)
    return r.stdout.strip() == "true"


def lildaemon_module(module: str, args: Sequence[str], env_extra: Optional[Dict[str, str]] = None, use_image_code: bool = False) -> Dict[str, Any]:
    """Run `python -m <module> args` in a throwaway lildaemon container against the live data volume.

    The lildaemon service is stopped first (DuckDB is single-process) and restarted afterwards, even on failure. The working tree's
    `goat/` is mounted over the image's copy unless `use_image_code`, so the tool runs the code you are looking at.
    """
    env = load_env()
    slug = env.get("ASSISTANT_WORKSPACE_SLUG") or DEFAULT_SLUG
    run_env = {
        "EDGEQUAKE_BASE_URL": env.get("EDGEQUAKE_BASE_URL", "http://10.0.0.192:8082"),
        "EDGEQUAKE_API_KEY": env.get("EDGEQUAKE_API_KEY", ""),
        "EDGEQUAKE_DEFAULT_TENANT": env.get("EDGEQUAKE_DEFAULT_TENANT", ""),
        "ASSISTANT_WORKSPACE_SLUG": slug,
        "LILDAEMON_DB_PATH": "/app/data/lildaemon.duc",
        # the tool runs as root in the container; hand the files it writes back to whoever ran this script
        "GROUND_STATE_CHOWN": f"{os.getuid()}:{os.getgid()}",
        **(env_extra or {}),
    }
    cmd = ["docker", "run", "--rm", "-v", f"{DATA_DIR}:/app/data", "-v", f"{CONTAINER_MOUNT[0]}:{CONTAINER_MOUNT[1]}"]
    if not use_image_code and (LILDAEMON_SRC / "goat").is_dir():
        cmd += ["-v", f"{LILDAEMON_SRC / 'goat'}:/app/goat:ro"]
    for k, v in run_env.items():
        cmd += ["-e", f"{k}={v}"]
    cmd += ["lildaemon:latest", "python", "-m", module, *args]
    was_running = DATA_DIR.resolve() == LIVE_DATA_DIR.resolve() and container_running(LILDAEMON_CONTAINER)
    if was_running:
        _run(["docker", "stop", LILDAEMON_CONTAINER], stdout=subprocess.DEVNULL)
    try:
        r = subprocess.run(cmd, capture_output=True, text=True)
    finally:
        if was_running:
            _run(["docker", "start", LILDAEMON_CONTAINER], check=False, stdout=subprocess.DEVNULL)
    out = r.stdout.strip()
    try:
        body = json.loads(out) if out else {}
    except json.JSONDecodeError:
        body = {"raw": out}
    if r.returncode not in (0, 1):
        raise SystemExit(f"{module} failed ({r.returncode}): {r.stderr.strip()[-600:]}")
    return {"returncode": r.returncode, "result": body, "stderr": r.stderr.strip()[-300:]}


def gs(args: Sequence[str], **kw) -> Dict[str, Any]:
    return lildaemon_module("goat.app.assistant.ground_state", args, **kw)


def maintenance(args: Sequence[str], **kw) -> Dict[str, Any]:
    return lildaemon_module("goat.app.assistant.maintenance", args, **kw)


def git_head(repo: Path) -> Optional[str]:
    r = subprocess.run(["git", "-C", str(repo), "rev-parse", "--short", "HEAD"], capture_output=True, text=True)
    return r.stdout.strip() or None


def slug_from_env() -> str:
    return load_env().get("ASSISTANT_WORKSPACE_SLUG") or DEFAULT_SLUG


def workspace_id_of(archive: Path) -> str:
    return json.loads((archive / "manifest.json").read_text())["workspace_id"]


# ── commands ────────────────────────────────────────────────────────────────


def terminate_dis_rituals() -> List[str]:
    """Terminate every Ritual Dis still lists as active (DELETE /ritual/{id}). Test debris accumulates there: every lildaemon start
    leaves its assistant standing Ritual behind, and Dis never reaps an active Ritual on its own."""
    import urllib.request

    ids = gk.live_ritual_ids(DIS_URL)
    done = []
    for rid in ids:
        req = urllib.request.Request(f"{DIS_URL.rstrip('/')}/ritual/{rid}", method="DELETE")
        try:
            urllib.request.urlopen(req, timeout=15).read()
            done.append(rid)
        except Exception as exc:  # noqa: BLE001
            print(f"warning: could not terminate ritual {rid}: {exc}", file=sys.stderr)
    return done


def cmd_status(args) -> int:
    out: Dict[str, Any] = {"kafka": gk.status(gk.Kafka(), DIS_URL)}
    ws = args.workspace_id
    if ws:
        pg = pg_handle()
        out["postgres"] = {"counts": gp.counts(pg, ws), "orphans": {k: v for k, v in gp.orphans(pg, ws).items() if k != "_ids"}}
    if args.with_db:
        out["lildaemon"] = gs(["status"])["result"]
        if not ws:
            ws = (out["lildaemon"] or {}).get("workspace_id")
            if ws:
                pg = pg_handle()
                out["postgres"] = {"counts": gp.counts(pg, ws), "orphans": {k: v for k, v in gp.orphans(pg, ws).items() if k != "_ids"}}
    print(json.dumps(out, indent=2, default=str))
    return 0


def _capture_into(dest: Path, allow_orphans: bool, allow_busy: bool = False) -> Dict[str, Any]:
    dest.mkdir(parents=True, exist_ok=True)
    cdest = to_container(dest)
    # 1. content + bookkeeping (needs Edgequake's API and the stopped DB)
    content = gs(["capture", "--to", cdest])
    if content["returncode"] != 0:
        raise SystemExit(f"content capture failed: {content}")
    # 2. exact Postgres snapshot of the same workspace (refuses if it carries orphaned rows)
    ws = workspace_id_of(dest)
    try:
        gp.export(pg_handle(), ws, dest, allow_orphans=allow_orphans, allow_busy=allow_busy)
    except gp.PgError as exc:
        raise SystemExit(f"postgres snapshot failed: {exc}")
    # 3. lildaemon baseline tables, 4. Kafka's expected shape
    duck = gs(["duckdb-export", "--to", cdest])["result"]
    kafka = gk.snapshot(gk.Kafka())
    (dest / "kafka.json").write_text(json.dumps(kafka, indent=2))
    summary = {
        "version": 1,
        "created_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "workspace_slug": slug_from_env(),
        "workspace_id": ws,
        "commits": {"lildaemon": git_head(LILDAEMON_SRC), "clara-cerebellum": git_head(REPO)},
        "layers": {
            "documents": content["result"].get("documents"),
            "postgres": json.loads((dest / "pg" / "manifest.json").read_text())["counts"],
            "duckdb": duck,
            "kafka_expected_topics": len(kafka["expected_topics"]),
        },
    }
    (dest / "baseline.json").write_text(json.dumps(summary, indent=2))
    return summary


def cmd_capture(args) -> int:
    dest = Path(args.to) if args.to else HOST_ARCHIVE_ROOT / f"{stamp()}_{slug_from_env()}"
    print(json.dumps({"archive": str(dest), **_capture_into(dest, args.allow_orphans, args.allow_busy)}, indent=2))
    return 0


def _confirm(what: str, yes: bool) -> None:
    if yes:
        return
    if input(f"{what}\nType RESET to continue: ").strip() != "RESET":
        raise SystemExit("aborted; nothing was changed")


def cmd_reset(args) -> int:
    comps = {c.strip() for c in args.components.split(",") if c.strip()}
    unknown = comps - {"edgequake", "kafka", "dis"}
    if unknown:
        raise SystemExit(f"unknown component(s): {sorted(unknown)}")
    plan: Dict[str, Any] = {"components": sorted(comps), "dry_run": args.dry_run}
    if args.dry_run:
        if "edgequake" in comps:
            plan["edgequake"] = gs(["reset", "--dry-run"])["result"]
        if "kafka" in comps:
            plan["kafka"] = gk.reset(gk.Kafka(), DIS_URL, args.all_kafka, dry_run=True)
        if "dis" in comps:
            plan["dis"] = {"would_terminate_rituals": len(gk.live_ritual_ids(DIS_URL))}
        print(json.dumps(plan, indent=2, default=str))
        return 0
    _confirm(f"This will empty Edgequake workspace '{slug_from_env()}' (archived first unless --no-archive), clear the assistant's "
             f"bookkeeping, and reset: {sorted(comps)}.", args.yes)
    archive = None if args.no_archive else (Path(args.to) if args.to else HOST_ARCHIVE_ROOT / f"{stamp()}_{slug_from_env()}_pre_reset")
    if "edgequake" in comps:
        if archive is not None:
            plan["archive"] = {"path": str(archive), **_capture_into(archive, allow_orphans=True)}
        ws = workspace_id_of(archive) if archive is not None else None
        args_ = ["reset", "--skip-capture"] + (["--no-archive"] if archive is None else ["--to", to_container(archive)])
        plan["edgequake"] = gs(args_)["result"]
        # Edgequake's own delete leaves vector/kv rows behind; clear them so "empty" is really empty.
        ws = ws or plan["edgequake"].get("workspace_id")
        if ws:
            plan["orphans_cleaned"] = gp.clean_orphans(pg_handle(), ws)
    if "dis" in comps:
        # Terminate first, so the Kafka step below sees them as not live. lildaemon caches its standing Ritual in memory, so it is
        # restarted afterwards to create a fresh one.
        plan["dis"] = {"terminated_rituals": len(terminate_dis_rituals())}
        if container_running(LILDAEMON_CONTAINER):
            _run(["docker", "restart", LILDAEMON_CONTAINER], stdout=subprocess.DEVNULL)
            plan["dis"]["lildaemon"] = "restarted"
    if "kafka" in comps:
        plan["kafka"] = gk.reset(gk.Kafka(), DIS_URL, args.all_kafka, dry_run=False)
    print(json.dumps(plan, indent=2, default=str))
    return 0


def cmd_restore(args) -> int:
    src = Path(args.src)
    comps = {c.strip() for c in args.components.split(",") if c.strip()}
    out: Dict[str, Any] = {"from": str(src), "components": sorted(comps), "dry_run": args.dry_run}
    if "pg" in comps:
        out["postgres"] = gp.restore(pg_handle(), src, dry_run=args.dry_run)
    if args.dry_run:
        print(json.dumps(out, indent=2, default=str))
        return 0
    steps = []
    if "bookkeeping" in comps:
        steps.append(("bookkeeping", gs(["restore-bookkeeping", "--from", to_container(src)])["result"]))
    if "duckdb" in comps:
        steps.append(("duckdb", gs(["duckdb-restore", "--from", to_container(src)])["result"]))
    for name, res in steps:
        out[name] = res
    if "kafka" in comps and (src / "kafka.json").exists():
        k = gk.Kafka()
        base = json.loads((src / "kafka.json").read_text())
        have = set(k.topics())
        for t in base["expected_topics"]:
            if t["name"] not in have:
                k.create_topic(t)
        out["kafka"] = "expected topics ensured"
    print(json.dumps(out, indent=2, default=str))
    return 0 if out.get("postgres", {}).get("matches_snapshot", True) else 2


def cmd_seed(args) -> int:
    r = gs(["seed", "--from", to_container(Path(args.src))])
    print(json.dumps(r["result"], indent=2))
    return r["returncode"]


def cmd_verify(args) -> int:
    base = Path(args.baseline)
    checks: List[Dict[str, Any]] = []
    content = gs(["verify", "--baseline", to_container(base)])["result"]
    checks += [{"layer": "edgequake+assistant", **c} for c in content.get("checks", [])]
    ws = workspace_id_of(base)
    pg = pg_handle()
    want = json.loads((base / "pg" / "manifest.json").read_text())["counts"] if (base / "pg" / "manifest.json").exists() else None
    if want is not None:
        have = gp.counts(pg, ws)
        diff = {k: {"want": v, "have": have.get(k)} for k, v in want.items() if have.get(k) != v}
        checks.append({"layer": "postgres", "check": "row counts match the snapshot", "ok": not diff, "detail": diff})
    o = gp.orphans(pg, ws)
    o.pop("_ids")
    checks.append({"layer": "postgres", "check": "no orphaned vector/kv rows from deleted documents", "ok": o["clean"], "detail": o})
    if (base / "kafka.json").exists():
        kv = gk.verify(gk.Kafka(), json.loads((base / "kafka.json").read_text()))
        checks += [{"layer": "kafka", **c} for c in kv["checks"]]
    ok = all(c["ok"] for c in checks)
    print(json.dumps({"ok": ok, "failed": [c for c in checks if not c["ok"]], "checks": len(checks)}, indent=2, default=str))
    return 0 if ok else 1


def cmd_curate(args) -> int:
    r = gs(["curate", "--baseline", to_container(Path(args.baseline)), "--keep-users", args.keep_users, "--empty", args.empty])
    print(json.dumps(r["result"], indent=2, default=str))
    return r["returncode"]


def cmd_queue(args) -> int:
    r = maintenance(args.rest)
    print(json.dumps(r["result"], indent=2, default=str))
    return r["returncode"]


def main(argv: Optional[Sequence[str]] = None) -> int:
    p = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = p.add_subparsers(dest="cmd", required=True)
    s = sub.add_parser("status")
    s.add_argument("--workspace-id", default=None)
    s.add_argument("--with-db", action="store_true", help="also read lildaemon's DB and Edgequake's API (stops lildaemon briefly)")
    c = sub.add_parser("capture")
    c.add_argument("--to", default=None)
    c.add_argument("--allow-orphans", action="store_true")
    c.add_argument("--allow-busy", action="store_true", help="snapshot even though documents are still being ingested")
    r = sub.add_parser("reset")
    r.add_argument("--to", default=None)
    r.add_argument("--no-archive", action="store_true")
    r.add_argument("--components", default="edgequake")
    r.add_argument("--all-kafka", action="store_true")
    r.add_argument("--dry-run", action="store_true")
    r.add_argument("--yes", action="store_true")
    rs = sub.add_parser("restore")
    rs.add_argument("--from", dest="src", required=True)
    rs.add_argument("--components", default="pg,bookkeeping,kafka")
    rs.add_argument("--dry-run", action="store_true")
    sd = sub.add_parser("seed")
    sd.add_argument("--from", dest="src", required=True)
    v = sub.add_parser("verify")
    v.add_argument("--baseline", required=True)
    cu = sub.add_parser("curate", help="make a captured baseline clean: keep only these users, empty these tables")
    cu.add_argument("--baseline", required=True)
    cu.add_argument("--keep-users", default="")
    cu.add_argument("--empty", default="")
    q = sub.add_parser("queue")
    q.add_argument("rest", nargs=argparse.REMAINDER)
    args = p.parse_args(argv)
    fn = {"status": cmd_status, "capture": cmd_capture, "reset": cmd_reset, "restore": cmd_restore, "seed": cmd_seed, "verify": cmd_verify, "queue": cmd_queue, "curate": cmd_curate}[args.cmd]
    try:
        return fn(args)
    except (gp.PgError, gk.KafkaError) as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
