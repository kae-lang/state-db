use smql_ast::command::{AlterMachineCommand, AlterOperation};
use smql_ast::machine::{MachineDefinition, StateDefinition, TransitionSource};
use smql_ast::types::Constraint;
use smql_ast::value::Value;
use smql_ast::{SmqlError, SmqlResult};
use smql_storage::instance::Mutation;

use crate::engine::Engine;
use crate::eval::{eval_expr, EvalContext};

/// Result of an ALTER MACHINE operation.
#[derive(Debug, Clone)]
pub struct AlterResult {
    pub machine: String,
    pub new_version: u64,
    pub operations_applied: usize,
    pub instances_migrated: u64,
    pub warnings: Vec<String>,
}

impl Engine {
    /// Execute an ALTER MACHINE command.
    ///
    /// This applies schema changes to the machine definition in the catalog
    /// and performs any necessary data migrations on existing instances.
    pub async fn execute_alter_machine(
        &self,
        cmd: &AlterMachineCommand,
    ) -> SmqlResult<AlterResult> {
        // Load current machine definition
        let mut machine_def = self.catalog.get(&cmd.machine)?;
        let mut warnings = Vec::new();
        let mut total_migrated = 0u64;

        // Validate and apply each operation sequentially.
        // Each operation is validated against the current (mutated) definition.
        for op in &cmd.operations {
            self.validate_alter_operation(&machine_def, op)?;
            let migrated = self
                .apply_alter_operation(&mut machine_def, op, &mut warnings)
                .await?;
            total_migrated += migrated;
        }

        // Update the catalog (increments version, preserves history)
        let catalog_warnings = self.catalog.update(machine_def.clone())?;
        for w in catalog_warnings {
            warnings.push(w.message);
        }

        let new_version = self.catalog.version(&cmd.machine)?;

        Ok(AlterResult {
            machine: cmd.machine.clone(),
            new_version,
            operations_applied: cmd.operations.len(),
            instances_migrated: total_migrated,
            warnings,
        })
    }

