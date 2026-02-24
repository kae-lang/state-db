use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

use crate::expression::Expression;
use crate::types::DataFieldDefinition;
use crate::value::SmqlDuration;

/// A complete machine definition, parsed from DEFINE MACHINE.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MachineDefinition {
    pub name: String,
    pub data: Vec<DataFieldDefinition>,
    pub states: Vec<StateDefinition>,
    pub initial_state: String,
    pub terminal_states: Vec<String>,
    pub transitions: Vec<TransitionDefinition>,
    pub children: Vec<ChildDefinition>,
    pub parent: Option<String>,
    pub hooks: Vec<HookDefinition>,
    pub roles: Vec<RoleDefinition>,
    pub version: u64,
    pub metadata: BTreeMap<String, String>,
    /// EXTENDS template_name — inherit from a template.
    #[serde(default)]
    pub extends: Option<String>,
}

impl MachineDefinition {
    pub fn new(name: String, initial_state: String) -> Self {
        Self {
            name,
            data: Vec::new(),
            states: Vec::new(),
            initial_state,
            terminal_states: Vec::new(),
            transitions: Vec::new(),
            children: Vec::new(),
            parent: None,
            hooks: Vec::new(),
            roles: Vec::new(),
            version: 1,
            metadata: BTreeMap::new(),
            extends: None,
        }
    }
}

impl fmt::Display for MachineDefinition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MACHINE {} (v{})", self.name, self.version)
    }
}

/// A state within a machine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StateDefinition {
    pub name: String,
    pub metadata: BTreeMap<String, String>,
}

impl StateDefinition {
    pub fn new(name: String) -> Self {
        Self {
            name,
            metadata: BTreeMap::new(),
        }
    }
}

impl fmt::Display for StateDefinition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)
    }
}

/// Source of a transition — can be a specific state, ANY, or a group.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TransitionSource {
    /// A specific named state.
    State(String),
    /// ANY state (wildcard), with optional exceptions.
    Any { except: Vec<String> },
    /// A named group of states.
    Group(String),
}

impl fmt::Display for TransitionSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransitionSource::State(s) => write!(f, "{}", s),
            TransitionSource::Any { except } => {
                if except.is_empty() {
                    write!(f, "ANY")
                } else {
                    write!(f, "ANY EXCEPT FROM {{{}}}", except.join(", "))
                }
            }
            TransitionSource::Group(g) => write!(f, "GROUP({})", g),
        }
    }
}

/// A named, reusable set of guard expressions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PolicyDefinition {
    pub name: String,
    pub guards: Vec<Expression>,
}

/// A reactive clause: auto-transition when condition is true after any mutation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReactiveClause {
    /// The condition expression — when true, the transition fires automatically.
    pub condition: Expression,
}

/// A transition definition between states.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransitionDefinition {
    pub from: TransitionSource,
    pub to: String,
    pub guards: Vec<Expression>,
    pub actions: Vec<Action>,
    pub mutates: Vec<MutateClause>,
    pub timeout: Option<TimeoutClause>,
    pub memo: Option<String>,
    /// APPLY POLICY references — expanded at guard evaluation time.
    pub policies: Vec<String>,
    /// REACTIVE clause — auto-fires when condition is true after any mutation.
    pub reactive: Option<ReactiveClause>,
}

impl TransitionDefinition {
    pub fn new(from: TransitionSource, to: String) -> Self {
        Self {
            from,
            to,
            guards: Vec::new(),
            actions: Vec::new(),
            mutates: Vec::new(),
            timeout: None,
            memo: None,
            policies: Vec::new(),
            reactive: None,
        }
    }
}

impl fmt::Display for TransitionDefinition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} -> {}", self.from, self.to)
    }
}

/// Timeout clause on a transition: after duration, auto-transition to target.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimeoutClause {
    pub duration: SmqlDuration,
    pub target_state: String,
}

impl fmt::Display for TimeoutClause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TIMEOUT: {} -> {}", self.duration, self.target_state)
    }
}

/// A mutation clause within a transition (MUTATE : field = expr).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MutateClause {
    pub field: String,
    pub value: Expression,
}

impl fmt::Display for MutateClause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} = {}", self.field, self.value)
    }
}

