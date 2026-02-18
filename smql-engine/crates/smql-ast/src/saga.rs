use crate::expression::Expression;
use crate::machine::Action;
use serde::{Deserialize, Serialize};
use std::fmt;

/// A single step in a saga — targets a machine with a transition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SagaStep {
    pub name: String,
    pub machine: String,
    /// Expression that resolves to the instance ID to transition.
    pub instance_expr: Expression,
    pub to_state: String,
    /// Optional compensation step to run if a later step fails.
    pub compensate: Option<CompensationStep>,
    /// Optional guard — skip this step if false.
    pub when: Option<Expression>,
}

/// A compensation step run when a saga step needs to be rolled back.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompensationStep {
    pub machine: String,
    pub instance_expr: Expression,
    pub to_state: String,
}

/// What triggers a saga to start.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SagaTrigger {
    /// Triggered by an ON ENTER event on a machine state.
    OnEnter { machine: String, state: String },
    /// Triggered by an ON SPAWN event on a machine.
    OnSpawn { machine: String },
    /// Triggered manually (via EXECUTE SAGA command).
    Manual,
}

impl fmt::Display for SagaTrigger {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SagaTrigger::OnEnter { machine, state } => {
                write!(f, "ON ENTER {} ON {}", state, machine)
            }
            SagaTrigger::OnSpawn { machine } => write!(f, "ON SPAWN {}", machine),
            SagaTrigger::Manual => write!(f, "MANUAL"),
        }
    }
}

/// A named multi-machine orchestration saga.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SagaDefinition {
    pub name: String,
    pub trigger: SagaTrigger,
    pub steps: Vec<SagaStep>,
    /// Actions to run on saga completion.
    pub on_complete: Vec<Action>,
    /// Actions to run if the saga fails (after compensation).
    pub on_failure: Vec<Action>,
}
