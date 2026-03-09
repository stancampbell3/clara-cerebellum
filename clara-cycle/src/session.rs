use clara_clips::ClipsEnvironment;
use clara_dagda::{Binding, Dagda, Kind, PredicateEntry, TruthValue};
use clara_prolog::PrologEnvironment;
use uuid::Uuid;

use crate::error::CycleError;
use crate::transduction::{BodyGoal, parse_prolog_rules};
use crate::transpile::{parse_prolog_term, render_prolog_term, Term};

/// A paired Prolog + CLIPS environment for a single deduction run.
///
/// Each deduction gets its own fresh engine pair so sessions are fully isolated.
/// Both engines are automatically assigned Coire session UUIDs by their
/// respective `::new()` constructors.
///
/// A `Dagda` tableau is also created per session and populated at seed time
/// with initial truth values derived from the seeded Prolog clauses.
pub struct DeductionSession {
    pub prolog:    PrologEnvironment,
    pub clips:     ClipsEnvironment,
    pub prolog_id: Uuid,
    pub clips_id:  Uuid,
    /// Live deduction tableau: tracks predicate truth values and bindings.
    pub tableau:   Dagda,
}

impl DeductionSession {
    /// Create a fresh Prolog + CLIPS engine pair with an empty tableau.
    pub fn new() -> Result<Self, CycleError> {
        let prolog    = PrologEnvironment::new()?;
        let clips     = ClipsEnvironment::new().map_err(CycleError::SessionCreationFailed)?;
        let prolog_id = prolog.session_id();
        let clips_id  = clips.session_id();
        let tableau   = Dagda::new().map_err(|e| {
            CycleError::SessionCreationFailed(format!("tableau init failed: {e}"))
        })?;
        Ok(Self { prolog, clips, prolog_id, clips_id, tableau })
    }

    /// Load Prolog clauses into the Prolog engine and populate the tableau.
    ///
    /// All clauses are joined and loaded via `consult_string`.  The same
    /// source text is parsed to build initial `Unknown` entries for every
    /// rule head and body goal, and `KnownTrue` entries for bare facts.
    pub fn seed_prolog(&mut self, clauses: &[String]) -> Result<(), CycleError> {
        if clauses.is_empty() {
            return Ok(());
        }
        let code = clauses.join("\n");
        log::debug!("Seeding Prolog with clauses:\n{}", code);
        self.prolog.consult_string(&code)?;
        self.seed_tableau_from_source(&code);
        Ok(())
    }

    /// Load a `.clp` file into the CLIPS engine by server-side path.
    pub fn seed_clips_file(&mut self, path: &str) -> Result<(), CycleError> {
        self.clips.load(path).map_err(CycleError::Clips)
    }

    /// Load CLIPS constructs (`defrule`, `deftemplate`, etc.) into the CLIPS engine.
    pub fn seed_clips(&mut self, constructs: &[String]) -> Result<(), CycleError> {
        for construct in constructs {
            self.clips.build(construct).map_err(CycleError::Clips)?;
        }
        Ok(())
    }

