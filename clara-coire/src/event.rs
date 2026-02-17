use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventStatus {
    Pending,
    Processed,
    Drained,
}

impl EventStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            EventStatus::Pending => "pending",
            EventStatus::Processed => "processed",
            EventStatus::Drained => "drained",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(EventStatus::Pending),
            "processed" => Some(EventStatus::Processed),
            "drained" => Some(EventStatus::Drained),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaraEvent {
    pub event_id: Uuid,
    pub session_id: Uuid,
    pub origin: String,
    pub created_at_ms: i64,
    pub payload: Value,
    pub status: EventStatus,
}

impl ClaraEvent {
    pub fn new(session_id: Uuid, origin: impl Into<String>, payload: Value) -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let created_at_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        Self {
            event_id: Uuid::new_v4(),
            session_id,
            origin: origin.into(),
            created_at_ms,
            payload,
            status: EventStatus::Pending,
        }
    }
}
