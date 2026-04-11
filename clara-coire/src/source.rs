//! Source registry — content-addressed storage for Prolog/CLIPS source files
//! and their derived artifacts (DOT graphs, parsed rule JSON, decorated sources).
//!
//! `SourceRegistry` shares the same `Arc<Mutex<duckdb::Connection>>` as
//! `CoireStore` — no additional DB connection is needed.
//!
//! # Content addressing
//!
//! Sources are deduplicated by `(SHA-256 hex of content, source_type)`.
//! Uploading the same source twice returns the same `source_id` without
//! inserting a second row.
//!
//! # Artifact caching
//!
//! Derived artifacts are generated lazily via `get_or_create_artifact`.
//! The caller supplies a generator closure that is only called on a cache miss.
//! After the first call the result is stored and subsequent calls skip the
//! generator entirely.
//!
//! # GC
//!
//! Both tables use `expires_at_ms` (millisecond Unix timestamp, `NULL` = no
//! expiry).  `sweep_expired` deletes sources whose TTL has elapsed and
//! cascades to their artifacts.  An orphan sweep removes artifacts whose
//! source no longer exists.

use std::sync::{Arc, Mutex};

use duckdb::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::error::CoireResult;

// ── Public types ──────────────────────────────────────────────────────────────

/// A registered source file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceEntry {
    pub source_id:     Uuid,
    /// SHA-256 hex digest of `content`.
    pub content_hash:  String,
    /// `"prolog"` or `"clips"`.
    pub source_type:   String,
    /// Optional human-readable label (e.g. `"ex1_clara"`).
    pub label:         Option<String>,
    /// Raw source text.
    pub content:       String,
    pub created_at_ms: i64,
    /// `None` = no expiry.
    pub expires_at_ms: Option<i64>,
}

/// A derived artifact produced from a registered source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactEntry {
    pub artifact_id:     Uuid,
    pub source_id:       Uuid,
    /// `"dot"`, `"parsed_rules"`, or `"decorated_pl"`.
    pub artifact_type:   String,
    /// Artifact text content.
    pub content:         String,
    pub generated_at_ms: i64,
    /// `None` = inherit source TTL.
    pub expires_at_ms:   Option<i64>,
}

// ── SourceRegistry ────────────────────────────────────────────────────────────

/// Content-addressed store for source files and their derived artifacts.
///
/// `Clone` is cheap — all clones share the same underlying connection.
#[derive(Clone)]
pub struct SourceRegistry {
    conn: Arc<Mutex<Connection>>,
}

