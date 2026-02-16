use smql_ast::machine::{MachineDefinition, TransitionSource};
use smql_ast::{SmqlError, SmqlResult};
use std::collections::{HashSet, VecDeque};

use crate::MachineCatalog;

/// A non-fatal warning from validation.
#[derive(Debug, Clone)]
pub struct ValidationWarning {
    pub message: String,
}

/// Validate a machine definition, returning errors for invalid definitions
/// and warnings for suspicious but legal definitions.
pub fn validate_machine(
    machine: &MachineDefinition,
    catalog: &MachineCatalog,
) -> SmqlResult<Vec<ValidationWarning>> {
    let mut warnings = Vec::new();
    let state_names: HashSet<&str> = machine.states.iter().map(|s| s.name.as_str()).collect();

    // 3.2.1: Initial state must be in STATES set
    if !machine.initial_state.is_empty() && !state_names.contains(machine.initial_state.as_str()) {
        return Err(SmqlError::ValidationError {
            message: format!(
                "Initial state '{}' is not in the STATES set",
                machine.initial_state
            ),
            field: Some("initial_state".into()),
            hint: Some(format!(
                "Add '{}' to STATES or choose one of: {}",
                machine.initial_state,
                state_names_display(&state_names)
            )),
        });
    }

    // 3.2.2: Terminal states must be in STATES set
    for ts in &machine.terminal_states {
        if !state_names.contains(ts.as_str()) {
            return Err(SmqlError::ValidationError {
                message: format!("Terminal state '{}' is not in the STATES set", ts),
                field: Some("terminal_states".into()),
                hint: Some(format!(
                    "Add '{}' to STATES or choose from: {}",
                    ts,
                    state_names_display(&state_names)
                )),
            });
        }
    }

    // 3.2.3: All transition sources/targets must be in STATES set
    for transition in &machine.transitions {
        match &transition.from {
            TransitionSource::State(s) => {
                if !state_names.contains(s.as_str()) {
                    return Err(SmqlError::ValidationError {
                        message: format!(
                            "Transition source state '{}' is not in the STATES set",
                            s
                        ),
                        field: Some("transitions".into()),
                        hint: Some(format!("Add '{}' to STATES", s)),
                    });
                }
            }
            TransitionSource::Any { except } => {
                for ex in except {
                    if !state_names.contains(ex.as_str()) {
                        return Err(SmqlError::ValidationError {
                            message: format!("EXCEPT FROM state '{}' is not in the STATES set", ex),
                            field: Some("transitions".into()),
                            hint: Some(format!("Add '{}' to STATES", ex)),
                        });
                    }
                }
            }
            TransitionSource::Group(_) => {
                // Groups are resolved at runtime
            }
        }

        if !state_names.contains(transition.to.as_str()) {
            return Err(SmqlError::ValidationError {
                message: format!(
                    "Transition target state '{}' is not in the STATES set",
                    transition.to
                ),
                field: Some("transitions".into()),
                hint: Some(format!("Add '{}' to STATES", transition.to)),
            });
        }

        // 3.2.10: Timeout target states must be valid
        if let Some(timeout) = &transition.timeout {
            if !state_names.contains(timeout.target_state.as_str()) {
                return Err(SmqlError::ValidationError {
                    message: format!(
                        "Timeout target state '{}' is not in the STATES set",
                        timeout.target_state
                    ),
                    field: Some("transitions".into()),
                    hint: Some(format!("Add '{}' to STATES", timeout.target_state)),
                });
            }
        }
    }

    // 3.2.4: No transitions FROM terminal states (warn)
    let terminal_set: HashSet<&str> = machine.terminal_states.iter().map(|s| s.as_str()).collect();
    for transition in &machine.transitions {
        if let TransitionSource::State(s) = &transition.from {
            if terminal_set.contains(s.as_str()) {
                warnings.push(ValidationWarning {
                    message: format!(
                        "Transition from terminal state '{}' to '{}' — terminal states typically have no outgoing transitions",
                        s, transition.to
                    ),
                });
            }
        }
    }

    // 3.2.5: All states must be reachable from initial state (warn on unreachable)
    if !machine.initial_state.is_empty() {
        let reachable = compute_reachable_states(machine);
        for state in &machine.states {
            if !reachable.contains(state.name.as_str()) {
                warnings.push(ValidationWarning {
                    message: format!(
                        "State '{}' is unreachable from the initial state '{}'",
                        state.name, machine.initial_state
                    ),
                });
            }
        }
    }

    // 3.2.6: Detect dead-end states (non-terminal states with no outgoing transitions)
    let states_with_outgoing = compute_states_with_outgoing(machine, &state_names);
    // Also account for ANY wildcards which provide outgoing for all states
    let has_any_wildcard = machine
        .transitions
        .iter()
        .any(|t| matches!(&t.from, TransitionSource::Any { .. }));

    for state in &machine.states {
        if !terminal_set.contains(state.name.as_str())
            && !states_with_outgoing.contains(state.name.as_str())
            && !has_any_wildcard
        {
            warnings.push(ValidationWarning {
                message: format!(
                    "State '{}' is a dead-end: it is not terminal and has no outgoing transitions",
                    state.name
                ),
            });
        }
    }

    // 3.2.8: REF targets must reference registered machines
    for field in &machine.data {
        if let smql_ast::types::TypeDefinition::Ref(target) = &field.field_type {
            if !catalog.contains(target) && target != &machine.name {
                warnings.push(ValidationWarning {
                    message: format!(
                        "Data field '{}' references machine '{}' which is not registered (may be registered later)",
                        field.name, target
                    ),
                });
            }
        }
    }

    // 3.2.9: CHILDREN machines must exist in catalog
    for child in &machine.children {
        if !catalog.contains(&child.machine) && child.machine != machine.name {
            warnings.push(ValidationWarning {
                message: format!(
                    "Child '{}' references machine '{}' which is not registered (may be registered later)",
                    child.name, child.machine
                ),
            });
        }
    }

    Ok(warnings)
}

