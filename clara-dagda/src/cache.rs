use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use duckdb::Connection;
use uuid::Uuid;

use crate::error::{DagdaError, DagdaResult};
use crate::kind::Kind;
use crate::predicate::{Binding, PredicateEntry};
use crate::truth::TruthValue;

// ---------------------------------------------------------------------------
// Schema
// ---------------------------------------------------------------------------

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS dagda_predicates (
    session_id    VARCHAR NOT NULL,
    entry_id      VARCHAR NOT NULL,
    functor       VARCHAR NOT NULL,
    arity         INTEGER NOT NULL,
    args_json     VARCHAR NOT NULL,
    kind          VARCHAR NOT NULL DEFAULT 'predicate',
    source        VARCHAR,
    bound_vars    VARCHAR NOT NULL DEFAULT '[]',
    truth_value   VARCHAR NOT NULL DEFAULT 'unknown',
    bindings_json VARCHAR NOT NULL DEFAULT '[]',
    parent_id     VARCHAR,
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

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before epoch")
        .as_millis() as i64
}

// ---------------------------------------------------------------------------
// Dagda
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct Dagda {
    conn: Arc<Mutex<Connection>>,
}

impl Dagda {
    /// Create a fresh in-memory tableau with schema applied.
    pub fn new() -> DagdaResult<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn: Arc::new(Mutex::new(conn)) })
    }

    // -----------------------------------------------------------------------
    // Simple upsert / get (backwards-compatible with original API)
    // -----------------------------------------------------------------------

    /// Upsert a predicate's truth value. Explicit Unknown rows are stored.
    ///
    /// Uses `entry_id = functor + args_json` hash (deterministic) when no full
    /// entry is provided, so repeated calls are idempotent.
    pub fn set(
        &self,
        session_id: Uuid,
        functor: &str,
        args: &[&str],
        truth: TruthValue,
    ) -> DagdaResult<()> {
        let args_json = serde_json::to_string(args)?;
        let arity = args.len() as i32;
        let entry_id = Uuid::new_v4().to_string();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO dagda_predicates
                 (session_id, entry_id, functor, arity, args_json,
                  kind, source, bound_vars, truth_value, bindings_json,
                  parent_id, updated_at_ms)
             VALUES (?, ?, ?, ?, ?, 'predicate', NULL, '[]', ?, '[]', NULL, ?)
             ON CONFLICT (session_id, functor, args_json) DO UPDATE SET
                 truth_value   = excluded.truth_value,
                 updated_at_ms = excluded.updated_at_ms",
            duckdb::params![
                session_id.to_string(),
                entry_id,
                functor,
                arity,
                args_json,
                truth.as_str(),
                now_ms(),
            ],
        )?;
        Ok(())
    }

    /// Insert or replace a complete [`PredicateEntry`].
    ///
    /// Unlike [`set`], this preserves all tableau fields (kind, source,
    /// bound_vars, bindings, parent_id).  On conflict the entire row is
    /// updated.
    pub fn set_entry(&self, entry: &PredicateEntry) -> DagdaResult<()> {
        let args_json   = serde_json::to_string(&entry.args)?;
        let bound_json  = serde_json::to_string(&entry.bound_vars)?;
        let bindings_json = serde_json::to_string(&entry.bindings)?;
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO dagda_predicates
                 (session_id, entry_id, functor, arity, args_json,
                  kind, source, bound_vars, truth_value, bindings_json,
                  parent_id, updated_at_ms)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT (session_id, functor, args_json) DO UPDATE SET
                 entry_id      = excluded.entry_id,
                 kind          = excluded.kind,
                 source        = excluded.source,
                 bound_vars    = excluded.bound_vars,
                 truth_value   = excluded.truth_value,
                 bindings_json = excluded.bindings_json,
                 parent_id     = excluded.parent_id,
                 updated_at_ms = excluded.updated_at_ms",
            duckdb::params![
                entry.session_id.to_string(),
                entry.entry_id.to_string(),
                entry.functor,
                entry.arity as i32,
                args_json,
                entry.kind.as_str(),
                entry.source.as_deref(),
                bound_json,
                entry.truth_value.as_str(),
                bindings_json,
                entry.parent_id.map(|u| u.to_string()),
                entry.updated_at_ms,
            ],
        )?;
        Ok(())
    }

    /// Update only the truth value and bindings of an existing entry.
    ///
    /// If no matching row exists, a new minimal `Predicate` row is created.
    pub fn update_truth(
        &self,
        session_id: Uuid,
        functor: &str,
        args: &[&str],
        truth: TruthValue,
        bindings: &[Binding],
    ) -> DagdaResult<()> {
        let args_json     = serde_json::to_string(args)?;
        let bindings_json = serde_json::to_string(bindings)?;
        let entry_id      = Uuid::new_v4().to_string();
        let arity         = args.len() as i32;
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO dagda_predicates
                 (session_id, entry_id, functor, arity, args_json,
                  kind, source, bound_vars, truth_value, bindings_json,
                  parent_id, updated_at_ms)
             VALUES (?, ?, ?, ?, ?, 'predicate', NULL, '[]', ?, ?, NULL, ?)
             ON CONFLICT (session_id, functor, args_json) DO UPDATE SET
                 truth_value   = excluded.truth_value,
                 bindings_json = excluded.bindings_json,
                 updated_at_ms = excluded.updated_at_ms",
            duckdb::params![
                session_id.to_string(),
                entry_id,
                functor,
                arity,
                args_json,
                truth.as_str(),
                bindings_json,
                now_ms(),
            ],
        )?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Read
    // -----------------------------------------------------------------------

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
            TruthValue::from_str(&s).ok_or_else(|| DagdaError::UnknownTruthValue(s))
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
            "SELECT session_id, entry_id, functor, arity, args_json,
                    kind, source, bound_vars, truth_value, bindings_json,
                    parent_id, updated_at_ms
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

    // -----------------------------------------------------------------------
    // Delete
    // -----------------------------------------------------------------------

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

    /// Delete all entries for a session. Returns count deleted.
    pub fn clear_session(&self, session_id: Uuid) -> DagdaResult<usize> {
        let conn = self.conn.lock().unwrap();
        let n = conn.execute(
            "DELETE FROM dagda_predicates WHERE session_id = ?",
            duckdb::params![session_id.to_string()],
        )?;
        Ok(n)
    }

    // -----------------------------------------------------------------------
    // List / count
    // -----------------------------------------------------------------------

    /// All entries for a session, ordered by updated_at_ms.
    pub fn list_session(&self, session_id: Uuid) -> DagdaResult<Vec<PredicateEntry>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT session_id, entry_id, functor, arity, args_json,
                    kind, source, bound_vars, truth_value, bindings_json,
                    parent_id, updated_at_ms
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
            "SELECT session_id, entry_id, functor, arity, args_json,
                    kind, source, bound_vars, truth_value, bindings_json,
                    parent_id, updated_at_ms
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
            "SELECT session_id, entry_id, functor, arity, args_json,
                    kind, source, bound_vars, truth_value, bindings_json,
                    parent_id, updated_at_ms
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

    // -----------------------------------------------------------------------
    // Snapshot persistence helpers
    // -----------------------------------------------------------------------

    /// Export all entries for a session as a `Vec<PredicateEntry>` suitable
    /// for inclusion in a `DeductionSnapshot`.
    pub fn export_session(&self, session_id: Uuid) -> DagdaResult<Vec<PredicateEntry>> {
        self.list_session(session_id)
    }

    /// Import entries that were previously exported, re-inserting them
    /// verbatim (preserving their original `session_id` and `entry_id`).
    /// Intended for restoring a tableau from a persisted snapshot.
    pub fn import_session(&self, entries: &[PredicateEntry]) -> DagdaResult<()> {
        for entry in entries {
            self.set_entry(entry)?;
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Convergence helper
    // -----------------------------------------------------------------------

    /// Returns `true` if any entry for the session was updated strictly after
    /// `since_ms`.  Used by the cycle controller to detect tableau progress.
    pub fn tableau_changed_since(&self, session_id: Uuid, since_ms: i64) -> DagdaResult<bool> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT COUNT(*) FROM dagda_predicates
             WHERE session_id = ? AND updated_at_ms > ?",
        )?;
        let mut rows = stmt.query(duckdb::params![session_id.to_string(), since_ms])?;
        let row = rows.next()?.expect("COUNT always returns a row");
        let count: i64 = row.get(0)?;
        Ok(count > 0)
    }
}

