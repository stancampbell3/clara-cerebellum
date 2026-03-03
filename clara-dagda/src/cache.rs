use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use duckdb::Connection;
use uuid::Uuid;

use crate::error::{DagdaError, DagdaResult};
use crate::predicate::PredicateEntry;
use crate::truth::TruthValue;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS dagda_predicates (
    session_id    VARCHAR NOT NULL,
    functor       VARCHAR NOT NULL,
    arity         INTEGER NOT NULL,
    args_json     VARCHAR NOT NULL,
    truth_value   VARCHAR NOT NULL DEFAULT 'unknown',
    updated_at_ms BIGINT  NOT NULL,
    PRIMARY KEY (session_id, functor, args_json)
);
CREATE INDEX IF NOT EXISTS idx_dagda_session
    ON dagda_predicates (session_id);
CREATE INDEX IF NOT EXISTS idx_dagda_functor
    ON dagda_predicates (session_id, functor, arity);
CREATE INDEX IF NOT EXISTS idx_dagda_truth
    ON dagda_predicates (session_id, truth_value);
";

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before epoch")
        .as_millis() as i64
}

#[derive(Clone)]
pub struct Dagda {
    conn: Arc<Mutex<Connection>>,
}

impl Dagda {
    /// Create a fresh in-memory cache with schema applied.
    pub fn new() -> DagdaResult<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Upsert a predicate's truth value. Explicit Unknown rows are stored.
    pub fn set(
        &self,
        session_id: Uuid,
        functor: &str,
        args: &[&str],
        truth: TruthValue,
    ) -> DagdaResult<()> {
        let args_json = serde_json::to_string(args)?;
        let arity = args.len() as i32;
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO dagda_predicates (session_id, functor, arity, args_json, truth_value, updated_at_ms)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT (session_id, functor, args_json) DO UPDATE SET
                 truth_value   = excluded.truth_value,
                 updated_at_ms = excluded.updated_at_ms",
            duckdb::params![
                session_id.to_string(),
                functor,
                arity,
                args_json,
                truth.as_str(),
                now_ms(),
            ],
        )?;
        Ok(())
    }

    /// Get truth value; returns TruthValue::Unknown if predicate not in cache.
    pub fn get(&self, session_id: Uuid, functor: &str, args: &[&str]) -> DagdaResult<TruthValue> {
        let args_json = serde_json::to_string(args)?;
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT truth_value FROM dagda_predicates
             WHERE session_id = ? AND functor = ? AND args_json = ?",
        )?;
        let mut rows = stmt.query(duckdb::params![
            session_id.to_string(),
            functor,
            args_json,
        ])?;
        if let Some(row) = rows.next()? {
            let s: String = row.get(0)?;
            TruthValue::from_str(&s)
                .ok_or_else(|| DagdaError::UnknownTruthValue(s))
        } else {
            Ok(TruthValue::Unknown)
        }
    }

    /// Get the full entry; returns None if predicate not in cache.
    pub fn get_entry(
        &self,
        session_id: Uuid,
        functor: &str,
        args: &[&str],
    ) -> DagdaResult<Option<PredicateEntry>> {
        let args_json = serde_json::to_string(args)?;
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT session_id, functor, arity, args_json, truth_value, updated_at_ms
             FROM dagda_predicates
             WHERE session_id = ? AND functor = ? AND args_json = ?",
        )?;
        let mut rows = stmt.query(duckdb::params![
            session_id.to_string(),
            functor,
            args_json,
        ])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row_to_entry(row)?))
        } else {
            Ok(None)
        }
    }

    /// Delete a specific predicate. Returns true if a row was deleted.
    pub fn delete(&self, session_id: Uuid, functor: &str, args: &[&str]) -> DagdaResult<bool> {
        let args_json = serde_json::to_string(args)?;
        let conn = self.conn.lock().unwrap();
        let n = conn.execute(
            "DELETE FROM dagda_predicates
             WHERE session_id = ? AND functor = ? AND args_json = ?",
            duckdb::params![session_id.to_string(), functor, args_json],
        )?;
        Ok(n > 0)
    }

    /// All entries for a session, ordered by updated_at_ms.
    pub fn list_session(&self, session_id: Uuid) -> DagdaResult<Vec<PredicateEntry>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT session_id, functor, arity, args_json, truth_value, updated_at_ms
             FROM dagda_predicates
             WHERE session_id = ?
             ORDER BY updated_at_ms",
        )?;
        collect_entries(stmt.query(duckdb::params![session_id.to_string()])?)
    }

    /// All entries for a session with the given truth value.
    pub fn list_by_truth(
        &self,
        session_id: Uuid,
        truth: TruthValue,
    ) -> DagdaResult<Vec<PredicateEntry>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT session_id, functor, arity, args_json, truth_value, updated_at_ms
             FROM dagda_predicates
             WHERE session_id = ? AND truth_value = ?
             ORDER BY updated_at_ms",
        )?;
        collect_entries(stmt.query(duckdb::params![
            session_id.to_string(),
            truth.as_str(),
        ])?)
    }

    /// All entries for a session matching functor + arity.
    pub fn list_by_functor(
        &self,
        session_id: Uuid,
        functor: &str,
        arity: u32,
    ) -> DagdaResult<Vec<PredicateEntry>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT session_id, functor, arity, args_json, truth_value, updated_at_ms
             FROM dagda_predicates
             WHERE session_id = ? AND functor = ? AND arity = ?
             ORDER BY updated_at_ms",
        )?;
        collect_entries(stmt.query(duckdb::params![
            session_id.to_string(),
            functor,
            arity as i32,
        ])?)
    }

    /// Delete all entries for a session. Returns count deleted.
    pub fn clear_session(&self, session_id: Uuid) -> DagdaResult<usize> {
        let conn = self.conn.lock().unwrap();
        let n = conn.execute(
            "DELETE FROM dagda_predicates WHERE session_id = ?",
            duckdb::params![session_id.to_string()],
        )?;
        Ok(n)
    }

    /// Count entries for a session matching the given truth value.
    pub fn count_by_truth(&self, session_id: Uuid, truth: TruthValue) -> DagdaResult<usize> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT COUNT(*) FROM dagda_predicates
             WHERE session_id = ? AND truth_value = ?",
        )?;
        let mut rows = stmt.query(duckdb::params![
            session_id.to_string(),
            truth.as_str(),
        ])?;
        let row = rows.next()?.expect("COUNT always returns a row");
        let count: i64 = row.get(0)?;
        Ok(count as usize)
    }
}

