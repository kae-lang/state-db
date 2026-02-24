use serde::{Deserialize, Serialize};
use std::fmt;

use crate::error::{GuardFailure, RecoveryOption};
use crate::expression::Expression;
use crate::types::{AggregateFunction, SortClause};

/// Top-level query AST node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Query {
    /// GET Machine instance_id
    Get(GetQuery),
    /// FIND Machine WHERE ...
    Find(FindQuery),
    /// AGGREGATE Machine ...
    Aggregate(AggregateQuery),
    /// TRAIL OF instance_id
    Trail(TrailQuery),
    /// PATHS FROM Machine WHERE ...
    Paths(PathsQuery),
    /// FUNNEL Machine THROUGH [states]
    Funnel(FunnelQuery),
    /// COMPARE PATHS Machine SEGMENT BY field
    ComparePaths(ComparePathsQuery),
    /// GET VIEW name — execute a named view
    GetView(GetViewQuery),
    /// GET PROJECTION name — execute a named projection
    GetProjection(GetProjectionQuery),
    /// EXPLAIN TRANSITIONS FOR Machine [instance_id] [AS actor]
    ExplainTransitions(ExplainTransitionsQuery),
    /// GET EVENTS [Machine] [AFTER "event_id"] [LIMIT n]
    GetEvents(GetEventsQuery),
}

impl fmt::Display for Query {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Query::Get(q) => write!(f, "GET {} {}", q.machine, q.instance_id),
            Query::Find(q) => write!(f, "FIND {}", q.machine),
            Query::Aggregate(q) => write!(f, "AGGREGATE {}", q.machine),
            Query::Trail(q) => write!(f, "TRAIL OF {}", q.instance_id),
            Query::Paths(q) => write!(f, "PATHS FROM {}", q.machine),
            Query::Funnel(q) => write!(f, "FUNNEL {}", q.machine),
            Query::ComparePaths(q) => write!(f, "COMPARE PATHS {}", q.machine),
            Query::GetView(q) => write!(f, "GET VIEW {}", q.name),
            Query::GetProjection(q) => write!(f, "GET PROJECTION {}", q.name),
            Query::ExplainTransitions(q) => {
                write!(f, "EXPLAIN TRANSITIONS FOR {}", q.machine)?;
                if let Some(id) = &q.instance_id {
                    write!(f, " {}", id)?;
                }
                Ok(())
            }
            Query::GetEvents(q) => {
                write!(f, "GET EVENTS")?;
                if let Some(m) = &q.machine {
                    write!(f, " {}", m)?;
                }
                if let Some(after) = &q.after_id {
                    write!(f, " AFTER \"{}\"", after)?;
                }
                if let Some(limit) = q.limit {
                    write!(f, " LIMIT {}", limit)?;
                }
                Ok(())
            }
        }
    }
}

/// GET query — retrieve a single instance by ID.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetQuery {
    pub machine: String,
    pub instance_id: String,
    /// Optional actor role for field-level read filtering.
    #[serde(default)]
    pub as_actor: Option<String>,
}

/// FIND query — search for instances matching criteria.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FindQuery {
    pub machine: String,
    /// Field projection: if Some, only include listed fields (plus id/state/machine).
    #[serde(default)]
    pub select: Option<Vec<String>>,
    pub filter: Option<Expression>,
    pub sort: Vec<SortClause>,
    pub limit: Option<u64>,
    pub offset: Option<u64>,
    /// Cursor-based pagination: return instances with ID > after.
    #[serde(default)]
    pub after: Option<String>,
    /// Optional actor role for field-level read filtering.
    #[serde(default)]
    pub as_actor: Option<String>,
}

/// AGGREGATE query — compute aggregations over instances.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AggregateQuery {
    pub machine: String,
    pub measures: Vec<MeasureClause>,
    pub filter: Option<Expression>,
    pub group_by: Vec<GroupByClause>,
}

/// A MEASURE clause in an AGGREGATE query.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeasureClause {
    pub function: AggregateFunction,
    pub field: Option<String>,
    pub alias: Option<String>,
}

/// GROUP BY clause — can be a field or time bucket.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GroupByClause {
    Field(String),
    State,
    TimeBucket { field: String, interval: String },
}

/// TRAIL query — retrieve transition history.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrailQuery {
    pub machine: Option<String>,
    pub instance_id: String,
    pub filter: Option<TrailFilter>,
}

/// Filters for trail queries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrailFilter {
    pub actor: Option<String>,
    pub from_state: Option<String>,
    pub to_state: Option<String>,
    pub since: Option<Expression>,
    pub until: Option<Expression>,
}

/// PATHS query — analyze state sequences from trail data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PathsQuery {
    pub machine: String,
    pub filter: Option<Expression>,
    pub limit: Option<u64>,
}

/// FUNNEL query — conversion analysis through ordered states.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FunnelQuery {
    pub machine: String,
    pub states: Vec<String>,
    pub filter: Option<Expression>,
}

/// COMPARE PATHS query — segment path analysis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComparePathsQuery {
    pub machine: String,
    pub segment_by: String,
    pub filter: Option<Expression>,
}

/// GET VIEW name — execute a named view and return its results.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetViewQuery {
    pub name: String,
}

/// GET PROJECTION name — execute a named projection and return its results.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetProjectionQuery {
    pub name: String,
}

/// EXPLAIN TRANSITIONS FOR Machine [instance_id] [AS actor]
/// Schema-level (no instance_id): returns all transitions from the machine definition.
/// Instance-level (with instance_id): evaluates guards against real instance data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExplainTransitionsQuery {
    pub machine: String,
    /// If set, evaluate guards against this specific instance.
    pub instance_id: Option<String>,
    /// Optional actor context for guard evaluation.
    pub as_actor: Option<String>,
}

/// A transition available from the current state, with guard evaluation results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvailableTransition {
    /// Source state (or "ANY" for wildcard transitions).
    pub from_state: String,
    /// Target state.
    pub to_state: String,
    /// Guard expression strings defined on this transition.
    pub guards: Vec<String>,
    /// Whether all guards are currently satisfied (always false for schema-level).
    pub guards_met: bool,
    /// Details of which guards are blocking (empty if guards_met is true).
    pub blocking_guards: Vec<GuardFailure>,
    /// Actionable recovery suggestions for blocked guards.
    pub recovery_options: Vec<RecoveryOption>,
    /// Data fields referenced in guard expressions.
    pub requires_data: Vec<String>,
    /// Required actor role if guard references ACTOR.role.
    pub requires_role: Option<String>,
}

/// GET EVENTS query — retrieve durable events with optional filters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetEventsQuery {
    /// Optional machine filter (None = all machines).
    pub machine: Option<String>,
    /// Optional event name filter.
    pub event_name: Option<String>,
    /// Cursor: only return events after this event ID.
    pub after_id: Option<String>,
    /// Max number of events to return.
    pub limit: Option<usize>,
}
