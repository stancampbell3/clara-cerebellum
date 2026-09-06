# Clara stack data ERD

**Status:** Reference documentation, verified live 2026-09-06 against the
real schema/containers on `limbic`.

Three independent persistence layers make up the Clara stack's data, plus
Kafka as the connective tissue between two of them. There is no single
shared database — each system below owns its own storage, and the
relationships between them are correlated by shared ID conventions
(ritual ids, document ids), not enforced foreign keys.

## 1. CoireStore (clara-api, DuckDB — `data/coire.duckdb`)

```mermaid
erDiagram
    RITUALS {
        varchar ritual_id PK
        varchar dis_domain
        varchar status
        timestamp created_at
        timestamp terminated_at
    }
    COIRE_EVENTS {
        varchar event_id PK
        varchar session_id
        varchar kind
        timestamp created_at
    }
    DEDUCTION_SNAPSHOTS {
        varchar deduction_id PK
        varchar source_id FK
        json seed_clauses
        json constructs
        json tableau_state
    }
    TABLEAU_CHANGES {
        varchar change_id PK
        varchar deduction_id FK
        int cycle
        json snapshot
    }
    SOURCE_REGISTRY {
        varchar source_id PK
        varchar content_hash
        varchar kind
    }
    SOURCE_ARTIFACTS {
        varchar artifact_id PK
        varchar source_id FK
        varchar artifact_type
    }

    SOURCE_REGISTRY ||--o{ DEDUCTION_SNAPSHOTS : "seeds"
    SOURCE_REGISTRY ||--o{ SOURCE_ARTIFACTS : "generates"
    DEDUCTION_SNAPSHOTS ||--o{ TABLEAU_CHANGES : "records"
```

`RitualRegistry` write-throughs to `rituals` on every create/join/
terminate and reloads *all* rows (active and terminated) on every
clara-api boot — a past incident let 250 dead rituals accumulate because
`terminate()` never deletes the row (`clara-ritual/src/registry.rs`).
`FieryPitRegistry` (which FieryPits are currently registered/discoverable)
is deliberately **not** here — it's pure in-memory, re-populated by
heartbeat, nothing to persist.

## 2. lildaemon (Python, DuckDB — `lildaemon.duc`)

```mermaid
erDiagram
    USERS {
        varchar user_id PK
        varchar username
        varchar hashed_password
        varchar role
    }
    REPL_SESSIONS {
        varchar session_id PK
        varchar user_id FK
        timestamp created_at
        timestamp deleted_at
    }
    ASSISTANT_SESSIONS {
        varchar session_id PK
        varchar user_id FK
        json history
        json topics_touched
    }
    ASSISTANT_TOPICS {
        varchar topic_slug PK
        varchar query
        timestamp created_at
    }
    ASSISTANT_DOCUMENT_TAGS {
        varchar document_id PK
        varchar topic_slug FK
        varchar source_url
        timestamp ingested_at
        timestamp last_cited_at
    }
    ASSISTANT_PENDING_RESEARCH {
        varchar request_id PK
        varchar session_id FK
        varchar status
        varchar research_deduction_id
        varchar answer_deduction_id
    }
    RITUAL_CONFIGS {
        varchar ritual_config_id PK
        varchar user_id FK
        varchar status
        varchar ritual_id
        varchar kafka_bootstrap
        varchar dis_url
    }
    RITUAL_PARTICIPANTS {
        varchar participant_id PK
        varchar ritual_config_id FK
        varchar url
        varchar role
    }

    USERS ||--o{ REPL_SESSIONS : "owns"
    USERS ||--o{ ASSISTANT_SESSIONS : "owns"
    USERS ||--o{ RITUAL_CONFIGS : "authors"
    ASSISTANT_SESSIONS ||--o{ ASSISTANT_PENDING_RESEARCH : "queues"
    ASSISTANT_TOPICS ||--o{ ASSISTANT_DOCUMENT_TAGS : "tags"
    RITUAL_CONFIGS ||--o{ RITUAL_PARTICIPANTS : "lists"
```

lildaemon holds only a *tag* mirror of Edgequake documents
(`assistant_document_tags`) — never the documents/chunks/embeddings
themselves. `ritual_configs.ritual_id` correlates to CoireStore's
`rituals.ritual_id` once a config is activated, but that's a cross-system
reference, not an enforced FK (see §4).

## 3. Edgequake (Rust, Postgres 18 + pgvector 0.8.5 + Apache AGE 1.8.0 —
one named Docker volume `edgequake-pg-data`, own compose project)

