use thiserror::Error;

#[derive(Debug, Error)]
pub enum RitualError {
    #[error("invalid topic name: {0}")]
    InvalidTopicName(String),

    #[error("topic not found: {0}")]
    TopicNotFound(String),

    #[error("broker error: {0}")]
    BrokerError(String),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}
