use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TruthValue {
    KnownTrue,
    KnownFalse,
    KnownUnresolved,
    Unknown,
}

impl TruthValue {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::KnownTrue => "known_true",
            Self::KnownFalse => "known_false",
            Self::KnownUnresolved => "known_unresolved",
            Self::Unknown => "unknown",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "known_true" => Some(Self::KnownTrue),
            "known_false" => Some(Self::KnownFalse),
            "known_unresolved" => Some(Self::KnownUnresolved),
            "unknown" => Some(Self::Unknown),
            _ => None,
        }
    }

    /// True for any value that has been explicitly evaluated (not Unknown).
    pub fn is_resolved(&self) -> bool {
        !matches!(self, Self::Unknown)
    }
}
