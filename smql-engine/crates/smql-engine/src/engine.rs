use chrono::Utc;
use smql_ast::command::{SpawnCommand, TransitionCommand};
use smql_ast::error::{GuardFailure, TransitionDeniedError};
use smql_ast::machine::{Action, HookTrigger, MachineDefinition, TransitionSource};
use smql_ast::types::{Constraint, DefaultValue, TypeDefinition};
use smql_ast::value::Value;
use smql_ast::{SmqlError, SmqlResult};
use smql_catalog::MachineCatalog;
use smql_hooks::{EventBus, HookContext, HookExecutor, ResolvedAction};
use smql_storage::instance::{Instance, Mutation, TrailEntry};
use smql_storage::traits::Storage;
use smql_timer::TimerManager;
use std::collections::HashMap;
use std::sync::Arc;

use crate::eval::{eval_expr, eval_guard, ActorInfo, EvalContext};

/// The core SMQL engine — executes spawn, transition, and query operations.
pub struct Engine {
    pub catalog: Arc<MachineCatalog>,
    pub storage: Arc<dyn Storage>,
    pub timer_manager: Arc<TimerManager>,
    pub hook_executor: Arc<HookExecutor>,
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
        let event_bus = Arc::new(EventBus::default());
        Self {
            catalog,
            storage,
            timer_manager: Arc::new(TimerManager::new()),
            hook_executor: Arc::new(HookExecutor::new(event_bus)),
        }
    }

    pub fn with_timer_manager(
        catalog: Arc<MachineCatalog>,
        storage: Arc<dyn Storage>,
        timer_manager: Arc<TimerManager>,
    ) -> Self {
        let event_bus = Arc::new(EventBus::default());
        Self {
            catalog,
            storage,
            timer_manager,
            hook_executor: Arc::new(HookExecutor::new(event_bus)),
        }
    }

    pub fn with_hooks(
        catalog: Arc<MachineCatalog>,
        storage: Arc<dyn Storage>,
        timer_manager: Arc<TimerManager>,
        hook_executor: Arc<HookExecutor>,
    ) -> Self {
        Self {
            catalog,
            storage,
            timer_manager,
            hook_executor,
        }
    }

    /// Get a reference to the event bus for subscribing to events.
    pub fn event_bus(&self) -> &Arc<EventBus> {
        &self.hook_executor.event_bus
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

        // --- Fire ON SPAWN hooks ---
        let hook_ctx = HookContext {
            instance_id: instance.id.as_str(),
            machine: cmd.machine.clone(),
            from_state: String::new(),
            to_state: machine_def.initial_state.clone(),
            data: instance.data.clone(),
            actor: None,
            memo: None,
        };
        let eval_ctx = EvalContext::new(instance.data.clone(), machine_def.initial_state.clone());
        let resolved = self.resolve_hooks_actions(&machine_def.hooks, &eval_ctx);
        let _ = self
            .hook_executor
            .fire_hooks(&machine_def.hooks, &HookTrigger::OnSpawn, &hook_ctx, &resolved)
            .await;

        // --- Fire ON ENTER(initial_state) hooks ---
        let _ = self
            .hook_executor
            .fire_hooks(
                &machine_def.hooks,
                &HookTrigger::OnEnter(machine_def.initial_state.clone()),
                &hook_ctx,
                &resolved,
            )
            .await;

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

        let timeout_remaining = self
            .timer_manager
            .timeout_remaining(&cmd.instance_id, &instance.state);

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
            timeout_remaining,
        };

        // Build HookContext for this transition
        let hook_ctx = HookContext {
            instance_id: cmd.instance_id.clone(),
            machine: instance.machine.clone(),
            from_state: instance.state.clone(),
            to_state: cmd.to_state.clone(),
            data: ctx.data.clone(),
            actor: cmd.as_actor.clone(),
            memo: cmd.memo.clone(),
        };

        // Resolve hook actions with current eval context
        let resolved_hook_actions = self.resolve_hooks_actions(&machine_def.hooks, &ctx);

        // --- 1. BEFORE EACH TRANSITION hooks (sync, can reject) ---
        if let Err(e) = self
            .hook_executor
            .fire_hooks(
                &machine_def.hooks,
                &HookTrigger::BeforeEachTransition,
                &hook_ctx,
                &resolved_hook_actions,
            )
            .await
        {
            // BEFORE hook rejected → treat like a guard failure
            return Err(SmqlError::TransitionDenied(TransitionDeniedError {
                instance_id: cmd.instance_id.clone(),
                from_state: instance.state.clone(),
                to_state: cmd.to_state.clone(),
                guard_failures: vec![GuardFailure {
                    guard_expr: "BEFORE EACH TRANSITION hook".to_string(),
                    actual_value: Some(e.to_string()),
                    expected: None,
                    hint: Some("Hook rejected the transition".to_string()),
                }],
                hint: None,
            }));
        }

        // --- 2. Evaluate ALL guard conditions — collect ALL failures ---
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

        // --- 3. Build mutations from WITH data and MUTATE clauses ---
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

        // --- 4. Atomic storage write ---
        self.storage
            .transition_instance(&id, instance.version, &cmd.to_state, &mutations, trail_entry)
            .await?;

        // --- 5. ON EXIT(old_state) hooks (fire-and-forget) ---
        let _ = self
            .hook_executor
            .fire_hooks(
                &machine_def.hooks,
                &HookTrigger::OnExit(instance.state.clone()),
                &hook_ctx,
                &resolved_hook_actions,
            )
            .await;

        // --- 6. Cancel old timeout, register new one ---
        self.timer_manager
            .cancel(&cmd.instance_id, &instance.state);

        if let Some(timeout) = &transition_def.timeout {
            self.timer_manager.register(
                &cmd.instance_id,
                &instance.machine,
                &cmd.to_state,
                &timeout.duration,
                &timeout.target_state,
            );
        }

        // --- 7. Transition ACTIONs (fire-and-forget) ---
        if !transition_def.actions.is_empty() {
            let resolved_transition_actions = self.resolve_actions(&transition_def.actions, &ctx);
            let _ = self
                .hook_executor
                .execute_actions(&resolved_transition_actions, &hook_ctx)
                .await;
        }

        // --- 8. ON ENTER(new_state) hooks (fire-and-forget) ---
        let _ = self
            .hook_executor
            .fire_hooks(
                &machine_def.hooks,
                &HookTrigger::OnEnter(cmd.to_state.clone()),
                &hook_ctx,
                &resolved_hook_actions,
            )
            .await;

        // --- 9. AFTER EACH TRANSITION hooks (fire-and-forget) ---
        let _ = self
            .hook_executor
            .fire_hooks(
                &machine_def.hooks,
                &HookTrigger::AfterEachTransition,
                &hook_ctx,
                &resolved_hook_actions,
            )
            .await;

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

    /// Execute a timeout-triggered transition (guard-free, as System actor).
    ///
    /// This bypasses normal guard evaluation since the timeout IS the condition.
    /// If the instance has already left the expected state, this is a no-op.
    pub async fn timeout_transition(
        &self,
        instance_id: &str,
        expected_from_state: &str,
        target_state: &str,
    ) -> SmqlResult<Option<TransitionResult>> {
        let id = match smql_storage::InstanceId::from_string(instance_id) {
            Ok(id) => id,
            Err(_) => return Ok(None), // Invalid ID, ignore
        };

        let instance = match self.storage.get_instance(&id).await? {
            Some(inst) => inst,
            None => return Ok(None), // Instance deleted, ignore
        };

        // Race condition: instance already moved to a different state
        if instance.state != expected_from_state {
            return Ok(None);
        }

        // Build mutations (none for timeout transitions)
        let mutations = Vec::new();

        // Create trail entry marked as TIMEOUT
        let trail_entry = TrailEntry {
            instance_id: id.clone(),
            machine: instance.machine.clone(),
            sequence: instance.trail_length + 1,
            from_state: instance.state.clone(),
            to_state: target_state.to_string(),
            transition_name: Some("TIMEOUT".to_string()),
            actor: Some("System".to_string()),
            memo: Some(format!(
                "Timeout expired: {} -> {}",
                expected_from_state, target_state
            )),
            timestamp: Utc::now(),
            data_snapshot: None,
        };

        // Atomic transition
        self.storage
            .transition_instance(
                &id,
                instance.version,
                target_state,
                &mutations,
                trail_entry,
            )
            .await?;

        // --- Fire hooks for timeout transition ---
        if let Ok(machine_def) = self.catalog.get(&instance.machine) {
            let hook_ctx = HookContext {
                instance_id: instance_id.to_string(),
                machine: instance.machine.clone(),
                from_state: instance.state.clone(),
                to_state: target_state.to_string(),
                data: instance.data.clone(),
                actor: Some("System".to_string()),
                memo: None,
            };
            let eval_ctx = EvalContext::new(instance.data.clone(), instance.state.clone());
            let resolved = self.resolve_hooks_actions(&machine_def.hooks, &eval_ctx);

            // ON EXIT(old_state)
            let _ = self
                .hook_executor
                .fire_hooks(
                    &machine_def.hooks,
                    &HookTrigger::OnExit(instance.state.clone()),
                    &hook_ctx,
                    &resolved,
                )
                .await;

            // Register new timeout if needed
            for t in &machine_def.transitions {
                let matches_source = match &t.from {
                    TransitionSource::State(s) => s == target_state,
                    _ => false,
                };
                if matches_source {
                    if let Some(timeout) = &t.timeout {
                        self.timer_manager.register(
                            instance_id,
                            &instance.machine,
                            target_state,
                            &timeout.duration,
                            &timeout.target_state,
                        );
                        break; // Only one timeout per state
                    }
                }
            }

            // ON ENTER(new_state)
            let _ = self
                .hook_executor
                .fire_hooks(
                    &machine_def.hooks,
                    &HookTrigger::OnEnter(target_state.to_string()),
                    &hook_ctx,
                    &resolved,
                )
                .await;

            // AFTER EACH TRANSITION
            let _ = self
                .hook_executor
                .fire_hooks(
                    &machine_def.hooks,
                    &HookTrigger::AfterEachTransition,
                    &hook_ctx,
                    &resolved,
                )
                .await;
        } else {
            // Machine not in catalog (shouldn't happen), just register timeouts
            if let Ok(machine_def) = self.catalog.get(&instance.machine) {
                for t in &machine_def.transitions {
                    let matches_source = match &t.from {
                        TransitionSource::State(s) => s == target_state,
                        _ => false,
                    };
                    if matches_source {
                        if let Some(timeout) = &t.timeout {
                            self.timer_manager.register(
                                instance_id,
                                &instance.machine,
                                target_state,
                                &timeout.duration,
                                &timeout.target_state,
                            );
                            break;
                        }
                    }
                }
            }
        }

        let updated = self
            .storage
            .get_instance(&id)
            .await?
            .ok_or_else(|| SmqlError::not_found("Instance", instance_id))?;

        Ok(Some(TransitionResult {
            from_state: instance.state,
            to_state: target_state.to_string(),
            instance: updated,
        }))
    }

    /// Start the background timer loop that checks for expired timers.
    ///
    /// Returns a JoinHandle that can be used to cancel the loop.
    /// The loop runs every `check_interval` and fires timeout transitions.
    pub fn start_timer_loop(
        self: &Arc<Self>,
        check_interval: std::time::Duration,
    ) -> tokio::task::JoinHandle<()> {
        let engine = Arc::clone(self);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(check_interval);
            loop {
                interval.tick().await;
                let expired = engine.timer_manager.drain_expired();
                for entry in expired {
                    // Handle race condition: instance may have already transitioned
                    let _ = engine
                        .timeout_transition(
                            &entry.instance_id,
                            &entry.from_state,
                            &entry.target_state,
                        )
                        .await;
                }
            }
        })
    }

    // -----------------------------------------------------------------------
    // Hook / Action resolution helpers
    // -----------------------------------------------------------------------

    /// Resolve a list of AST Actions into ResolvedActions using the eval context.
    fn resolve_actions(&self, actions: &[Action], ctx: &EvalContext) -> Vec<ResolvedAction> {
        actions
            .iter()
            .filter_map(|action| self.resolve_action(action, ctx).ok())
            .collect()
    }

    /// Resolve a single AST Action into a ResolvedAction.
    fn resolve_action(&self, action: &Action, ctx: &EvalContext) -> SmqlResult<ResolvedAction> {
        match action {
            Action::Log(msg) => Ok(ResolvedAction::Log(msg.clone())),
            Action::Emit { event, payload } => {
                let resolved_payload = payload
                    .as_ref()
                    .map(|expr| eval_expr(expr, ctx))
                    .transpose()?;
                Ok(ResolvedAction::Emit {
                    event: event.clone(),
                    payload: resolved_payload,
                })
            }
            Action::Notify { target, event } => {
                let resolved_target = eval_expr(target, ctx)?;
                Ok(ResolvedAction::Notify {
                    target: resolved_target,
                    event: event.clone(),
                })
            }
            Action::Webhook { url, payload } => {
                let resolved_payload = payload
                    .as_ref()
                    .map(|expr| eval_expr(expr, ctx))
                    .transpose()?;
                Ok(ResolvedAction::Webhook {
                    url: url.clone(),
                    payload: resolved_payload,
                })
            }
            Action::SpawnChild { machine, data } => {
                let resolved_data: SmqlResult<Vec<(String, Value)>> = data
                    .iter()
                    .map(|(k, expr)| {
                        let val = eval_expr(expr, ctx)?;
                        Ok((k.clone(), val))
                    })
                    .collect();
                Ok(ResolvedAction::SpawnChild {
                    machine: machine.clone(),
                    data: resolved_data?,
                })
            }
            Action::SignalParent { target_state } => Ok(ResolvedAction::SignalParent {
                target_state: target_state.clone(),
            }),
        }
    }

    /// Resolve actions for all hooks in a machine definition.
    /// Returns a Vec parallel to the hooks slice, each containing the resolved actions.
    fn resolve_hooks_actions(
        &self,
        hooks: &[smql_ast::machine::HookDefinition],
        ctx: &EvalContext,
    ) -> Vec<Vec<ResolvedAction>> {
        hooks
            .iter()
            .map(|hook| self.resolve_actions(&hook.actions, ctx))
            .collect()
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
