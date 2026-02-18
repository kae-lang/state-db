use crate::expression::Expression;
use serde::{Deserialize, Serialize};
use std::fmt;

/// When a rule should be evaluated.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RuleTrigger {
    /// Evaluate before every transition on the named machine.
    BeforeTransition { machine: String },
    /// Evaluate after every transition on the named machine.
    AfterTransition { machine: String },
    /// Evaluate before any transition on any machine.
    BeforeAnyTransition,
    /// Evaluate before spawning an instance of the named machine.
    BeforeSpawn { machine: String },
}

impl fmt::Display for RuleTrigger {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RuleTrigger::BeforeTransition { machine } => {
                write!(f, "BEFORE TRANSITION ON {}", machine)
            }
            RuleTrigger::AfterTransition { machine } => {
                write!(f, "AFTER TRANSITION ON {}", machine)
            }
            RuleTrigger::BeforeAnyTransition => write!(f, "BEFORE ANY TRANSITION"),
            RuleTrigger::BeforeSpawn { machine } => {
                write!(f, "BEFORE SPAWN ON {}", machine)
            }
        }
    }
}

/// A named cross-instance invariant checked at transition time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuleDefinition {
    pub name: String,
    pub trigger: RuleTrigger,
    /// The invariant expression — must evaluate to true, else transition is rejected.
    pub invariant: Expression,
    /// Optional human-readable message when the rule fails.
    pub message: Option<String>,
}