fn row_to_entry(row: &duckdb::Row<'_>) -> DagdaResult<PredicateEntry> {
    let session_str: String = row.get(0)?;
    let functor: String = row.get(1)?;
    let arity: i32 = row.get(2)?;
    let args_json: String = row.get(3)?;
    let truth_str: String = row.get(4)?;
    let updated_at_ms: i64 = row.get(5)?;

    let session_id = session_str
        .parse::<Uuid>()
        .map_err(|e| DagdaError::UnknownTruthValue(format!("bad uuid: {e}")))?;
    let args: Vec<String> = serde_json::from_str(&args_json)?;
    let truth_value = TruthValue::from_str(&truth_str)
        .ok_or_else(|| DagdaError::UnknownTruthValue(truth_str))?;

    Ok(PredicateEntry {
        session_id,
        functor,
        arity: arity as u32,
        args,
        truth_value,
        updated_at_ms,
    })
}

fn collect_entries(mut rows: duckdb::Rows<'_>) -> DagdaResult<Vec<PredicateEntry>> {
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(row_to_entry(row)?);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    fn new_session() -> Uuid {
        Uuid::new_v4()
    }

    #[test]
    fn set_and_get_known_true() {
        let d = Dagda::new().unwrap();
        let s = new_session();
        d.set(s, "likes", &["alice", "bob"], TruthValue::KnownTrue).unwrap();
        assert_eq!(d.get(s, "likes", &["alice", "bob"]).unwrap(), TruthValue::KnownTrue);
    }

    #[test]
    fn set_and_get_known_false() {
        let d = Dagda::new().unwrap();
        let s = new_session();
        d.set(s, "likes", &["alice", "carol"], TruthValue::KnownFalse).unwrap();
        assert_eq!(d.get(s, "likes", &["alice", "carol"]).unwrap(), TruthValue::KnownFalse);
    }

    #[test]
    fn set_known_unresolved() {
        let d = Dagda::new().unwrap();
        let s = new_session();
        d.set(s, "maybe", &["x"], TruthValue::KnownUnresolved).unwrap();
        assert_eq!(d.get(s, "maybe", &["x"]).unwrap(), TruthValue::KnownUnresolved);
    }

    #[test]
    fn set_unknown_explicit_stores_row() {
        let d = Dagda::new().unwrap();
        let s = new_session();
        d.set(s, "foo", &["1"], TruthValue::Unknown).unwrap();
        let entry = d.get_entry(s, "foo", &["1"]).unwrap();
        assert!(entry.is_some(), "explicit Unknown should still store a row");
        assert_eq!(entry.unwrap().truth_value, TruthValue::Unknown);
    }

    #[test]
    fn get_missing_defaults_to_unknown() {
        let d = Dagda::new().unwrap();
        let s = new_session();
        assert_eq!(d.get(s, "never_set", &["a"]).unwrap(), TruthValue::Unknown);
    }

    #[test]
    fn get_entry_missing_returns_none() {
        let d = Dagda::new().unwrap();
        let s = new_session();
        assert!(d.get_entry(s, "never_set", &["a"]).unwrap().is_none());
    }

    #[test]
    fn upsert_updates_truth_value() {
        let d = Dagda::new().unwrap();
        let s = new_session();
        d.set(s, "p", &["x"], TruthValue::KnownTrue).unwrap();
        d.set(s, "p", &["x"], TruthValue::KnownFalse).unwrap();
        assert_eq!(d.get(s, "p", &["x"]).unwrap(), TruthValue::KnownFalse);
        // Only one row (upsert, not insert)
        assert_eq!(d.list_session(s).unwrap().len(), 1);
    }

    #[test]
    fn delete_existing_returns_true() {
        let d = Dagda::new().unwrap();
        let s = new_session();
        d.set(s, "p", &["a"], TruthValue::KnownTrue).unwrap();
        assert!(d.delete(s, "p", &["a"]).unwrap());
    }

    #[test]
    fn delete_missing_returns_false() {
        let d = Dagda::new().unwrap();
        let s = new_session();
        assert!(!d.delete(s, "p", &["a"]).unwrap());
    }

    #[test]
    fn list_session_returns_all_entries() {
        let d = Dagda::new().unwrap();
        let s = new_session();
        d.set(s, "p", &["a"], TruthValue::KnownTrue).unwrap();
        d.set(s, "q", &["b"], TruthValue::KnownFalse).unwrap();
        d.set(s, "r", &["c"], TruthValue::KnownUnresolved).unwrap();
        let entries = d.list_session(s).unwrap();
        assert_eq!(entries.len(), 3);
    }

    #[test]
    fn list_by_truth_filters_correctly() {
        let d = Dagda::new().unwrap();
        let s = new_session();
        d.set(s, "p", &["a"], TruthValue::KnownTrue).unwrap();
        d.set(s, "q", &["b"], TruthValue::KnownTrue).unwrap();
        d.set(s, "r", &["c"], TruthValue::KnownFalse).unwrap();
        let true_entries = d.list_by_truth(s, TruthValue::KnownTrue).unwrap();
        assert_eq!(true_entries.len(), 2);
        let false_entries = d.list_by_truth(s, TruthValue::KnownFalse).unwrap();
        assert_eq!(false_entries.len(), 1);
    }

    #[test]
    fn list_by_functor_matches_arity() {
        let d = Dagda::new().unwrap();
        let s = new_session();
        d.set(s, "edge", &["a", "b"], TruthValue::KnownTrue).unwrap();
        d.set(s, "edge", &["b", "c"], TruthValue::KnownTrue).unwrap();
        d.set(s, "node", &["a"], TruthValue::KnownTrue).unwrap();
        let edges = d.list_by_functor(s, "edge", 2).unwrap();
        assert_eq!(edges.len(), 2);
        let nodes = d.list_by_functor(s, "node", 1).unwrap();
        assert_eq!(nodes.len(), 1);
        // Arity mismatch returns nothing
        let wrong = d.list_by_functor(s, "edge", 1).unwrap();
        assert_eq!(wrong.len(), 0);
    }

    #[test]
    fn clear_session_removes_all() {
        let d = Dagda::new().unwrap();
        let s = new_session();
        d.set(s, "p", &["a"], TruthValue::KnownTrue).unwrap();
        d.set(s, "q", &["b"], TruthValue::KnownFalse).unwrap();
        let deleted = d.clear_session(s).unwrap();
        assert_eq!(deleted, 2);
        assert_eq!(d.list_session(s).unwrap().len(), 0);
    }

    #[test]
    fn count_by_truth_reflects_state() {
        let d = Dagda::new().unwrap();
        let s = new_session();
        d.set(s, "p", &["a"], TruthValue::KnownTrue).unwrap();
        d.set(s, "q", &["b"], TruthValue::KnownTrue).unwrap();
        d.set(s, "r", &["c"], TruthValue::Unknown).unwrap();
        assert_eq!(d.count_by_truth(s, TruthValue::KnownTrue).unwrap(), 2);
        assert_eq!(d.count_by_truth(s, TruthValue::Unknown).unwrap(), 1);
        assert_eq!(d.count_by_truth(s, TruthValue::KnownFalse).unwrap(), 0);
    }

    #[test]
    fn session_isolation() {
        let d = Dagda::new().unwrap();
        let s1 = new_session();
        let s2 = new_session();
        d.set(s1, "p", &["x"], TruthValue::KnownTrue).unwrap();
        d.set(s2, "p", &["x"], TruthValue::KnownFalse).unwrap();
        assert_eq!(d.get(s1, "p", &["x"]).unwrap(), TruthValue::KnownTrue);
        assert_eq!(d.get(s2, "p", &["x"]).unwrap(), TruthValue::KnownFalse);
    }

    #[test]
    fn clone_shares_data() {
        let d1 = Dagda::new().unwrap();
        let d2 = d1.clone();
        let s = new_session();
        d1.set(s, "p", &["a"], TruthValue::KnownTrue).unwrap();
        // d2 sees d1's write because they share the Arc
        assert_eq!(d2.get(s, "p", &["a"]).unwrap(), TruthValue::KnownTrue);
    }
}