impl SourceRegistry {
    /// Create a `SourceRegistry` wrapping an existing connection.
    ///
    /// The `source_registry` and `source_artifacts` tables must have been
    /// created before calling this (done by `CoireStore::open`).
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }

    // ── Write ──────────────────────────────────────────────────────────────

    /// Register source content.
    ///
    /// If a source with the same `(content_hash, source_type)` already exists
    /// the existing `source_id` is returned without inserting a new row
    /// (`is_new = false`).  Otherwise a new row is inserted (`is_new = true`).
    ///
    /// Returns `(source_id, is_new)`.
    pub fn register(
        &self,
        source_type:  &str,
        label:        Option<&str>,
        content:      &str,
        expires_at_ms: Option<i64>,
    ) -> CoireResult<(Uuid, bool)> {
        let hash = sha256_hex(content);
        let conn = self.conn.lock().unwrap();

        // Dedup check.
        let existing: Option<String> = conn
            .query_row(
                "SELECT source_id FROM source_registry \
                 WHERE content_hash = ? AND source_type = ?",
                duckdb::params![hash, source_type],
                |r| r.get(0),
            )
            .ok();

        if let Some(id_str) = existing {
            let source_id = id_str.parse::<Uuid>().unwrap_or_else(|_| Uuid::new_v4());
            return Ok((source_id, false));
        }

        let source_id  = Uuid::new_v4();
        let now_ms     = now_ms();
        conn.execute(
            "INSERT INTO source_registry \
             (source_id, content_hash, source_type, label, content, created_at_ms, expires_at_ms) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
            duckdb::params![
                source_id.to_string(),
                hash,
                source_type,
                label,
                content,
                now_ms,
                expires_at_ms,
            ],
        )?;
        Ok((source_id, true))
    }

    // ── Read ───────────────────────────────────────────────────────────────

    /// Retrieve a source entry by ID.  Returns `None` if not found.
    pub fn get(&self, source_id: Uuid) -> CoireResult<Option<SourceEntry>> {
        let conn = self.conn.lock().unwrap();
        let result = conn.query_row(
            "SELECT source_id, content_hash, source_type, label, content, \
                    created_at_ms, expires_at_ms \
             FROM source_registry WHERE source_id = ?",
            duckdb::params![source_id.to_string()],
            row_to_source,
        );
        match result {
            Ok(e)  => Ok(Some(e)),
            Err(duckdb::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Retrieve a source entry by ID, omitting the content field.
    ///
    /// Useful for listing endpoints where callers don't need the full text.
    pub fn get_meta(&self, source_id: Uuid) -> CoireResult<Option<SourceEntry>> {
        let conn = self.conn.lock().unwrap();
        let result = conn.query_row(
            "SELECT source_id, content_hash, source_type, label, '' AS content, \
                    created_at_ms, expires_at_ms \
             FROM source_registry WHERE source_id = ?",
            duckdb::params![source_id.to_string()],
            row_to_source,
        );
        match result {
            Ok(e)  => Ok(Some(e)),
            Err(duckdb::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Find an existing `source_id` by content hash + type.
    ///
    /// Returns `None` if no matching source exists.
    pub fn find_by_hash(
        &self,
        content_hash: &str,
        source_type:  &str,
    ) -> CoireResult<Option<Uuid>> {
        let conn = self.conn.lock().unwrap();
        let result: Result<String, _> = conn.query_row(
            "SELECT source_id FROM source_registry \
             WHERE content_hash = ? AND source_type = ?",
            duckdb::params![content_hash, source_type],
            |r| r.get(0),
        );
        match result {
            Ok(s)  => Ok(s.parse::<Uuid>().ok()),
            Err(duckdb::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    // ── Artifacts ─────────────────────────────────────────────────────────

    /// Return an existing artifact of `artifact_type` for `source_id`, or
    /// call `generator` to produce it and cache the result.
    ///
    /// `generator` receives the source content string.  It is called at most
    /// once per `(source_id, artifact_type)` pair.
    ///
    /// Returns `None` if the source itself does not exist.
    pub fn get_or_create_artifact<F>(
        &self,
        source_id:     Uuid,
        artifact_type: &str,
        expires_at_ms: Option<i64>,
        generator:     F,
    ) -> CoireResult<Option<ArtifactEntry>>
    where
        F: FnOnce(&str) -> CoireResult<String>,
    {
        // Fast path: artifact already cached.
        if let Some(art) = self.get_artifact_by_source(source_id, artifact_type)? {
            return Ok(Some(art));
        }

        // Load source content for the generator.
        let entry = match self.get(source_id)? {
            Some(e) => e,
            None    => return Ok(None),
        };

        let content     = generator(&entry.content)?;
        let artifact_id = Uuid::new_v4();
        let now_ms      = now_ms();

        {
            let conn = self.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO source_artifacts \
                 (artifact_id, source_id, artifact_type, content, generated_at_ms, expires_at_ms) \
                 VALUES (?, ?, ?, ?, ?, ?)",
                duckdb::params![
                    artifact_id.to_string(),
                    source_id.to_string(),
                    artifact_type,
                    &content,
                    now_ms,
                    expires_at_ms,
                ],
            )?;
        }

        Ok(Some(ArtifactEntry {
            artifact_id,
            source_id,
            artifact_type: artifact_type.to_string(),
            content,
            generated_at_ms: now_ms,
            expires_at_ms,
        }))
    }

    /// Retrieve a specific artifact by its UUID.
    pub fn get_artifact(&self, artifact_id: Uuid) -> CoireResult<Option<ArtifactEntry>> {
        let conn = self.conn.lock().unwrap();
        let result = conn.query_row(
            "SELECT artifact_id, source_id, artifact_type, content, \
                    generated_at_ms, expires_at_ms \
             FROM source_artifacts WHERE artifact_id = ?",
            duckdb::params![artifact_id.to_string()],
            row_to_artifact,
        );
        match result {
            Ok(e)  => Ok(Some(e)),
            Err(duckdb::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Retrieve a cached artifact by source ID and type.  Returns `None` on miss.
    pub fn get_artifact_by_source(
        &self,
        source_id:     Uuid,
        artifact_type: &str,
    ) -> CoireResult<Option<ArtifactEntry>> {
        let conn = self.conn.lock().unwrap();
        let result = conn.query_row(
            "SELECT artifact_id, source_id, artifact_type, content, \
                    generated_at_ms, expires_at_ms \
             FROM source_artifacts \
             WHERE source_id = ? AND artifact_type = ? \
             ORDER BY generated_at_ms DESC LIMIT 1",
            duckdb::params![source_id.to_string(), artifact_type],
            row_to_artifact,
        );
        match result {
            Ok(e)  => Ok(Some(e)),
            Err(duckdb::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    // ── Delete ─────────────────────────────────────────────────────────────

    /// Delete a source and cascade-delete all its artifacts.
    pub fn delete(&self, source_id: Uuid) -> CoireResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM source_artifacts WHERE source_id = ?",
            duckdb::params![source_id.to_string()],
        )?;
        conn.execute(
            "DELETE FROM source_registry WHERE source_id = ?",
            duckdb::params![source_id.to_string()],
        )?;
        Ok(())
    }

    // ── GC ─────────────────────────────────────────────────────────────────

    /// Delete sources whose `expires_at_ms <= now_ms` and cascade to their
    /// artifacts.  Then sweep orphaned artifacts (whose source is gone).
    ///
    /// Returns the total number of rows deleted across both tables.
    pub fn sweep_expired(&self, now_ms: i64) -> CoireResult<usize> {
        let conn = self.conn.lock().unwrap();

        // Cascade: delete artifacts for expired sources first.
        let art_from_sources = conn.execute(
            "DELETE FROM source_artifacts \
             WHERE source_id IN ( \
                 SELECT source_id FROM source_registry \
                 WHERE expires_at_ms IS NOT NULL AND expires_at_ms <= ? \
             )",
            duckdb::params![now_ms],
        )?;

        // Delete the expired sources themselves.
        let sources = conn.execute(
            "DELETE FROM source_registry \
             WHERE expires_at_ms IS NOT NULL AND expires_at_ms <= ?",
            duckdb::params![now_ms],
        )?;

        // Orphan sweep: artifacts whose source was deleted manually.
        let orphans = conn.execute(
            "DELETE FROM source_artifacts \
             WHERE source_id NOT IN (SELECT source_id FROM source_registry)",
            [],
        )?;

        // Expired artifacts with their own TTL (source still alive).
        let art_expired = conn.execute(
            "DELETE FROM source_artifacts \
             WHERE expires_at_ms IS NOT NULL AND expires_at_ms <= ?",
            duckdb::params![now_ms],
        )?;

        Ok(art_from_sources + sources + orphans + art_expired)
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn sha256_hex(s: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(s.as_bytes());
    hex::encode(hasher.finalize())
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn row_to_source(r: &duckdb::Row<'_>) -> duckdb::Result<SourceEntry> {
    let source_id: String = r.get(0)?;
    let source_id = source_id.parse::<Uuid>().unwrap_or_else(|_| Uuid::nil());
    Ok(SourceEntry {
        source_id,
        content_hash:  r.get(1)?,
        source_type:   r.get(2)?,
        label:         r.get(3)?,
        content:       r.get(4)?,
        created_at_ms: r.get(5)?,
        expires_at_ms: r.get(6)?,
    })
}

fn row_to_artifact(r: &duckdb::Row<'_>) -> duckdb::Result<ArtifactEntry> {
    let artifact_id: String = r.get(0)?;
    let artifact_id = artifact_id.parse::<Uuid>().unwrap_or_else(|_| Uuid::nil());
    let source_id: String = r.get(1)?;
    let source_id = source_id.parse::<Uuid>().unwrap_or_else(|_| Uuid::nil());
    Ok(ArtifactEntry {
        artifact_id,
        source_id,
        artifact_type:   r.get(2)?,
        content:         r.get(3)?,
        generated_at_ms: r.get(4)?,
        expires_at_ms:   r.get(5)?,
    })
}


#[cfg(test)]
mod tests {
    use super::*;
    use duckdb::Connection;
    use std::sync::{Arc, Mutex};

    fn make_registry() -> SourceRegistry {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE source_registry (
                source_id     TEXT NOT NULL PRIMARY KEY,
                content_hash  TEXT NOT NULL,
                source_type   TEXT NOT NULL,
                label         TEXT,
                content       TEXT NOT NULL,
                created_at_ms BIGINT NOT NULL,
                expires_at_ms BIGINT
            );
            CREATE UNIQUE INDEX idx_source_hash_type
                ON source_registry (content_hash, source_type);
            CREATE TABLE source_artifacts (
                artifact_id      TEXT NOT NULL PRIMARY KEY,
                source_id        TEXT NOT NULL,
                artifact_type    TEXT NOT NULL,
                content          TEXT NOT NULL,
                generated_at_ms  BIGINT NOT NULL,
                expires_at_ms    BIGINT
            );
            CREATE INDEX idx_artifact_source_type
                ON source_artifacts (source_id, artifact_type);",
        ).unwrap();
        SourceRegistry::new(Arc::new(Mutex::new(conn)))
    }

    #[test]
    fn register_and_dedup() {
        let reg = make_registry();
        let src = "unlocked :- tumbler(1,set).";

        let (id1, new1) = reg.register("prolog", Some("ex1"), src, None).unwrap();
        let (id2, new2) = reg.register("prolog", Some("ex1"), src, None).unwrap();

        assert!(new1);
        assert!(!new2, "same content should not insert a second row");
        assert_eq!(id1, id2, "same content must return the same source_id");
    }

    #[test]
    fn different_type_is_different_entry() {
        let reg = make_registry();
        let src = "(defrule foo => (assert (bar)))";

        let (id_pl, _)   = reg.register("prolog", None, src, None).unwrap();
        let (id_clp, _)  = reg.register("clips",  None, src, None).unwrap();

        assert_ne!(id_pl, id_clp, "same content, different type → different id");
    }

    #[test]
    fn get_returns_entry() {
        let reg = make_registry();
        let (id, _) = reg.register("prolog", Some("test"), "foo.", None).unwrap();
        let entry = reg.get(id).unwrap().expect("should find entry");
        assert_eq!(entry.source_type, "prolog");
        assert_eq!(entry.label.as_deref(), Some("test"));
        assert_eq!(entry.content, "foo.");
    }

    #[test]
    fn artifact_cache_aside() {
        let reg   = make_registry();
        let (id, _) = reg.register("prolog", None, "foo.", None).unwrap();

        let mut call_count = 0usize;
        let art1 = reg
            .get_or_create_artifact(id, "dot", None, |_src| {
                call_count += 1;
                Ok("digraph Clara {}".to_string())
            })
            .unwrap()
            .unwrap();
        assert_eq!(call_count, 1);
        assert_eq!(art1.content, "digraph Clara {}");

        // Second call must return cached value without invoking generator.
        let art2 = reg
            .get_or_create_artifact(id, "dot", None, |_src| {
                call_count += 1;
                Ok("should not be called".to_string())
            })
            .unwrap()
            .unwrap();
        assert_eq!(call_count, 1, "generator should not run on cache hit");
        assert_eq!(art2.artifact_id, art1.artifact_id);
    }

    #[test]
    fn delete_cascades_to_artifacts() {
        let reg = make_registry();
        let (id, _) = reg.register("prolog", None, "foo.", None).unwrap();
        reg.get_or_create_artifact(id, "dot", None, |_| Ok("dot".into()))
            .unwrap();
        reg.delete(id).unwrap();
        assert!(reg.get(id).unwrap().is_none());
        assert!(reg.get_artifact_by_source(id, "dot").unwrap().is_none());
    }

    #[test]
    fn sweep_expired() {
        let reg = make_registry();
        let past_ms = now_ms() - 10_000;
        let (id, _) = reg.register("prolog", None, "bar.", Some(past_ms)).unwrap();
        reg.get_or_create_artifact(id, "dot", None, |_| Ok("dot".into()))
            .unwrap();

        let deleted = reg.sweep_expired(now_ms()).unwrap();
        assert!(deleted >= 2, "source row + artifact row should both be deleted");
        assert!(reg.get(id).unwrap().is_none());
    }
}