/// Compute all states reachable from the initial state via BFS.
fn compute_reachable_states(machine: &MachineDefinition) -> HashSet<&str> {
    let mut reachable = HashSet::new();
    let mut queue = VecDeque::new();
    reachable.insert(machine.initial_state.as_str());
    queue.push_back(machine.initial_state.as_str());

    while let Some(current) = queue.pop_front() {
        for transition in &machine.transitions {
            let matches = match &transition.from {
                TransitionSource::State(s) => s == current,
                TransitionSource::Any { except } => !except.iter().any(|e| e == current),
                TransitionSource::Group(_) => false,
            };

            if matches && !reachable.contains(transition.to.as_str()) {
                reachable.insert(transition.to.as_str());
                queue.push_back(transition.to.as_str());
            }
        }
    }

    reachable
}

/// Compute set of states that have at least one explicit outgoing transition.
fn compute_states_with_outgoing<'a>(
    machine: &'a MachineDefinition,
    state_names: &HashSet<&'a str>,
) -> HashSet<&'a str> {
    let mut result = HashSet::new();
    for transition in &machine.transitions {
        match &transition.from {
            TransitionSource::State(s) => {
                if let Some(&name) = state_names.get(s.as_str()) {
                    result.insert(name);
                }
            }
            TransitionSource::Any { except } => {
                // ANY gives outgoing to all states except those in except list
                for &state in state_names {
                    if !except.iter().any(|e| e == state) {
                        result.insert(state);
                    }
                }
            }
            TransitionSource::Group(_) => {}
        }
    }
    result
}

fn state_names_display(names: &HashSet<&str>) -> String {
    let mut sorted: Vec<&str> = names.iter().copied().collect();
    sorted.sort();
    sorted.join(", ")
}
