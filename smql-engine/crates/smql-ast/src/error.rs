use serde::{Deserialize, Serialize};
use std::fmt;

use crate::span::Span;

/// The top-level error type for all SMQL operations.
#[derive(Debug, Clone, thiserror::Error, Serialize, Deserialize)]
pub enum SmqlError {
    #[error("Parse error: {message}")]
    ParseError {
        message: String,
        span: Option<Span>,
        hint: Option<String>,
    },

    #[error("Validation error: {message}")]
    ValidationError {
        message: String,
        field: Option<String>,
        hint: Option<String>,
    },

    #[error("Transition denied: {0}")]
    TransitionDenied(TransitionDeniedError),

    #[error("Guard failed: {message}")]
    GuardFailed {
        message: String,
        guard_expr: String,
        actual_value: Option<String>,
        hint: Option<String>,
    },

    #[error("Spawn rejected: {message}")]
    SpawnRejected {
        message: String,
        field: Option<String>,
        hint: Option<String>,
    },

    #[error("Query error: {message}")]
    QueryError {
        message: String,
        hint: Option<String>,
    },

    #[error("Storage error: {message}")]
    StorageError { message: String, retryable: bool },

    #[error("Timeout error: {message}")]
    TimeoutError {
        message: String,
        instance_id: Option<String>,
        state: Option<String>,
    },

    #[error("Not found: {entity_type} '{id}'")]
    NotFound { entity_type: String, id: String },

    #[error("Conflict: {message}")]
    Conflict {
        message: String,
        hint: Option<String>,
    },

    #[error("Internal error: {message}")]
    Internal { message: String },
}

impl SmqlError {
    pub fn parse(message: impl Into<String>) -> Self {
        SmqlError::ParseError {
            message: message.into(),
            span: None,
            hint: None,
        }
    }

    pub fn parse_with_span(message: impl Into<String>, span: Span) -> Self {
        SmqlError::ParseError {
            message: message.into(),
            span: Some(span),
            hint: None,
        }
    }

    pub fn validation(message: impl Into<String>) -> Self {
        SmqlError::ValidationError {
            message: message.into(),
            field: None,
            hint: None,
        }
    }

    pub fn not_found(entity_type: impl Into<String>, id: impl Into<String>) -> Self {
        SmqlError::NotFound {
            entity_type: entity_type.into(),
            id: id.into(),
        }
    }

    pub fn storage(message: impl Into<String>) -> Self {
        SmqlError::StorageError {
            message: message.into(),
            retryable: false,
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        SmqlError::Internal {
            message: message.into(),
        }
    }
}

/// Structured error for denied transitions, including all guard failures.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransitionDeniedError {
    pub instance_id: String,
    pub from_state: String,
    pub to_state: String,
    pub guard_failures: Vec<GuardFailure>,
    pub hint: Option<String>,
}

impl fmt::Display for TransitionDeniedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Transition {} -> {} denied for instance {}",
            self.from_state, self.to_state, self.instance_id
        )?;
        if !self.guard_failures.is_empty() {
            write!(f, ":")?;
            for failure in &self.guard_failures {
                write!(f, "\n  - {}", failure)?;
            }
        }
        if let Some(hint) = &self.hint {
            write!(f, "\n  Hint: {}", hint)?;
        }
        Ok(())
    }
}

/// A single guard failure with context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardFailure {
    pub guard_expr: String,
    pub actual_value: Option<String>,
    pub expected: Option<String>,
    pub hint: Option<String>,
}

impl fmt::Display for GuardFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Guard '{}' failed", self.guard_expr)?;
        if let Some(actual) = &self.actual_value {
            write!(f, " (got: {})", actual)?;
        }
        if let Some(expected) = &self.expected {
            write!(f, " (expected: {})", expected)?;
        }
        if let Some(hint) = &self.hint {
            write!(f, " — {}", hint)?;
        }
        Ok(())
    }
}

/// Convenience result type.
pub type SmqlResult<T> = Result<T, SmqlError>;
