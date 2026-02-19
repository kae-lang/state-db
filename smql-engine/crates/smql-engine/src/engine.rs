use chrono::Utc;
use smql_ast::command::{BatchTransitionCommand, SpawnCommand, TransitionCommand};
use smql_ast::error::{GuardFailure, TransitionDeniedError};
use smql_ast::machine::{Action, HookTrigger, MachineDefinition, TransitionSource};
use smql_ast::types::{Constraint, DefaultValue, TypeDefinition};
use smql_ast::value::Value;
use smql_ast::{SmqlError, SmqlResult};
use smql_catalog::MachineCatalog;
use smql_hooks::{EngineCallback, EventBus, HookContext, HookError, HookExecutor, ResolvedAction};
use smql_storage::instance::{Filter, Instance, Mutation, TrailEntry};
use smql_storage::traits::Storage;
use smql_timer::TimerManager;
use std::collections::HashMap;
use std::sync::Arc;

use crate::eval::{eval_expr, eval_guard, ActorInfo, ChildInfo, EvalContext};

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

/// Result of a batch transition operation.
#[derive(Debug, Clone)]
pub struct BatchTransitionResult {
    pub matched: usize,
    pub transitioned: usize,
    pub failures: Vec<BatchTransitionFailure>,
    /// from_state counts for metric updates (state -> count).
    pub from_states: HashMap<String, usize>,
}

/// A single failure in a batch transition.
#[derive(Debug, Clone)]
pub struct BatchTransitionFailure {
    pub instance_id: String,
    pub error: String,
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
    pub fn spawn<'a>(
        &'a self,
        cmd: &'a SpawnCommand,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = SmqlResult<SpawnResult>> + Send + 'a>> {
        Box::pin(self.spawn_inner(cmd))
    }