/// Actions that fire on transitions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Action {
    /// NOTIFY(target, event_name)
    Notify { target: Expression, event: String },
    /// LOG(message_template)
    Log(String),
    /// EMIT(event_name, payload)
    Emit {
        event: String,
        payload: Option<Expression>,
    },
    /// WEBHOOK(url, payload) [STORE response_field] [ON_FAILURE state]
    Webhook {
        url: String,
        payload: Option<Expression>,
        /// Field name to store the webhook response body.
        #[serde(default)]
        response_field: Option<String>,
        /// State to transition to if the webhook fails.
        #[serde(default)]
        on_failure_state: Option<String>,
    },
    /// SPAWN child machine
    SpawnChild {
        machine: String,
        data: Vec<(String, Expression)>,
    },
    /// SIGNAL PARENT TO state
    SignalParent { target_state: String },
    /// ACTION WHEN condition : action — conditional action
    Conditional {
        condition: Expression,
        action: Box<Action>,
    },
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Action::Notify { target, event } => write!(f, "NOTIFY({}, \"{}\")", target, event),
            Action::Log(msg) => write!(f, "LOG(\"{}\")", msg),
            Action::Emit { event, .. } => write!(f, "EMIT(\"{}\")", event),
            Action::Webhook { url, .. } => write!(f, "WEBHOOK(\"{}\")", url),
            Action::SpawnChild { machine, .. } => write!(f, "SPAWN {}", machine),
            Action::SignalParent { target_state } => {
                write!(f, "SIGNAL PARENT TO {}", target_state)
            }
            Action::Conditional { condition, action } => {
                write!(f, "ACTION WHEN {} : {}", condition, action)
            }
        }
    }
}

/// Child machine relationship definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChildDefinition {
    pub name: String,
    pub machine: String,
    pub cardinality: ChildCardinality,
}

/// Cardinality of child relationships.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ChildCardinality {
    /// LIST(Machine) -> optional MIN/MAX constraints
    List { min: Option<u64>, max: Option<u64> },
    /// OPTIONAL(Machine) — zero or one
    Optional,
    /// Single required child
    Required,
}

impl fmt::Display for ChildCardinality {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ChildCardinality::List { min, max } => {
                write!(f, "LIST")?;
                match (min, max) {
                    (Some(lo), Some(hi)) => write!(f, "({}..{})", lo, hi),
                    (Some(lo), None) => write!(f, "({}+)", lo),
                    (None, Some(hi)) => write!(f, "(..{})", hi),
                    (None, None) => Ok(()),
                }
            }
            ChildCardinality::Optional => write!(f, "OPTIONAL"),
            ChildCardinality::Required => write!(f, "REQUIRED"),
        }
    }
}

/// Hook definitions (ON SPAWN, BEFORE EACH TRANSITION, etc.)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HookDefinition {
    pub trigger: HookTrigger,
    pub actions: Vec<Action>,
}

/// When a hook fires.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum HookTrigger {
    OnSpawn,
    BeforeEachTransition,
    AfterEachTransition,
    OnEnter(String),
    OnExit(String),
    OnDwell {
        state: String,
        duration: SmqlDuration,
    },
}

impl fmt::Display for HookTrigger {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HookTrigger::OnSpawn => write!(f, "ON SPAWN"),
            HookTrigger::BeforeEachTransition => write!(f, "BEFORE EACH TRANSITION"),
            HookTrigger::AfterEachTransition => write!(f, "AFTER EACH TRANSITION"),
            HookTrigger::OnEnter(state) => write!(f, "ON ENTER {}", state),
            HookTrigger::OnExit(state) => write!(f, "ON EXIT {}", state),
            HookTrigger::OnDwell { state, duration } => {
                write!(f, "ON DWELL({}, > {})", state, duration)
            }
        }
    }
}

/// Role-based access control definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoleDefinition {
    pub name: String,
    pub permissions: Vec<RolePermission>,
}

/// Permissions within a role.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RolePermission {
    CanSpawn,
    CanTransition(Vec<String>),
    CanQuery,
    CanAlter,
    /// CAN ALL — shorthand for all permissions
    CanAll,
    /// CAN READ { field1, field2 } — allowlist of readable fields
    CanReadFields(Vec<String>),
    /// CANNOT READ { field1, field2 } — denylist of readable fields
    CannotReadFields(Vec<String>),
    /// CAN WRITE { field1, field2 } — allowlist of writable fields
    CanWriteFields(Vec<String>),
    /// CANNOT WRITE { field1, field2 } — denylist of writable fields
    CannotWriteFields(Vec<String>),
}
