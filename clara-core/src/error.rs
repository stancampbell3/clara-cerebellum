use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Main error type for Clara Cerebrum operations
#[derive(Error, Debug)]
pub enum ClaraError {
    // Session errors
    #[error("Session not found: {0}")]
    SessionNotFound(String),

    #[error("Session already exists: {0}")]
    SessionAlreadyExists(String),

    #[error("Too many sessions for user")]
    UserSessionLimitExceeded,

    #[error("Global session limit exceeded")]
    GlobalSessionLimitExceeded,

    #[error("Session terminated")]
    SessionTerminated,

    // Evaluation errors
    #[error("Evaluation failed: {0}")]
    EvalFailed(String),

    #[error("Evaluation timeout after {timeout_ms}ms")]
    EvalTimeout { timeout_ms: u64 },

    #[error("Command not found: {0}")]
    CommandNotFound(String),

    #[error("Syntax error in scripts-dev: {0}")]
    SyntaxError(String),

    #[error("Runtime error: {0}")]
    RuntimeError(String),

    // Resource errors
    #[error("Resource limit exceeded: {resource}")]
    ResourceLimitExceeded { resource: String },

    #[error("Memory limit exceeded")]
    MemoryLimitExceeded,

    // Security errors
    #[error("Security violation: {0}")]
    SecurityViolation(String),

    #[error("Command blocked: {0}")]
    CommandBlocked(String),

    #[error("Invalid file path: {0}")]
    InvalidFilePath(String),

    #[error("File access denied: {0}")]
    FileAccessDenied(String),

    // Input validation errors
    #[error("Validation error: {0}")]
    ValidationError(String),

    #[error("Invalid request body: {0}")]
    InvalidRequestBody(String),

    #[error("Missing required field: {0}")]
    MissingField(String),

    // Concurrency errors
    #[error("Concurrency limit exceeded")]
    ConcurrencyLimitExceeded,

    #[error("Queue full, please retry later")]
    QueueFull,

    // Process/subprocess errors
    #[error("Subprocess error: {0}")]
    SubprocessError(String),

    #[error("Failed to spawn process: {0}")]
    ProcessSpawnError(String),

    #[error("Failed to communicate with subprocess: {0}")]
    ProcessCommunicationError(String),

    #[error("Subprocess crashed")]
    SubprocessCrashed,

    // Configuration errors
    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Missing configuration: {0}")]
    MissingConfig(String),

    // Database errors (if DB feature enabled)
    #[error("Database error: {0}")]
    DatabaseError(String),

    // Internal errors
    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Lock poisoned")]
    LockPoisoned,

    // Generic errors
    #[error("{0}")]
    Other(String),
}

impl ClaraError {
    /// Get the HTTP status code for this error
    pub fn status_code(&self) -> u16 {
        match self {
            // 400 Bad Request
            ClaraError::ValidationError(_)
            | ClaraError::InvalidRequestBody(_)
            | ClaraError::MissingField(_)
            | ClaraError::SyntaxError(_)
            | ClaraError::InvalidFilePath(_) => 400,

            // 401 Unauthorized (if needed later for auth)
            // ClaraError::Unauthorized => 401,

            // 403 Forbidden
            ClaraError::SecurityViolation(_)
            | ClaraError::CommandBlocked(_)
            | ClaraError::FileAccessDenied(_) => 403,

            // 404 Not Found
            ClaraError::SessionNotFound(_)
            | ClaraError::CommandNotFound(_) => 404,

            // 409 Conflict
            ClaraError::SessionAlreadyExists(_) => 409,

            // 429 Too Many Requests
            ClaraError::UserSessionLimitExceeded
            | ClaraError::GlobalSessionLimitExceeded
            | ClaraError::ConcurrencyLimitExceeded
            | ClaraError::QueueFull => 429,

            // 500 Internal Server Error
            ClaraError::Internal(_)
            | ClaraError::LockPoisoned
            | ClaraError::SubprocessError(_)
            | ClaraError::ProcessSpawnError(_)
            | ClaraError::ProcessCommunicationError(_)
            | ClaraError::SubprocessCrashed
            | ClaraError::DatabaseError(_) => 500,

            // 503 Service Unavailable
            ClaraError::EvalTimeout { .. } => 504,

            // Other runtime errors
            ClaraError::EvalFailed(_)
            | ClaraError::RuntimeError(_)
            | ClaraError::ResourceLimitExceeded { .. }
            | ClaraError::MemoryLimitExceeded
            | ClaraError::SessionTerminated
            | ClaraError::ConfigError(_)
            | ClaraError::MissingConfig(_)
            | ClaraError::Other(_) => 500,
        }
    }

