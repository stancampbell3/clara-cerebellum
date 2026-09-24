#!/usr/bin/env python3
"""
Workspace-scoped snapshot and restore of Edgequake's Postgres data, so a baseline can be brought back EXACTLY (entities, graph,
vectors and all) without re-running the LLM extraction that ingestion costs and that is not deterministic.

Edgequake is multi-tenant (other workspaces live in the same database), so this never dumps or restores the whole database. It moves
only ONE workspace's rows:

    relational tables carrying workspace_id      COPY ... WHERE workspace_id = W          (binary)
    eq_eq_default_kv (chunks, lineage, hashes)   keyed by document id / workspace id, see _kv_predicate
    eq_eq_default_ws_<W8>_vectors (+ _stats)     the workspace's own vector table, whole  (binary)
    eq_eq_default_graph Node / EDGE (Apache AGE) rows whose properties carry workspace_id (text staging)

Restore replaces that workspace's rows inside ONE transaction (all or nothing). The workspace must already exist with the SAME id
(rows embed it), so reset must EMPTY the workspace, never delete and recreate it.

    ground_state_pg.py export  --workspace-id W --out DIR   [--container edgequake-postgres]
    ground_state_pg.py restore --from DIR                   [--container ...] [--dry-run]
    ground_state_pg.py counts  --workspace-id W             [--container ...]

Stdlib only; talks to Postgres through `docker exec <container> psql`.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Dict, List, Optional, Sequence

DEFAULT_CONTAINER = "edgequake-postgres"
DEFAULT_USER = "edgequake"
DEFAULT_DB = "edgequake"
GRAPH = "eq_eq_default_graph"
KV = "eq_eq_default_kv"
# Knowledge tables (workspace_id column). Job history, conversations, keys and audit tables are deliberately not part of a baseline.
KNOWLEDGE_TABLES = [
    "documents",
    "chunks",
    "entities",
    "relationships",
    "chunk_entity_links",
    "chunk_relation_links",
    "failed_chunks",
    "folders",
    "document_originals",
    "document_mm_assets",
    "pdf_documents",
    "eq_eq_default_vectors",
]
AGE_PRE = "LOAD 'age'; SET search_path = ag_catalog, \"$user\", public;"


class PgError(RuntimeError):
    pass


class Pg:
    def __init__(self, container: str = DEFAULT_CONTAINER, user: str = DEFAULT_USER, db: str = DEFAULT_DB):
        self.container, self.user, self.db = container, user, db

    def _base(self, interactive: bool = False) -> List[str]:
        cmd = ["docker", "exec"] + (["-i"] if interactive else []) + [self.container, "psql", "-U", self.user, "-d", self.db, "-X", "-q", "-v", "ON_ERROR_STOP=1"]
        return cmd

    def scalar(self, sql: str) -> str:
        r = subprocess.run(self._base() + ["-At", "-c", sql], capture_output=True, text=True)
        if r.returncode != 0:
            raise PgError(r.stderr.strip() or "psql failed")
        return r.stdout.strip()

    def lines(self, sql: str) -> List[str]:
        out = self.scalar(sql)
        return out.splitlines() if out else []

    def copy_out(self, select_sql: str, dest: Path, binary: bool, preamble: str = "") -> int:
        """COPY (select) TO STDOUT into `dest`; returns the byte count."""
        fmt = "FORMAT binary" if binary else "FORMAT text"
        sql = f"{preamble} COPY ({select_sql}) TO STDOUT ({fmt});"
        with dest.open("wb") as fh:
            r = subprocess.run(self._base() + ["-c", sql], stdout=fh, stderr=subprocess.PIPE)
        if r.returncode != 0:
            dest.unlink(missing_ok=True)
            raise PgError(r.stderr.decode(errors="replace").strip() or "psql copy failed")
        return dest.stat().st_size

    def run_script(self, script: str) -> None:
        r = subprocess.run(self._base(interactive=True) + ["-f", "-"], input=script, capture_output=True, text=True)
        if r.returncode != 0:
            raise PgError(r.stderr.strip() or "psql script failed")


def pub(table: str) -> str:
    """Schema-qualify a table. The `edgequake` schema (the DB user's own, first in the search path) holds compatibility VIEWS with
    fewer columns (documents, chunks, entities, ...), so an unqualified name silently reads the wrong relation."""
    return f"public.{table}"


def q(s: str) -> str:
    """SQL string literal."""
    return "'" + s.replace("'", "''") + "'"


def _validate_id(workspace_id: str) -> str:
    import re

    if not re.fullmatch(r"[0-9a-fA-F-]{36}", workspace_id):
        raise PgError(f"not a workspace id (expected a UUID): {workspace_id!r}")
    return workspace_id.lower()


def vectors_table(workspace_id: str) -> str:
    return f"eq_eq_default_ws_{workspace_id.replace('-', '')[:8]}_vectors"


def _kv_predicate(workspace_id: str, doc_ids: Sequence[str], alias: str = "k") -> str:
    """The kv rows that belong to a workspace: hash-index and wsdoc keys embed the workspace id; everything else is keyed by document id."""
    ids = "ARRAY[" + ",".join(q(d) for d in doc_ids) + "]::text[]" if doc_ids else "ARRAY[]::text[]"
    return (
        f"({alias}.key LIKE {q('wsdoc:' + workspace_id + ':%')} OR {alias}.key LIKE {q('doc:hash:' + workspace_id + ':%')} "
        f"OR EXISTS (SELECT 1 FROM unnest({ids}) AS d(id) WHERE {alias}.key LIKE d.id || '-%' OR {alias}.key LIKE 'staging:' || d.id || '-%'))"
    )


def _graph_filter(workspace_id: str) -> str:
    return f"(properties::jsonb)->>'workspace_id' = {q(workspace_id)}"


def _existing_tables(pg: Pg, names: Sequence[str]) -> List[str]:
    have = set(pg.lines("select table_name from information_schema.tables where table_schema='public'"))
    return [n for n in names if n in have]


def counts(pg: Pg, workspace_id: str) -> Dict[str, int]:
    """Row counts of everything a snapshot of this workspace covers (used to check an export against a restore)."""
    ws = _validate_id(workspace_id)
    out: Dict[str, int] = {}
    doc_ids = pg.lines(f"select id::text from public.documents where workspace_id = {q(ws)}")
    for t in _existing_tables(pg, KNOWLEDGE_TABLES):
        out[t] = int(pg.scalar(f"select count(*) from {pub(t)} where workspace_id = {q(ws)}"))
    out[KV] = int(pg.scalar(f"select count(*) from {pub(KV)} k where {_kv_predicate(ws, doc_ids)}"))
    vt = vectors_table(ws)
    if _existing_tables(pg, [vt]):
        out[vt] = int(pg.scalar(f"select count(*) from {pub(vt)}"))
    out[f"{GRAPH}.Node"] = int(pg.scalar(f"{AGE_PRE} select count(*) from {GRAPH}.\"Node\" where {_graph_filter(ws)}").splitlines()[-1])
    out[f"{GRAPH}.EDGE"] = int(pg.scalar(f"{AGE_PRE} select count(*) from {GRAPH}.\"EDGE\" where {_graph_filter(ws)}").splitlines()[-1])
    return out


TERMINAL_DOCUMENT_STATUSES = frozenset({"indexed", "completed", "failed", "partial_failure", "cancelled"})


def is_terminal_status(status: str) -> bool:
    return status.lower() in TERMINAL_DOCUMENT_STATUSES


def busy_documents(pg: Pg, workspace_id: str) -> List[str]:
    """Titles of documents still being ingested. Exporting while ingestion runs would capture a half-written workspace, because the
    tables are copied one after another rather than in one snapshot."""
    ws = _validate_id(workspace_id)
    rows = pg.lines(f"select coalesce(title, id::text) || ' [' || status::text || ']' from public.documents where workspace_id = {q(ws)}")
    return [r for r in rows if not is_terminal_status(r.rsplit("[", 1)[-1].rstrip("]"))]


def _stale_hash_index_sql(ws: str) -> str:
    """kv rows of the content-hash dedup index (`doc:hash:<ws>:<sha>` -> "<document id>") whose document no longer exists. Left behind,
    they make Edgequake treat re-ingested identical content as a duplicate of a deleted document."""
    return (
        f"k.key LIKE {q('doc:hash:' + ws + ':%')} AND NOT EXISTS "
        f"(SELECT 1 FROM public.documents d WHERE d.id::text = trim(both '\"' from k.value::text))"
    )


def _stale_entity_vectors_sql(ws: str) -> str:
    """Entity vectors carry no document id, so the document-based check cannot see them: they are stale when the workspace's graph
    holds no node for the entity (node ids are `<workspace>::<entity name>`)."""
    return (
        f"v.document_id IS NULL AND v.id LIKE 'entity:%' AND NOT EXISTS "
        f"(SELECT 1 FROM {GRAPH}.\"Node\" n WHERE n.eq_node_id = {q(ws + '::')} || substr(v.id, 8))"
    )


def orphans(pg: Pg, workspace_id: str) -> Dict:
    """Rows that belong to documents which no longer exist in this workspace: leftovers of an earlier delete.

    Edgequake's document delete cascades through the graph but (measured 2026-09-24 on assistant.general after a manual drop) can leave
    the workspace's vector rows and key/value rows behind, so retrieval can cite text from documents that are gone. A baseline must
    not carry them. Vector rows and kv rows are attributed to this workspace only through evidence: their document id appears in this
    workspace's own vector table or in a `wsdoc:<workspace>:<document>` key.
    """
    ws = _validate_id(workspace_id)
    vt = pub(vectors_table(ws))
    ids = pg.lines(
        f"select distinct document_id from {vt} v where v.document_id is not null and not exists "
        f"(select 1 from public.documents d where d.id::text = v.document_id) "
        f"union select split_part(k.key, ':', 3) from {pub(KV)} k where k.key like {q('wsdoc:' + ws + ':%')} and not exists "
        f"(select 1 from public.documents d where d.id::text = split_part(k.key, ':', 3))"
    )
    arr = "ARRAY[" + ",".join(q(i) for i in ids) + "]::text[]" if ids else "ARRAY[]::text[]"
    vec_rows = int(pg.scalar(f"select count(*) from {vt} where document_id = ANY({arr})"))
    kv_rows = int(
        pg.scalar(
            f"select count(*) from {pub(KV)} k where k.key like {q('wsdoc:' + ws + ':%')} and split_part(k.key, ':', 3) = ANY({arr}) "
            f"or exists (select 1 from unnest({arr}) as d(id) where k.key like d.id || '-%' or k.key like 'staging:' || d.id || '-%')"
        )
    )
    graph = pg.scalar(
        f"{AGE_PRE} select count(*) from {GRAPH}.\"Node\" n where {_graph_filter(ws)} and not exists "
        f"(select 1 from public.documents d where d.id::text = n.properties::jsonb->>'source_document_id')"
    ).splitlines()[-1]
    hash_rows = int(pg.scalar(f"select count(*) from {pub(KV)} k where {_stale_hash_index_sql(ws)}"))
    entity_vecs = int(
        pg.scalar(f"{AGE_PRE} select count(*) from {vt} v where {_stale_entity_vectors_sql(ws)}").splitlines()[-1]
    )
    return {
        "workspace_id": ws,
        "orphan_document_ids": len(ids),
        "vector_rows": vec_rows,
        "kv_rows": kv_rows,
        "stale_hash_index_rows": hash_rows,
        "entity_vectors_without_node": entity_vecs,
        "graph_nodes_without_document": int(graph),
        "clean": vec_rows == 0 and kv_rows == 0 and hash_rows == 0 and entity_vecs == 0 and int(graph) == 0,
        "_ids": ids,
    }


def clean_orphans(pg: Pg, workspace_id: str, dry_run: bool = False) -> Dict:
    """Delete the orphaned vector and kv rows (never graph nodes: those are reported only) in ONE transaction."""
    ws = _validate_id(workspace_id)
    report = orphans(pg, ws)
    ids = report.pop("_ids")
    report["dry_run"] = dry_run
    if dry_run or report["clean"]:
        return report
    arr = "ARRAY[" + ",".join(q(i) for i in ids) + "]::text[]"
    vt = pub(vectors_table(ws))
    pg.run_script(
        "\\set ON_ERROR_STOP on\nBEGIN;\n"
        f"{AGE_PRE}\n"
        f"DELETE FROM {pub(KV)} k WHERE {_stale_hash_index_sql(ws)};\n"
        f"DELETE FROM {vt} v WHERE {_stale_entity_vectors_sql(ws)};\n"
        f"DELETE FROM {vt} WHERE document_id = ANY({arr});\n"
        f"DELETE FROM {pub(KV)} k WHERE (k.key LIKE {q('wsdoc:' + ws + ':%')} AND split_part(k.key, ':', 3) = ANY({arr})) "
        f"OR EXISTS (SELECT 1 FROM unnest({arr}) AS d(id) WHERE k.key LIKE d.id || '-%' OR k.key LIKE 'staging:' || d.id || '-%');\n"
        "COMMIT;\n"
    )
    after = orphans(pg, ws)
    after.pop("_ids")
    report["after"] = after
    return report


def export(pg: Pg, workspace_id: str, out_dir: Path, allow_orphans: bool = False, allow_busy: bool = False) -> Dict:
    ws = _validate_id(workspace_id)
    busy = [] if allow_busy else busy_documents(pg, ws)
    if busy:
        raise PgError(f"the workspace is still ingesting ({len(busy)} document(s): {busy[:3]}); snapshot it once ingestion has finished, or pass --allow-busy")
    if not allow_orphans:
        o = orphans(pg, ws)
        if not o["clean"]:
            o.pop("_ids")
            raise PgError(
                "refusing to snapshot a workspace that carries orphaned rows from deleted documents (a baseline must not preserve "
                f"them): {o}. Run `clean-orphans` first, or pass --allow-orphans."
            )
    out = Path(out_dir)
    (out / "pg").mkdir(parents=True, exist_ok=True)
    if pg.scalar(f"select count(*) from public.workspaces where workspace_id = {q(ws)}") != "1":
        raise PgError(f"workspace {ws} does not exist in this Edgequake database")
    doc_ids = pg.lines(f"select id::text from public.documents where workspace_id = {q(ws)} order by id")
    files: Dict[str, Dict] = {}
    for t in _existing_tables(pg, KNOWLEDGE_TABLES):
        size = pg.copy_out(f"select * from {pub(t)} where workspace_id = {q(ws)}", out / "pg" / f"{t}.bin", binary=True)
        files[t] = {"file": f"pg/{t}.bin", "kind": "table", "bytes": size}
    size = pg.copy_out(f"select k.* from {pub(KV)} k where {_kv_predicate(ws, doc_ids)}", out / "pg" / f"{KV}.bin", binary=True)
    files[KV] = {"file": f"pg/{KV}.bin", "kind": "kv", "bytes": size}
    vt = vectors_table(ws)
    for t in _existing_tables(pg, [vt, vt + "_stats"]):
        size = pg.copy_out(f"select * from {pub(t)}", out / "pg" / f"{t}.bin", binary=True)
        files[t] = {"file": f"pg/{t}.bin", "kind": "vectors", "bytes": size}
    pg.copy_out(
        f"select id::text, eq_node_id, properties::text from {GRAPH}.\"Node\" where {_graph_filter(ws)}",
        out / "pg" / "graph_node.txt",
        binary=False,
        preamble=AGE_PRE,
    )
    pg.copy_out(
        f"select id::text, start_id::text, end_id::text, eq_source_id, eq_target_id, properties::text from {GRAPH}.\"EDGE\" where {_graph_filter(ws)}",
        out / "pg" / "graph_edge.txt",
        binary=False,
        preamble=AGE_PRE,
    )
    files[f"{GRAPH}.Node"] = {"file": "pg/graph_node.txt", "kind": "graph_node"}
    files[f"{GRAPH}.EDGE"] = {"file": "pg/graph_edge.txt", "kind": "graph_edge"}
    manifest = {"version": 1, "workspace_id": ws, "document_ids": doc_ids, "files": files, "counts": counts(pg, ws)}
    (out / "pg" / "manifest.json").write_text(json.dumps(manifest, indent=2), encoding="utf-8")
    return manifest


def restore_script(manifest: Dict, remote_dir: str) -> str:
    """One transaction: delete the workspace's current rows, load the snapshot's."""
    ws = manifest["workspace_id"]
    doc_ids = manifest["document_ids"]
    files = manifest["files"]
    s: List[str] = ["\\set ON_ERROR_STOP on", "BEGIN;", AGE_PRE]
    # kv first: its predicate reads `documents`, which is about to change. The snapshot's own document ids cover rows whose
    # documents are already gone (reset deletes documents through the API but can leave kv rows behind).
    live_ids = f"(SELECT array_agg(id::text) FROM public.documents WHERE workspace_id = {q(ws)})"
    s.append(
        f"DELETE FROM {pub(KV)} k WHERE {_kv_predicate(ws, doc_ids)} OR EXISTS (SELECT 1 FROM unnest(COALESCE({live_ids}, ARRAY[]::text[])) AS d(id) "
        f"WHERE k.key LIKE d.id || '-%' OR k.key LIKE 'staging:' || d.id || '-%');"
    )
    s.append(f"DELETE FROM {GRAPH}.\"EDGE\" WHERE {_graph_filter(ws)};")
    s.append(f"DELETE FROM {GRAPH}.\"Node\" WHERE {_graph_filter(ws)};")
    # Dependents before documents (links reference chunks/entities), and the tables in FK-safe reverse order of loading.
    order = [t for t in KNOWLEDGE_TABLES if t in files]
    for t in reversed(order):
        s.append(f"DELETE FROM {pub(t)} WHERE workspace_id = {q(ws)};")
    for t, meta in files.items():
        if meta["kind"] == "vectors":
            s.append(f"TRUNCATE {pub(t)};")
    for t in order:
        s.append(f"\\copy {pub(t)} from '{remote_dir}/{files[t]['file']}' with (format binary)")
    s.append(f"\\copy {pub(KV)} from '{remote_dir}/{files[KV]['file']}' with (format binary)")
    for t, meta in files.items():
        if meta["kind"] == "vectors":
            s.append(f"\\copy {pub(t)} from '{remote_dir}/{meta['file']}' with (format binary)")
    s += [
        "CREATE TEMP TABLE _gs_node (id text, eq_node_id text, properties text) ON COMMIT DROP;",
        "CREATE TEMP TABLE _gs_edge (id text, start_id text, end_id text, eq_source_id text, eq_target_id text, properties text) ON COMMIT DROP;",
        f"\\copy _gs_node from '{remote_dir}/{files[GRAPH + '.Node']['file']}'",
        f"\\copy _gs_edge from '{remote_dir}/{files[GRAPH + '.EDGE']['file']}'",
        f"INSERT INTO {GRAPH}.\"Node\" (id, properties, eq_node_id) SELECT id::graphid, properties::agtype, eq_node_id FROM _gs_node;",
        f"INSERT INTO {GRAPH}.\"EDGE\" (id, start_id, end_id, properties, eq_source_id, eq_target_id) "
        "SELECT id::graphid, start_id::graphid, end_id::graphid, properties::agtype, eq_source_id, eq_target_id FROM _gs_edge;",
        "COMMIT;",
    ]
    return "\n".join(s) + "\n"


def restore(pg: Pg, from_dir: Path, dry_run: bool = False) -> Dict:
    src = Path(from_dir)
    manifest = json.loads((src / "pg" / "manifest.json").read_text(encoding="utf-8"))
    ws = manifest["workspace_id"]
    if pg.scalar(f"select count(*) from public.workspaces where workspace_id = {q(ws)}") != "1":
        raise PgError(
            f"workspace {ws} does not exist here. A restore needs the SAME workspace id (the rows embed it): "
            "empty the workspace, do not delete and recreate it."
        )
    vt = vectors_table(ws)
    if manifest["files"].get(vt) and not _existing_tables(pg, [vt]):
        raise PgError(f"the workspace's vector table {vt} is missing; recreate the workspace through Edgequake first")
    script = restore_script(manifest, "/tmp/gs_restore")
    if dry_run:
        return {"dry_run": True, "workspace_id": ws, "expected_counts": manifest["counts"], "statements": script.count("\n")}
    subprocess.run(["docker", "exec", pg.container, "rm", "-rf", "/tmp/gs_restore"], check=False)
    r = subprocess.run(["docker", "exec", pg.container, "mkdir", "-p", "/tmp/gs_restore"], capture_output=True, text=True)
    if r.returncode != 0:
        raise PgError(r.stderr)
    r = subprocess.run(["docker", "cp", str(src / "pg"), f"{pg.container}:/tmp/gs_restore/pg"], capture_output=True, text=True)
    if r.returncode != 0:
        raise PgError(r.stderr)
    try:
        pg.run_script(script)
    finally:
        subprocess.run(["docker", "exec", pg.container, "rm", "-rf", "/tmp/gs_restore"], check=False)
    after = counts(pg, ws)
    mismatch = {k: {"want": v, "have": after.get(k)} for k, v in manifest["counts"].items() if after.get(k) != v}
    return {"workspace_id": ws, "restored": after, "matches_snapshot": not mismatch, "mismatch": mismatch}


def main(argv: Optional[Sequence[str]] = None) -> int:
    p = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    p.add_argument("--container", default=DEFAULT_CONTAINER)
    p.add_argument("--user", default=DEFAULT_USER)
    p.add_argument("--db", default=DEFAULT_DB)
    sub = p.add_subparsers(dest="cmd", required=True)
    e = sub.add_parser("export")
    e.add_argument("--workspace-id", required=True)
    e.add_argument("--out", required=True)
    e.add_argument("--allow-orphans", action="store_true")
    e.add_argument("--allow-busy", action="store_true")
    r = sub.add_parser("restore")
    r.add_argument("--from", dest="src", required=True)
    r.add_argument("--dry-run", action="store_true")
    c = sub.add_parser("counts")
    c.add_argument("--workspace-id", required=True)
    o = sub.add_parser("orphans", help="report vector/kv/graph rows left behind by deleted documents")
    o.add_argument("--workspace-id", required=True)
    co = sub.add_parser("clean-orphans", help="delete the orphaned vector and kv rows (graph nodes are only reported)")
    co.add_argument("--workspace-id", required=True)
    co.add_argument("--dry-run", action="store_true")
    args = p.parse_args(argv)
    pg = Pg(args.container, args.user, args.db)
    try:
        if args.cmd == "export":
            out = export(pg, args.workspace_id, Path(args.out), allow_orphans=args.allow_orphans, allow_busy=args.allow_busy)
            out = {"exported_to": args.out, "counts": out["counts"]}
        elif args.cmd == "restore":
            out = restore(pg, Path(args.src), dry_run=args.dry_run)
        elif args.cmd == "orphans":
            out = orphans(pg, args.workspace_id)
            out.pop("_ids")
        elif args.cmd == "clean-orphans":
            out = clean_orphans(pg, args.workspace_id, dry_run=args.dry_run)
        else:
            out = counts(pg, args.workspace_id)
    except PgError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1
    print(json.dumps(out, indent=2))
    return 0 if out.get("matches_snapshot", True) else 2


if __name__ == "__main__":
    sys.exit(main())