    #[tracing::instrument(skip(self, cmd), fields(machine = %cmd.machine))]
    async fn spawn_inner(&self, cmd: &SpawnCommand) -> SmqlResult<SpawnResult> {
        let machine_def = self.catalog.get(&cmd.machine)?;

        // Evaluate data expressions and collect into HashMap
        let ctx = EvalContext::new(HashMap::new(), machine_def.initial_state.clone());
        let mut data = HashMap::new();
        for (field, expr) in &cmd.data {
            let val = eval_expr(expr, &ctx)?;
            data.insert(field.clone(), val);
        }

        // Reject computed fields provided in spawn data
        for field_def in &machine_def.data {
            use smql_ast::types::Constraint;
            let is_computed = field_def.constraints.iter().any(|c| matches!(c, Constraint::Computed(_)));
            if is_computed && data.contains_key(&field_def.name) {
                return Err(SmqlError::SpawnRejected {
                    message: format!("Field '{}' is COMPUTED and cannot be set directly", field_def.name),
                    field: Some(field_def.name.clone()),
                    hint: Some("Remove this field from SPAWN data — it is auto-derived".to_string()),
                });
            }
        }

        // Check field-level write permissions on spawn data
        let spawn_actor_role = cmd.as_actor.as_deref();
        for (field, _) in &data {
            if !self.can_write_field(&machine_def, field, spawn_actor_role) {
                return Err(SmqlError::SpawnRejected {
                    message: format!(
                        "Role '{}' cannot write field '{}' on spawn",
                        spawn_actor_role.unwrap_or("unknown"),
                        field
                    ),
                    field: Some(field.clone()),
                    hint: Some(format!("Add CAN WRITE {{ {} }} to the role definition", field)),
                });
            }
        }

        // Validate data against machine DATA definition
        self.validate_spawn_data(&machine_def, &mut data)?;

        // Evaluate DEFINE RULE BeforeSpawn invariants
        {
            use smql_ast::rule::RuleTrigger;
            use smql_ast::error::GuardFailure;
            let spawn_ctx = EvalContext::new(data.clone(), machine_def.initial_state.clone());
            let applicable_rules = self.catalog.rules_for_machine(&cmd.machine);
            let mut rule_failures: Vec<GuardFailure> = Vec::new();
            for rule in &applicable_rules {
                if matches!(&rule.trigger, RuleTrigger::BeforeSpawn { machine: m } if m == &cmd.machine) {
                    match eval_guard(&rule.invariant, &spawn_ctx) {
                        Ok(true) => {}
                        Ok(false) => {
                            rule_failures.push(GuardFailure {
                                guard_expr: format!("[RULE {}] {}", rule.name, rule.invariant),
                                actual_value: None,
                                expected: Some("true".to_string()),
                                hint: rule.message.clone().or_else(|| Some(format!("Rule '{}' invariant failed on spawn", rule.name))),
                            });
                        }
                        Err(e) => {
                            rule_failures.push(GuardFailure {
                                guard_expr: format!("[RULE {}] {}", rule.name, rule.invariant),
                                actual_value: Some(e.to_string()),
                                expected: None,
                                hint: None,
                            });
                        }
                    }
                }
            }
            if !rule_failures.is_empty() {
                return Err(SmqlError::SpawnRejected {
                    message: format!(
                        "Spawn rejected by rule invariant: {}",
                        rule_failures.iter().map(|f| f.guard_expr.clone()).collect::<Vec<_>>().join("; ")
                    ),
                    field: None,
                    hint: rule_failures.first().and_then(|f| f.hint.clone()),
                });
            }
        }

        // Evaluate COMPUTED fields after validation
        self.evaluate_computed_fields(&machine_def, &mut data, &machine_def.initial_state);

        // Create instance (with optional parent linkage)
        let instance = if let (Some(parent_id_str), Some(parent_machine)) =
            (&cmd.parent_id, &cmd.parent_machine)
        {
            let parent_id = smql_storage::InstanceId::from_string(parent_id_str)
                .map_err(|_| SmqlError::not_found("Parent instance", parent_id_str))?;
            // Validate parent exists
            let _parent = self
                .storage
                .get_instance(&parent_id)
                .await?
                .ok_or_else(|| SmqlError::not_found("Parent instance", parent_id_str))?;
            Instance::new_child(
                cmd.machine.clone(),
                machine_def.initial_state.clone(),
                data,
                parent_id,
                parent_machine.clone(),
            )
        } else {
            Instance::new(cmd.machine.clone(), machine_def.initial_state.clone(), data)
        };

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
            .fire_hooks(
                &machine_def.hooks,
                &HookTrigger::OnSpawn,
                &hook_ctx,
                &resolved,
            )
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

        // --- Register dwell timers for initial state ---
        let instance_id_str = instance.id.as_str();
        self.register_dwell_timers(
            &instance_id_str,
            &cmd.machine,
            &machine_def.initial_state,
            &machine_def,
        );

        // --- Fire ON SPAWN subscriptions ---
        let spawn_eval_ctx = EvalContext::new(instance.data.clone(), machine_def.initial_state.clone());
        self.fire_subscriptions_for_spawn(&hook_ctx, &cmd.machine, &spawn_eval_ctx).await;

        // --- Auto-trigger OnSpawn sagas ---
        {
            let sagas = self.catalog.sagas_for_spawn(&cmd.machine);
            for saga in sagas {
                let saga_name = saga.name.clone();
                let iid = instance.id.as_str().to_string();
                let result = self.execute_saga(&saga_name, Some(&iid)).await;
                if let Err(e) = result {
                    tracing::warn!(saga = %saga_name, error = %e, "OnSpawn saga failed");
                }
            }
        }

        // Handle THEN TRANSITION if specified
        if let Some(target_state) = &cmd.then_transition {
            let transition_cmd = TransitionCommand {
                machine: cmd.machine.clone(),
                instance_id: instance.id.as_str(),
                to_state: target_state.clone(),
                with_data: Vec::new(),
                memo: None,
                as_actor: None,
                through: Vec::new(),
                or_stay: false,
                cascade: false,
            };
            let result = self.transition(&transition_cmd).await?;
            return Ok(SpawnResult {
                instance: result.instance,
            });
        }

        tracing::info!(id = %instance.id, state = %instance.state, "instance spawned");
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
                let default = field_def.constraints.iter().find_map(|c| {
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
                let default = field_def.constraints.iter().find_map(|c| {
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
            (Value::Int(_), TypeDefinition::Float) => true,    // Int -> Float coercion
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
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = SmqlResult<TransitionResult>> + Send + 'a>,
    > {
        Box::pin(self.transition_inner(cmd))
    }

    /// Execute a transition and then check for REACTIVE WHEN auto-transitions.
    pub async fn transition_reactive(
        &self,
        cmd: &TransitionCommand,
    ) -> SmqlResult<TransitionResult> {
        let result = self.transition_inner(cmd).await?;
        let iid = result.instance.id.to_string();
        let machine = result.instance.machine.clone();
        let new_state = result.to_state.clone();
        self.check_and_fire_reactive(&iid, &machine, &new_state).await;
        Ok(result)
    }

    fn transition_inner<'a>(
        &'a self,
        cmd: &'a TransitionCommand,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = SmqlResult<TransitionResult>> + Send + 'a>> {
        Box::pin(self.transition_inner_impl(cmd))
    }

    #[tracing::instrument(skip(self, cmd), fields(instance_id = %cmd.instance_id, to_state = %cmd.to_state))]
    async fn transition_inner_impl(&self, cmd: &TransitionCommand) -> SmqlResult<TransitionResult> {
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

        // Validate machine name matches the instance's actual machine
        if !cmd.machine.is_empty() && cmd.machine != instance.machine {
            return Err(SmqlError::ValidationError {
                message: format!(
                    "Machine mismatch: command specifies '{}' but instance belongs to '{}'",
                    cmd.machine, instance.machine
                ),
                field: Some("machine".to_string()),
                hint: Some(format!(
                    "Use TRANSITION {} \"{}\" TO ...",
                    instance.machine, cmd.instance_id
                )),
            });
        }

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
            children: HashMap::new(),
            parent_data: None,
            parent_state: None,
        };

        // Populate children/parent context for composition guards
        if !machine_def.children.is_empty() || instance.parent_id.is_some() {
            self.populate_composition_context(&mut ctx, &instance, &machine_def)
                .await;
        }

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
        // First expand APPLY POLICY guards, then evaluate inline guards
        let mut guard_failures = Vec::new();

        for policy_name in &transition_def.policies {
            match self.catalog.get_policy(policy_name) {
                Ok(policy) => {
                    for guard in &policy.guards {
                        match eval_guard(guard, &ctx) {
                            Ok(true) => {}
                            Ok(false) => {
                                guard_failures.push(GuardFailure {
                                    guard_expr: format!("[POLICY {}] {}", policy_name, guard),
                                    actual_value: None,
                                    expected: Some("true".to_string()),
                                    hint: Some(format!("Guard from policy '{}'", policy_name)),
                                });
                            }
                            Err(e) => {
                                guard_failures.push(GuardFailure {
                                    guard_expr: format!("[POLICY {}] {}", policy_name, guard),
                                    actual_value: Some(e.to_string()),
                                    expected: None,
                                    hint: None,
                                });
                            }
                        }
                    }
                }
                Err(_) => {
                    guard_failures.push(GuardFailure {
                        guard_expr: format!("APPLY POLICY {}", policy_name),
                        actual_value: Some(format!("Policy '{}' not found", policy_name)),
                        expected: None,
                        hint: Some(format!("Register policy '{}' with DEFINE POLICY", policy_name)),
                    });
                }
            }
        }

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

        // --- 2b. Evaluate DEFINE RULE invariants (always, not just when guards fail) ---
        let applicable_rules = self.catalog.rules_for_machine(&instance.machine);
        for rule in &applicable_rules {
            match eval_guard(&rule.invariant, &ctx) {
                Ok(true) => {}
                Ok(false) => {
                    guard_failures.push(GuardFailure {
                        guard_expr: format!("[RULE {}] {}", rule.name, rule.invariant),
                        actual_value: None,
                        expected: Some("true".to_string()),
                        hint: rule.message.clone().or_else(|| Some(format!("Rule '{}' invariant failed", rule.name))),
                    });
                }
                Err(e) => {
                    guard_failures.push(GuardFailure {
                        guard_expr: format!("[RULE {}] {}", rule.name, rule.invariant),
                        actual_value: Some(e.to_string()),
                        expected: None,
                        hint: None,
                    });
                }
            }
        }

        if !guard_failures.is_empty() {
            tracing::warn!(failures = guard_failures.len(), "guard evaluation failed");
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
        // Check field-level write permissions first
        let actor_role = cmd.as_actor.as_deref();
        for (field, _) in &cmd.with_data {
            if !self.can_write_field(&machine_def, field, actor_role) {
                return Err(SmqlError::TransitionDenied(TransitionDeniedError {
                    instance_id: cmd.instance_id.clone(),
                    from_state: instance.state.clone(),
                    to_state: cmd.to_state.clone(),
                    guard_failures: vec![GuardFailure {
                        guard_expr: format!("WRITE permission for field '{}'", field),
                        actual_value: Some(format!("Role '{}' cannot write field '{}'", actor_role.unwrap_or("unknown"), field)),
                        expected: None,
                        hint: Some(format!("Add CAN WRITE {{ {} }} to the role definition", field)),
                    }],
                    hint: None,
                }));
            }
        }

        let mut mutations = Vec::new();
        for (field, expr) in &cmd.with_data {
            let val = eval_expr(expr, &ctx)?;
            mutations.push(Mutation::SetField(field.clone(), val));
        }
        for mutate in &transition_def.mutates {
            // Check for __spawn FunctionCall in MUTATE
            if let smql_ast::expression::ExpressionKind::FunctionCall { name, args } =
                &mutate.value.kind
            {
                if name == "__spawn" {
                    // __spawn(machine_name, key1, val1, key2, val2, ...)
                    if let Some(first_arg) = args.first() {
                        let machine_val = eval_expr(first_arg, &ctx)?;
                        let child_machine = match &machine_val {
                            Value::Text(s) => s.clone(),
                            _ => first_arg.to_string(),
                        };
                        // Collect remaining args as key-value pairs
                        let mut child_data = Vec::new();
                        let mut i = 1;
                        while i + 1 < args.len() {
                            let key_val = eval_expr(&args[i], &ctx)?;
                            let val = eval_expr(&args[i + 1], &ctx)?;
                            let key = match key_val {
                                Value::Text(k) => k,
                                _ => args[i].to_string(),
                            };
                            child_data.push((key, val));
                            i += 2;
                        }
                        // Spawn child with parent linkage
                        let child_data_exprs: Vec<(String, smql_ast::expression::Expression)> =
                            child_data
                                .into_iter()
                                .map(|(k, v)| {
                                    (
                                        k,
                                        smql_ast::expression::Expression::new(
                                            smql_ast::expression::ExpressionKind::Literal(v),
                                        ),
                                    )
                                })
                                .collect();
                        let child_cmd = SpawnCommand {
                            machine: child_machine.clone(),
                            data: child_data_exprs,
                            then_transition: None,
                            batch: false,
                            batch_data: Vec::new(),
                            parent_id: Some(cmd.instance_id.clone()),
                            parent_machine: Some(instance.machine.clone()),
                            as_actor: None,
                        };
                        match self.spawn(&child_cmd).await {
                            Ok(result) => {
                                let child_id = result.instance.id.as_str();
                                mutations.push(Mutation::SetField(
                                    mutate.field.clone(),
                                    Value::Ref(child_machine, child_id),
                                ));
                            }
                            Err(e) => {
                                return Err(SmqlError::internal(format!(
                                    "MUTATE SPAWN failed: {}",
                                    e
                                )));
                            }
                        }
                    }
                    continue;
                }
            }
            let val = eval_expr(&mutate.value, &ctx)?;
            mutations.push(Mutation::SetField(mutate.field.clone(), val));
        }

        // Apply mutations to context data for trail snapshot
        for m in &mutations {
            if let Mutation::SetField(field, val) = m {
                ctx.data.insert(field.clone(), val.clone());
            }
        }

        // Evaluate COMPUTED fields after mutations, add them as additional mutations
        let mut computed_data = ctx.data.clone();
        self.evaluate_computed_fields(&machine_def, &mut computed_data, &cmd.to_state);
        for field_def in &machine_def.data {
            use smql_ast::types::Constraint;
            let is_computed = field_def.constraints.iter().any(|c| matches!(c, Constraint::Computed(_)));
            if is_computed {
                if let Some(val) = computed_data.get(&field_def.name) {
                    mutations.push(Mutation::SetField(field_def.name.clone(), val.clone()));
                    ctx.data.insert(field_def.name.clone(), val.clone());
                }
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
            .transition_instance(
                &id,
                instance.version,
                &cmd.to_state,
                &mutations,
                trail_entry,
            )
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

        // --- 6. Cancel old timeout, cancel old dwell timers, register new ones ---
        self.timer_manager.cancel(&cmd.instance_id, &instance.state);
        self.timer_manager.cancel_dwell_for_state(&cmd.instance_id, &instance.state);
        let _ = self
            .storage
            .remove_timer(&cmd.instance_id, &instance.state)
            .await;

        if let Some(timeout) = &transition_def.timeout {
            self.timer_manager.register(
                &cmd.instance_id,
                &instance.machine,
                &cmd.to_state,
                &timeout.duration,
                &timeout.target_state,
            );
            // Persist timer to storage for crash recovery
            if let Some(entry) = self
                .timer_manager
                .get_timer(&cmd.instance_id, &cmd.to_state)
            {
                let stored = smql_storage::StoredTimer {
                    instance_id: cmd.instance_id.clone(),
                    machine: instance.machine.clone(),
                    from_state: cmd.to_state.clone(),
                    target_state: timeout.target_state.clone(),
                    deadline: entry.deadline,
                    registered_at: entry.registered_at,
                };
                let _ = self.storage.store_timer(&stored).await;
            }
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

        // --- 8b. Register dwell timers for the new state ---
        self.register_dwell_timers(&cmd.instance_id, &instance.machine, &cmd.to_state, &machine_def);

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

        // --- 10. Fire matching DEFINE SUBSCRIPTION actions ---
        self.fire_subscriptions_for_transition(
            &hook_ctx,
            &instance.machine,
            &instance.state,
            &cmd.to_state,
            &ctx,
        )
        .await;

        // --- 10b. Auto-trigger sagas on state entry ---
        {
            let sagas = self.catalog.sagas_for_enter(&instance.machine, &cmd.to_state);
            for saga in sagas {
                let saga_name = saga.name.clone();
                let iid = cmd.instance_id.to_string();
                let result = self.execute_saga(&saga_name, Some(&iid)).await;
                if let Err(e) = result {
                    tracing::warn!(saga = %saga_name, error = %e, "OnEnter saga failed");
                }
            }
        }

        // --- 11. Refresh OnTransition projections (fire-and-forget) ---
        {
            use smql_ast::view::RefreshPolicy;
            let proj_names = self.catalog.list_projections();
            for proj_name in proj_names {
                if let Ok(proj) = self.catalog.get_projection_def(&proj_name) {
                    if proj.refresh == RefreshPolicy::OnTransition
                        && proj.query.machine == instance.machine
                    {
                        let q = smql_ast::query::GetProjectionQuery { name: proj_name.clone() };
                        let _ = self.execute_get_projection(&q).await;
                        tracing::debug!(projection = %proj_name, "OnTransition projection refreshed");
                    }
                }
            }
        }

        // --- 12. Check REACTIVE WHEN clauses on the new state (fire-and-forget) ---
        // We cannot call check_and_fire_reactive directly (needs Arc<Self>),
        // so we store the info and let the caller handle it via the result.
        // The reactive check is triggered by the public transition() wrapper via spawn.

        // --- 10. CASCADE: transition all children to terminal states ---
        if cmd.cascade {
            self.cascade_children(&id, &instance.machine).await;
        }

        let updated = self
            .storage
            .get_instance(&id)
            .await?
            .ok_or_else(|| SmqlError::not_found("Instance", &cmd.instance_id))?;

        tracing::info!(from = %instance.state, to = %cmd.to_state, "transition complete");
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

    /// Execute a batch transition — transition all matching instances.
    ///
    /// Best-effort: transitions what it can and reports failures.
    #[tracing::instrument(skip(self, cmd), fields(machine = %cmd.machine, to_state = %cmd.to_state))]
    pub async fn batch_transition(
        &self,
        cmd: &BatchTransitionCommand,
    ) -> SmqlResult<BatchTransitionResult> {
        // Validate machine exists
        let _machine_def = self.catalog.get(&cmd.machine)?;

        // Find all instances for this machine
        let filter = Filter::default();
        let instances = self.storage.find_instances(&cmd.machine, &filter).await?;

        // Apply WHERE filter
        let matching: Vec<&Instance> = instances
            .iter()
            .filter(|inst| {
                let ctx = EvalContext::new(inst.data.clone(), inst.state.clone());
                eval_guard(&cmd.filter, &ctx).unwrap_or(false)
            })
            .collect();

        let matched = matching.len();
        let mut transitioned = 0;
        let mut failures = Vec::new();
        let mut from_states: HashMap<String, usize> = HashMap::new();

        for inst in matching {
            let from_state = inst.state.clone();
            let t_cmd = TransitionCommand {
                machine: cmd.machine.clone(),
                instance_id: inst.id.as_str(),
                to_state: cmd.to_state.clone(),
                with_data: cmd.with_data.clone(),
                memo: cmd.memo.clone(),
                as_actor: cmd.as_actor.clone(),
                through: Vec::new(),
                or_stay: false,
                cascade: false,
            };

            match self.transition(&t_cmd).await {
                Ok(_) => {
                    transitioned += 1;
                    *from_states.entry(from_state).or_insert(0) += 1;
                }
                Err(e) => {
                    failures.push(BatchTransitionFailure {
                        instance_id: inst.id.as_str(),
                        error: e.to_string(),
                    });
                }
            }
        }

        tracing::info!(matched, transitioned, failed = failures.len(), "batch transition complete");
        Ok(BatchTransitionResult {
            matched,
            transitioned,
            failures,
            from_states,
        })
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
                cascade: false,
            };

            let result = self.transition(&step_cmd).await?;
            current_id = result.instance.id.as_str();
            last_result = Some(result);
        }

        last_result.ok_or_else(|| SmqlError::internal("THROUGH transition had no steps"))
    }

    /// Cascade: transition all children to their machine's first terminal state.
    /// Errors on child cascade are logged but don't fail the parent.
    async fn cascade_children(&self, parent_id: &smql_storage::InstanceId, _parent_machine: &str) {
        let children = match self.storage.find_children(parent_id, None).await {
            Ok(c) => c,
            Err(_) => return,
        };

        for child in children {
            // Find the child's machine definition to get terminal states
            let child_machine_def = match self.catalog.get(&child.machine) {
                Ok(m) => m,
                Err(_) => continue,
            };

            // Skip if already in a terminal state
            if child_machine_def.terminal_states.contains(&child.state) {
                continue;
            }

            // Try to find a transition from current state to the first terminal state
            if let Some(terminal) = child_machine_def.terminal_states.first() {
                let cmd = TransitionCommand {
                    machine: child.machine.clone(),
                    instance_id: child.id.as_str(),
                    to_state: terminal.clone(),
                    with_data: Vec::new(),
                    memo: Some("CASCADE from parent".to_string()),
                    as_actor: Some("System".to_string()),
                    through: Vec::new(),
                    or_stay: false,
                    cascade: true, // Recursive cascade for grandchildren
                };
                // Use try_transition so failures don't propagate
                let _ = self.try_transition(&cmd).await;
            }
        }
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
                TransitionSource::Any { except } => !except.iter().any(|e| e == from_state),
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
    #[tracing::instrument(skip(self), fields(instance_id = %instance_id, target_state = %target_state))]
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
            .transition_instance(&id, instance.version, target_state, &mutations, trail_entry)
            .await?;

        // Remove fired timer from storage
        let _ = self
            .storage
            .remove_timer(instance_id, expected_from_state)
            .await;

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

            // Cancel dwell timers for the old state
            self.timer_manager.cancel_dwell_for_state(instance_id, &instance.state);

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
                        // Persist new timer to storage
                        if let Some(entry) = self.timer_manager.get_timer(instance_id, target_state)
                        {
                            let stored = smql_storage::StoredTimer {
                                instance_id: instance_id.to_string(),
                                machine: instance.machine.clone(),
                                from_state: target_state.to_string(),
                                target_state: timeout.target_state.clone(),
                                deadline: entry.deadline,
                                registered_at: entry.registered_at,
                            };
                            let _ = self.storage.store_timer(&stored).await;
                        }
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

            // Register dwell timers for the new state
            self.register_dwell_timers(instance_id, &instance.machine, target_state, &machine_def);

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
                            // Persist new timer to storage
                            if let Some(entry) =
                                self.timer_manager.get_timer(instance_id, target_state)
                            {
                                let stored = smql_storage::StoredTimer {
                                    instance_id: instance_id.to_string(),
                                    machine: instance.machine.clone(),
                                    from_state: target_state.to_string(),
                                    target_state: timeout.target_state.clone(),
                                    deadline: entry.deadline,
                                    registered_at: entry.registered_at,
                                };
                                let _ = self.storage.store_timer(&stored).await;
                            }
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

        tracing::info!("timeout transition fired");
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

                // Fire expired timeout transitions
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

                // Fire expired dwell hooks (no state transition, just actions)
                let expired_dwell = engine.timer_manager.drain_expired_dwell();
                for entry in expired_dwell {
                    engine
                        .fire_dwell_hook(&entry.instance_id, &entry.state, &entry.duration)
                        .await;
                }
            }
        })
    }

    /// Restore persisted timers from storage into the in-memory TimerManager.
    ///
    /// Call this on startup before `start_timer_loop()` to recover timers
    /// that were registered before a restart. Returns the number of timers restored.
    pub async fn restore_timers(&self) -> SmqlResult<usize> {
        let stored_timers = self.storage.load_all_timers().await?;
        let mut count = 0;
        for timer in &stored_timers {
            self.timer_manager.register_with_deadline(
                &timer.instance_id,
                &timer.machine,
                &timer.from_state,
                &timer.target_state,
                timer.deadline,
                timer.registered_at,
            );
            count += 1;
        }
        if count > 0 {
            tracing::info!(count, "restored timers from storage");
        }
        Ok(count)
    }

    // -----------------------------------------------------------------------
    // Hook / Action resolution helpers
    // -----------------------------------------------------------------------

    /// Execute a named saga: run each step in order, compensating on failure.
    pub async fn execute_saga(
        &self,
        saga_name: &str,
        _trigger_instance_id: Option<&str>,
    ) -> Result<(), String> {
        let saga = self.catalog.get_saga(saga_name).map_err(|e| e.to_string())?;

        let mut completed_steps: Vec<usize> = Vec::new();

        for (i, step) in saga.steps.iter().enumerate() {
            // Evaluate WHEN guard if present
            if let Some(when_expr) = &step.when {
                let ctx = EvalContext::new(std::collections::HashMap::new(), String::new());
                match eval_guard(when_expr, &ctx) {
                    Ok(false) => {
                        tracing::info!(saga = saga_name, step = %step.name, "SAGA step skipped (WHEN false)");
                        continue;
                    }
                    Err(e) => {
                        tracing::warn!(saga = saga_name, step = %step.name, error = %e, "SAGA step WHEN evaluation error");
                        continue;
                    }
                    Ok(true) => {}
                }
            }

            // Evaluate instance_expr to get the instance ID
            let ctx = EvalContext::new(std::collections::HashMap::new(), String::new());
            let instance_id_val = eval_expr(&step.instance_expr, &ctx)
                .map_err(|e| format!("SAGA step '{}' instance_expr error: {}", step.name, e))?;
            let instance_id = match instance_id_val {
                Value::Text(s) => s,
                other => format!("{}", other),
            };

            let cmd = TransitionCommand {
                machine: step.machine.clone(),
                instance_id: instance_id.clone(),
                to_state: step.to_state.clone(),
                with_data: Vec::new(),
                memo: Some(format!("SAGA {} step {}", saga_name, step.name)),
                as_actor: Some("System".to_string()),
                through: Vec::new(),
                or_stay: false,
                cascade: false,
            };

            match self.transition_inner(&cmd).await {
                Ok(_) => {
                    tracing::info!(saga = saga_name, step = %step.name, "SAGA step completed");
                    completed_steps.push(i);
                }
                Err(e) => {
                    tracing::warn!(saga = saga_name, step = %step.name, error = %e, "SAGA step failed — compensating");

                    // Run compensation for all completed steps in reverse order
                    for &ci in completed_steps.iter().rev() {
                        if let Some(comp) = &saga.steps[ci].compensate {
                            let comp_ctx = EvalContext::new(std::collections::HashMap::new(), String::new());
                            if let Ok(comp_id_val) = eval_expr(&comp.instance_expr, &comp_ctx) {
                                let comp_id = match comp_id_val {
                                    Value::Text(s) => s,
                                    other => format!("{}", other),
                                };
                                let comp_cmd = TransitionCommand {
                                    machine: comp.machine.clone(),
                                    instance_id: comp_id,
                                    to_state: comp.to_state.clone(),
                                    with_data: Vec::new(),
                                    memo: Some(format!("SAGA {} compensation for step {}", saga_name, saga.steps[ci].name)),
                                    as_actor: Some("System".to_string()),
                                    through: Vec::new(),
                                    or_stay: false,
                                    cascade: false,
                                };
                                let _ = self.transition_inner(&comp_cmd).await;
                            }
                        }
                    }

                    return Err(format!("SAGA '{}' failed at step '{}': {}", saga_name, step.name, e));
                }
            }
        }

        tracing::info!(saga = saga_name, "SAGA completed successfully");
        Ok(())
    }

    /// Check for REACTIVE WHEN clauses on transitions from the current state.
    /// If a reactive condition is true, auto-fire the transition (as "System").
    /// Uses an iterative loop (depth-limited) to prevent infinite reactive chains.
    pub async fn check_and_fire_reactive(
        &self,
        instance_id: &str,
        machine: &str,
        initial_state: &str,
    ) {
        let mut current_state = initial_state.to_string();
        let mut depth: u8 = 0;

        loop {
            if depth > 8 {
                tracing::warn!(instance_id, "REACTIVE chain depth limit reached");
                return;
            }

            let machine_def = match self.catalog.get(machine) {
                Ok(m) => m,
                Err(_) => return,
            };

            let id = match smql_storage::InstanceId::from_string(instance_id) {
                Ok(id) => id,
                Err(_) => return,
            };

            let instance = match self.storage.get_instance(&id).await {
                Ok(Some(inst)) => inst,
                _ => return,
            };

            if instance.state != current_state {
                return; // State changed externally
            }

            let ctx = EvalContext::new(instance.data.clone(), instance.state.clone());
            let mut fired = false;

            for transition in &machine_def.transitions {
                let from_matches = match &transition.from {
                    smql_ast::machine::TransitionSource::State(s) => s == &current_state,
                    smql_ast::machine::TransitionSource::Any { except } => {
                        !except.contains(&current_state)
                    }
                    smql_ast::machine::TransitionSource::Group(_) => false,
                };

                if !from_matches {
                    continue;
                }

                if let Some(reactive) = &transition.reactive {
                    match eval_guard(&reactive.condition, &ctx) {
                        Ok(true) => {
                            tracing::info!(
                                instance_id,
                                from = %current_state,
                                to = %transition.to,
                                "REACTIVE auto-transition firing"
                            );
                            let cmd = TransitionCommand {
                                machine: machine.to_string(),
                                instance_id: instance_id.to_string(),
                                to_state: transition.to.clone(),
                                with_data: Vec::new(),
                                memo: Some("REACTIVE auto-transition".to_string()),
                                as_actor: Some("System".to_string()),
                                through: Vec::new(),
                                or_stay: false,
                                cascade: false,
                            };
                            if let Ok(result) = self.transition_inner(&cmd).await {
                                current_state = result.to_state;
                                depth += 1;
                                fired = true;
                            }
                            break; // Only fire the first matching reactive transition per loop
                        }
                        Ok(false) => {}
                        Err(e) => {
                            tracing::warn!(
                                instance_id,
                                error = %e,
                                "REACTIVE condition evaluation error"
                            );
                        }
                    }
                }
            }

            if !fired {
                break; // No reactive transition fired — done
            }
        }
    }

    /// Fire DEFINE SUBSCRIPTION actions that match a transition event.
    async fn fire_subscriptions_for_transition(
        &self,
        hook_ctx: &HookContext,
        machine: &str,
        from_state: &str,
        to_state: &str,
        eval_ctx: &EvalContext,
    ) {
        use smql_ast::subscription::SubscriptionEvent;
        let subs = self.catalog.subscriptions_for_machine(machine);
        for sub in &subs {
            let matches = match &sub.event {
                SubscriptionEvent::OnEnter { machine: m, state } => {
                    m == machine && state == to_state
                }
                SubscriptionEvent::OnExit { machine: m, state } => {
                    m == machine && state == from_state
                }
                SubscriptionEvent::OnTransition { machine: m, from_state: fs, to_state: ts } => {
                    m == machine
                        && fs.as_deref().map_or(true, |s| s == from_state)
                        && ts.as_deref().map_or(true, |s| s == to_state)
                }
                SubscriptionEvent::OnSpawn { .. } => false,
            };
            if matches {
                let resolved = self.resolve_actions(&sub.actions, eval_ctx);
                if let Err(e) = self.hook_executor.execute_actions(&resolved, hook_ctx).await {
                    tracing::warn!(
                        subscription = %sub.name,
                        error = %e,
                        "DEFINE SUBSCRIPTION action failed"
                    );
                }
            }
        }
    }

    /// Fire DEFINE SUBSCRIPTION actions that match an ON SPAWN event.
    async fn fire_subscriptions_for_spawn(
        &self,
        hook_ctx: &HookContext,
        machine: &str,
        eval_ctx: &EvalContext,
    ) {
        use smql_ast::subscription::SubscriptionEvent;
        let subs = self.catalog.subscriptions_for_machine(machine);
        for sub in &subs {
            if matches!(&sub.event, SubscriptionEvent::OnSpawn { machine: m } if m == machine) {
                let resolved = self.resolve_actions(&sub.actions, eval_ctx);
                if let Err(e) = self.hook_executor.execute_actions(&resolved, hook_ctx).await {
                    tracing::warn!(
                        subscription = %sub.name,
                        error = %e,
                        "DEFINE SUBSCRIPTION ON SPAWN action failed"
                    );
                }
            }
        }
    }

    /// Filter instance data based on actor role's field-level read permissions.
    /// Returns a filtered copy of the data map.
    pub fn filter_readable_fields(
        &self,
        machine_def: &MachineDefinition,
        data: &HashMap<String, Value>,
        actor_role: Option<&str>,
    ) -> HashMap<String, Value> {
        use smql_ast::machine::RolePermission;

        let role_def = actor_role.and_then(|role_name| {
            machine_def.roles.iter().find(|r| r.name == role_name)
        });

        let Some(role) = role_def else {
            return data.clone(); // No role restriction — return all fields
        };

        // Check for CAN ALL
        if role.permissions.iter().any(|p| matches!(p, RolePermission::CanAll)) {
            return data.clone();
        }

        // Check for explicit CAN READ allowlist
        let can_read: Option<&Vec<String>> = role.permissions.iter().find_map(|p| {
            if let RolePermission::CanReadFields(fields) = p { Some(fields) } else { None }
        });

        // Check for CANNOT READ denylist
        let cannot_read: Option<&Vec<String>> = role.permissions.iter().find_map(|p| {
            if let RolePermission::CannotReadFields(fields) = p { Some(fields) } else { None }
        });

        data.iter()
            .filter(|(field, _)| {
                // If there's an allowlist, only include listed fields
                if let Some(allowed) = can_read {
                    return allowed.contains(field);
                }
                // If there's a denylist, exclude denied fields
                if let Some(denied) = cannot_read {
                    return !denied.contains(field);
                }
                // No field restrictions — include all
                true
            })
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    /// Check if an actor role can write a specific field.
    pub fn can_write_field(
        &self,
        machine_def: &MachineDefinition,
        field: &str,
        actor_role: Option<&str>,
    ) -> bool {
        use smql_ast::machine::RolePermission;

        let role_def = actor_role.and_then(|role_name| {
            machine_def.roles.iter().find(|r| r.name == role_name)
        });

        let Some(role) = role_def else {
            return true; // No role restriction — allow all writes
        };

        // Check for CAN ALL
        if role.permissions.iter().any(|p| matches!(p, RolePermission::CanAll)) {
            return true;
        }

        // Check for explicit CAN WRITE allowlist
        let can_write: Option<&Vec<String>> = role.permissions.iter().find_map(|p| {
            if let RolePermission::CanWriteFields(fields) = p { Some(fields) } else { None }
        });

        // Check for CANNOT WRITE denylist
        let cannot_write: Option<&Vec<String>> = role.permissions.iter().find_map(|p| {
            if let RolePermission::CannotWriteFields(fields) = p { Some(fields) } else { None }
        });

        if let Some(allowed) = can_write {
            return allowed.contains(&field.to_string());
        }
        if let Some(denied) = cannot_write {
            return !denied.contains(&field.to_string());
        }
        true
    }

    /// Evaluate all COMPUTED fields in a machine definition and update the data map.
    /// Called after spawn data validation and after each transition mutation.
    fn evaluate_computed_fields(
        &self,
        machine_def: &MachineDefinition,
        data: &mut HashMap<String, Value>,
        state: &str,
    ) {
        use smql_ast::types::Constraint;
        for field_def in &machine_def.data {
            let computed_expr = field_def.constraints.iter().find_map(|c| {
                if let Constraint::Computed(expr) = c {
                    Some(expr)
                } else {
                    None
                }
            });
            if let Some(expr) = computed_expr {
                let ctx = EvalContext::new(data.clone(), state.to_string());
                match eval_expr(expr, &ctx) {
                    Ok(val) => {
                        data.insert(field_def.name.clone(), val);
                    }
                    Err(e) => {
                        tracing::warn!(
                            field = %field_def.name,
                            error = %e,
                            "COMPUTED field evaluation failed"
                        );
                    }
                }
            }
        }
    }

    /// Register dwell timers for all ON DWELL hooks matching the given state.
    fn register_dwell_timers(
        &self,
        instance_id: &str,
        machine: &str,
        state: &str,
        machine_def: &MachineDefinition,
    ) {
        for hook in &machine_def.hooks {
            if let HookTrigger::OnDwell { state: dwell_state, duration } = &hook.trigger {
                if dwell_state == state {
                    self.timer_manager.register_dwell(instance_id, machine, state, duration);
                }
            }
        }
    }

    /// Fire dwell hook actions for an instance that has dwelled in a state.
    /// This does NOT transition the instance — it only fires actions.
    pub async fn fire_dwell_hook(
        &self,
        instance_id: &str,
        expected_state: &str,
        duration: &smql_ast::value::SmqlDuration,
    ) {
        let id = match smql_storage::InstanceId::from_string(instance_id) {
            Ok(id) => id,
            Err(_) => return,
        };

        let instance = match self.storage.get_instance(&id).await {
            Ok(Some(inst)) => inst,
            _ => return,
        };

        // Race condition: instance already left the state
        if instance.state != expected_state {
            return;
        }

        let machine_def = match self.catalog.get(&instance.machine) {
            Ok(m) => m,
            Err(_) => return,
        };

        // Find matching ON DWELL hooks for this state + duration
        let matching_hooks: Vec<&smql_ast::machine::HookDefinition> = machine_def
            .hooks
            .iter()
            .filter(|h| {
                matches!(
                    &h.trigger,
                    HookTrigger::OnDwell { state, duration: d }
                    if state == expected_state && d.seconds == duration.seconds
                )
            })
            .collect();

        if matching_hooks.is_empty() {
            return;
        }

        let hook_ctx = HookContext {
            instance_id: instance_id.to_string(),
            machine: instance.machine.clone(),
            from_state: instance.state.clone(),
            to_state: instance.state.clone(),
            data: instance.data.clone(),
            actor: Some("System".to_string()),
            memo: None,
        };

        let eval_ctx = EvalContext::new(instance.data.clone(), instance.state.clone());

        for hook in matching_hooks {
            let resolved = self.resolve_actions(&hook.actions, &eval_ctx);
            if let Err(e) = self.hook_executor.execute_actions(&resolved, &hook_ctx).await {
                tracing::warn!(
                    instance_id = %instance_id,
                    state = %expected_state,
                    error = %e,
                    "ON DWELL hook action failed"
                );
            }
        }

        tracing::info!(
            instance_id = %instance_id,
            state = %expected_state,
            duration_secs = %duration.seconds,
            "ON DWELL hook fired"
        );
    }

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
            Action::Conditional { condition, action } => {
                match eval_guard(condition, ctx) {
                    Ok(true) => self.resolve_action(action, ctx),
                    Ok(false) => Err(SmqlError::internal("__conditional_skip")),
                    Err(_) => Err(SmqlError::internal("__conditional_skip")),
                }
            }
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

// ---------------------------------------------------------------------------
// EngineCallback — allows hooks to spawn children / signal parent
// ---------------------------------------------------------------------------

/// Wrapper that holds an Arc<Engine> for use as EngineCallback.
/// We can't impl EngineCallback directly on Engine because we need Arc<Self>.
pub struct EngineCallbackImpl {
    pub catalog: Arc<MachineCatalog>,
    pub storage: Arc<dyn Storage>,
    pub timer_manager: Arc<TimerManager>,
    pub hook_executor: Arc<HookExecutor>,
}

#[async_trait::async_trait]
impl EngineCallback for EngineCallbackImpl {
    async fn spawn_child(
        &self,
        parent_instance_id: &str,
        machine: &str,
        data: Vec<(String, Value)>,
    ) -> Result<String, HookError> {
        let parent_id =
            smql_storage::InstanceId::from_string(parent_instance_id).map_err(|_| {
                HookError::ActionFailed {
                    message: format!("Invalid parent instance ID: {}", parent_instance_id),
                }
            })?;
        let parent = self
            .storage
            .get_instance(&parent_id)
            .await
            .map_err(|e| HookError::ActionFailed {
                message: format!("Failed to get parent: {}", e),
            })?
            .ok_or_else(|| HookError::ActionFailed {
                message: format!("Parent instance not found: {}", parent_instance_id),
            })?;

        let data_exprs: Vec<(String, smql_ast::expression::Expression)> = data
            .into_iter()
            .map(|(k, v)| {
                (
                    k,
                    smql_ast::expression::Expression::new(
                        smql_ast::expression::ExpressionKind::Literal(v),
                    ),
                )
            })
            .collect();

        let cmd = SpawnCommand {
            machine: machine.to_string(),
            data: data_exprs,
            then_transition: None,
            batch: false,
            batch_data: Vec::new(),
            parent_id: Some(parent_instance_id.to_string()),
            parent_machine: Some(parent.machine.clone()),
            as_actor: None,
        };

        // Create a temporary engine to perform the spawn
        let engine = Engine {
            catalog: self.catalog.clone(),
            storage: self.storage.clone(),
            timer_manager: self.timer_manager.clone(),
            hook_executor: self.hook_executor.clone(),
        };
        let result = engine
            .spawn(&cmd)
            .await
            .map_err(|e| HookError::ActionFailed {
                message: format!("Failed to spawn child: {}", e),
            })?;
        Ok(result.instance.id.as_str())
    }

    async fn signal_parent(
        &self,
        child_instance_id: &str,
        target_state: &str,
    ) -> Result<(), HookError> {
        let child_id = smql_storage::InstanceId::from_string(child_instance_id).map_err(|_| {
            HookError::ActionFailed {
                message: format!("Invalid child instance ID: {}", child_instance_id),
            }
        })?;
        let child = self
            .storage
            .get_instance(&child_id)
            .await
            .map_err(|e| HookError::ActionFailed {
                message: format!("Failed to get child: {}", e),
            })?
            .ok_or_else(|| HookError::ActionFailed {
                message: format!("Child instance not found: {}", child_instance_id),
            })?;

        let parent_id = match &child.parent_id {
            Some(pid) => pid.clone(),
            None => return Ok(()), // No parent — no-op
        };

        let parent_machine = child.parent_machine.clone().unwrap_or_default();
        let cmd =
            TransitionCommand::new(parent_machine, parent_id.as_str(), target_state.to_string());
        let engine = Engine {
            catalog: self.catalog.clone(),
            storage: self.storage.clone(),
            timer_manager: self.timer_manager.clone(),
            hook_executor: self.hook_executor.clone(),
        };
        // Use try_transition so if the parent can't transition, we don't fail the hook
        let _ = engine
            .try_transition(&cmd)
            .await
            .map_err(|e| HookError::ActionFailed {
                message: format!("Failed to signal parent: {}", e),
            })?;
        Ok(())
    }
}

impl Engine {
    /// Wire up the engine callback on the hook executor.
    /// Call this after constructing an Arc<Engine> to enable SPAWN CHILD / SIGNAL PARENT.
    pub fn wire_callback(&self) {
        let cb = Arc::new(EngineCallbackImpl {
            catalog: self.catalog.clone(),
            storage: self.storage.clone(),
            timer_manager: self.timer_manager.clone(),
            hook_executor: self.hook_executor.clone(),
        });
        self.hook_executor.set_callback(cb);
    }

    /// Populate the EvalContext with children and parent data for composition guards.
    async fn populate_composition_context(
        &self,
        ctx: &mut EvalContext,
        instance: &Instance,
        machine_def: &MachineDefinition,
    ) {
        // Populate children for each child definition in the machine
        for child_def in &machine_def.children {
            let children = self
                .storage
                .find_children(&instance.id, Some(&child_def.machine))
                .await
                .unwrap_or_default();

            let child_infos: Vec<ChildInfo> = children
                .into_iter()
                .map(|c| ChildInfo {
                    id: c.id.as_str(),
                    machine: c.machine,
                    state: c.state,
                    data: c.data,
                })
                .collect();

            ctx.children.insert(child_def.name.clone(), child_infos);
        }

        // Populate parent data if this instance has a parent
        if let Some(parent_id) = &instance.parent_id {
            if let Ok(Some(parent)) = self.storage.get_instance(parent_id).await {
                ctx.parent_data = Some(parent.data);
                ctx.parent_state = Some(parent.state);
            }
        }
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
