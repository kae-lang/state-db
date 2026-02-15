use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::time::Duration as StdDuration;

/// Runtime values in SMQL. Used for instance data, expression evaluation, and query results.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Value {
    Text(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Uuid(uuid::Uuid),
    Date(NaiveDate),
    DateTime(DateTime<Utc>),
    Duration(SmqlDuration),
    List(Vec<Value>),
    Set(Vec<Value>),
    Map(BTreeMap<String, Value>),
    Ref(String, String), // (machine_name, instance_id)
    Money(i64, String),  // (amount_in_minor_units, currency)
    Blob(Vec<u8>),
    Json(serde_json::Value),
    Null,
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Text(s) => write!(f, "\"{}\"", s),
            Value::Int(v) => write!(f, "{}", v),
            Value::Float(v) => write!(f, "{}", v),
            Value::Bool(v) => write!(f, "{}", v),
            Value::Uuid(v) => write!(f, "{}", v),
            Value::Date(v) => write!(f, "{}", v),
            Value::DateTime(v) => write!(f, "{}", v.to_rfc3339()),
            Value::Duration(d) => write!(f, "{}", d),
            Value::List(items) => {
                let strs: Vec<String> = items.iter().map(|v| v.to_string()).collect();
                write!(f, "[{}]", strs.join(", "))
            }
            Value::Set(items) => {
                let strs: Vec<String> = items.iter().map(|v| v.to_string()).collect();
                write!(f, "{{{}}}", strs.join(", "))
            }
            Value::Map(entries) => {
                let strs: Vec<String> = entries
                    .iter()
                    .map(|(k, v)| format!("{}: {}", k, v))
                    .collect();
                write!(f, "{{{}}}", strs.join(", "))
            }
            Value::Ref(machine, id) => write!(f, "{}#{}", machine, id),
            Value::Money(amount, currency) => {
                let major = amount / 100;
                let minor = (amount % 100).abs();
                write!(f, "{}.{:02} {}", major, minor, currency)
            }
            Value::Blob(data) => write!(f, "<blob {} bytes>", data.len()),
            Value::Json(v) => write!(f, "{}", v),
            Value::Null => write!(f, "NULL"),
        }
    }
}

/// SMQL duration type — represents durations like 24h, 7d, 30m.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SmqlDuration {
    pub seconds: u64,
}

impl SmqlDuration {
    pub fn from_seconds(s: u64) -> Self {
        Self { seconds: s }
    }

    pub fn from_minutes(m: u64) -> Self {
        Self { seconds: m * 60 }
    }

    pub fn from_hours(h: u64) -> Self {
        Self { seconds: h * 3600 }
    }

    pub fn from_days(d: u64) -> Self {
        Self { seconds: d * 86400 }
    }

    pub fn to_std(&self) -> StdDuration {
        StdDuration::from_secs(self.seconds)
    }
}

impl fmt::Display for SmqlDuration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = self.seconds;
        if s == 0 {
            return write!(f, "0s");
        }
        let days = s / 86400;
        let hours = (s % 86400) / 3600;
        let minutes = (s % 3600) / 60;
        let secs = s % 60;

        let mut parts = Vec::new();
        if days > 0 {
            parts.push(format!("{}d", days));
        }
        if hours > 0 {
            parts.push(format!("{}h", hours));
        }
        if minutes > 0 {
            parts.push(format!("{}m", minutes));
        }
        if secs > 0 {
            parts.push(format!("{}s", secs));
        }
        write!(f, "{}", parts.join(""))
    }
}

impl PartialOrd for SmqlDuration {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SmqlDuration {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.seconds.cmp(&other.seconds)
    }
}
