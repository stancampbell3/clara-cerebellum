use thiserror::Error;

#[derive(Debug, Error)]
#[allow(dead_code)]
pub enum PitsnakeError {
    #[error("LSP process I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("LSP JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("LSP request timeout after {timeout_secs}s for method '{method}'")]
    Timeout { method: String, timeout_secs: u64 },

    #[error("LSP server returned error {code}: {message}")]
    LspError { code: i32, message: String },

    #[error("LSP process died unexpectedly")]
    ProcessDied,

    #[error("Missing required argument: {0}")]
    MissingArgument(String),

    #[error("File not found: {0}")]
    FileNotFound(String),

    #[error("Channel send error: {0}")]
    ChannelError(String),
}