    /// Get an error type string suitable for API responses
    pub fn error_type(&self) -> String {
        match self {
            ClaraError::SessionNotFound(_) => "SessionNotFound",
            ClaraError::SessionAlreadyExists(_) => "SessionAlreadyExists",
            ClaraError::UserSessionLimitExceeded => "UserSessionLimitExceeded",
            ClaraError::GlobalSessionLimitExceeded => "GlobalSessionLimitExceeded",
            ClaraError::SessionTerminated => "SessionTerminated",
            ClaraError::EvalFailed(_) => "EvalFailed",
            ClaraError::EvalTimeout { .. } => "EvalTimeout",
            ClaraError::CommandNotFound(_) => "CommandNotFound",
            ClaraError::SyntaxError(_) => "SyntaxError",
            ClaraError::RuntimeError(_) => "RuntimeError",
            ClaraError::ResourceLimitExceeded { .. } => "ResourceLimitExceeded",
            ClaraError::MemoryLimitExceeded => "MemoryLimitExceeded",
            ClaraError::SecurityViolation(_) => "SecurityViolation",
            ClaraError::CommandBlocked(_) => "CommandBlocked",
            ClaraError::InvalidFilePath(_) => "InvalidFilePath",
            ClaraError::FileAccessDenied(_) => "FileAccessDenied",
            ClaraError::ValidationError(_) => "ValidationError",
            ClaraError::InvalidRequestBody(_) => "InvalidRequestBody",
            ClaraError::MissingField(_) => "MissingField",
            ClaraError::ConcurrencyLimitExceeded => "ConcurrencyLimitExceeded",
            ClaraError::QueueFull => "QueueFull",
            ClaraError::SubprocessError(_) => "SubprocessError",
            ClaraError::ProcessSpawnError(_) => "ProcessSpawnError",
            ClaraError::ProcessCommunicationError(_) => "ProcessCommunicationError",
            ClaraError::SubprocessCrashed => "SubprocessCrashed",
            ClaraError::ConfigError(_) => "ConfigError",
            ClaraError::MissingConfig(_) => "MissingConfig",
            ClaraError::DatabaseError(_) => "DatabaseError",
            ClaraError::Internal(_) => "InternalError",
            ClaraError::LockPoisoned => "LockPoisoned",
            ClaraError::Other(_) => "UnknownError",
        }
        .to_string()
    }
}

/// Result type alias for Clara operations
pub type ClaraResult<T> = Result<T, ClaraError>;

/// Error response that matches the API design
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
    pub error_type: String,
    pub details: String,
    pub code: u16,
}

impl ErrorResponse {
    /// Create an error response from a ClaraError
    pub fn from_error(error: &ClaraError) -> Self {
        Self {
            error: error.to_string(),
            error_type: error.error_type(),
            details: error.to_string(),
            code: error.status_code(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_status_codes() {
        assert_eq!(
            ClaraError::ValidationError("test".to_string()).status_code(),
            400
        );
        assert_eq!(
            ClaraError::SessionNotFound("sess-1".to_string()).status_code(),
            404
        );
        assert_eq!(
            ClaraError::SecurityViolation("blocked".to_string()).status_code(),
            403
        );
        assert_eq!(
            ClaraError::UserSessionLimitExceeded.status_code(),
            429
        );
    }

    #[test]
    fn test_error_response() {
        let error = ClaraError::SessionNotFound("sess-123".to_string());
        let response = ErrorResponse::from_error(&error);

        assert_eq!(response.code, 404);
        assert_eq!(response.error_type, "SessionNotFound");
    }
}
