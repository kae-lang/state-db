use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use smql_ast::value::Value;
use std::collections::HashMap;
use std::fmt;

/// A unique identifier for a machine instance, backed by ULID.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InstanceId(pub ulid::Ulid);

impl InstanceId {
    pub fn new() -> Self {
        Self(ulid::Ulid::new())
    }

    pub fn from_string(s: &str) -> Result<Self, ulid::DecodeError> {
        Ok(Self(ulid::Ulid::from_string(s)?))
    }

    pub fn as_str(&self) -> String {
        self.0.to_string()
    }
}

impl Default for InstanceId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for InstanceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A live instance of a state machine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Instance {
    pub id: InstanceId,
    pub machine: String,
    pub state: String,
    pub data: HashMap<String, Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub state_entered_at: DateTime<Utc>,
    pub trail_length: u64,
    pub version: u64, // optimistic concurrency control
    /// Parent instance ID (for child machines in composition)
    pub parent_id: Option<InstanceId>,
    /// Parent machine name (for child machines in composition)
    pub parent_machine: Option<String>,
}

impl Instance {
    pub fn new(machine: String, initial_state: String, data: HashMap<String, Value>) -> Self {
        let now = Utc::now();
        Self {
            id: InstanceId::new(),
            machine,
            state: initial_state,
            data,
            created_at: now,
            updated_at: now,
            state_entered_at: now,
            trail_length: 0,
            version: 1,
            parent_id: None,
            parent_machine: None,
        }
    }

    /// Create a child instance linked to a parent.
    pub fn new_child(
        machine: String,
        initial_state: String,
        data: HashMap<String, Value>,
        parent_id: InstanceId,
        parent_machine: String,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: InstanceId::new(),
            machine,
            state: initial_state,
            data,
            created_at: now,
            updated_at: now,
            state_entered_at: now,
            trail_length: 0,
            version: 1,
            parent_id: Some(parent_id),
            parent_machine: Some(parent_machine),
        }
    }
}

/// An immutable entry in the transition trail (event log).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrailEntry {
    pub instance_id: InstanceId,
    pub machine: String,
    pub sequence: u64,
    pub from_state: String,
    pub to_state: String,
    pub transition_name: Option<String>,
    pub actor: Option<String>,
    pub memo: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub data_snapshot: Option<HashMap<String, Value>>,
}

/// Filter for querying instances.
#[derive(Debug, Clone, Default)]
pub struct Filter {
    pub state: Option<String>,
    pub states: Option<Vec<String>>,
    pub predicate: Option<FilterPredicate>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    /// Cursor-based pagination: only return instances with ID > after_id.
    pub after_id: Option<String>,
}

/// A filter predicate for data field comparisons.
#[derive(Debug, Clone)]
pub enum FilterPredicate {
    Eq(String, Value),
    Ne(String, Value),
    Gt(String, Value),
    Gte(String, Value),
    Lt(String, Value),
    Lte(String, Value),
    IsNull(String),
    IsNotNull(String),
    And(Box<FilterPredicate>, Box<FilterPredicate>),
    Or(Box<FilterPredicate>, Box<FilterPredicate>),
}

impl FilterPredicate {
    pub fn matches(&self, data: &HashMap<String, Value>) -> bool {
        match self {
            FilterPredicate::Eq(field, val) => data.get(field).map_or(false, |v| v == val),
            FilterPredicate::Ne(field, val) => data.get(field).map_or(true, |v| v != val),
            FilterPredicate::Gt(field, val) => {
                data.get(field).map_or(false, |v| compare_values(v, val) == Some(std::cmp::Ordering::Greater))
            }
            FilterPredicate::Gte(field, val) => {
                data.get(field).map_or(false, |v| {
                    matches!(compare_values(v, val), Some(std::cmp::Ordering::Greater | std::cmp::Ordering::Equal))
                })
            }
            FilterPredicate::Lt(field, val) => {
                data.get(field).map_or(false, |v| compare_values(v, val) == Some(std::cmp::Ordering::Less))
            }
            FilterPredicate::Lte(field, val) => {
                data.get(field).map_or(false, |v| {
                    matches!(compare_values(v, val), Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal))
                })
            }
            FilterPredicate::IsNull(field) => {
                data.get(field).map_or(true, |v| matches!(v, Value::Null))
            }
            FilterPredicate::IsNotNull(field) => {
                data.get(field).map_or(false, |v| !matches!(v, Value::Null))
            }
            FilterPredicate::And(a, b) => a.matches(data) && b.matches(data),
            FilterPredicate::Or(a, b) => a.matches(data) || b.matches(data),
        }
    }
}

/// Compare two Values for ordering (only works for comparable types).
fn compare_values(a: &Value, b: &Value) -> Option<std::cmp::Ordering> {
    match (a, b) {
        (Value::Int(a), Value::Int(b)) => Some(a.cmp(b)),
        (Value::Float(a), Value::Float(b)) => a.partial_cmp(b),
        (Value::Text(a), Value::Text(b)) => Some(a.cmp(b)),
        (Value::DateTime(a), Value::DateTime(b)) => Some(a.cmp(b)),
        (Value::Date(a), Value::Date(b)) => Some(a.cmp(b)),
        (Value::Duration(a), Value::Duration(b)) => Some(a.cmp(b)),
        _ => None,
    }
}

/// A persisted timer that survives server restarts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredTimer {
    pub instance_id: String,
    pub machine: String,
    pub from_state: String,
    pub target_state: String,
    pub deadline: DateTime<Utc>,
    pub registered_at: DateTime<Utc>,
}

/// A mutation to apply to an instance's data.
#[derive(Debug, Clone)]
pub enum Mutation {
    SetField(String, Value),
    RemoveField(String),
    IncrementField(String, i64),
    AppendToList(String, Value),
}

/// Filter for querying trail entries.
#[derive(Debug, Clone, Default)]
pub struct TrailFilter {
    pub from_state: Option<String>,
    pub to_state: Option<String>,
    pub actor: Option<String>,
    pub after: Option<DateTime<Utc>>,
    pub before: Option<DateTime<Utc>>,
    pub limit: Option<usize>,
}
