use thiserror::Error;
use uuid::Uuid;

pub type CoireResult<T> = Result<T, CoireError>;

#[derive(Debug, Error)]
pub enum CoireError {
    #[error("DuckDB error: {0}")]
    Duckdb(#[from] duckdb::Error),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Event not found: {0}")]
    EventNotFound(Uuid),

    #[error("Global Coire already initialized")]
    AlreadyInitialized,
}
