use std::sync::{Arc, Mutex};

use duckdb::Connection;
use uuid::Uuid;

use crate::error::{CoireError, CoireResult};
use crate::event::{ClaraEvent, EventStatus};

#[derive(Clone)]
pub struct Coire {
    conn: Arc<Mutex<Connection>>,
}

impl Coire {
    pub fn new() -> CoireResult<Self> {
        let conn = Connection::open_in_memory()?;
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

    pub fn write_event(&self, event: &ClaraEvent) -> CoireResult<()> {
        let payload_str = serde_json::to_string(&event.payload)?;
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO coire_events (event_id, session_id, origin, created_at_ms, payload, status)
             VALUES (?, ?, ?, ?, ?, ?)",
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

    pub fn read_pending(&self, session_id: Uuid) -> CoireResult<Vec<ClaraEvent>> {
        self.query_events(session_id, Some(EventStatus::Pending))
    }

    pub fn read_all(&self, session_id: Uuid) -> CoireResult<Vec<ClaraEvent>> {
        self.query_events(session_id, None)
    }

    fn query_events(
        &self,
        session_id: Uuid,
        status_filter: Option<EventStatus>,
    ) -> CoireResult<Vec<ClaraEvent>> {
        let conn = self.conn.lock().unwrap();
        let sid = session_id.to_string();

        let mut events = Vec::new();

        match status_filter {
            Some(status) => {
                let mut stmt = conn.prepare(
                    "SELECT event_id, session_id, origin, created_at_ms, payload, status
                     FROM coire_events
                     WHERE session_id = ? AND status = ?
                     ORDER BY created_at_ms ASC",
                )?;
                let rows = stmt.query_map(duckdb::params![sid, status.as_str()], |row| {
                    Ok(RawRow {
                        event_id: row.get(0)?,
                        session_id: row.get(1)?,
                        origin: row.get(2)?,
                        created_at_ms: row.get(3)?,
                        payload: row.get(4)?,
                        status: row.get(5)?,
                    })
                })?;
                for row in rows {
                    events.push(raw_to_event(row?)?);
                }
            }
            None => {
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
                for row in rows {
                    events.push(raw_to_event(row?)?);
                }
            }
        }

        Ok(events)
    }

    pub fn mark_processed(&self, event_id: Uuid) -> CoireResult<()> {
        let conn = self.conn.lock().unwrap();
        let updated = conn.execute(
            "UPDATE coire_events SET status = 'processed' WHERE event_id = ? AND status = 'pending'",
            duckdb::params![event_id.to_string()],
        )?;
        if updated == 0 {
            return Err(CoireError::EventNotFound(event_id));
        }
        Ok(())
    }

    /// Read all pending events for a session and atomically mark them as processed.
    /// Returns the events (with status flipped to Processed).
    pub fn poll_pending(&self, session_id: Uuid) -> CoireResult<Vec<ClaraEvent>> {
        let conn = self.conn.lock().unwrap();
        let sid = session_id.to_string();

        // Read pending events
        let mut stmt = conn.prepare(
            "SELECT event_id, session_id, origin, created_at_ms, payload, status
             FROM coire_events
             WHERE session_id = ? AND status = 'pending'
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
        let mut ids = Vec::new();
        for row in rows {
            let raw = row?;
            ids.push(raw.event_id.clone());
            let mut event = raw_to_event(raw)?;
            event.status = EventStatus::Processed;
            events.push(event);
        }

        // Mark them all processed
        if !ids.is_empty() {
            for id in &ids {
                conn.execute(
                    "UPDATE coire_events SET status = 'processed' WHERE event_id = ?",
                    duckdb::params![id],
                )?;
            }
        }

        Ok(events)
    }

    pub fn drain_session(&self, session_id: Uuid) -> CoireResult<usize> {
        let conn = self.conn.lock().unwrap();
        let count = conn.execute(
            "UPDATE coire_events SET status = 'drained' WHERE session_id = ? AND status = 'pending'",
            duckdb::params![session_id.to_string()],
        )?;
        Ok(count)
    }

    pub fn clear_session(&self, session_id: Uuid) -> CoireResult<usize> {
        let conn = self.conn.lock().unwrap();
        let count = conn.execute(
            "DELETE FROM coire_events WHERE session_id = ?",
            duckdb::params![session_id.to_string()],
        )?;
        Ok(count)
    }

    pub fn count_pending(&self, session_id: Uuid) -> CoireResult<usize> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT COUNT(*) FROM coire_events WHERE session_id = ? AND status = 'pending'",
        )?;
        let count: i64 = stmt.query_row(duckdb::params![session_id.to_string()], |row| row.get(0))?;
        Ok(count as usize)
    }
}

struct RawRow {
    event_id: String,
    session_id: String,
    origin: String,
    created_at_ms: i64,
    payload: String,
    status: String,
}

fn raw_to_event(raw: RawRow) -> CoireResult<ClaraEvent> {
    Ok(ClaraEvent {
        event_id: Uuid::parse_str(&raw.event_id).unwrap(),
        session_id: Uuid::parse_str(&raw.session_id).unwrap(),
        origin: raw.origin,
        created_at_ms: raw.created_at_ms,
        payload: serde_json::from_str(&raw.payload)?,
        status: EventStatus::from_str(&raw.status).unwrap_or(EventStatus::Pending),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_event(session_id: Uuid, origin: &str, payload: serde_json::Value) -> ClaraEvent {
        ClaraEvent::new(session_id, origin, payload)
    }

    #[test]
    fn write_read_cycle() {
        let coire = Coire::new().unwrap();
        let sid = Uuid::new_v4();

        let a = make_event(sid, "prolog", json!({"fact": "mortal(socrates)"}));
        coire.write_event(&a).unwrap();

        let pending = coire.read_pending(sid).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].event_id, a.event_id);

        coire.mark_processed(a.event_id).unwrap();

        let b = make_event(sid, "clips", json!({"rule": "fire"}));
        coire.write_event(&b).unwrap();

        let pending = coire.read_pending(sid).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].event_id, b.event_id);

