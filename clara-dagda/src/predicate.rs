use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::truth::TruthValue;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredicateEntry {
    pub session_id:    Uuid,
    pub functor:       String,
    pub arity:         u32,
    pub args:          Vec<String>,
    pub truth_value:   TruthValue,
    pub updated_at_ms: i64,
}
