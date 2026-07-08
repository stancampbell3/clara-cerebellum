use std::collections::HashMap;

use crate::envelope::RitualConfig;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq)]
pub enum RitualState {
    Active,
    Terminated,
}

impl RitualState {
    /// Stable string form used in the persisted `rituals` table.
    pub fn as_str(&self) -> &'static str {
        match self {
            RitualState::Active     => "active",
            RitualState::Terminated => "terminated",
        }
    }

    /// Inverse of [`as_str`]. Returns `None` for unknown strings.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "active"     => Some(RitualState::Active),
            "terminated" => Some(RitualState::Terminated),
            _            => None,
        }
    }
}

pub struct Ritual {
    pub ritual_id: Uuid,
    pub config:    RitualConfig,
    pub state:     RitualState,
    /// Pre-computed Kafka topic name for this Ritual.
    pub topic:     String,
    /// Maps stable participant keys (e.g. FieryPit URL or caller-supplied ID)
    /// to the `performance_id` issued for that participant.
    /// Used to make `GET /ritual/{id}/join` idempotent: the same key always
    /// receives the same `performance_id`. Anonymous joins (no key) always
    /// receive a fresh `performance_id` and are not recorded here.
    pub participants: HashMap<String, Uuid>,
}
