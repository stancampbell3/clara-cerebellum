"""Pure-logic tests for the ground-state host scripts (no docker, no network): python3 -m pytest scripts/test_ground_state_scripts.py"""

import json
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).parent))
import ground_state_kafka as gk  # noqa: E402
import ground_state_pg as gp  # noqa: E402

WS = "e2fd2658-c252-4eed-94ad-40051fea2209"
R1, R2 = "11111111-1111-1111-1111-111111111111", "22222222-2222-2222-2222-222222222222"


# ── kafka ────────────────────────────────────────────────────────────────────


def test_topic_and_group_names_map_to_ritual_ids():
    assert gk.ritual_id_of_topic(f"dis.local.ritual.{R1}") == R1
    assert gk.ritual_id_of_topic("dis.local.coire.snek.foo") is None
    assert gk.ritual_id_of_group(f"ritual-{R2}-http://lildaemon:6666#n1") == R2
    assert gk.ritual_id_of_group("some-other-group") is None
    assert gk.is_ephemeral_topic("dis.local.coire.x") and not gk.is_ephemeral_topic("__consumer_offsets")


def test_describe_line_parsing():
    line = "Topic: t1\tTopicId: abc\tPartitionCount: 3\tReplicationFactor: 1\tConfigs: cleanup.policy=compact,segment.bytes=1"
    assert gk.parse_describe_line(line) == {
        "name": "t1",
        "partitions": 3,
        "replication_factor": 1,
        "configs": {"cleanup.policy": "compact", "segment.bytes": "1"},
    }
    assert gk.parse_describe_line("\tTopic: t1\tPartition: 0\tLeader: 1") is None


def test_default_reset_only_deletes_topics_no_live_ritual_owns_and_never_expected_topics():
    topics = ["__consumer_offsets", f"dis.local.ritual.{R1}", f"dis.local.ritual.{R2}", "dis.local.coire.snek.x"]
    groups = [f"ritual-{R1}-a", f"ritual-{R2}-b", "unrelated"]
    plan = gk.plan_reset(topics, groups, live_rituals=[R1], delete_all=False)
    assert plan == {"topics": [f"dis.local.ritual.{R2}"], "groups": [f"ritual-{R2}-b"]}


def test_delete_all_takes_every_ephemeral_topic_but_not_the_expected_ones():
    topics = ["__consumer_offsets", f"dis.local.ritual.{R1}", "dis.local.coire.snek.x", "custom.topic"]
    plan = gk.plan_reset(topics, [f"ritual-{R1}-a"], live_rituals=[R1], delete_all=True)
    assert plan["topics"] == ["dis.local.coire.snek.x", f"dis.local.ritual.{R1}"]
    assert plan["groups"] == [f"ritual-{R1}-a"]


def test_verify_flags_missing_expected_and_leftover_ephemeral(monkeypatch):
    class K:
        def topics(self):
            return ["__consumer_offsets", f"dis.local.ritual.{R1}"]

    base = {"expected_topics": [{"name": "__consumer_offsets"}, {"name": "needed.topic"}]}
    r = gk.verify(K(), base)
    failed = {c["check"] for c in r["checks"] if not c["ok"]}
    assert not r["ok"] and failed == {"expected topics exist", "no ephemeral (ritual/coire) topics left"}
    assert gk.verify(K(), {"expected_topics": [{"name": "__consumer_offsets"}]}, allow_ephemeral=True)["ok"]


def test_reset_without_dis_or_all_refuses():
    class K:
        def topics(self):
            return []

        def groups(self):
            return []

    with pytest.raises(gk.KafkaError):
        gk.reset(K(), None, delete_all=False, dry_run=True)


# ── postgres ─────────────────────────────────────────────────────────────────


def test_workspace_id_is_validated_and_names_are_derived():
    assert gp._validate_id(WS.upper()) == WS
    with pytest.raises(gp.PgError):
        gp._validate_id("1'; drop table documents; --")
    assert gp.vectors_table(WS) == "eq_eq_default_ws_e2fd2658_vectors"
    assert gp.pub("documents") == "public.documents"  # never the `edgequake` schema's compatibility views


