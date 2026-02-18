use crate::machine::Action;
use serde::{Deserialize, Serialize};
use std::fmt;

/// What event pattern a subscription listens for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SubscriptionEvent {
    /// ON ENTER state ON machine
    OnEnter { machine: String, state: String },
    /// ON EXIT state ON machine
    OnExit { machine: String, state: String },
    /// ON SPAWN machine
    OnSpawn { machine: String },
    /// ON TRANSITION machine FROM state TO state (wildcards supported via "*")
    OnTransition {
        machine: String,
        from_state: Option<String>,
        to_state: Option<String>,
    },
}

impl fmt::Display for SubscriptionEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SubscriptionEvent::OnEnter { machine, state } => {
                write!(f, "ON ENTER {} ON {}", state, machine)
            }
            SubscriptionEvent::OnExit { machine, state } => {
                write!(f, "ON EXIT {} ON {}", state, machine)
            }
            SubscriptionEvent::OnSpawn { machine } => write!(f, "ON SPAWN {}", machine),
            SubscriptionEvent::OnTransition { machine, from_state, to_state } => {
                write!(
                    f,
                    "ON TRANSITION {} FROM {} TO {}",
                    machine,
                    from_state.as_deref().unwrap_or("*"),
                    to_state.as_deref().unwrap_or("*")
                )
            }
        }
    }
}

/// A named declarative subscription that routes events to actions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubscriptionDefinition {
    pub name: String,
    pub event: SubscriptionEvent,
    pub actions: Vec<Action>,
}