```mermaid
erDiagram
    TENANTS {
        uuid tenant_id PK
        varchar slug
        jsonb settings
    }
    WORKSPACES {
        uuid workspace_id PK
        uuid tenant_id FK
        varchar slug
    }
    EQ_USERS {
        uuid user_id PK
        uuid tenant_id FK
        varchar email
        varchar role
    }
    MEMBERSHIPS {
        uuid tenant_id FK
        uuid workspace_id FK
        uuid user_id FK
    }
    DOCUMENTS {
        uuid document_id PK
        uuid tenant_id FK
        uuid workspace_id FK
        text content
        varchar content_hash
        varchar status
    }
    CHUNKS {
        uuid chunk_id PK
        uuid document_id FK
        uuid tenant_id FK
        uuid workspace_id FK
        text content
        vector embedding
    }
    ENTITIES {
        uuid entity_id PK
        uuid tenant_id FK
        uuid workspace_id FK
        vector embedding
    }
    RELATIONSHIPS {
        uuid relationship_id PK
        uuid tenant_id FK
        uuid workspace_id FK
        vector embedding
    }
    TASKS {
        uuid task_id PK
        uuid tenant_id FK
        uuid workspace_id FK
        varchar task_type
        varchar status
        jsonb payload
    }
    PDF_DOCUMENTS {
        uuid document_id FK
        bytea pdf_data
    }
    DOCUMENT_ORIGINALS {
        uuid document_id FK
        bytea original_data
    }
    DOCUMENT_MM_ASSETS {
        uuid document_id FK
        bytea asset_data
    }
    CONVERSATIONS {
        uuid conversation_id PK
        uuid tenant_id FK
        uuid user_id FK
    }
    MESSAGES {
        uuid message_id PK
        uuid conversation_id FK
    }

    TENANTS ||--o{ WORKSPACES : "hosts"
    TENANTS ||--o{ EQ_USERS : "has"
    WORKSPACES ||--o{ MEMBERSHIPS : "grants"
    EQ_USERS ||--o{ MEMBERSHIPS : "holds"
    WORKSPACES ||--o{ DOCUMENTS : "contains"
    DOCUMENTS ||--o{ CHUNKS : "splits into"
    DOCUMENTS ||--o| PDF_DOCUMENTS : "raw PDF bytes"
    DOCUMENTS ||--o| DOCUMENT_ORIGINALS : "raw upload bytes"
    DOCUMENTS ||--o{ DOCUMENT_MM_ASSETS : "extracted assets"
    WORKSPACES ||--o{ ENTITIES : "extracts"
    WORKSPACES ||--o{ RELATIONSHIPS : "extracts"
    WORKSPACES ||--o{ TASKS : "runs"
    TENANTS ||--o{ CONVERSATIONS : "hosts"
    CONVERSATIONS ||--o{ MESSAGES : "contains"
```

Not shown as static entities, since they're not fixed schema:
- **Apache AGE graph data** — nodes/edges live inside the same Postgres
  instance via the AGE extension (Cypher queries against a graph catalog,
  not ordinary tables), scoped by `workspace_id` property on each node.
- **Per-workspace dynamic vector tables** — beyond the static
  `chunks.embedding`/`entities.embedding`/`relationships.embedding`
  columns, each workspace also gets its own dedicated vector table named
  `eq_<namespace>_ws_<uuid-short>_vectors` with its own HNSW index sized
  to that workspace's embedding provider. A plain `TRUNCATE chunks` does
  **not** clear these — see `reset_edgequake_clara_workspace.sh`, which
  uses Edgequake's own scoped workspace-wipe endpoint instead of raw SQL.

Every core table carries `tenant_id`/`workspace_id`, enforced by
Row-Level Security — this is a genuinely multi-tenant instance, not
Clara-exclusive, even though it currently only runs on `limbic` for
Clara's own use.

## 4. How Kafka ties CoireStore and lildaemon together

Not a real ERD relationship (no FK, no shared database) — Kafka topic
names and IDs are the only connective tissue, and Ritual consumer groups
have no database row at all:

```mermaid
flowchart LR
    subgraph CoireStore["clara-api / CoireStore (DuckDB)"]
        R[rituals.ritual_id]
        SR[source_registry]
        DS[deduction_snapshots]
    end
    subgraph lildaemon["lildaemon (DuckDB)"]
        RC[ritual_configs.ritual_id]
        RP[ritual_participants]
        ADT[assistant_document_tags.document_id]
    end
    subgraph Kafka["Kafka (no persistent volume)"]
        RT["topic: {domain}.ritual.{ritual_id}"]
        CT["topic: {domain}.coire.{path}"]
        CG["consumer group:\nritual-{ritual_id}-{node_id}\n(never explicitly deleted on leave)"]
    end
    subgraph Edgequake["Edgequake (Postgres, separate host-local service)"]
        DOC[documents.document_id]
    end

    R -- "topic name convention" --> RT
    RC -- "same ritual_id" --> RT
    RT -- "each joined participant creates" --> CG
    SR -- "ad hoc topic convention" --> CT
    DS -.-> CT
    ADT -. "loose, cross-system,\nunenforced correlation" .-> DOC
```

## Verification

Schema confirmed live against the running containers:
`duckdb data/coire.duckdb ".tables"`,
`duckdb lildaemon/data/lildaemon.duc ".tables"`, and
`docker exec edgequake-postgres psql -U edgequake -d edgequake -c '\dt'`
(and `\dGe`/AGE catalog for graph data) should each match the tables
listed above — re-run these after any schema-changing migration to keep
this doc accurate.
