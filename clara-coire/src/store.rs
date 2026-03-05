use std::collections::HashSet;
use std::path::Path;
use std::sync::{Arc, Mutex};

use duckdb::Connection;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::coire::{raw_to_event, Coire, RawRow};
use crate::error::CoireResult;
use crate::event::ClaraEvent;

/// Full snapshot of a deduction run: seed knowledge + result metadata.
///
/// Stored in the `deduction_snapshots` table of a [`CoireStore`] when a
/// `POST /deduce` request is made with `persist: true`. The associated Coire
/// pending events are stored in `coire_events` under `prolog_session_id` /
/// `clips_session_id`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeductionSnapshot {
    pub deduction_id:      Uuid,
    /// Prolog seed clauses (facts and rules).
    pub prolog_clauses:    Vec<String>,
    /// CLIPS seed constructs (defrule, deftemplate, etc.).
    pub clips_constructs:  Vec<String>,
    /// Optional server-side `.clp` file path consulted before constructs.
    pub clips_file:        Option<String>,
    /// Prolog goal executed on cycle 0 (none = skip goal on resume).
    pub initial_goal:      Option<String>,
    /// Cycle budget used for the saved run.
    pub max_cycles:        u32,
    /// Final `CycleStatus` display string.
    pub status:            String,
    /// Number of Prolog↔CLIPS cycles that actually ran.
    pub cycles_run:        u32,
    /// Prolog engine mailbox UUID — links to `coire_events`.
    pub prolog_session_id: Uuid,
    /// CLIPS engine mailbox UUID — links to `coire_events`.
    pub clips_session_id:  Uuid,
    /// Unix timestamp (ms) when the snapshot was created.
    pub created_at_ms:     i64,
    /// Unix timestamp (ms) after which the snapshot is eligible for deletion.
    pub expires_at_ms:     i64,
}

/// Persistent DuckDB-backed store for Coire session snapshots.
///
/// Uses the same schema as the in-memory [`Coire`] but writes to a file,
/// enabling save/restore of session event state across process restarts.
///
/// `CoireStore` is `Clone` — clones share the same underlying connection.
#[derive(Clone)]
pub struct CoireStore {
    conn: Arc<Mutex<Connection>>,
}

impl CoireStore {
    /// Open (or create) a persistent store at the given file path.
    pub fn open(path: impl AsRef<Path>) -> CoireResult<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS coire_events (
                event_id      VARCHAR NOT NULL PRIMARY KEY,
                session_id    VARCHAR NOT NULL,
                origin        VARCHAR NOT NULL,
                created_at_ms BIGINT  NOT NULL,
                payload       VARCHAR NOT NULL,
                status        VARCHAR NOT NULL DEFAULT 'pending'
            );
            CREATE INDEX IF NOT EXISTS idx_coire_session_status
                ON coire_events (session_id, status);

            CREATE TABLE IF NOT EXISTS deduction_snapshots (
                deduction_id      VARCHAR NOT NULL PRIMARY KEY,
                prolog_clauses    VARCHAR NOT NULL,
                clips_constructs  VARCHAR NOT NULL,
                clips_file        VARCHAR,
                initial_goal      VARCHAR,
                max_cycles        INTEGER NOT NULL,
                status            VARCHAR NOT NULL,
                cycles_run        INTEGER NOT NULL,
                prolog_session_id VARCHAR NOT NULL,
                clips_session_id  VARCHAR NOT NULL,
                created_at_ms     BIGINT  NOT NULL,
                expires_at_ms     BIGINT  NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_snapshots_expires
                ON deduction_snapshots (expires_at_ms);",
        )?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Save pending events for `session_id` from a live `Coire` into this store.
    ///
    /// Only `Pending` events are persisted — processed events have already been
    /// acted upon and carry no value for session resumption. Events are upserted
    /// by `event_id`, so calling this multiple times is safe.
    /// Returns the number of events saved.
    pub fn save_session(&self, session_id: Uuid, coire: &Coire) -> CoireResult<usize> {
        let events = coire.read_pending(session_id)?;
        let count = events.len();
        {
            let conn = self.conn.lock().unwrap();
            for event in &events {
                self.upsert_event_with_conn(&conn, event)?;
            }
        }
        log::info!(
            "CoireStore: saved {} events for session {}",
            count,
            session_id
        );
        Ok(count)
    }