    /// Validate a single alter operation against the current machine definition.
    fn validate_alter_operation(
        &self,
        machine_def: &MachineDefinition,
        op: &AlterOperation,
    ) -> SmqlResult<()> {
        match op {
            AlterOperation::AddState(name) => {
                if machine_def.states.iter().any(|s| s.name == *name) {
                    return Err(SmqlError::ValidationError {
                        field: None,
                        message: format!(
                            "State '{}' already exists in machine '{}'",
                            name, machine_def.name
                        ),
                        hint: None,
                    });
                }
            }

            AlterOperation::RemoveState { state, migrate_to } => {
                if !machine_def.states.iter().any(|s| s.name == *state) {
                    return Err(SmqlError::ValidationError {
                        field: None,
                        message: format!(
                            "State '{}' does not exist in machine '{}'",
                            state, machine_def.name
                        ),
                        hint: None,
                    });
                }
                if !machine_def.states.iter().any(|s| s.name == *migrate_to) {
                    return Err(SmqlError::ValidationError {
                        field: None,
                        message: format!(
                            "Migration target state '{}' does not exist in machine '{}'",
                            migrate_to, machine_def.name
                        ),
                        hint: None,
                    });
                }
                if *state == machine_def.initial_state {
                    return Err(SmqlError::ValidationError {
                        field: None,
                        message: format!(
                            "Cannot remove initial state '{}' from machine '{}'",
                            state, machine_def.name
                        ),
                        hint: Some("Change the initial state first".to_string()),
                    });
                }
                if *state == *migrate_to {
                    return Err(SmqlError::ValidationError {
                        field: None,
                        message: format!("Cannot migrate state '{}' to itself", state),
                        hint: None,
                    });
                }
            }

            AlterOperation::AddTransition(t) => {
                let from_name = match &t.from {
                    TransitionSource::State(s) => Some(s.as_str()),
                    TransitionSource::Any { .. } => None,
                    TransitionSource::Group(_) => None,
                };
                if let Some(from) = from_name {
                    if !machine_def.states.iter().any(|s| s.name == from) {
                        return Err(SmqlError::ValidationError {
                            field: None,
                            message: format!("Transition source state '{}' does not exist", from),
                            hint: None,
                        });
                    }
                }
                if !machine_def.states.iter().any(|s| s.name == t.to) {
                    return Err(SmqlError::ValidationError {
                        field: None,
                        message: format!("Transition target state '{}' does not exist", t.to),
                        hint: None,
                    });
                }
            }

            AlterOperation::RemoveTransition { from, to } => {
                let exists = machine_def.transitions.iter().any(|t| {
                    t.to == *to
                        && match &t.from {
                            TransitionSource::State(s) => s == from,
                            _ => false,
                        }
                });
                if !exists {
                    return Err(SmqlError::ValidationError {
                        field: None,
                        message: format!("No transition from '{}' to '{}' exists", from, to),
                        hint: None,
                    });
                }
            }

            AlterOperation::ModifyTransition(t) => {
                let from_name = match &t.from {
                    TransitionSource::State(s) => s.clone(),
                    TransitionSource::Any { .. } => "ANY".to_string(),
                    TransitionSource::Group(g) => g.clone(),
                };
                let exists = machine_def.transitions.iter().any(|existing| {
                    existing.to == t.to
                        && match (&existing.from, &t.from) {
                            (TransitionSource::State(a), TransitionSource::State(b)) => a == b,
                            (TransitionSource::Any { .. }, TransitionSource::Any { .. }) => true,
                            _ => false,
                        }
                });
                if !exists {
                    return Err(SmqlError::ValidationError {
                        field: None,
                        message: format!(
                            "No transition from '{}' to '{}' exists to modify",
                            from_name, t.to
                        ),
                        hint: None,
                    });
                }
            }

            AlterOperation::AddData { field, backfill } => {
                if machine_def.data.iter().any(|d| d.name == field.name) {
                    return Err(SmqlError::ValidationError {
                        field: None,
                        message: format!(
                            "Data field '{}' already exists in machine '{}'",
                            field.name, machine_def.name
                        ),
                        hint: None,
                    });
                }
                // Warn if required and no default or backfill
                let is_required = field
                    .constraints
                    .iter()
                    .any(|c| matches!(c, Constraint::Required));
                let has_default = field
                    .constraints
                    .iter()
                    .any(|c| matches!(c, Constraint::Default(_)));
                if is_required && !has_default && backfill.is_none() {
                    return Err(SmqlError::ValidationError {
                        field: Some(field.name.clone()),
                        message: format!(
                            "Adding REQUIRED field '{}' without DEFAULT or BACKFILL expression",
                            field.name
                        ),
                        hint: Some("Add a DEFAULT value or BACKFILL expression".to_string()),
                    });
                }
            }

            AlterOperation::RemoveData(name) => {
                if !machine_def.data.iter().any(|d| d.name == *name) {
                    return Err(SmqlError::ValidationError {
                        field: None,
                        message: format!(
                            "Data field '{}' does not exist in machine '{}'",
                            name, machine_def.name
                        ),
                        hint: None,
                    });
                }
            }

            AlterOperation::Backfill { field, .. } => {
                if !machine_def.data.iter().any(|d| d.name == *field) {
                    return Err(SmqlError::ValidationError {
                        field: None,
                        message: format!(
                            "Data field '{}' does not exist in machine '{}'",
                            field, machine_def.name
                        ),
                        hint: Some("Add the field first with ADD DATA".to_string()),
                    });
                }
            }
        }

        Ok(())
    }

