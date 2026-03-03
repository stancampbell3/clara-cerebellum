use thiserror::Error;

#[derive(Debug, Error)]
pub enum DagdaError {
    #[error("DuckDB error: {0}")]
    Duckdb(#[from] duckdb::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Unknown truth value: {0}")]
    UnknownTruthValue(String),
}

pub type DagdaResult<T> = Result<T, DagdaError>;
