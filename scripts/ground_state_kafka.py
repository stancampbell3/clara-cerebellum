#!/usr/bin/env python3
"""
Kafka's part of a ground state: which topics a baseline EXPECTS, and cleanup of the ephemeral ones Rituals leave behind.

Every Ritual gets a topic `dis.local.ritual.<ritual id>` and an ad hoc Coire channel gets `dis.local.coire.<subject>`; consumer groups
are named `ritual-<ritual id>-<participant>`. Those are *ephemeral*: a baseline holds none of them. Everything else (for example
`__consumer_offsets`, or a topic a deployment deliberately provisions) is *expected* and is recorded with its partitions and configs so
it can be recreated. Messages are never part of a baseline, only the shape.

    ground_state_kafka.py snapshot --out FILE                       write kafka.json (expected topics + ephemeral prefixes)
    ground_state_kafka.py status   [--dis URL]                      counts, and how many ephemeral topics no live Ritual owns
    ground_state_kafka.py reset    [--dis URL] [--all] [--dry-run]  delete ephemeral topics + their consumer groups
    ground_state_kafka.py verify   --baseline FILE                  expected topics present, no ephemeral topics left

By default `reset` deletes only ephemeral topics whose Ritual Dis does not list (orphans). `--all` deletes every ephemeral topic and is
for use AFTER Dis's own state was reset, since a Ritual Dis still lists would lose its topic.

Stdlib only; talks to Kafka through `docker exec <container> kafka-*.sh`.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import urllib.request
from pathlib import Path
from typing import Dict, List, Optional, Sequence

DEFAULT_CONTAINER = "docker-kafka-1"
BIN = "/opt/kafka/bin"
BOOTSTRAP = "localhost:9092"
EPHEMERAL_PREFIXES = ("dis.local.ritual.", "dis.local.coire.")
_RITUAL_TOPIC = re.compile(r"^dis\.local\.ritual\.([0-9a-fA-F-]{36})$")
_RITUAL_GROUP = re.compile(r"^ritual-([0-9a-fA-F-]{36})-")


class KafkaError(RuntimeError):
    pass


def is_ephemeral_topic(name: str, prefixes: Sequence[str] = EPHEMERAL_PREFIXES) -> bool:
    return any(name.startswith(p) for p in prefixes)


def ritual_id_of_topic(name: str) -> Optional[str]:
    m = _RITUAL_TOPIC.match(name)
    return m.group(1).lower() if m else None


def ritual_id_of_group(name: str) -> Optional[str]:
    m = _RITUAL_GROUP.match(name)
    return m.group(1).lower() if m else None


def parse_describe_line(line: str) -> Optional[Dict]:
    """`Topic: X  TopicId: ..  PartitionCount: 50  ReplicationFactor: 1  Configs: a=b,c=d` -> a dict (tab-separated)."""
    if not line.startswith("Topic:") or "PartitionCount:" not in line:
        return None
    fields = {}
    for part in line.split("\t"):
        if ":" in part:
            k, v = part.split(":", 1)
            fields[k.strip()] = v.strip()
    configs = {}
    for kv in filter(None, fields.get("Configs", "").split(",")):
        if "=" in kv:
            k, v = kv.split("=", 1)
            configs[k] = v
    return {
        "name": fields["Topic"],
        "partitions": int(fields["PartitionCount"]),
        "replication_factor": int(fields["ReplicationFactor"]),
        "configs": configs,
    }


def plan_reset(topics: Sequence[str], groups: Sequence[str], live_rituals: Sequence[str], delete_all: bool) -> Dict[str, List[str]]:
    """Which ephemeral topics and consumer groups to delete. Pure, so it is tested without Kafka."""
    live = {r.lower() for r in live_rituals}
    doomed_topics = []
    for t in topics:
        if not is_ephemeral_topic(t):
            continue
        rid = ritual_id_of_topic(t)
        # An ad hoc Coire channel has no Ritual to be orphaned from, so only --all removes it.
        if delete_all or (rid is not None and rid not in live):
            doomed_topics.append(t)
    doomed_groups = []
    for g in groups:
        rid = ritual_id_of_group(g)
        if rid is not None and (delete_all or rid not in live):
            doomed_groups.append(g)
    return {"topics": sorted(doomed_topics), "groups": sorted(doomed_groups)}


class Kafka:
    def __init__(self, container: str = DEFAULT_CONTAINER, bootstrap: str = BOOTSTRAP):
        self.container, self.bootstrap = container, bootstrap

    def _run(self, script: str, *args: str, check: bool = True) -> str:
        r = subprocess.run(
            ["docker", "exec", self.container, f"{BIN}/{script}", "--bootstrap-server", self.bootstrap, *args], capture_output=True, text=True
        )
        if check and r.returncode != 0:
            raise KafkaError((r.stderr or r.stdout).strip())
        return r.stdout

    def topics(self) -> List[str]:
        return sorted(t for t in self._run("kafka-topics.sh", "--list").splitlines() if t.strip())

    def describe(self, topic: str) -> Dict:
        for line in self._run("kafka-topics.sh", "--describe", "--topic", topic).splitlines():
            parsed = parse_describe_line(line)
            if parsed:
                return parsed
        raise KafkaError(f"could not describe topic {topic}")

    def groups(self) -> List[str]:
        return sorted(g for g in self._run("kafka-consumer-groups.sh", "--list").splitlines() if g.strip())

    def delete_topics(self, names: Sequence[str]) -> None:
        for n in names:
            self._run("kafka-topics.sh", "--delete", "--topic", n)

    def delete_groups(self, names: Sequence[str]) -> None:
        for n in names:
            self._run("kafka-consumer-groups.sh", "--delete", "--group", n, check=False)  # a group with live members refuses; fine

    def create_topic(self, spec: Dict) -> None:
        args = ["--create", "--if-not-exists", "--topic", spec["name"], "--partitions", str(spec["partitions"]), "--replication-factor", str(spec["replication_factor"])]
        for k, v in (spec.get("configs") or {}).items():
            args += ["--config", f"{k}={v}"]
        self._run("kafka-topics.sh", *args)


def live_ritual_ids(dis_url: str) -> List[str]:
    with urllib.request.urlopen(dis_url.rstrip("/") + "/ritual", timeout=10) as r:
        body = json.load(r)
    return [x["ritual_id"] for x in body.get("rituals", []) if str(x.get("state", "")).lower() == "active"]


def snapshot(k: Kafka) -> Dict:
    expected = [k.describe(t) for t in k.topics() if not is_ephemeral_topic(t)]
    return {"version": 1, "ephemeral_prefixes": list(EPHEMERAL_PREFIXES), "expected_topics": expected}


def status(k: Kafka, dis_url: Optional[str]) -> Dict:
    topics = k.topics()
    eph = [t for t in topics if is_ephemeral_topic(t)]
    out: Dict = {"topics": len(topics), "ephemeral_topics": len(eph), "consumer_groups": len(k.groups())}
    if dis_url:
        live = live_ritual_ids(dis_url)
        out["dis_active_rituals"] = len(live)
        out["ephemeral_topics_without_a_live_ritual"] = len(plan_reset(topics, [], live, False)["topics"])
    return out


def reset(k: Kafka, dis_url: Optional[str], delete_all: bool, dry_run: bool) -> Dict:
    live = live_ritual_ids(dis_url) if dis_url else []
    if not dis_url and not delete_all:
        raise KafkaError("need --dis URL to know which Rituals are live (or --all after Dis was reset)")
    plan = plan_reset(k.topics(), k.groups(), live, delete_all)
    if not dry_run:
        k.delete_groups(plan["groups"])
        k.delete_topics(plan["topics"])
    return {"dry_run": dry_run, "delete_all": delete_all, "topics": len(plan["topics"]), "groups": len(plan["groups"]), "sample": plan["topics"][:5]}


def verify(k: Kafka, baseline: Dict, allow_ephemeral: bool = False, live_rituals: Optional[Sequence[str]] = None) -> Dict:
    """Expected topics exist, and no ephemeral topic is left over. When the live Rituals are known (`live_rituals`), a topic a live
    Ritual owns is not left over: the composed analysts are permanent residents with topics of their own."""
    topics = k.topics()
    have = set(topics)
    missing = [t["name"] for t in baseline.get("expected_topics", []) if t["name"] not in have]
    if live_rituals is not None:
        eph = plan_reset(topics, [], live_rituals, False)["topics"]  # ephemeral topics no live Ritual owns
        label = "no ephemeral topics without a live Ritual"
    else:
        eph = [t for t in have if is_ephemeral_topic(t, baseline.get("ephemeral_prefixes", EPHEMERAL_PREFIXES))]
        label = "no ephemeral (ritual/coire) topics left"
    checks = [
        {"check": "expected topics exist", "ok": not missing, "detail": missing},
        {"check": label, "ok": allow_ephemeral or not eph, "detail": len(eph)},
    ]
    return {"ok": all(c["ok"] for c in checks), "checks": checks}


def main(argv: Optional[Sequence[str]] = None) -> int:
    p = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    p.add_argument("--container", default=DEFAULT_CONTAINER)
    p.add_argument("--dis", default=None, help="Dis base URL, e.g. http://127.0.0.1:8080")
    sub = p.add_subparsers(dest="cmd", required=True)
    s = sub.add_parser("snapshot")
    s.add_argument("--out", required=True)
    sub.add_parser("status")
    r = sub.add_parser("reset")
    r.add_argument("--all", action="store_true")
    r.add_argument("--dry-run", action="store_true")
    v = sub.add_parser("verify")
    v.add_argument("--baseline", required=True)
    e = sub.add_parser("ensure", help="create the baseline's expected topics that are missing")
    e.add_argument("--baseline", required=True)
    args = p.parse_args(argv)
    k = Kafka(args.container)
    try:
        if args.cmd == "snapshot":
            out = snapshot(k)
            Path(args.out).parent.mkdir(parents=True, exist_ok=True)
            Path(args.out).write_text(json.dumps(out, indent=2), encoding="utf-8")
            out = {"written": args.out, "expected_topics": len(out["expected_topics"])}
        elif args.cmd == "status":
            out = status(k, args.dis)
        elif args.cmd == "reset":
            out = reset(k, args.dis, args.all, args.dry_run)
        elif args.cmd == "ensure":
            base = json.loads(Path(args.baseline).read_text(encoding="utf-8"))
            have = set(k.topics())
            made = []
            for t in base.get("expected_topics", []):
                if t["name"] not in have:
                    k.create_topic(t)
                    made.append(t["name"])
            out = {"created": made}
        else:
            out = verify(
                k,
                json.loads(Path(args.baseline).read_text(encoding="utf-8")),
                live_rituals=live_ritual_ids(args.dis) if args.dis else None,
            )
    except (KafkaError, OSError) as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1
    print(json.dumps(out, indent=2))
    return 0 if out.get("ok", True) else 2


if __name__ == "__main__":
    sys.exit(main())
