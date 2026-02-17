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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = SdkError::Server("internal error".to_string());
        assert_eq!(err.to_string(), "Server error: internal error");

        let err = SdkError::TransitionDenied("guard failed".to_string());
        assert_eq!(err.to_string(), "Transition denied: guard failed");

        let err = SdkError::NotFound("instance abc not found".to_string());
        assert_eq!(err.to_string(), "Not found: instance abc not found");

        let err = SdkError::Parse("unexpected token".to_string());
        assert_eq!(err.to_string(), "Parse error: unexpected token");

        let err = SdkError::Subscription("connection closed".to_string());
        assert_eq!(err.to_string(), "Subscription error: connection closed");

        let err = SdkError::Deserialize("invalid json".to_string());
        assert_eq!(err.to_string(), "Deserialization error: invalid json");

        let err = SdkError::InvalidUrl("bad url".to_string());
        assert_eq!(err.to_string(), "Invalid URL: bad url");
    }
}
