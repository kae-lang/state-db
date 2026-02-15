use serde::{Deserialize, Serialize};
use std::fmt;

/// All type definitions supported in SMQL DATA blocks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TypeDefinition {
    Text,
    Int,
    Float,
    Bool,
    Uuid,
    Date,
    DateTime,
    Duration,
    /// Enum with named variants, e.g. ENUM(low, medium, high)
    Enum(Vec<String>),
    /// Reference to another machine, e.g. REF(Agent)
    Ref(String),
    /// Ordered list of values, e.g. LIST(TEXT)
    List(Box<TypeDefinition>),
    /// Unordered set of unique values, e.g. SET(TEXT)
    Set(Box<TypeDefinition>),
    /// Key-value map, e.g. MAP(TEXT, INT)
    Map(Box<TypeDefinition>, Box<TypeDefinition>),
    /// Binary blob
    Blob,
    /// Money with currency, e.g. MONEY(USD)
    Money(String),
    /// Arbitrary JSON
    Json,
}

impl fmt::Display for TypeDefinition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypeDefinition::Text => write!(f, "TEXT"),
            TypeDefinition::Int => write!(f, "INT"),
            TypeDefinition::Float => write!(f, "FLOAT"),
            TypeDefinition::Bool => write!(f, "BOOL"),
            TypeDefinition::Uuid => write!(f, "UUID"),
            TypeDefinition::Date => write!(f, "DATE"),
            TypeDefinition::DateTime => write!(f, "DATETIME"),
            TypeDefinition::Duration => write!(f, "DURATION"),
            TypeDefinition::Enum(variants) => write!(f, "ENUM({})", variants.join(", ")),
            TypeDefinition::Ref(target) => write!(f, "REF({})", target),
            TypeDefinition::List(inner) => write!(f, "LIST({})", inner),
            TypeDefinition::Set(inner) => write!(f, "SET({})", inner),
            TypeDefinition::Map(k, v) => write!(f, "MAP({}, {})", k, v),
            TypeDefinition::Blob => write!(f, "BLOB"),
            TypeDefinition::Money(currency) => write!(f, "MONEY({})", currency),
            TypeDefinition::Json => write!(f, "JSON"),
        }
    }
}

/// Constraints that can be applied to data fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Constraint {
    Required,
    Optional,
    Max(i64),
    Min(i64),
    Range(i64, i64),
    Default(DefaultValue),
    Unique,
    Pattern(String),
}

impl fmt::Display for Constraint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Constraint::Required => write!(f, "REQUIRED"),
            Constraint::Optional => write!(f, "OPTIONAL"),
            Constraint::Max(v) => write!(f, "MAX({})", v),
            Constraint::Min(v) => write!(f, "MIN({})", v),
            Constraint::Range(lo, hi) => write!(f, "RANGE({}, {})", lo, hi),
            Constraint::Default(v) => write!(f, "DEFAULT({})", v),
            Constraint::Unique => write!(f, "UNIQUE"),
            Constraint::Pattern(p) => write!(f, "PATTERN({})", p),
        }
    }
}

/// Default values for data fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DefaultValue {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    EmptySet,
    EmptyList,
    EmptyMap,
    Null,
}

impl fmt::Display for DefaultValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DefaultValue::String(s) => write!(f, "\"{}\"", s),
            DefaultValue::Int(v) => write!(f, "{}", v),
            DefaultValue::Float(v) => write!(f, "{}", v),
            DefaultValue::Bool(v) => write!(f, "{}", v),
            DefaultValue::EmptySet => write!(f, "{{}}"),
            DefaultValue::EmptyList => write!(f, "[]"),
            DefaultValue::EmptyMap => write!(f, "{{}}"),
            DefaultValue::Null => write!(f, "NULL"),
        }
    }
}

/// Data field definition within a machine's DATA block.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DataFieldDefinition {
    pub name: String,
    pub field_type: TypeDefinition,
    pub constraints: Vec<Constraint>,
}

impl fmt::Display for DataFieldDefinition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} : {}", self.name, self.field_type)?;
        if !self.constraints.is_empty() {
            let constraints: Vec<String> = self.constraints.iter().map(|c| c.to_string()).collect();
            write!(f, " -> {}", constraints.join(", "))?;
        }
        Ok(())
    }
}

/// Sort direction for query ORDER BY / SORT clauses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SortDirection {
    Asc,
    Desc,
}

impl fmt::Display for SortDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SortDirection::Asc => write!(f, "ASC"),
            SortDirection::Desc => write!(f, "DESC"),
        }
    }
}

/// A sort clause: field + direction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SortClause {
    pub field: String,
    pub direction: SortDirection,
}

/// Aggregate functions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AggregateFunction {
    Count,
    Sum,
    Avg,
    Min,
    Max,
    Percentile(f64),
}

impl fmt::Display for AggregateFunction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AggregateFunction::Count => write!(f, "COUNT"),
            AggregateFunction::Sum => write!(f, "SUM"),
            AggregateFunction::Avg => write!(f, "AVG"),
            AggregateFunction::Min => write!(f, "MIN"),
            AggregateFunction::Max => write!(f, "MAX"),
            AggregateFunction::Percentile(p) => write!(f, "PERCENTILE({})", p),
        }
    }
}
