use serde::{Deserialize, Serialize};
use std::fmt;

use crate::span::Span;
use crate::value::{SmqlDuration, Value};

/// Expressions used in guards, WHERE clauses, and MUTATE clauses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Expression {
    pub kind: ExpressionKind,
    pub span: Option<Span>,
}

impl Expression {
    pub fn new(kind: ExpressionKind) -> Self {
        Self { kind, span: None }
    }

    pub fn with_span(kind: ExpressionKind, span: Span) -> Self {
        Self {
            kind,
            span: Some(span),
        }
    }
}

/// The core expression types.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ExpressionKind {
    // Literals
    Literal(Value),
    DurationLiteral(SmqlDuration),

    // Field access
    /// Simple field reference: `priority`, `assignee`
    FieldAccess(Vec<String>),
    /// SELF keyword — refers to current instance
    SelfRef,
    /// ACTOR keyword — refers to the actor performing the transition
    ActorRef,
    /// ACTOR.field or SELF.field with path
    QualifiedAccess {
        root: Box<Expression>,
        path: Vec<String>,
    },

    // Binary operations
    BinaryOp {
        left: Box<Expression>,
        op: BinaryOperator,
        right: Box<Expression>,
    },

    // Unary operations
    UnaryOp {
        op: UnaryOperator,
        operand: Box<Expression>,
    },

    // Function calls
    FunctionCall {
        name: String,
        args: Vec<Expression>,
    },

    // State predicates
    /// STATE IS <state_name>
    StateIs(String),
    /// STATE IN { state1, state2, ... }
    StateIn(Vec<String>),

    // Null checks
    /// field IS SET
    IsSet(Box<Expression>),
    /// field IS NOT SET / IS NULL
    IsNotSet(Box<Expression>),

    // Set membership
    /// expr IN { val1, val2, ... }
    InSet {
        expr: Box<Expression>,
        values: Vec<Expression>,
    },
    /// expr IN (string_val1, string_val2, ...)
    InList {
        expr: Box<Expression>,
        values: Vec<Expression>,
    },

    // Collection predicates
    /// ALL(collection, predicate)
    All {
        collection: Box<Expression>,
        predicate: Box<Expression>,
    },
    /// ANY(collection, predicate)
    Any {
        collection: Box<Expression>,
        predicate: Box<Expression>,
    },
    /// COUNT(collection) or COUNT()
    Count(Option<Box<Expression>>),

    // Signal predicate (for transition guards)
    /// SIGNAL FROM Machine WHERE condition
    SignalFrom {
        machine: String,
        condition: Box<Expression>,
    },

    // Pattern matching
    /// PATTERN(regex_string)
    Pattern(String),
}

/// Binary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BinaryOperator {
    // Comparison
    Eq,
    NotEq,
    Lt,
    Gt,
    LtEq,
    GtEq,
    // Arithmetic
    Add,
    Sub,
    Mul,
    Div,
    // Logical
    And,
    Or,
}

impl fmt::Display for BinaryOperator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BinaryOperator::Eq => write!(f, "=="),
            BinaryOperator::NotEq => write!(f, "!="),
            BinaryOperator::Lt => write!(f, "<"),
            BinaryOperator::Gt => write!(f, ">"),
            BinaryOperator::LtEq => write!(f, "<="),
            BinaryOperator::GtEq => write!(f, ">="),
            BinaryOperator::Add => write!(f, "+"),
            BinaryOperator::Sub => write!(f, "-"),
            BinaryOperator::Mul => write!(f, "*"),
            BinaryOperator::Div => write!(f, "/"),
            BinaryOperator::And => write!(f, "AND"),
            BinaryOperator::Or => write!(f, "OR"),
        }
    }
}

/// Unary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnaryOperator {
    Not,
    Neg,
}

impl fmt::Display for UnaryOperator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UnaryOperator::Not => write!(f, "NOT"),
            UnaryOperator::Neg => write!(f, "-"),
        }
    }
}

impl fmt::Display for Expression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            ExpressionKind::Literal(v) => write!(f, "{}", v),
            ExpressionKind::DurationLiteral(d) => write!(f, "{}", d),
            ExpressionKind::FieldAccess(path) => write!(f, "{}", path.join(".")),
            ExpressionKind::SelfRef => write!(f, "SELF"),
            ExpressionKind::ActorRef => write!(f, "ACTOR"),
            ExpressionKind::QualifiedAccess { root, path } => {
                write!(f, "{}.{}", root, path.join("."))
            }
            ExpressionKind::BinaryOp { left, op, right } => {
                write!(f, "({} {} {})", left, op, right)
            }
            ExpressionKind::UnaryOp { op, operand } => write!(f, "({} {})", op, operand),
            ExpressionKind::FunctionCall { name, args } => {
                let arg_strs: Vec<String> = args.iter().map(|a| a.to_string()).collect();
                write!(f, "{}({})", name, arg_strs.join(", "))
            }
            ExpressionKind::StateIs(state) => write!(f, "STATE IS {}", state),
            ExpressionKind::StateIn(states) => write!(f, "STATE IN {{{}}}", states.join(", ")),
            ExpressionKind::IsSet(expr) => write!(f, "{} IS SET", expr),
            ExpressionKind::IsNotSet(expr) => write!(f, "{} IS NOT SET", expr),
            ExpressionKind::InSet { expr, values } => {
                let val_strs: Vec<String> = values.iter().map(|v| v.to_string()).collect();
                write!(f, "{} IN {{{}}}", expr, val_strs.join(", "))
            }
            ExpressionKind::InList { expr, values } => {
                let val_strs: Vec<String> = values.iter().map(|v| v.to_string()).collect();
                write!(f, "{} IN ({})", expr, val_strs.join(", "))
            }
            ExpressionKind::All {
                collection,
                predicate,
            } => write!(f, "ALL({}, {})", collection, predicate),
            ExpressionKind::Any {
                collection,
                predicate,
            } => write!(f, "ANY({}, {})", collection, predicate),
            ExpressionKind::Count(expr) => match expr {
                Some(e) => write!(f, "COUNT({})", e),
                None => write!(f, "COUNT()"),
            },
            ExpressionKind::SignalFrom { machine, condition } => {
                write!(f, "SIGNAL FROM {} WHERE {}", machine, condition)
            }
            ExpressionKind::Pattern(regex) => write!(f, "PATTERN({})", regex),
        }
    }
}
