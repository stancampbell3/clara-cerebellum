//! Error types for the clara-prolog crate

use thiserror::Error;

/// Result type alias for Prolog operations
pub type PrologResult<T> = Result<T, PrologError>;

/// Errors that can occur during Prolog operations
#[derive(Error, Debug)]
pub enum PrologError {
    /// Failed to initialize the Prolog runtime
    #[error("Prolog initialization failed: {0}")]
    InitializationFailed(String),

    /// Failed to create a new Prolog engine
    #[error("Failed to create Prolog engine: {0}")]
    EngineCreationFailed(String),

    /// Failed to set/switch engine context
    #[error("Failed to set engine context: code {0}")]
    EngineSetFailed(i32),

    /// Failed to parse a Prolog term/goal
    #[error("Failed to parse Prolog term: {0}")]
    ParseError(String),

    /// Query execution failed
    #[error("Query failed: {0}")]
    QueryFailed(String),

    /// Prolog raised an exception during execution
    #[error("Prolog exception: {0}")]
    PrologException(String),

    /// Failed to convert between Rust and Prolog types
    #[error("Type conversion error: {0}")]
    ConversionError(String),

    /// Invalid UTF-8 in string data
    #[error("Invalid UTF-8: {0}")]
    Utf8Error(#[from] std::str::Utf8Error),

    /// Null pointer encountered
    #[error("Null pointer error: {0}")]
    NullPointer(String),

    /// JSON serialization/deserialization error
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    /// Generic internal error
    #[error("Internal error: {0}")]
    Internal(String),
}

impl From<String> for PrologError {
    fn from(s: String) -> Self {
        PrologError::Internal(s)
    }
}

impl From<&str> for PrologError {
    fn from(s: &str) -> Self {
        PrologError::Internal(s.to_string())
    }
}