    /// Restore all stored events for `session_id` into a live `Coire`.
    ///
    /// Existing events in the `Coire` with the same `event_id` will cause a
    /// DuckDB primary-key error. Call [`Coire::clear_session`] first if needed.
    /// Returns the number of events restored.
    pub fn restore_session(&self, session_id: Uuid, coire: &Coire) -> CoireResult<usize> {
        let events = self.read_session(session_id)?;
        let count = events.len();
        for event in events {
            coire.write_event(&event)?;
        }
        log::info!(
            "CoireStore: restored {} events for session {}",
            count,
            session_id
        );
        Ok(count)
    }

    /// Delete all stored events for `session_id`.
    /// Returns the number of rows deleted.
    pub fn delete_session(&self, session_id: Uuid) -> CoireResult<usize> {
        log::info!("CoireStore: deleting session {}", session_id);
        let conn = self.conn.lock().unwrap();
        let count = conn.execute(
            "DELETE FROM coire_events WHERE session_id = ?",
            duckdb::params![session_id.to_string()],
        )?;
        Ok(count)
    }

    /// Return the distinct session IDs that have stored events, ordered by ID.
    pub fn list_sessions(&self) -> CoireResult<Vec<Uuid>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT DISTINCT session_id FROM coire_events ORDER BY session_id")?;
        let rows = stmt.query_map([], |row| {
            let s: String = row.get(0)?;
            Ok(s)
        })?;
        let mut sessions = Vec::new();
        for row in rows {
            if let Ok(id) = Uuid::parse_str(&row?) {
                sessions.push(id);
            }
        }
        Ok(sessions)
    }

    /// Read all stored events for `session_id` ordered by `created_at_ms`.
    /// Does not modify any state.
    pub fn read_session(&self, session_id: Uuid) -> CoireResult<Vec<ClaraEvent>> {
        let conn = self.conn.lock().unwrap();
        let sid = session_id.to_string();
        let mut stmt = conn.prepare(
            "SELECT event_id, session_id, origin, created_at_ms, payload, status
             FROM coire_events
             WHERE session_id = ?
             ORDER BY created_at_ms ASC",
        )?;
        let rows = stmt.query_map(duckdb::params![sid], |row| {
            Ok(RawRow {
                event_id: row.get(0)?,
                session_id: row.get(1)?,
                origin: row.get(2)?,
                created_at_ms: row.get(3)?,
                payload: row.get(4)?,
                status: row.get(5)?,
            })
        })?;
        let mut events = Vec::new();
        for row in rows {
            events.push(raw_to_event(row?)?);
        }
        Ok(events)
    }

    /// Restore stored events for `from_id` into a live `Coire`, rewriting each
    /// event's `session_id` to `into_id`.
    ///
    /// Use this when resuming a previous run under a new `DeductionSession` whose
    /// UUIDs differ from the original. Returns the number of events restored.
    pub fn restore_session_as(
        &self,
        from_id: Uuid,
        into_id: Uuid,
        coire: &Coire,
    ) -> CoireResult<usize> {
        let events = self.read_session(from_id)?;
        let count = events.len();
        for mut event in events {
            event.session_id = into_id;
            coire.write_event(&event)?;
        }
        log::info!(
            "CoireStore: restored {} events from session {} as session {}",
            count,
            from_id,
            into_id
        );
        Ok(count)
    }

    /// Return session IDs whose newest event (`MAX(created_at_ms)`) is older
    /// than `older_than_ms` (Unix milliseconds), excluding any IDs in `exclude`.
    /// Used by the carrion-picker to find stale sessions eligible for deletion.
    pub fn sessions_older_than(
        &self,
        older_than_ms: i64,
        exclude: &HashSet<Uuid>,
    ) -> CoireResult<Vec<Uuid>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT session_id
             FROM coire_events
             GROUP BY session_id
             HAVING MAX(created_at_ms) < ?
             ORDER BY MAX(created_at_ms) ASC",
        )?;
        let rows = stmt.query_map(duckdb::params![older_than_ms], |row| {
            let s: String = row.get(0)?;
            Ok(s)
        })?;
        let mut sessions = Vec::new();
        for row in rows {
            if let Ok(id) = Uuid::parse_str(&row?) {
                if !exclude.contains(&id) {
                    sessions.push(id);
                }
            }
        }
        Ok(sessions)
    }

    // ── Snapshot methods ────────────────────────────────────────────────────

    /// Persist a [`DeductionSnapshot`], replacing any previous row with the
    /// same `deduction_id`.
    pub fn save_snapshot(&self, snap: &DeductionSnapshot) -> CoireResult<()> {
        let clauses    = serde_json::to_string(&snap.prolog_clauses)?;
        let constructs = serde_json::to_string(&snap.clips_constructs)?;
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO deduction_snapshots
                (deduction_id, prolog_clauses, clips_constructs, clips_file,
                 initial_goal, max_cycles, status, cycles_run,
                 prolog_session_id, clips_session_id, created_at_ms, expires_at_ms)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT (deduction_id) DO UPDATE SET
                prolog_clauses    = excluded.prolog_clauses,
                clips_constructs  = excluded.clips_constructs,
                clips_file        = excluded.clips_file,
                initial_goal      = excluded.initial_goal,
                max_cycles        = excluded.max_cycles,
                status            = excluded.status,
                cycles_run        = excluded.cycles_run,
                prolog_session_id = excluded.prolog_session_id,
                clips_session_id  = excluded.clips_session_id,
                created_at_ms     = excluded.created_at_ms,
                expires_at_ms     = excluded.expires_at_ms",
            duckdb::params![
                snap.deduction_id.to_string(),
                clauses,
                constructs,
                snap.clips_file.as_deref(),
                snap.initial_goal.as_deref(),
                snap.max_cycles as i64,
                snap.status,
                snap.cycles_run as i64,
                snap.prolog_session_id.to_string(),
                snap.clips_session_id.to_string(),
                snap.created_at_ms,
                snap.expires_at_ms,
            ],
        )?;
        log::info!("CoireStore: saved snapshot {}", snap.deduction_id);
        Ok(())
    }

    /// Load a snapshot by its `deduction_id`. Returns `None` if not found.
    pub fn load_snapshot(&self, deduction_id: Uuid) -> CoireResult<Option<DeductionSnapshot>> {
        let conn = self.conn.lock().unwrap();
        let did  = deduction_id.to_string();
        let mut stmt = conn.prepare(
            "SELECT deduction_id, prolog_clauses, clips_constructs, clips_file,
                    initial_goal, max_cycles, status, cycles_run,
                    prolog_session_id, clips_session_id, created_at_ms, expires_at_ms
             FROM deduction_snapshots
             WHERE deduction_id = ?",
        )?;
        let mut rows = stmt.query_map(duckdb::params![did], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, i64>(10)?,
                row.get::<_, i64>(11)?,
            ))
        })?;
        match rows.next() {
            None => Ok(None),
            Some(row) => {
                let (did, clauses_s, constructs_s, clips_file, initial_goal,
                     max_cycles, status, cycles_run, prolog_sid, clips_sid,
                     created_at_ms, expires_at_ms) = row?;
                Ok(Some(DeductionSnapshot {
                    deduction_id:      Uuid::parse_str(&did).unwrap(),
                    prolog_clauses:    serde_json::from_str(&clauses_s)?,
                    clips_constructs:  serde_json::from_str(&constructs_s)?,
                    clips_file,
                    initial_goal,
                    max_cycles:        max_cycles as u32,
                    status,
                    cycles_run:        cycles_run as u32,
                    prolog_session_id: Uuid::parse_str(&prolog_sid).unwrap(),
                    clips_session_id:  Uuid::parse_str(&clips_sid).unwrap(),
                    created_at_ms,
                    expires_at_ms,
                }))
            }
        }
    }

    /// Delete a snapshot and all associated Coire events (both engine mailboxes).
    ///
    /// Returns the number of Coire event rows deleted. Returns `0` if no
    /// snapshot with that ID exists.
    pub fn delete_snapshot(&self, deduction_id: Uuid) -> CoireResult<usize> {
        let conn = self.conn.lock().unwrap();
        let did  = deduction_id.to_string();

        // Load session IDs under the same lock before deleting.
        let ids: Option<(String, String)> = {
            let mut stmt = conn.prepare(
                "SELECT prolog_session_id, clips_session_id
                 FROM deduction_snapshots WHERE deduction_id = ?",
            )?;
            let mut rows = stmt.query_map(duckdb::params![did], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            rows.next().transpose()?
        };

        let Some((prolog_sid, clips_sid)) = ids else {
            return Ok(0);
        };

        let n_prolog = conn.execute(
            "DELETE FROM coire_events WHERE session_id = ?",
            duckdb::params![prolog_sid],
        )?;
        let n_clips = conn.execute(
            "DELETE FROM coire_events WHERE session_id = ?",
            duckdb::params![clips_sid],
        )?;
        conn.execute(
            "DELETE FROM deduction_snapshots WHERE deduction_id = ?",
            duckdb::params![did],
        )?;

        log::info!(
            "CoireStore: deleted snapshot {} ({} prolog + {} clips events)",
            deduction_id,
            n_prolog,
            n_clips,
        );
        Ok(n_prolog + n_clips)
    }

    /// Return all snapshots whose `expires_at_ms` is before `now_ms`, excluding
    /// any whose `prolog_session_id` or `clips_session_id` is in `exclude`.
    /// Used by the carrion-picker to find snapshots eligible for deletion.
    pub fn snapshots_expired(
        &self,
        now_ms:  i64,
        exclude: &HashSet<Uuid>,
    ) -> CoireResult<Vec<DeductionSnapshot>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT deduction_id, prolog_clauses, clips_constructs, clips_file,
                    initial_goal, max_cycles, status, cycles_run,
                    prolog_session_id, clips_session_id, created_at_ms, expires_at_ms
             FROM deduction_snapshots
             WHERE expires_at_ms < ?
             ORDER BY expires_at_ms ASC",
        )?;
        let rows = stmt.query_map(duckdb::params![now_ms], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, i64>(10)?,
                row.get::<_, i64>(11)?,
            ))
        })?;
        let mut snaps = Vec::new();
        for row in rows {
            let (did, clauses_s, constructs_s, clips_file, initial_goal,
                 max_cycles, status, cycles_run, prolog_sid, clips_sid,
                 created_at_ms, expires_at_ms) = row?;
            let prolog_id = Uuid::parse_str(&prolog_sid).unwrap();
            let clips_id  = Uuid::parse_str(&clips_sid).unwrap();
            if exclude.contains(&prolog_id) || exclude.contains(&clips_id) {
                continue;
            }
            snaps.push(DeductionSnapshot {
                deduction_id:      Uuid::parse_str(&did).unwrap(),
                prolog_clauses:    serde_json::from_str(&clauses_s)?,
                clips_constructs:  serde_json::from_str(&constructs_s)?,
                clips_file,
                initial_goal,
                max_cycles:        max_cycles as u32,
                status,
                cycles_run:        cycles_run as u32,
                prolog_session_id: prolog_id,
                clips_session_id:  clips_id,
                created_at_ms,
                expires_at_ms,
            });
        }
        Ok(snaps)
    }

    // ── Coire event methods ──────────────────────────────────────────────────

    fn upsert_event_with_conn(
        &self,
        conn: &Connection,
        event: &ClaraEvent,
    ) -> CoireResult<()> {
        let payload_str = serde_json::to_string(&event.payload)?;
        conn.execute(
            "INSERT INTO coire_events
                (event_id, session_id, origin, created_at_ms, payload, status)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT (event_id) DO UPDATE SET
                session_id    = excluded.session_id,
                origin        = excluded.origin,
                created_at_ms = excluded.created_at_ms,
                payload       = excluded.payload,
                status        = excluded.status",
            duckdb::params![
                event.event_id.to_string(),
                event.session_id.to_string(),
                event.origin,
                event.created_at_ms,
                payload_str,
                event.status.as_str(),
            ],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coire::Coire;
    use crate::event::EventStatus;
    use serde_json::json;
    use tempfile;

    fn tmp_store() -> (CoireStore, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.duckdb");
        let store = CoireStore::open(&path).unwrap();
        (store, dir)
    }

    fn make_coire_with_events(session_id: Uuid) -> Coire {
        let coire = Coire::new().unwrap();
        coire
            .write_event(&ClaraEvent::new(session_id, "prolog", json!({"fact": "a"})))
            .unwrap();
        coire
            .write_event(&ClaraEvent::new(session_id, "clips", json!({"rule": "b"})))
            .unwrap();
        coire
    }

    #[test]
    fn save_and_read_session() {
        let (store, _f) = tmp_store();
        let sid = Uuid::new_v4();
        let coire = make_coire_with_events(sid);

        let saved = store.save_session(sid, &coire).unwrap();
        assert_eq!(saved, 2);

        let events = store.read_session(sid).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].origin, "prolog");
        assert_eq!(events[1].origin, "clips");
    }

    #[test]
    fn save_is_idempotent() {
        let (store, _f) = tmp_store();
        let sid = Uuid::new_v4();
        let coire = make_coire_with_events(sid);

        store.save_session(sid, &coire).unwrap();
        store.save_session(sid, &coire).unwrap(); // second save should upsert cleanly

        let events = store.read_session(sid).unwrap();
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn restore_session() {
        let (store, _f) = tmp_store();
        let sid = Uuid::new_v4();
        let src = make_coire_with_events(sid);
        // Mark one processed — only the remaining pending event should be saved.
        let all = src.read_all(sid).unwrap();
        src.mark_processed(all[0].event_id).unwrap();

        store.save_session(sid, &src).unwrap();

        let dst = Coire::new().unwrap();
        let restored = store.restore_session(sid, &dst).unwrap();
        assert_eq!(restored, 1); // only the pending event was saved

        let events = dst.read_all(sid).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].status, EventStatus::Pending);
    }

    #[test]
    fn delete_session() {
        let (store, _f) = tmp_store();
        let sid = Uuid::new_v4();
        let coire = make_coire_with_events(sid);

        store.save_session(sid, &coire).unwrap();
        let deleted = store.delete_session(sid).unwrap();
        assert_eq!(deleted, 2);
        assert_eq!(store.read_session(sid).unwrap().len(), 0);
    }

    #[test]
    fn list_sessions() {
        let (store, _f) = tmp_store();
        let s1 = Uuid::new_v4();
        let s2 = Uuid::new_v4();

        store
            .save_session(s1, &make_coire_with_events(s1))
            .unwrap();
        store
            .save_session(s2, &make_coire_with_events(s2))
            .unwrap();

        let sessions = store.list_sessions().unwrap();
        assert_eq!(sessions.len(), 2);
        assert!(sessions.contains(&s1));
        assert!(sessions.contains(&s2));
    }

    #[test]
    fn session_isolation_in_store() {
        let (store, _f) = tmp_store();
        let s1 = Uuid::new_v4();
        let s2 = Uuid::new_v4();

        store
            .save_session(s1, &make_coire_with_events(s1))
            .unwrap();
        store
            .save_session(s2, &make_coire_with_events(s2))
            .unwrap();

        store.delete_session(s1).unwrap();

        assert_eq!(store.read_session(s1).unwrap().len(), 0);
        assert_eq!(store.read_session(s2).unwrap().len(), 2);
    }

    #[test]
    fn restore_session_as_remaps_id() {
        let (store, _f) = tmp_store();
        let old_sid = Uuid::new_v4();
        let new_sid = Uuid::new_v4();

        let src = make_coire_with_events(old_sid);
        store.save_session(old_sid, &src).unwrap();

        let dst = Coire::new().unwrap();
        let restored = store.restore_session_as(old_sid, new_sid, &dst).unwrap();
        assert_eq!(restored, 2);

        // Events appear under the new session id, not the old one
        assert_eq!(dst.read_all(new_sid).unwrap().len(), 2);
        assert_eq!(dst.read_all(old_sid).unwrap().len(), 0);

        // Each event's session_id field is rewritten
        for event in dst.read_all(new_sid).unwrap() {
            assert_eq!(event.session_id, new_sid);
        }
    }

    fn make_snapshot(store: &CoireStore, expires_at_ms: i64) -> (DeductionSnapshot, Uuid, Uuid) {
        let sid = Uuid::new_v4();
        let prolog_id = Uuid::new_v4();
        let clips_id  = Uuid::new_v4();
        // Save placeholder Coire events so delete_snapshot has rows to remove.
        let coire = make_coire_with_events(prolog_id);
        store.save_session(prolog_id, &coire).unwrap();
        let snap = DeductionSnapshot {
            deduction_id:      sid,
            prolog_clauses:    vec!["man(socrates).".into()],
            clips_constructs:  vec![],
            clips_file:        None,
            initial_goal:      Some("mortal(X)".into()),
            max_cycles:        50,
            status:            "Converged".into(),
            cycles_run:        3,
            prolog_session_id: prolog_id,
            clips_session_id:  clips_id,
            created_at_ms:     expires_at_ms - 1000,
            expires_at_ms,
        };
        store.save_snapshot(&snap).unwrap();
        (snap, prolog_id, clips_id)
    }

    #[test]
    fn save_and_load_snapshot() {
        let (store, _f) = tmp_store();
        let now_ms = 1_000_000_i64;
        let (snap, _, _) = make_snapshot(&store, now_ms + 86_400_000);

        let loaded = store.load_snapshot(snap.deduction_id).unwrap().unwrap();
        assert_eq!(loaded.deduction_id, snap.deduction_id);
        assert_eq!(loaded.prolog_clauses, snap.prolog_clauses);
        assert_eq!(loaded.status, "Converged");
        assert_eq!(loaded.cycles_run, 3);
        assert_eq!(loaded.initial_goal, Some("mortal(X)".into()));
    }

    #[test]
    fn load_snapshot_missing_returns_none() {
        let (store, _f) = tmp_store();
        let result = store.load_snapshot(Uuid::new_v4()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn delete_snapshot_removes_coire_events() {
        let (store, _f) = tmp_store();
        let (snap, prolog_id, _) = make_snapshot(&store, 9_999_999_999);

        // Coire events exist before delete
        assert!(!store.read_session(prolog_id).unwrap().is_empty());

        let deleted = store.delete_snapshot(snap.deduction_id).unwrap();
        assert!(deleted > 0);

        // Snapshot gone
        assert!(store.load_snapshot(snap.deduction_id).unwrap().is_none());
        // Coire events gone
        assert!(store.read_session(prolog_id).unwrap().is_empty());
    }

    #[test]
    fn delete_snapshot_missing_returns_zero() {
        let (store, _f) = tmp_store();
        let n = store.delete_snapshot(Uuid::new_v4()).unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn snapshots_expired_filters_correctly() {
        let (store, _f) = tmp_store();
        let now_ms = 1_000_000_000_i64;
        let (expired, _, _) = make_snapshot(&store, now_ms - 1000);   // expired
        let (fresh,   _, _) = make_snapshot(&store, now_ms + 86_400_000); // not yet

        let found = store.snapshots_expired(now_ms, &HashSet::new()).unwrap();
        let ids: Vec<_> = found.iter().map(|s| s.deduction_id).collect();
        assert!(ids.contains(&expired.deduction_id), "expired snapshot should appear");
        assert!(!ids.contains(&fresh.deduction_id), "fresh snapshot must not appear");
    }

    #[test]
    fn snapshots_expired_skips_active_sessions() {
        let (store, _f) = tmp_store();
        let now_ms = 1_000_000_000_i64;
        let (snap, prolog_id, _) = make_snapshot(&store, now_ms - 1000);

        let exclude = HashSet::from([prolog_id]);
        let found = store.snapshots_expired(now_ms, &exclude).unwrap();
        assert!(found.is_empty(), "active session must be skipped");
        // The snapshot itself should still be in the store
        assert!(store.load_snapshot(snap.deduction_id).unwrap().is_some());
    }

    #[test]
    fn save_snapshot_is_idempotent() {
        let (store, _f) = tmp_store();
        let (mut snap, _, _) = make_snapshot(&store, 9_999_999_999);
        snap.status = "Interrupted".into();
        store.save_snapshot(&snap).unwrap(); // second save should upsert
        let loaded = store.load_snapshot(snap.deduction_id).unwrap().unwrap();
        assert_eq!(loaded.status, "Interrupted");
    }

    #[test]
    fn clone_shares_store() {
        let (store, _f) = tmp_store();
        let sid = Uuid::new_v4();
        let clone = store.clone();

        store
            .save_session(sid, &make_coire_with_events(sid))
            .unwrap();

        let events = clone.read_session(sid).unwrap();
        assert_eq!(events.len(), 2);
    }
}