    /// Apply a single alter operation to the machine definition and storage.
    /// Returns the number of instances affected by data migrations.
    async fn apply_alter_operation(
        &self,
        machine_def: &mut MachineDefinition,
        op: &AlterOperation,
        warnings: &mut Vec<String>,
    ) -> SmqlResult<u64> {
        match op {
            AlterOperation::AddState(name) => {
                machine_def.states.push(StateDefinition::new(name.clone()));
                Ok(0)
            }

            AlterOperation::RemoveState { state, migrate_to } => {
                // Migrate instances in the removed state to the target state
                let migrated = self
                    .storage
                    .migrate_instances_state(&machine_def.name, state, migrate_to)
                    .await?;

                if migrated > 0 {
                    warnings.push(format!(
                        "Migrated {} instance(s) from '{}' to '{}'",
                        migrated, state, migrate_to
                    ));
                }

                // Remove the state from the definition
                machine_def.states.retain(|s| s.name != *state);

                // Remove from terminal states if present
                machine_def.terminal_states.retain(|s| s != state);

                // Remove transitions involving this state
                let removed_count = machine_def.transitions.len();
                machine_def.transitions.retain(|t| {
                    let from_matches = match &t.from {
                        TransitionSource::State(s) => s == state,
                        _ => false,
                    };
                    !(from_matches || t.to == *state)
                });
                let removed = removed_count - machine_def.transitions.len();
                if removed > 0 {
                    warnings.push(format!(
                        "Removed {} transition(s) involving state '{}'",
                        removed, state
                    ));
                }

                // Clean up ANY transition except lists that reference the removed state
                for t in &mut machine_def.transitions {
                    if let TransitionSource::Any { except } = &mut t.from {
                        except.retain(|s| s != state);
                    }
                }

                Ok(migrated)
            }

            AlterOperation::AddTransition(t) => {
                machine_def.transitions.push(t.clone());
                Ok(0)
            }

            AlterOperation::RemoveTransition { from, to } => {
                machine_def.transitions.retain(|t| {
                    let from_matches = match &t.from {
                        TransitionSource::State(s) => s == from,
                        _ => false,
                    };
                    !(from_matches && t.to == *to)
                });
                Ok(0)
            }

            AlterOperation::ModifyTransition(new_t) => {
                for t in &mut machine_def.transitions {
                    let matches = t.to == new_t.to
                        && match (&t.from, &new_t.from) {
                            (TransitionSource::State(a), TransitionSource::State(b)) => a == b,
                            (TransitionSource::Any { .. }, TransitionSource::Any { .. }) => true,
                            _ => false,
                        };
                    if matches {
                        *t = new_t.clone();
                        break;
                    }
                }
                Ok(0)
            }

            AlterOperation::AddData { field, backfill } => {
                machine_def.data.push(field.clone());

                // Backfill existing instances if expression provided
                if let Some(backfill_expr) = backfill {
                    let ctx = EvalContext::new(std::collections::HashMap::new(), String::new());
                    let value = eval_expr(backfill_expr, &ctx)?;
                    let mutations = vec![Mutation::SetField(field.name.clone(), value)];
                    let count = self
                        .storage
                        .bulk_update_instances(&machine_def.name, &mutations)
                        .await?;
                    if count > 0 {
                        warnings.push(format!(
                            "Backfilled field '{}' on {} instance(s)",
                            field.name, count
                        ));
                    }
                    return Ok(count);
                }

                // If field has a DEFAULT, backfill with default value
                let default = field.constraints.iter().find_map(|c| {
                    if let Constraint::Default(d) = c {
                        Some(d)
                    } else {
                        None
                    }
                });
                if let Some(default_val) = default {
                    let value = default_to_value(default_val);
                    let mutations = vec![Mutation::SetField(field.name.clone(), value)];
                    let count = self
                        .storage
                        .bulk_update_instances(&machine_def.name, &mutations)
                        .await?;
                    if count > 0 {
                        warnings.push(format!(
                            "Set default for field '{}' on {} instance(s)",
                            field.name, count
                        ));
                    }
                    return Ok(count);
                }

                Ok(0)
            }

            AlterOperation::RemoveData(name) => {
                machine_def.data.retain(|d| d.name != *name);

                // Remove field from all existing instances
                let mutations = vec![Mutation::RemoveField(name.clone())];
                let count = self
                    .storage
                    .bulk_update_instances(&machine_def.name, &mutations)
                    .await?;
                if count > 0 {
                    warnings.push(format!(
                        "Removed field '{}' from {} instance(s)",
                        name, count
                    ));
                }
                Ok(count)
            }

            AlterOperation::Backfill { field, value } => {
                let ctx = EvalContext::new(std::collections::HashMap::new(), String::new());
                let val = eval_expr(value, &ctx)?;
                let mutations = vec![Mutation::SetField(field.clone(), val)];
                let count = self
                    .storage
                    .bulk_update_instances(&machine_def.name, &mutations)
                    .await?;
                Ok(count)
            }
        }
    }
}

/// Convert a DefaultValue to a runtime Value (mirrors engine.rs::default_to_value).
fn default_to_value(default: &smql_ast::types::DefaultValue) -> Value {
    match default {
        smql_ast::types::DefaultValue::String(s) => Value::Text(s.clone()),
        smql_ast::types::DefaultValue::Int(v) => Value::Int(*v),
        smql_ast::types::DefaultValue::Float(v) => Value::Float(*v),
        smql_ast::types::DefaultValue::Bool(v) => Value::Bool(*v),
        smql_ast::types::DefaultValue::EmptySet => Value::Set(Vec::new()),
        smql_ast::types::DefaultValue::EmptyList => Value::List(Vec::new()),
        smql_ast::types::DefaultValue::EmptyMap => Value::Map(std::collections::BTreeMap::new()),
        smql_ast::types::DefaultValue::Null => Value::Null,
    }
}
