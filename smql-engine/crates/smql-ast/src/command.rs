use serde::{Deserialize, Serialize};
use std::fmt;

use crate::expression::Expression;
use crate::machine::MachineDefinition;

/// Top-level command AST node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Command {
    /// DEFINE MACHINE ...
    DefineMachine(MachineDefinition),
    /// DEFINE POLICY ...
    DefinePolicy(crate::machine::PolicyDefinition),
    /// DEFINE VIEW Name AS FIND ...
    DefineView(crate::view::ViewDefinition),
    /// DEFINE PROJECTION Name AS AGGREGATE ... REFRESH ON ...
    DefineProjection(crate::view::ProjectionDefinition),
    /// DEFINE RULE Name ...
    DefineRule(crate::rule::RuleDefinition),
    /// DEFINE SUBSCRIPTION Name ...
    DefineSubscription(crate::subscription::SubscriptionDefinition),
    /// DEFINE SAGA Name ...
    DefineSaga(crate::saga::SagaDefinition),
    /// SPAWN Machine { data }
    Spawn(SpawnCommand),
    /// TRANSITION instance TO state
    Transition(TransitionCommand),
    /// TRY TRANSITION instance TO state
    TryTransition(TransitionCommand),
    /// TRANSITION ALL Machine WHERE ... TO state
    BatchTransition(BatchTransitionCommand),
    /// ALTER MACHINE ...
    AlterMachine(AlterMachineCommand),
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Command::DefineMachine(m) => write!(f, "DEFINE {}", m),
            Command::DefinePolicy(p) => write!(f, "DEFINE POLICY {}", p.name),
            Command::DefineView(v) => write!(f, "DEFINE VIEW {}", v.name),
            Command::DefineProjection(p) => write!(f, "DEFINE PROJECTION {}", p.name),
            Command::DefineRule(r) => write!(f, "DEFINE RULE {}", r.name),
            Command::DefineSubscription(s) => write!(f, "DEFINE SUBSCRIPTION {}", s.name),
            Command::DefineSaga(s) => write!(f, "DEFINE SAGA {}", s.name),
            Command::Spawn(s) => write!(f, "SPAWN {}", s.machine),
            Command::Transition(t) => write!(
                f,
                "TRANSITION {} {} TO {}",
                t.machine, t.instance_id, t.to_state
            ),
            Command::TryTransition(t) => {
                write!(
                    f,
                    "TRY TRANSITION {} {} TO {}",
                    t.machine, t.instance_id, t.to_state
                )
            }
            Command::BatchTransition(b) => {
                write!(f, "TRANSITION ALL {} TO {}", b.machine, b.to_state)
            }
            Command::AlterMachine(a) => write!(f, "ALTER MACHINE {}", a.machine),
        }
    }
}

/// A top-level SMQL statement — either a command or a query.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Statement {
    Command(Command),
    Query(crate::query::Query),
}

/// SPAWN command — create a new instance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpawnCommand {
    pub machine: String,
    pub data: Vec<(String, Expression)>,
    /// Optional: immediately transition after spawn
    pub then_transition: Option<String>,
    /// Is this a batch spawn?
    pub batch: bool,
    /// For batch spawn, list of data sets
    pub batch_data: Vec<Vec<(String, Expression)>>,
    /// Parent instance ID (for child spawn in composition)
    pub parent_id: Option<String>,
    /// Parent machine name (for child spawn in composition)
    pub parent_machine: Option<String>,
    /// Optional actor role for field-level write permission checking.
    #[serde(default)]
    pub as_actor: Option<String>,
}

impl SpawnCommand {
    pub fn new(machine: String, data: Vec<(String, Expression)>) -> Self {
        Self {
            machine,
            data,
            then_transition: None,
            batch: false,
            batch_data: Vec::new(),
            parent_id: None,
            parent_machine: None,
            as_actor: None,
        }
    }
}

/// TRANSITION command — move an instance to a new state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransitionCommand {
    pub machine: String,
    pub instance_id: String,
    pub to_state: String,
    /// WITH { field: value, ... } — data mutations
    pub with_data: Vec<(String, Expression)>,
    /// MEMO "text" — human-readable note
    pub memo: Option<String>,
    /// AS actor — specify who is performing
    pub as_actor: Option<String>,
    /// THROUGH [state1, state2, ...] — multi-hop transition
    pub through: Vec<String>,
    /// OR STAY — apply mutations even if guard fails
    pub or_stay: bool,
    /// CASCADE — also transition all children to terminal states
    pub cascade: bool,
}

impl TransitionCommand {
    pub fn new(machine: String, instance_id: String, to_state: String) -> Self {
        Self {
            machine,
            instance_id,
            to_state,
            with_data: Vec::new(),
            memo: None,
            as_actor: None,
            through: Vec::new(),
            or_stay: false,
            cascade: false,
        }
    }
}

/// Batch TRANSITION command — transition multiple instances.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BatchTransitionCommand {
    pub machine: String,
    pub filter: Expression,
    pub to_state: String,
    pub with_data: Vec<(String, Expression)>,
    pub memo: Option<String>,
    pub as_actor: Option<String>,
}

/// ALTER MACHINE command — modify an existing machine definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AlterMachineCommand {
    pub machine: String,
    pub operations: Vec<AlterOperation>,
}

/// Individual alter operations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AlterOperation {
    /// ADD STATE state_name
    AddState(String),
    /// REMOVE STATE state_name MIGRATE TO target_state
    RemoveState { state: String, migrate_to: String },
    /// ADD TRANSITION from -> to { ... }
    AddTransition(crate::machine::TransitionDefinition),
    /// REMOVE TRANSITION from -> to
    RemoveTransition { from: String, to: String },
    /// MODIFY TRANSITION from -> to { ... }
    ModifyTransition(crate::machine::TransitionDefinition),
    /// ADD DATA field_definition
    AddData {
        field: crate::types::DataFieldDefinition,
        backfill: Option<Expression>,
    },
    /// REMOVE DATA field_name
    RemoveData(String),
    /// BACKFILL field = expression
    Backfill { field: String, value: Expression },
}