def test_sql_literals_escape_quotes():
    assert gp.q("it's") == "'it''s'"


def test_kv_predicate_covers_workspace_keys_and_document_keys():
    sql = gp._kv_predicate(WS, ["doc-a", "doc-b"])
    assert f"wsdoc:{WS}:%" in sql and f"doc:hash:{WS}:%" in sql
    assert "'doc-a'" in sql and "'doc-b'" in sql and "staging:" in sql
    assert "ARRAY[]::text[]" in gp._kv_predicate(WS, [])


def _manifest():
    files = {
        "documents": {"file": "pg/documents.bin", "kind": "table"},
        "chunk_entity_links": {"file": "pg/chunk_entity_links.bin", "kind": "table"},
        gp.KV: {"file": f"pg/{gp.KV}.bin", "kind": "kv"},
        gp.vectors_table(WS): {"file": "pg/v.bin", "kind": "vectors"},
        f"{gp.GRAPH}.Node": {"file": "pg/graph_node.txt", "kind": "graph_node"},
        f"{gp.GRAPH}.EDGE": {"file": "pg/graph_edge.txt", "kind": "graph_edge"},
    }
    return {"workspace_id": WS, "document_ids": ["doc-a"], "files": files, "counts": {}}


def test_restore_is_one_transaction_that_deletes_this_workspace_only_then_loads():
    script = gp.restore_script(_manifest(), "/tmp/gs_restore")
    lines = script.splitlines()
    assert lines[0] == "\\set ON_ERROR_STOP on" and lines[1] == "BEGIN;" and lines[-1] == "COMMIT;"
    assert script.count("BEGIN;") == 1 and script.count("COMMIT;") == 1
    # every delete is scoped to the workspace (or to this workspace's kv/vector/graph rows), and tables are schema-qualified
    for l in lines:
        if l.startswith("DELETE FROM public.") and "eq_eq_default_kv" not in l:
            assert f"workspace_id = '{WS}'" in l, l
    assert "DELETE FROM public.documents" in script and "\\copy public.documents from '/tmp/gs_restore/pg/documents.bin'" in script
    assert f"TRUNCATE public.{gp.vectors_table(WS)};" in script  # the workspace's OWN vector table, not a shared one
    # dependents are deleted before documents and loaded after it
    assert script.index("DELETE FROM public.chunk_entity_links") < script.index("DELETE FROM public.documents")
    assert script.index("\\copy public.documents") < script.index("\\copy public.chunk_entity_links")
    # kv is cleared while `documents` still exists (its predicate reads it), graph rows before nodes' edges are re-added
    assert script.index("DELETE FROM public.eq_eq_default_kv") < script.index("DELETE FROM public.documents")
    assert script.index('DELETE FROM eq_eq_default_graph."EDGE"') < script.index('DELETE FROM eq_eq_default_graph."Node"')
    assert 'INSERT INTO eq_eq_default_graph."Node"' in script and "::graphid" in script and "::agtype" in script


def test_only_finished_documents_count_as_settled():
    assert all(gp.is_terminal_status(s) for s in ("indexed", "Completed", "failed", "partial_failure", "cancelled"))
    assert not any(gp.is_terminal_status(s) for s in ("pending", "processing", "queued", "chunking", "extracting"))


def test_busy_documents_lists_only_unfinished_ones():
    class FakePg:
        def lines(self, sql):
            return ["a.md [indexed]", "b.md [processing]", "c [failed]", "d.md [pending]"]

    assert gp.busy_documents(FakePg(), WS) == ["b.md [processing]", "d.md [pending]"]


def test_export_refuses_while_documents_are_still_ingesting(tmp_path):
    class FakePg:
        def lines(self, sql):
            return ["x.md [processing]"]

    with pytest.raises(gp.PgError, match="still ingesting"):
        gp.export(FakePg(), WS, tmp_path)
