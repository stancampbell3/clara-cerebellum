use serde::{Deserialize, Serialize};

/// The role a predicate entry plays in the tableau.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// A Prolog rule head (`:- body`).
    Rule,
    /// A concrete predicate — either a base fact or a body goal being evaluated.
    Predicate,
    /// A built-in or arithmetic condition in a rule body (e.g. `X == 4`, `X > 0`).
    Condition,
}

impl Kind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Rule      => "rule",
            Self::Predicate => "predicate",
            Self::Condition => "condition",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "rule"      => Some(Self::Rule),
            "predicate" => Some(Self::Predicate),
            "condition" => Some(Self::Condition),
            _ => None,
        }
    }
}
