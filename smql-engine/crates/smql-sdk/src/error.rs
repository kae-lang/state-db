use thiserror::Error;

/// Errors returned by the SDK.
#[derive(Debug, Error)]
pub enum SdkError {
    /// HTTP transport error.
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// Server returned an error response.
    #[error("Server error: {0}")]
    Server(String),

    /// Transition was denied (guard failure).
    #[error("Transition denied: {0}")]
    TransitionDenied(String),

    /// Resource not found.
    #[error("Not found: {0}")]
    NotFound(String),

    /// SMQL parse or format error.
    #[error("Parse error: {0}")]
    Parse(String),

    /// WebSocket subscription error.
    #[error("Subscription error: {0}")]
    Subscription(String),

    /// JSON deserialization error.
    #[error("Deserialization error: {0}")]
    Deserialize(String),

    /// Invalid URL.
    #[error("Invalid URL: {0}")]
    InvalidUrl(String),
}

pub type SdkResult<T> = Result<T, SdkError>;