// ---------------------------------------------------------------------------
// Row → Entry deserialization
// ---------------------------------------------------------------------------

fn row_to_entry(row: &duckdb::Row<'_>) -> DagdaResult<PredicateEntry> {
    let session_str:   String        = row.get(0)?;
    let entry_id_str:  String        = row.get(1)?;
    let functor:       String        = row.get(2)?;
    let arity:         i32           = row.get(3)?;
    let args_json:     String        = row.get(4)?;
    let kind_str:      String        = row.get(5)?;
    let source:        Option<String> = row.get(6)?;
    let bound_vars_json: String      = row.get(7)?;
    let truth_str:     String        = row.get(8)?;
    let bindings_json: String        = row.get(9)?;
    let parent_str:    Option<String> = row.get(10)?;
    let updated_at_ms: i64           = row.get(11)?;

    let session_id = session_str
        .parse::<Uuid>()
        .map_err(|e| DagdaError::UnknownTruthValue(format!("bad session uuid: {e}")))?;
    let entry_id = entry_id_str
        .parse::<Uuid>()
        .map_err(|e| DagdaError::UnknownTruthValue(format!("bad entry uuid: {e}")))?;
    let parent_id = parent_str
        .map(|s| s.parse::<Uuid>())
        .transpose()
        .map_err(|e| DagdaError::UnknownTruthValue(format!("bad parent uuid: {e}")))?;
    let args: Vec<String> = serde_json::from_str(&args_json)?;
    let bound_vars: Vec<String> = serde_json::from_str(&bound_vars_json)?;
    let bindings: Vec<crate::predicate::Binding> = serde_json::from_str(&bindings_json)?;
    let truth_value = TruthValue::from_str(&truth_str)
        .ok_or_else(|| DagdaError::UnknownTruthValue(truth_str))?;
    let kind = Kind::from_str(&kind_str)
        .ok_or_else(|| DagdaError::UnknownTruthValue(format!("unknown kind: {kind_str}")))?;

    Ok(PredicateEntry {
        session_id,
        entry_id,
        functor,
        arity: arity as u32,
        args,
        kind,
        source,
        bound_vars,
        truth_value,
        bindings,
        parent_id,
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

    fn sid() -> Uuid { Uuid::new_v4() }

    fn make_entry(session_id: Uuid, functor: &str, args: Vec<&str>, kind: Kind, truth: TruthValue) -> PredicateEntry {
        PredicateEntry {
            session_id,
            entry_id: Uuid::new_v4(),
            functor: functor.to_string(),
            arity: args.len() as u32,
            args: args.into_iter().map(String::from).collect(),
            kind,
            source: None,
            bound_vars: vec![],
            truth_value: truth,
            bindings: vec![],
            parent_id: None,
            updated_at_ms: now_ms(),
        }
    }

    #[test]
    fn set_and_get_known_true() {
        let d = Dagda::new().unwrap();
        let s = sid();
        d.set(s, "likes", &["alice", "bob"], TruthValue::KnownTrue).unwrap();
        assert_eq!(d.get(s, "likes", &["alice", "bob"]).unwrap(), TruthValue::KnownTrue);
    }

    #[test]
    fn set_and_get_known_false() {
        let d = Dagda::new().unwrap();
        let s = sid();
        d.set(s, "likes", &["alice", "carol"], TruthValue::KnownFalse).unwrap();
        assert_eq!(d.get(s, "likes", &["alice", "carol"]).unwrap(), TruthValue::KnownFalse);
    }

    #[test]
    fn set_known_unresolved() {
        let d = Dagda::new().unwrap();
        let s = sid();
        d.set(s, "maybe", &["x"], TruthValue::KnownUnresolved).unwrap();
        assert_eq!(d.get(s, "maybe", &["x"]).unwrap(), TruthValue::KnownUnresolved);
    }

    #[test]
    fn set_unknown_explicit_stores_row() {
        let d = Dagda::new().unwrap();
        let s = sid();
        d.set(s, "foo", &["1"], TruthValue::Unknown).unwrap();
        let entry = d.get_entry(s, "foo", &["1"]).unwrap();
        assert!(entry.is_some(), "explicit Unknown should still store a row");
        assert_eq!(entry.unwrap().truth_value, TruthValue::Unknown);
    }

    #[test]
    fn get_missing_defaults_to_unknown() {
        let d = Dagda::new().unwrap();
        let s = sid();
        assert_eq!(d.get(s, "never_set", &["a"]).unwrap(), TruthValue::Unknown);
    }

    #[test]
    fn get_entry_missing_returns_none() {
        let d = Dagda::new().unwrap();
        let s = sid();
        assert!(d.get_entry(s, "never_set", &["a"]).unwrap().is_none());
    }

    #[test]
    fn upsert_updates_truth_value() {
        let d = Dagda::new().unwrap();
        let s = sid();
        d.set(s, "p", &["x"], TruthValue::KnownTrue).unwrap();
        d.set(s, "p", &["x"], TruthValue::KnownFalse).unwrap();
        assert_eq!(d.get(s, "p", &["x"]).unwrap(), TruthValue::KnownFalse);
        assert_eq!(d.list_session(s).unwrap().len(), 1);
    }

    #[test]
    fn set_entry_roundtrip() {
        let d = Dagda::new().unwrap();
        let s = sid();
        let entry = make_entry(s, "launch_missiles", vec![], Kind::Rule, TruthValue::Unknown);
        d.set_entry(&entry).unwrap();
        let got = d.get_entry(s, "launch_missiles", &[]).unwrap().unwrap();
        assert_eq!(got.kind, Kind::Rule);
        assert_eq!(got.truth_value, TruthValue::Unknown);
        assert_eq!(got.entry_id, entry.entry_id);
    }

    #[test]
    fn set_entry_updates_on_conflict() {
        let d = Dagda::new().unwrap();
        let s = sid();
        let mut entry = make_entry(s, "defcon", vec!["*"], Kind::Predicate, TruthValue::Unknown);
        d.set_entry(&entry).unwrap();
        entry.truth_value = TruthValue::KnownTrue;
        entry.bindings = vec![Binding { var: "Level".into(), val: "4".into() }];
        entry.updated_at_ms = now_ms();
        d.set_entry(&entry).unwrap();
        let got = d.get_entry(s, "defcon", &["*"]).unwrap().unwrap();
        assert_eq!(got.truth_value, TruthValue::KnownTrue);
        assert_eq!(got.bindings.len(), 1);
        assert_eq!(got.bindings[0].val, "4");
    }

    #[test]
    fn update_truth_creates_row() {
        let d = Dagda::new().unwrap();
        let s = sid();
        d.update_truth(s, "commie", &["mary"], TruthValue::KnownTrue, &[
            Binding { var: "Bastard".into(), val: "mary".into() },
        ]).unwrap();
        let got = d.get(s, "commie", &["mary"]).unwrap();
        assert_eq!(got, TruthValue::KnownTrue);
    }

    #[test]
    fn update_truth_updates_bindings() {
        let d = Dagda::new().unwrap();
        let s = sid();
        d.set(s, "defcon", &["*"], TruthValue::Unknown).unwrap();
        d.update_truth(s, "defcon", &["*"], TruthValue::KnownTrue, &[
            Binding { var: "Level".into(), val: "4".into() },
        ]).unwrap();
        let got = d.get_entry(s, "defcon", &["*"]).unwrap().unwrap();
        assert_eq!(got.truth_value, TruthValue::KnownTrue);
        assert_eq!(got.bindings[0].var, "Level");
    }

    #[test]
    fn delete_existing_returns_true() {
        let d = Dagda::new().unwrap();
        let s = sid();
        d.set(s, "p", &["a"], TruthValue::KnownTrue).unwrap();
        assert!(d.delete(s, "p", &["a"]).unwrap());
    }

    #[test]
    fn delete_missing_returns_false() {
        let d = Dagda::new().unwrap();
        let s = sid();
        assert!(!d.delete(s, "p", &["a"]).unwrap());
    }

    #[test]
    fn list_session_returns_all_entries() {
        let d = Dagda::new().unwrap();
        let s = sid();
        d.set(s, "p", &["a"], TruthValue::KnownTrue).unwrap();
        d.set(s, "q", &["b"], TruthValue::KnownFalse).unwrap();
        d.set(s, "r", &["c"], TruthValue::KnownUnresolved).unwrap();
        assert_eq!(d.list_session(s).unwrap().len(), 3);
    }

    #[test]
    fn list_by_truth_filters_correctly() {
        let d = Dagda::new().unwrap();
        let s = sid();
        d.set(s, "p", &["a"], TruthValue::KnownTrue).unwrap();
        d.set(s, "q", &["b"], TruthValue::KnownTrue).unwrap();
        d.set(s, "r", &["c"], TruthValue::KnownFalse).unwrap();
        assert_eq!(d.list_by_truth(s, TruthValue::KnownTrue).unwrap().len(), 2);
        assert_eq!(d.list_by_truth(s, TruthValue::KnownFalse).unwrap().len(), 1);
    }

    #[test]
    fn list_by_functor_matches_arity() {
        let d = Dagda::new().unwrap();
        let s = sid();
        d.set(s, "edge", &["a", "b"], TruthValue::KnownTrue).unwrap();
        d.set(s, "edge", &["b", "c"], TruthValue::KnownTrue).unwrap();
        d.set(s, "node", &["a"], TruthValue::KnownTrue).unwrap();
        assert_eq!(d.list_by_functor(s, "edge", 2).unwrap().len(), 2);
        assert_eq!(d.list_by_functor(s, "node", 1).unwrap().len(), 1);
        assert_eq!(d.list_by_functor(s, "edge", 1).unwrap().len(), 0);
    }

    #[test]
    fn clear_session_removes_all() {
        let d = Dagda::new().unwrap();
        let s = sid();
        d.set(s, "p", &["a"], TruthValue::KnownTrue).unwrap();
        d.set(s, "q", &["b"], TruthValue::KnownFalse).unwrap();
        assert_eq!(d.clear_session(s).unwrap(), 2);
        assert_eq!(d.list_session(s).unwrap().len(), 0);
    }

    #[test]
    fn count_by_truth_reflects_state() {
        let d = Dagda::new().unwrap();
        let s = sid();
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
        let s1 = sid();
        let s2 = sid();
        d.set(s1, "p", &["x"], TruthValue::KnownTrue).unwrap();
        d.set(s2, "p", &["x"], TruthValue::KnownFalse).unwrap();
        assert_eq!(d.get(s1, "p", &["x"]).unwrap(), TruthValue::KnownTrue);
        assert_eq!(d.get(s2, "p", &["x"]).unwrap(), TruthValue::KnownFalse);
    }

    #[test]
    fn clone_shares_data() {
        let d1 = Dagda::new().unwrap();
        let d2 = d1.clone();
        let s = sid();
        d1.set(s, "p", &["a"], TruthValue::KnownTrue).unwrap();
        assert_eq!(d2.get(s, "p", &["a"]).unwrap(), TruthValue::KnownTrue);
    }

    #[test]
    fn export_import_roundtrip() {
        let d = Dagda::new().unwrap();
        let s = sid();
        d.set(s, "commie", &["mary"], TruthValue::KnownTrue).unwrap();
        d.set(s, "defcon", &["*"], TruthValue::Unknown).unwrap();
        let exported = d.export_session(s).unwrap();
        assert_eq!(exported.len(), 2);

        // Import into a fresh Dagda instance.
        let d2 = Dagda::new().unwrap();
        d2.import_session(&exported).unwrap();
        assert_eq!(d2.get(s, "commie", &["mary"]).unwrap(), TruthValue::KnownTrue);
        assert_eq!(d2.get(s, "defcon", &["*"]).unwrap(), TruthValue::Unknown);
    }

    #[test]
    fn tableau_changed_since_detects_update() {
        let d = Dagda::new().unwrap();
        let s = sid();
        let before = now_ms();
        // small delay to ensure updated_at_ms > before
        std::thread::sleep(std::time::Duration::from_millis(2));
        d.set(s, "p", &["a"], TruthValue::KnownTrue).unwrap();
        assert!(d.tableau_changed_since(s, before).unwrap());
    }

    #[test]
    fn tableau_changed_since_no_change() {
        let d = Dagda::new().unwrap();
        let s = sid();
        d.set(s, "p", &["a"], TruthValue::KnownTrue).unwrap();
        let after = now_ms() + 1000;
        assert!(!d.tableau_changed_since(s, after).unwrap());
    }
}