    /// Inject conversational context (external message history) into the Prolog engine.
    pub fn seed_context(&mut self, context: &[serde_json::Value]) -> Result<(), CycleError> {
        if context.is_empty() {
            return Ok(());
        }
        let json = serde_json::to_string(context)
            .map_err(|e| CycleError::ContextSeedFailed(e.to_string()))?;
        let escaped = json.replace('\'', "\\'");
        self.prolog
            .assertz(&format!("deduce_context_json('{escaped}')"))
            .map_err(CycleError::Prolog)?;
        log::debug!("DeductionSession: seeded {} context message(s)", context.len());
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Tableau initialization
    // -----------------------------------------------------------------------

    /// Parse Prolog source and populate the tableau with initial entries.
    ///
    /// - Bare **facts** (`foo(a,b).`) → `Kind::Predicate`, `KnownTrue`
    /// - **Rule heads** (`h :- ...`) → `Kind::Rule`, `Unknown`
    /// - **Body goals** → `Kind::Predicate` or `Kind::Condition`, `Unknown`
    ///
    /// Failures are logged and silently skipped so a parse error never blocks
    /// the reasoning cycle.
    fn seed_tableau_from_source(&mut self, source: &str) {
        let rules = parse_prolog_rules(source);
        let session_id = self.prolog_id;

        for rule in &rules {
            let head_functor = term_functor(&rule.head);
            let (head_args, head_vars) = term_args_pattern(&rule.head);
            if rule.body.is_empty() {
                // Bare fact — known true from the start.
                let entry = PredicateEntry {
                    session_id,
                    entry_id: Uuid::new_v4(),
                    functor: head_functor.clone(),
                    arity: head_args.len() as u32,
                    args: head_args.clone(),
                    kind: Kind::Predicate,
                    source: None,
                    bound_vars: head_vars,
                    truth_value: TruthValue::KnownTrue,
                    bindings: concrete_bindings(&rule.head),
                    parent_id: None,
                    updated_at_ms: clara_dagda_now_ms(),
                };
                if let Err(e) = self.tableau.set_entry(&entry) {
                    log::warn!("tableau: failed to insert fact {}: {}", head_functor, e);
                }
            } else {
                // Rule head — truth unknown until body goals are evaluated.
                let head_entry = PredicateEntry {
                    session_id,
                    entry_id: Uuid::new_v4(),
                    functor: head_functor.clone(),
                    arity: head_args.len() as u32,
                    args: head_args.clone(),
                    kind: Kind::Rule,
                    source: None,
                    bound_vars: head_vars,
                    truth_value: TruthValue::Unknown,
                    bindings: vec![],
                    parent_id: None,
                    updated_at_ms: clara_dagda_now_ms(),
                };
                if let Err(e) = self.tableau.set_entry(&head_entry) {
                    log::warn!("tableau: failed to insert rule head {}: {}", head_functor, e);
                }

                // Body goals.
                for goal in &rule.body {
                    let term = match goal {
                        BodyGoal::Positive(t) => t,
                        BodyGoal::Negative(t) => t,
                    };
                    let goal_functor = term_functor(term);
                    let (goal_args, goal_vars) = term_args_pattern(term);
                    let kind = if is_condition_goal(&goal_functor) {
                        Kind::Condition
                    } else {
                        Kind::Predicate
                    };
                    let goal_entry = PredicateEntry {
                        session_id,
                        entry_id: Uuid::new_v4(),
                        functor: goal_functor.clone(),
                        arity: goal_args.len() as u32,
                        args: goal_args,
                        kind,
                        source: Some(head_functor.clone()),
                        bound_vars: goal_vars,
                        truth_value: TruthValue::Unknown,
                        bindings: vec![],
                        parent_id: Some(head_entry.entry_id),
                        updated_at_ms: clara_dagda_now_ms(),
                    };
                    if let Err(e) = self.tableau.set_entry(&goal_entry) {
                        log::warn!(
                            "tableau: failed to insert body goal {} (in {}): {}",
                            goal_functor, head_functor, e
                        );
                    }
                }
            }
        }

        log::debug!(
            "DeductionSession: tableau seeded with {} rule(s) from source",
            rules.len()
        );
    }

    // -----------------------------------------------------------------------
    // Live tableau updates
    // -----------------------------------------------------------------------

    /// Update the tableau from a single Coire event payload.
    ///
    /// Handles "assert" (→ KnownTrue) and "retract" (→ KnownFalse) payloads.
    /// All other event types (e.g. "goal") are silently ignored.
    /// Parse failures are logged at DEBUG and silently skipped.
    pub fn record_event_in_tableau(&mut self, payload: &serde_json::Value) {
        let ev_type = payload.get("type").and_then(|v| v.as_str());
        let data    = payload.get("data").and_then(|v| v.as_str());
        let (Some(ev_type), Some(data)) = (ev_type, data) else { return; };

        let truth = match ev_type {
            "assert"  => TruthValue::KnownTrue,
            "retract" => TruthValue::KnownFalse,
            _         => return,
        };

        let term = match parse_prolog_term(data) {
            Ok(t)  => t,
            Err(e) => {
                log::debug!("tableau: could not parse event data {:?}: {}", data, e);
                return;
            }
        };

        let functor     = term_functor(&term);
        let ground_args = extract_ground_args(&term);
        let args_ref: Vec<&str> = ground_args.iter().map(String::as_str).collect();

        // 1. Upsert a ground entry for the exact fact.
        if let Err(e) = self.tableau.update_truth(
            self.prolog_id, &functor, &args_ref, truth.clone(), &[],
        ) {
            log::warn!("tableau: update_truth failed for {}: {}", functor, e);
        }

        // 2. If a seeded wildcard entry exists for this functor/arity, update it
        //    and populate variable bindings by zipping bound_vars with ground args.
        let arity = ground_args.len() as u32;
        let wildcard: Vec<&str> = (0..arity).map(|_| "*").collect();
        if let Ok(Some(seed)) = self.tableau.get_entry(self.prolog_id, &functor, &wildcard) {
            let bindings: Vec<Binding> = seed.bound_vars.iter()
                .zip(ground_args.iter())
                .map(|(var, val)| Binding { var: var.clone(), val: val.clone() })
                .collect();
            if let Err(e) = self.tableau.update_truth(
                self.prolog_id, &functor, &wildcard, truth, &bindings,
            ) {
                log::warn!("tableau: wildcard update failed for {}: {}", functor, e);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Term helpers (private)
// ---------------------------------------------------------------------------

/// Extract the top-level functor name of a term.
fn term_functor(term: &Term) -> String {
    match term {
        Term::Atom(s) | Term::Str(s) => s.clone(),
        Term::Variable(s) => s.clone(),
        Term::Integer(n) => n.to_string(),
        Term::Float(f) => f.to_string(),
        Term::Compound { functor, .. } => functor.clone(),
    }
}

/// Render each argument of a term as a Prolog term string.
fn extract_ground_args(term: &Term) -> Vec<String> {
    match term {
        Term::Compound { args, .. } => args.iter().map(|a| render_prolog_term(a)).collect(),
        _                           => vec![],
    }
}

/// Build the `(args_pattern, bound_var_names)` for a term.
///
/// Variables become `"*"` in the pattern; everything else is its string value.
fn term_args_pattern(term: &Term) -> (Vec<String>, Vec<String>) {
    let raw_args = match term {
        Term::Compound { args, .. } => args.as_slice(),
        _ => &[],
    };
    let mut pattern = Vec::with_capacity(raw_args.len());
    let mut vars = Vec::new();
    for arg in raw_args {
        if let Term::Variable(v) = arg {
            pattern.push("*".to_string());
            if !vars.contains(v) {
                vars.push(v.clone());
            }
        } else {
            pattern.push(render_prolog_term(arg));
        }
    }
    (pattern, vars)
}

/// For a bare fact term, produce concrete bindings (no variable substitution).
fn concrete_bindings(term: &Term) -> Vec<Binding> {
    let raw_args = match term {
        Term::Compound { args, .. } => args.as_slice(),
        _ => &[],
    };
    // Facts have ground args — no variable bindings to record at init.
    // (Future: record ground arg values if useful for query.)
    let _ = raw_args;
    vec![]
}

/// Returns true for arithmetic/comparison goals that should be `Kind::Condition`.
fn is_condition_goal(functor: &str) -> bool {
    matches!(
        functor,
        "==" | "\\=" | "=" | "\\==" | "<" | ">" | ">=" | "=<"
            | "=:=" | "=\\=" | "is" | "@<" | "@>" | "@=<" | "@>="
    )
}

/// Millisecond timestamp used when building tableau entries from the session
/// (mirrors the private `now_ms()` in clara-dagda).
fn clara_dagda_now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before epoch")
        .as_millis() as i64
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_session() -> DeductionSession {
        DeductionSession::new().expect("DeductionSession::new failed")
    }

    #[test]
    fn record_assert_creates_ground_entry() {
        let mut session = make_session();
        let payload = json!({"type": "assert", "data": "defcon(4)"});
        session.record_event_in_tableau(&payload);
        let tv = session.tableau.get(session.prolog_id, "defcon", &["4"]).unwrap();
        assert_eq!(tv, TruthValue::KnownTrue);
    }

    #[test]
    fn record_assert_updates_seeded_wildcard() {
        let mut session = make_session();
        // Pre-seed a wildcard entry as a rule body goal.
        let seed = PredicateEntry {
            session_id:   session.prolog_id,
            entry_id:     uuid::Uuid::new_v4(),
            functor:      "commie".into(),
            arity:        1,
            args:         vec!["*".into()],
            kind:         Kind::Predicate,
            source:       None,
            bound_vars:   vec!["Bastard".into()],
            truth_value:  TruthValue::Unknown,
            bindings:     vec![],
            parent_id:    None,
            updated_at_ms: 0,
        };
        session.tableau.set_entry(&seed).unwrap();

        let payload = json!({"type": "assert", "data": "commie(mary)"});
        session.record_event_in_tableau(&payload);

        // Ground entry should be KnownTrue.
        let tv = session.tableau.get(session.prolog_id, "commie", &["mary"]).unwrap();
        assert_eq!(tv, TruthValue::KnownTrue);

        // Wildcard entry should be updated with bindings.
        let entry = session.tableau.get_entry(session.prolog_id, "commie", &["*"]).unwrap().unwrap();
        assert_eq!(entry.truth_value, TruthValue::KnownTrue);
        assert_eq!(entry.bindings, vec![Binding { var: "Bastard".into(), val: "mary".into() }]);
    }

    #[test]
    fn record_retract_marks_known_false() {
        let mut session = make_session();
        let payload = json!({"type": "retract", "data": "commie(josef)"});
        session.record_event_in_tableau(&payload);
        let tv = session.tableau.get(session.prolog_id, "commie", &["josef"]).unwrap();
        assert_eq!(tv, TruthValue::KnownFalse);
    }

    #[test]
    fn record_ignores_goal_events() {
        let mut session = make_session();
        let payload = json!({"type": "goal", "data": "(do-for-all-facts ((?f foo)) TRUE (retract ?f))"});
        session.record_event_in_tableau(&payload);
        // No entry should have been created for a "goal" event type.
        let tv = session.tableau.get(session.prolog_id, "do-for-all-facts", &[]).unwrap();
        assert_eq!(tv, TruthValue::Unknown);
    }

    #[test]
    fn record_ignores_malformed_data() {
        let mut session = make_session();
        // Garbage data should not panic; it is silently skipped.
        let payload = json!({"type": "assert", "data": "!!!not prolog!!!"});
        session.record_event_in_tableau(&payload);
        // No assertion: we only care that this does not panic.
    }
}
