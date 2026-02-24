use smql_ast::machine::{HookTrigger, MachineDefinition, TransitionSource};
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

    // 3.2.0a: Detect duplicate state names
    if state_names.len() != machine.states.len() {
        let mut seen = HashSet::new();
        for s in &machine.states {
            if !seen.insert(&s.name) {
                return Err(SmqlError::ValidationError {
                    message: format!("Duplicate state name '{}'", s.name),
                    field: Some("states".into()),
                    hint: Some("Each state name must be unique within a machine".into()),
                });
            }
        }
    }

    // 3.2.0b: Detect duplicate data field names
    {
        let mut field_names = HashSet::new();
        for field in &machine.data {
            if !field_names.insert(&field.name) {
                return Err(SmqlError::ValidationError {
                    message: format!("Duplicate data field name '{}'", field.name),
                    field: Some("data".into()),
                    hint: Some("Each data field name must be unique within a machine".into()),
                });
            }
        }
    }

    // 3.2.0c: Validate constraint consistency on data fields
    for field in &machine.data {
        let has_required = field.constraints.iter().any(|c| matches!(c, smql_ast::types::Constraint::Required));
        let has_optional = field.constraints.iter().any(|c| matches!(c, smql_ast::types::Constraint::Optional));
        if has_required && has_optional {
            return Err(SmqlError::ValidationError {
                message: format!("Data field '{}' has both REQUIRED and OPTIONAL constraints", field.name),
                field: Some("data".into()),
                hint: Some("Remove one of the contradictory constraints".into()),
            });
        }
        // Check Range(min, max) consistency
        for c in &field.constraints {
            if let smql_ast::types::Constraint::Range(lo, hi) = c {
                if lo > hi {
                    return Err(SmqlError::ValidationError {
                        message: format!("Data field '{}' has RANGE({}, {}) where min > max", field.name, lo, hi),
                        field: Some("data".into()),
                        hint: Some("Swap the min and max values".into()),
                    });
                }
            }
        }
        // Check Min/Max consistency
        let min_val = field.constraints.iter().find_map(|c| if let smql_ast::types::Constraint::Min(v) = c { Some(*v) } else { None });
        let max_val = field.constraints.iter().find_map(|c| if let smql_ast::types::Constraint::Max(v) = c { Some(*v) } else { None });
        if let (Some(mn), Some(mx)) = (min_val, max_val) {
            if mn > mx {
                return Err(SmqlError::ValidationError {
                    message: format!("Data field '{}' has MIN({}) > MAX({})", field.name, mn, mx),
                    field: Some("data".into()),
                    hint: Some("MIN must be <= MAX".into()),
                });
            }
        }
    }

    // 3.2.0d: Validate child cardinality consistency
    for child in &machine.children {
        if let smql_ast::machine::ChildCardinality::List { min, max } = &child.cardinality {
            if let (Some(mn), Some(mx)) = (min, max) {
                if mn > mx {
                    return Err(SmqlError::ValidationError {
                        message: format!("Child '{}' has LIST cardinality min({}) > max({})", child.name, mn, mx),
                        field: Some("children".into()),
                        hint: Some("min must be <= max in LIST cardinality".into()),
                    });
                }
            }
        }
    }

    // 3.2.0e: Machine must have at least one state and an initial state
    if machine.states.is_empty() {
        return Err(SmqlError::ValidationError {
            message: "Machine must define at least one state".to_string(),
            field: Some("states".into()),
            hint: Some("Add a STATES { ... } block with at least one state".into()),
        });
    }
    if machine.initial_state.is_empty() {
        return Err(SmqlError::ValidationError {
            message: "Machine must define an INITIAL STATE".to_string(),
            field: Some("initial_state".into()),
            hint: Some("Add INITIAL STATE <state_name> to the machine definition".into()),
        });
    }

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

        // 3.2.10: Timeout target states must be valid, and duration must be > 0
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
            if timeout.duration.seconds == 0 {
                return Err(SmqlError::ValidationError {
                    message: "Timeout duration must be greater than zero".to_string(),
                    field: Some("transitions".into()),
                    hint: Some("A zero-duration timeout fires immediately, which can cause infinite loops".into()),
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

    // 3.2.10b: Hook state references must exist in STATES set
    for hook in &machine.hooks {
        match &hook.trigger {
            HookTrigger::OnEnter(state) | HookTrigger::OnExit(state) => {
                if !state_names.contains(state.as_str()) {
                    return Err(SmqlError::ValidationError {
                        message: format!(
                            "Hook trigger references state '{}' which is not in the STATES set",
                            state
                        ),
                        field: Some("hooks".into()),
                        hint: Some(format!("Add '{}' to STATES or fix the hook trigger", state)),
                    });
                }
            }
            HookTrigger::OnDwell { state, .. } => {
                if !state_names.contains(state.as_str()) {
                    return Err(SmqlError::ValidationError {
                        message: format!(
                            "Dwell hook references state '{}' which is not in the STATES set",
                            state
                        ),
                        field: Some("hooks".into()),
                        hint: Some(format!("Add '{}' to STATES or fix the hook trigger", state)),
                    });
                }
            }
            _ => {} // OnSpawn, BeforeEachTransition, AfterEachTransition don't reference states
        }
    }

    // 3.2.11: Detect circular CHILDREN composition (A → B → A)
    if !machine.children.is_empty() {
        if let Some(cycle) = detect_composition_cycle(machine, catalog) {
            return Err(SmqlError::ValidationError {
                message: format!(
                    "Circular composition detected: {}",
                    cycle.join(" -> ")
                ),
                field: Some("children".into()),
                hint: Some("Remove the circular CHILDREN reference to prevent infinite CASCADE".into()),
            });
        }
    }

    Ok(warnings)
}

/// Detect circular CHILDREN references via DFS.
/// Takes the new machine definition directly (since it's not in the catalog yet).
fn detect_composition_cycle(
    new_machine: &MachineDefinition,
    catalog: &MachineCatalog,
) -> Option<Vec<String>> {
    let mut visited = HashSet::new();
    let mut path = Vec::new();

    // Start DFS from the new machine using its children directly
    visited.insert(new_machine.name.clone());
    path.push(new_machine.name.clone());

    for child in &new_machine.children {
        if let Some(cycle) =
            detect_cycle_dfs(&child.machine, &new_machine.name, catalog, &mut visited, &mut path)
        {
            return Some(cycle);
        }
    }
    None
}

fn detect_cycle_dfs(
    machine_name: &str,
    root_name: &str,
    catalog: &MachineCatalog,
    visited: &mut HashSet<String>,
    path: &mut Vec<String>,
) -> Option<Vec<String>> {
    if !visited.insert(machine_name.to_string()) {
        // Found a cycle — build the cycle path from where this machine first appears
        path.push(machine_name.to_string());
        return Some(path.clone());
    }
    path.push(machine_name.to_string());

    // Look up children from catalog (skip if this is the root — already handled by caller)
    if machine_name != root_name {
        if let Ok(def) = catalog.get(machine_name) {
            for child in &def.children {
                if let Some(cycle) =
                    detect_cycle_dfs(&child.machine, root_name, catalog, visited, path)
                {
                    return Some(cycle);
                }
            }
        }
    }

    path.pop();
    visited.remove(machine_name);
    None
}

/// Compute all states reachable from the initial state via BFS.
/// Includes timeout target states as reachable edges.
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

            if matches {
                // Normal transition target
                if !reachable.contains(transition.to.as_str()) {
                    reachable.insert(transition.to.as_str());
                    queue.push_back(transition.to.as_str());
                }
                // Timeout target (reachable via automatic timeout)
                if let Some(timeout) = &transition.timeout {
                    if !reachable.contains(timeout.target_state.as_str()) {
                        reachable.insert(timeout.target_state.as_str());
                        queue.push_back(timeout.target_state.as_str());
                    }
                }
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
