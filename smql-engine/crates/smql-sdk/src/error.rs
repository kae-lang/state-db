use crate::types::TransitionDeniedDetail;
use thiserror::Error;

/// Errors returned by the SDK.
#[derive(Debug, Error)]
pub enum SdkError {
    /// HTTP transport error.
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// Server returned an error response.
    #[error("Server error: {message}")]
    Server {
        message: String,
        retryable: bool,
        category: String,
    },

    /// Transition was denied (guard failure).
    #[error("Transition denied: {message}")]
    TransitionDenied {
        message: String,
        /// Structured detail with guard failures, recovery options, etc.
        detail: Option<TransitionDeniedDetail>,
    },

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

impl SdkError {
    /// Returns the recovery options if this is a TransitionDenied error with detail.
    pub fn recovery_options(&self) -> Option<&[crate::types::RecoveryOptionDetail]> {
        if let SdkError::TransitionDenied {
            detail: Some(ref d), ..
        } = self
        {
            Some(&d.recovery_options)
        } else {
            None
        }
    }

    /// Returns the LLM prompt if this is a TransitionDenied error with detail.
    pub fn llm_prompt(&self) -> Option<&str> {
        if let SdkError::TransitionDenied {
            detail: Some(ref d), ..
        } = self
        {
            d.llm_prompt.as_deref()
        } else {
            None
        }
    }

    /// Returns true if the error is retryable.
    pub fn is_retryable(&self) -> bool {
        match self {
            SdkError::Server { retryable, .. } => *retryable,
            _ => false,
        }
    }
}

pub type SdkResult<T> = Result<T, SdkError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = SdkError::Server {
            message: "internal error".to_string(),
            retryable: false,
            category: "internal".to_string(),
        };
        assert_eq!(err.to_string(), "Server error: internal error");

        let err = SdkError::TransitionDenied {
            message: "guard failed".to_string(),
            detail: None,
        };
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

    #[test]
    fn test_transition_denied_with_structured_detail() {
        use crate::types::{GuardFailureDetail, RecoveryOptionDetail, TransitionDeniedDetail};

        let detail = TransitionDeniedDetail {
            instance_id: "abc123".to_string(),
            from_state: "open".to_string(),
            to_state: "resolved".to_string(),
            guard_failures: vec![GuardFailureDetail {
                guard_expr: "resolution != NULL".to_string(),
                actual_value: Some("NULL".to_string()),
                expected: Some("non-null value".to_string()),
                hint: Some("Set resolution before resolving".to_string()),
            }],
            recovery_options: vec![RecoveryOptionDetail {
                action: "SET_FIELD".to_string(),
                field: Some("resolution".to_string()),
                suggested_value: Some("fixed".to_string()),
                reason: "Guard requires resolution to be set".to_string(),
                example: Some(
                    r#"TRANSITION Ticket "abc123" TO resolved WITH { resolution: "fixed" }"#
                        .to_string(),
                ),
            }],
            llm_prompt: Some("Set the resolution field before transitioning".to_string()),
            hint: None,
        };

        let err = SdkError::TransitionDenied {
            message: "Transition open -> resolved denied".to_string(),
            detail: Some(detail),
        };

        // recovery_options accessor
        let opts = err.recovery_options().unwrap();
        assert_eq!(opts.len(), 1);
        assert_eq!(opts[0].action, "SET_FIELD");
        assert_eq!(opts[0].field.as_deref(), Some("resolution"));

        // llm_prompt accessor
        assert_eq!(
            err.llm_prompt(),
            Some("Set the resolution field before transitioning")
        );
    }

    #[test]
    fn test_server_error_retryable() {
        let err = SdkError::Server {
            message: "version conflict".to_string(),
            retryable: true,
            category: "conflict".to_string(),
        };
        assert!(err.is_retryable());

        let err = SdkError::Server {
            message: "internal".to_string(),
            retryable: false,
            category: "internal".to_string(),
        };
        assert!(!err.is_retryable());
    }

    #[test]
    fn test_transition_denied_no_detail_accessors() {
        let err = SdkError::TransitionDenied {
            message: "denied".to_string(),
            detail: None,
        };
        assert!(err.recovery_options().is_none());
        assert!(err.llm_prompt().is_none());
        assert!(!err.is_retryable());
    }
}
