use serde::{Deserialize, Serialize};
use std::fmt;

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
        }
    }
}

/// GET query — retrieve a single instance by ID.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetQuery {
    pub machine: String,
    pub instance_id: String,
}

/// FIND query — search for instances matching criteria.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FindQuery {
    pub machine: String,
    pub filter: Option<Expression>,
    pub sort: Vec<SortClause>,
    pub limit: Option<u64>,
    pub offset: Option<u64>,
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
    TimeBucket {
        field: String,
        interval: String,
    },
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
