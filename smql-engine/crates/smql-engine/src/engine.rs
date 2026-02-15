use chrono::Utc;
use smql_ast::command::{SpawnCommand, TransitionCommand};
use smql_ast::error::{GuardFailure, TransitionDeniedError};
use smql_ast::machine::{MachineDefinition, TransitionSource};
use smql_ast::types::{Constraint, DefaultValue, TypeDefinition};
use smql_ast::value::Value;
use smql_ast::{SmqlError, SmqlResult};
use smql_catalog::MachineCatalog;
use smql_storage::instance::{Instance, Mutation, TrailEntry};
use smql_storage::traits::Storage;
use std::collections::HashMap;
use std::sync::Arc;

use crate::eval::{eval_expr, eval_guard, ActorInfo, EvalContext};

/// The core SMQL engine — executes spawn, transition, and query operations.
pub struct Engine {
    pub catalog: Arc<MachineCatalog>,
    pub storage: Arc<dyn Storage>,
}

/// Result of a spawn operation.
#[derive(Debug, Clone)]
pub struct SpawnResult {
    pub instance: Instance,
}

/// Result of a transition operation.
#[derive(Debug, Clone)]
pub struct TransitionResult {
    pub instance: Instance,
    pub from_state: String,
    pub to_state: String,
}

impl Engine {
    pub fn new(catalog: Arc<MachineCatalog>, storage: Arc<dyn Storage>) -> Self {
        Self { catalog, storage }
    }

    /// Spawn a new machine instance.
    pub async fn spawn(&self, cmd: &SpawnCommand) -> SmqlResult<SpawnResult> {
        let machine_def = self.catalog.get(&cmd.machine)?;

        // Evaluate data expressions and collect into HashMap
        let ctx = EvalContext::new(HashMap::new(), machine_def.initial_state.clone());
        let mut data = HashMap::new();
        for (field, expr) in &cmd.data {
            let val = eval_expr(expr, &ctx)?;
            data.insert(field.clone(), val);
        }

        // Validate data against machine DATA definition
        self.validate_spawn_data(&machine_def, &mut data)?;

        // Create instance
        let instance = Instance::new(
            cmd.machine.clone(),
            machine_def.initial_state.clone(),
            data,
        );

        // Create initial trail entry (spawn event)
        let trail_entry = TrailEntry {
            instance_id: instance.id.clone(),
            machine: cmd.machine.clone(),
            sequence: 0,
            from_state: String::new(),
            to_state: machine_def.initial_state.clone(),
            transition_name: Some("SPAWN".to_string()),
            actor: None,
            memo: None,
            timestamp: Utc::now(),
            data_snapshot: Some(instance.data.clone()),
        };

        // Store atomically
        self.storage.store_instance(&instance).await?;
        self.storage.append_trail_entry(&trail_entry).await?;

        // Handle THEN TRANSITION if specified
        if let Some(target_state) = &cmd.then_transition {
            let transition_cmd = TransitionCommand {
                machine: Some(cmd.machine.clone()),
                instance_id: instance.id.as_str(),
                to_state: target_state.clone(),
                with_data: Vec::new(),
                memo: None,
                as_actor: None,
                through: Vec::new(),
                or_stay: false,
            };
            let result = self.transition(&transition_cmd).await?;
            return Ok(SpawnResult {
                instance: result.instance,
            });
        }

        Ok(SpawnResult { instance })
    }

    /// Validate spawn data against the machine's DATA definition.
    fn validate_spawn_data(
        &self,
        machine_def: &MachineDefinition,
        data: &mut HashMap<String, Value>,
    ) -> SmqlResult<()> {
        for field_def in &machine_def.data {
            let is_required = field_def
                .constraints
                .iter()
                .any(|c| matches!(c, Constraint::Required));

            if let Some(value) = data.get(&field_def.name) {
                // Type check
                if !matches!(value, Value::Null) {
                    self.check_type_compat(&field_def.name, value, &field_def.field_type)?;
                }
            } else if is_required {
                // Check for default value
                let default = field_def
                    .constraints
                    .iter()
                    .find_map(|c| {
                        if let Constraint::Default(d) = c {
                            Some(d)
                        } else {
                            None
                        }
                    });

                if let Some(default_val) = default {
                    data.insert(field_def.name.clone(), default_to_value(default_val));
                } else {
                    return Err(SmqlError::SpawnRejected {
                        message: format!(
                            "Required field '{}' is missing and has no default",
                            field_def.name
                        ),
                        field: Some(field_def.name.clone()),
                        hint: Some(format!("Provide '{}' in SPAWN data", field_def.name)),
                    });
                }
            } else {
                // Optional field not provided — apply default if one exists
                let default = field_def
                    .constraints
                    .iter()
                    .find_map(|c| {
                        if let Constraint::Default(d) = c {
                            Some(d)
                        } else {
                            None
                        }
                    });

                if let Some(default_val) = default {
                    data.insert(field_def.name.clone(), default_to_value(default_val));
                }
            }
        }

        Ok(())
    }

