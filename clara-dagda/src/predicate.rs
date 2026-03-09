use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::kind::Kind;
use crate::truth::TruthValue;

/// A single row in the deduction tableau.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredicateEntry {
    pub session_id:    Uuid,
    /// Stable identifier for this row (UUID). Used as `parent_id` in child entries.
    pub entry_id:      Uuid,
    pub functor:       String,
    pub arity:         u32,
    /// Argument pattern. Unbound variables are represented as `"*"`.
    pub args:          Vec<String>,
    /// Whether this is a rule head, a concrete predicate, or a built-in condition.
    pub kind:          Kind,
    /// The functor of the rule or assertion that introduced this entry, if known.
    pub source:        Option<String>,
    /// Names of Prolog variables that appear in this predicate's arguments.
    pub bound_vars:    Vec<String>,
    pub truth_value:   TruthValue,
    /// Variable bindings discovered so far, e.g. `[{"var": "X", "val": "4"}]`.
    pub bindings:      Vec<Binding>,
    /// Reserved for future explanation tree — ID of the parent tableau entry.
    pub parent_id:     Option<Uuid>,
    pub updated_at_ms: i64,
}

/// A single variable→value binding produced during deduction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Binding {
    pub var: String,
    pub val: String,
}