        let all = coire.read_all(sid).unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn session_isolation() {
        let coire = Coire::new().unwrap();
        let s1 = Uuid::new_v4();
        let s2 = Uuid::new_v4();

        coire.write_event(&make_event(s1, "prolog", json!("s1"))).unwrap();
        coire.write_event(&make_event(s2, "clips", json!("s2"))).unwrap();

        let p1 = coire.read_pending(s1).unwrap();
        let p2 = coire.read_pending(s2).unwrap();
        assert_eq!(p1.len(), 1);
        assert_eq!(p2.len(), 1);
        assert_eq!(p1[0].origin, "prolog");
        assert_eq!(p2[0].origin, "clips");
    }

    #[test]
    fn drain_session() {
        let coire = Coire::new().unwrap();
        let sid = Uuid::new_v4();

        coire.write_event(&make_event(sid, "a", json!(1))).unwrap();
        coire.write_event(&make_event(sid, "b", json!(2))).unwrap();

        let drained = coire.drain_session(sid).unwrap();
        assert_eq!(drained, 2);
        assert_eq!(coire.read_pending(sid).unwrap().len(), 0);

        let all = coire.read_all(sid).unwrap();
        assert_eq!(all.len(), 2);
        assert!(all.iter().all(|e| e.status == EventStatus::Drained));
    }

    #[test]
    fn clear_session() {
        let coire = Coire::new().unwrap();
        let sid = Uuid::new_v4();

        coire.write_event(&make_event(sid, "x", json!("bye"))).unwrap();
        let deleted = coire.clear_session(sid).unwrap();
        assert_eq!(deleted, 1);
        assert_eq!(coire.read_all(sid).unwrap().len(), 0);
    }

    #[test]
    fn mark_processed_not_found() {
        let coire = Coire::new().unwrap();
        let ghost = Uuid::new_v4();
        let err = coire.mark_processed(ghost).unwrap_err();
        assert!(matches!(err, CoireError::EventNotFound(_)));
    }

    #[test]
    fn count_pending_reflects_state() {
        let coire = Coire::new().unwrap();
        let sid = Uuid::new_v4();

        assert_eq!(coire.count_pending(sid).unwrap(), 0);

        let e1 = make_event(sid, "a", json!(1));
        coire.write_event(&e1).unwrap();
        assert_eq!(coire.count_pending(sid).unwrap(), 1);

        coire.write_event(&make_event(sid, "b", json!(2))).unwrap();
        assert_eq!(coire.count_pending(sid).unwrap(), 2);

        coire.mark_processed(e1.event_id).unwrap();
        assert_eq!(coire.count_pending(sid).unwrap(), 1);

        coire.drain_session(sid).unwrap();
        assert_eq!(coire.count_pending(sid).unwrap(), 0);
    }

    #[test]
    fn clone_shares_data() {
        let coire = Coire::new().unwrap();
        let sid = Uuid::new_v4();
        let clone = coire.clone();

        coire.write_event(&make_event(sid, "src", json!("shared"))).unwrap();
        let from_clone = clone.read_pending(sid).unwrap();
        assert_eq!(from_clone.len(), 1);
        assert_eq!(from_clone[0].origin, "src");
    }

    #[test]
    fn poll_pending_reads_and_marks() {
        let coire = Coire::new().unwrap();
        let sid = Uuid::new_v4();

        coire.write_event(&make_event(sid, "a", json!(1))).unwrap();
        coire.write_event(&make_event(sid, "b", json!(2))).unwrap();

        let polled = coire.poll_pending(sid).unwrap();
        assert_eq!(polled.len(), 2);
        assert!(polled.iter().all(|e| e.status == EventStatus::Processed));

        // After polling, no pending events remain
        assert_eq!(coire.count_pending(sid).unwrap(), 0);
        assert_eq!(coire.read_pending(sid).unwrap().len(), 0);

        // But all events still exist
        assert_eq!(coire.read_all(sid).unwrap().len(), 2);
    }

    #[test]
    fn poll_pending_empty_session() {
        let coire = Coire::new().unwrap();
        let sid = Uuid::new_v4();
        let polled = coire.poll_pending(sid).unwrap();
        assert!(polled.is_empty());
    }

    #[test]
    fn ordering_by_created_at() {
        let coire = Coire::new().unwrap();
        let sid = Uuid::new_v4();

        let mut e1 = make_event(sid, "first", json!(1));
        e1.created_at_ms = 1000;
        let mut e2 = make_event(sid, "second", json!(2));
        e2.created_at_ms = 2000;
        let mut e3 = make_event(sid, "third", json!(3));
        e3.created_at_ms = 3000;

        // Write out of order
        coire.write_event(&e2).unwrap();
        coire.write_event(&e3).unwrap();
        coire.write_event(&e1).unwrap();

        let events = coire.read_pending(sid).unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].origin, "first");
        assert_eq!(events[1].origin, "second");
        assert_eq!(events[2].origin, "third");
    }
}