    /// Basic type compatibility check between a Value and a TypeDefinition.
    fn check_type_compat(
        &self,
        field_name: &str,
        value: &Value,
        type_def: &TypeDefinition,
    ) -> SmqlResult<()> {
        let compatible = match (value, type_def) {
            (Value::Text(_), TypeDefinition::Text) => true,
            (Value::Int(_), TypeDefinition::Int) => true,
            (Value::Float(_), TypeDefinition::Float) => true,
            (Value::Bool(_), TypeDefinition::Bool) => true,
            (Value::Uuid(_), TypeDefinition::Uuid) => true,
            (Value::Date(_), TypeDefinition::Date) => true,
            (Value::DateTime(_), TypeDefinition::DateTime) => true,
            (Value::Duration(_), TypeDefinition::Duration) => true,
            (Value::List(_), TypeDefinition::List(_)) => true,
            (Value::Set(_), TypeDefinition::Set(_)) => true,
            (Value::Map(_), TypeDefinition::Map(_, _)) => true,
            (Value::Blob(_), TypeDefinition::Blob) => true,
            (Value::Money(_, _), TypeDefinition::Money(_)) => true,
            (Value::Json(_), TypeDefinition::Json) => true,
            (Value::Ref(_, _), TypeDefinition::Ref(_)) => true,
            (Value::Text(_), TypeDefinition::Enum(_)) => true, // Enums stored as text
            (Value::Int(_), TypeDefinition::Float) => true, // Int -> Float coercion
            _ => false,
        };

        if !compatible {
            return Err(SmqlError::SpawnRejected {
                message: format!(
                    "Field '{}' has type {} but got value {}",
                    field_name, type_def, value
                ),
                field: Some(field_name.to_string()),
                hint: Some(format!("Expected type: {}", type_def)),
            });
        }

        Ok(())
    }

    /// Execute a transition on an instance.
    pub fn transition<'a>(
        &'a self,
        cmd: &'a TransitionCommand,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = SmqlResult<TransitionResult>> + Send + 'a>>
    {
        Box::pin(self.transition_inner(cmd))
    }

    async fn transition_inner(&self, cmd: &TransitionCommand) -> SmqlResult<TransitionResult> {
        // Handle THROUGH (multi-hop)
        if !cmd.through.is_empty() {
            return self.transition_through(cmd).await;
        }

        let id = smql_storage::InstanceId::from_string(&cmd.instance_id)
            .map_err(|_| SmqlError::not_found("Instance", &cmd.instance_id))?;

        let instance = self
            .storage
            .get_instance(&id)
            .await?
            .ok_or_else(|| SmqlError::not_found("Instance", &cmd.instance_id))?;

        let machine_def = self.catalog.get(&instance.machine)?;

        // Find matching transition definition
        let transition_def = self.find_transition(&machine_def, &instance.state, &cmd.to_state)?;

        // Build evaluation context
        let mut eval_data = instance.data.clone();
        // Apply WITH data mutations for guard evaluation
        let with_ctx = EvalContext::new(instance.data.clone(), instance.state.clone());
        for (field, expr) in &cmd.with_data {
            let val = eval_expr(expr, &with_ctx)?;
            eval_data.insert(field.clone(), val);
        }

        let mut ctx = EvalContext {
            data: eval_data,
            state: instance.state.clone(),
            actor: cmd.as_actor.as_ref().map(|a| ActorInfo {
                id: a.clone(),
                role: None,
                fields: HashMap::new(),
            }),
            state_entered_at: instance.state_entered_at,
            created_at: instance.created_at,
            now: Utc::now(),
        };

        // Evaluate ALL guard conditions — collect ALL failures
        let mut guard_failures = Vec::new();
        for guard in &transition_def.guards {
            match eval_guard(guard, &ctx) {
                Ok(true) => {}
                Ok(false) => {
                    guard_failures.push(GuardFailure {
                        guard_expr: guard.to_string(),
                        actual_value: None,
                        expected: Some("true".to_string()),
                        hint: None,
                    });
                }
                Err(e) => {
                    guard_failures.push(GuardFailure {
                        guard_expr: guard.to_string(),
                        actual_value: Some(e.to_string()),
                        expected: None,
                        hint: None,
                    });
                }
            }
        }

        if !guard_failures.is_empty() {
            if cmd.or_stay {
                // OR STAY: apply data mutations but don't transition
                let mut mutations = Vec::new();
                for (field, expr) in &cmd.with_data {
                    let val = eval_expr(expr, &with_ctx)?;
                    mutations.push(Mutation::SetField(field.clone(), val));
                }
                if !mutations.is_empty() {
                    self.storage
                        .update_instance(&id, instance.version, &mutations)
                        .await?;
                }
                let updated = self
                    .storage
                    .get_instance(&id)
                    .await?
                    .ok_or_else(|| SmqlError::not_found("Instance", &cmd.instance_id))?;
                return Ok(TransitionResult {
                    from_state: instance.state.clone(),
                    to_state: instance.state.clone(),
                    instance: updated,
                });
            }

            return Err(SmqlError::TransitionDenied(TransitionDeniedError {
                instance_id: cmd.instance_id.clone(),
                from_state: instance.state.clone(),
                to_state: cmd.to_state.clone(),
                guard_failures,
                hint: None,
            }));
        }

        // Build mutations from WITH data and MUTATE clauses
        let mut mutations = Vec::new();
        for (field, expr) in &cmd.with_data {
            let val = eval_expr(expr, &ctx)?;
            mutations.push(Mutation::SetField(field.clone(), val));
        }
        for mutate in &transition_def.mutates {
            let val = eval_expr(&mutate.value, &ctx)?;
            mutations.push(Mutation::SetField(mutate.field.clone(), val));
        }

        // Apply mutations to context data for trail snapshot
        for m in &mutations {
            if let Mutation::SetField(field, val) = m {
                ctx.data.insert(field.clone(), val.clone());
            }
        }

        // Create trail entry
        let trail_entry = TrailEntry {
            instance_id: id.clone(),
            machine: instance.machine.clone(),
            sequence: instance.trail_length + 1,
            from_state: instance.state.clone(),
            to_state: cmd.to_state.clone(),
            transition_name: Some(format!("{} -> {}", instance.state, cmd.to_state)),
            actor: cmd.as_actor.clone(),
            memo: cmd.memo.clone(),
            timestamp: Utc::now(),
            data_snapshot: None,
        };

        // Atomic transition
        self.storage
            .transition_instance(&id, instance.version, &cmd.to_state, &mutations, trail_entry)
            .await?;

        let updated = self
            .storage
            .get_instance(&id)
            .await?
            .ok_or_else(|| SmqlError::not_found("Instance", &cmd.instance_id))?;

        Ok(TransitionResult {
            from_state: instance.state,
            to_state: cmd.to_state.clone(),
            instance: updated,
        })
    }

