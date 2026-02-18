use crate::query::{AggregateQuery, FindQuery};
use serde::{Deserialize, Serialize};
use std::fmt;

/// A named live view over a FIND query.
/// Executed at query time — always reflects current data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ViewDefinition {
    pub name: String,
    pub query: FindQuery,
}

/// When a projection should be refreshed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RefreshPolicy {
    /// Refresh after every transition on the target machine.
    OnTransition,
    /// Refresh on a fixed interval (seconds).
    OnInterval(u64),
    /// Only refresh when explicitly requested.
    Manual,
}

impl fmt::Display for RefreshPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RefreshPolicy::OnTransition => write!(f, "ON TRANSITION"),
            RefreshPolicy::OnInterval(secs) => write!(f, "ON INTERVAL {}s", secs),
            RefreshPolicy::Manual => write!(f, "MANUAL"),
        }
    }
}

/// A named materialized projection over an AGGREGATE query.
/// Cached result is updated according to the refresh policy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectionDefinition {
    pub name: String,
    pub query: AggregateQuery,
    pub refresh: RefreshPolicy,
}
