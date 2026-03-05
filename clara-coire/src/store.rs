use std::collections::HashSet;
use std::path::Path;
use std::sync::{Arc, Mutex};

use duckdb::Connection;
use uuid::Uuid;

use crate::coire::{raw_to_event, Coire, RawRow};
use crate::error::CoireResult;
use crate::event::ClaraEvent;

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
                ON coire_events (session_id, status);",
        )?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Save all events for `session_id` from a live `Coire` into this store.
    ///
    /// Events are upserted by `event_id`, so calling this multiple times is safe.
    /// Returns the number of events saved.
    pub fn save_session(&self, session_id: Uuid, coire: &Coire) -> CoireResult<usize> {
        let events = coire.read_all(session_id)?;
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
        // Mark one processed so we confirm status is preserved
        let all = src.read_all(sid).unwrap();
        src.mark_processed(all[0].event_id).unwrap();

        store.save_session(sid, &src).unwrap();

        let dst = Coire::new().unwrap();
        let restored = store.restore_session(sid, &dst).unwrap();
        assert_eq!(restored, 2);

        let events = dst.read_all(sid).unwrap();
        assert_eq!(events.len(), 2);
        // Statuses are preserved
        assert_eq!(events[0].status, EventStatus::Processed);
        assert_eq!(events[1].status, EventStatus::Pending);
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