    /// Try a transition — returns Ok(None) if guards fail instead of an error.
    pub async fn try_transition(
        &self,
        cmd: &TransitionCommand,
    ) -> SmqlResult<Option<TransitionResult>> {
        match self.transition(cmd).await {
            Ok(result) => Ok(Some(result)),
            Err(SmqlError::TransitionDenied(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Execute a multi-hop transition through intermediate states.
    async fn transition_through(&self, cmd: &TransitionCommand) -> SmqlResult<TransitionResult> {
        let mut current_id = cmd.instance_id.clone();
        let mut all_states = cmd.through.clone();
        all_states.push(cmd.to_state.clone());

        let mut last_result = None;

        for target in &all_states {
            let step_cmd = TransitionCommand {
                machine: cmd.machine.clone(),
                instance_id: current_id.clone(),
                to_state: target.clone(),
                with_data: Vec::new(), // WITH data only applies to final transition
                memo: cmd.memo.clone(),
                as_actor: cmd.as_actor.clone(),
                through: Vec::new(),
                or_stay: false,
            };

            let result = self.transition(&step_cmd).await?;
            current_id = result.instance.id.as_str();
            last_result = Some(result);
        }

        last_result.ok_or_else(|| SmqlError::internal("THROUGH transition had no steps"))
    }

    /// Find a matching transition definition for the given from -> to states.
    fn find_transition(
        &self,
        machine: &MachineDefinition,
        from_state: &str,
        to_state: &str,
    ) -> SmqlResult<smql_ast::machine::TransitionDefinition> {
        // First check direct transitions
        for t in &machine.transitions {
            if t.to != to_state {
                continue;
            }

            let matches_source = match &t.from {
                TransitionSource::State(s) => s == from_state,
                TransitionSource::Any { except } => {
                    !except.iter().any(|e| e == from_state)
                }
                TransitionSource::Group(_) => false, // Groups resolved at runtime
            };

            if matches_source {
                return Ok(t.clone());
            }
        }

        Err(SmqlError::TransitionDenied(TransitionDeniedError {
            instance_id: String::new(),
            from_state: from_state.to_string(),
            to_state: to_state.to_string(),
            guard_failures: Vec::new(),
            hint: Some(format!(
                "No transition defined from '{}' to '{}' in machine '{}'",
                from_state, to_state, machine.name
            )),
        }))
    }
}

/// Convert a DefaultValue to a runtime Value.
fn default_to_value(default: &DefaultValue) -> Value {
    match default {
        DefaultValue::String(s) => Value::Text(s.clone()),
        DefaultValue::Int(v) => Value::Int(*v),
        DefaultValue::Float(v) => Value::Float(*v),
        DefaultValue::Bool(v) => Value::Bool(*v),
        DefaultValue::EmptySet => Value::Set(Vec::new()),
        DefaultValue::EmptyList => Value::List(Vec::new()),
        DefaultValue::EmptyMap => Value::Map(std::collections::BTreeMap::new()),
        DefaultValue::Null => Value::Null,
    }
}
