use chrono::Utc;
use dashmap::DashMap;
use smql_ast::command::{BatchTransitionCommand, ClaimCommand, ReleaseCommand, SpawnCommand, TransitionCommand, WatchCommand};
use smql_ast::error::{GuardFailure, RecoveryAction, RecoveryOption, TransitionDeniedError};
use smql_ast::expression::Expression;
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

/// A registered watcher waiting for a condition to become true.
struct Watcher {
    id: String,
    condition: Expression,
    instance_id: Option<String>,
    filter: Option<Expression>,
    sender: tokio::sync::oneshot::Sender<Instance>,
}

/// Result of a watch operation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WatchResult {
    pub instance: Instance,
    pub waited_ms: u64,
}

/// Result of one step in a transaction.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum TransactionStepResult {
    Spawned { instance_id: String, state: String },
    Transitioned { instance_id: String, from_state: String, to_state: String },
    Skipped,
}

/// The core SMQL engine — executes spawn, transition, and query operations.
pub struct Engine {
    pub catalog: Arc<MachineCatalog>,
    pub storage: Arc<dyn Storage>,
    pub timer_manager: Arc<TimerManager>,
    pub hook_executor: Arc<HookExecutor>,
    /// Watcher registry: machine name -> list of watchers.
    watchers: DashMap<String, Vec<Watcher>>,
}

/// Result of a spawn operation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SpawnResult {
    pub instance: Instance,
}

/// Result of a transition operation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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

/// Result of a batch spawn operation.
#[derive(Debug, Clone)]
pub struct BatchSpawnResult {
    pub created: Vec<Instance>,
    pub failures: Vec<BatchSpawnFailure>,
}

/// A single failure in a batch spawn.
#[derive(Debug, Clone)]
pub struct BatchSpawnFailure {
    pub index: usize,
    pub error: String,
}

/// A single failure in a batch transition.
#[derive(Debug, Clone)]
pub struct BatchTransitionFailure {
    pub instance_id: String,
    pub error: String,
}

/// Result of a claim operation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ClaimResult {
    pub instance: Instance,
    pub agent_id: String,
    pub lease_expires_at: chrono::DateTime<chrono::Utc>,
}

/// Result of a release operation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReleaseResult {
    pub instance_id: String,
    pub agent_id: String,
}

impl Engine {
    pub fn new(catalog: Arc<MachineCatalog>, storage: Arc<dyn Storage>) -> Self {
        let event_bus = Arc::new(EventBus::default());
        Self {
            catalog,
            storage,
            timer_manager: Arc::new(TimerManager::new()),
            hook_executor: Arc::new(HookExecutor::new(event_bus)),
            watchers: DashMap::new(),
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
            watchers: DashMap::new(),
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
            watchers: DashMap::new(),
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
        Box::pin(self.spawn_inner(cmd, None))
    }

    /// Spawn a new machine instance with an explicit actor override (from JWT auth).
    pub fn spawn_with_actor<'a>(
        &'a self,
        cmd: &'a SpawnCommand,
        actor: ActorInfo,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = SmqlResult<SpawnResult>> + Send + 'a>> {
        Box::pin(self.spawn_inner(cmd, Some(actor)))
    }

    #[tracing::instrument(skip(self, cmd, actor_override), fields(machine = %cmd.machine))]
    async fn spawn_inner(&self, cmd: &SpawnCommand, actor_override: Option<ActorInfo>) -> SmqlResult<SpawnResult> {
        // Idempotency check: if key is set, check for cached result
        if let Some(ref ikey) = cmd.idempotency_key {
            if let Some(cached) = self.storage.get_idempotency(ikey).await? {
                if let Ok(result) = serde_json::from_slice::<SpawnResult>(&cached) {
                    tracing::debug!(key = %ikey, "Returning cached idempotent spawn result");
                    return Ok(result);
                }
            }
        }

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
        // If actor_override is present (from JWT), use its role; otherwise fall back to as_actor
        let spawn_actor_role = actor_override
            .as_ref()
            .and_then(|a| a.role.as_deref())
            .or_else(|| cmd.as_actor.as_deref());
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
        let mut instance = if let (Some(parent_id_str), Some(parent_machine)) =
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

        // Apply tags if provided
        if !cmd.tags.is_empty() {
            for (key, value) in &cmd.tags {
                instance.tags.insert(key.clone(), value.clone());
            }
        }

        // Apply TTL if provided
        if let Some(ref ttl) = cmd.ttl {
            let duration = chrono::Duration::seconds(ttl.seconds as i64);
            instance.expires_at = Some(Utc::now() + duration);
        }

        // Create initial trail entry (spawn event)
        // If actor_override is present (from JWT), record the authenticated identity
        let spawn_actor_id = actor_override
            .as_ref()
            .map(|a| a.id.clone())
            .or_else(|| cmd.as_actor.clone());
        let trail_entry = TrailEntry {
            instance_id: instance.id.clone(),
            machine: cmd.machine.clone(),
            sequence: 0,
            from_state: String::new(),
            to_state: machine_def.initial_state.clone(),
            transition_name: Some("SPAWN".to_string()),
            actor: spawn_actor_id.clone(),
            memo: None,
            timestamp: Utc::now(),
            data_snapshot: Some(instance.data.clone()),
        };

        // --- Store durable event ---
        let spawn_event = smql_storage::instance::StoredEvent {
            id: ulid::Ulid::new().to_string(),
            timestamp: Utc::now(),
            machine: cmd.machine.clone(),
            event_name: "spawn".to_string(),
            instance_id: instance.id.as_str(),
            payload: serde_json::json!({
                "machine": cmd.machine,
                "initial_state": machine_def.initial_state,
            }),
            actor: spawn_actor_id.clone(),
        };

        // Store atomically: instance + trail + event in one write
        self.storage
            .spawn_instance(&instance, &trail_entry, Some(&spawn_event), None)
            .await?;

        // --- Fire ON SPAWN hooks ---
        let hook_ctx = HookContext {
            instance_id: instance.id.as_str(),
            machine: cmd.machine.clone(),
            from_state: String::new(),
            to_state: machine_def.initial_state.clone(),
            data: instance.data.clone(),
            actor: spawn_actor_id,
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
                idempotency_key: None,
                tags: Vec::new(),
            };
            let result = self.transition(&transition_cmd).await?;
            return Ok(SpawnResult {
                instance: result.instance,
            });
        }

        tracing::info!(id = %instance.id, state = %instance.state, "instance spawned");

        // Notify any watchers waiting on this machine
        self.notify_watchers(&cmd.machine, &instance);

        let result = SpawnResult { instance };

        // Store idempotency entry if key was provided
        if let Some(ref ikey) = cmd.idempotency_key {
            if let Ok(serialized) = serde_json::to_vec(&result) {
                let expires_at = Utc::now() + chrono::Duration::hours(24);
                if let Err(e) = self.storage.store_idempotency(ikey, &serialized, expires_at).await {
                    tracing::warn!(key = %ikey, error = %e, "Failed to store idempotency key after spawn");
                }
            }
        }

        Ok(result)
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
        Box::pin(self.transition_inner(cmd, None))
    }

    /// Execute a transition with an explicit actor override (from JWT auth).
    pub fn transition_with_actor<'a>(
        &'a self,
        cmd: &'a TransitionCommand,
        actor: ActorInfo,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = SmqlResult<TransitionResult>> + Send + 'a>,
    > {
        Box::pin(self.transition_inner(cmd, Some(actor)))
    }

    /// Execute a transition and then check for REACTIVE WHEN auto-transitions.
    pub async fn transition_reactive(
        &self,
        cmd: &TransitionCommand,
    ) -> SmqlResult<TransitionResult> {
        let result = self.transition_inner(cmd, None).await?;
        let iid = result.instance.id.to_string();
        let machine = result.instance.machine.clone();
        let new_state = result.to_state.clone();
        self.check_and_fire_reactive(&iid, &machine, &new_state).await;
        Ok(result)
    }

    fn transition_inner<'a>(
        &'a self,
        cmd: &'a TransitionCommand,
        actor_override: Option<ActorInfo>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = SmqlResult<TransitionResult>> + Send + 'a>> {
        Box::pin(self.transition_inner_impl(cmd, actor_override))
    }

    #[tracing::instrument(skip(self, cmd, actor_override), fields(instance_id = %cmd.instance_id, to_state = %cmd.to_state))]
    async fn transition_inner_impl(&self, cmd: &TransitionCommand, actor_override: Option<ActorInfo>) -> SmqlResult<TransitionResult> {
        // Idempotency check: if key is set, check for cached result
        if let Some(ref ikey) = cmd.idempotency_key {
            if let Some(cached) = self.storage.get_idempotency(ikey).await? {
                if let Ok(result) = serde_json::from_slice::<TransitionResult>(&cached) {
                    tracing::debug!(key = %ikey, "Returning cached idempotent transition result");
                    return Ok(result);
                }
            }
        }

        // Handle THROUGH (multi-hop)
        if !cmd.through.is_empty() {
            return self.transition_through(cmd, actor_override).await;
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
            actor: actor_override.clone().or_else(|| {
                cmd.as_actor.as_ref().map(|a| ActorInfo {
                    id: a.clone(),
                    role: None,
                    capabilities: Vec::new(),
                    fields: HashMap::new(),
                })
            }),
            state_entered_at: instance.state_entered_at,
            created_at: instance.created_at,
            now: Utc::now(),
            timeout_remaining,
            children: HashMap::new(),
            parent_data: None,
            parent_state: None,
            terminal_states: None,
            visited_states: None,
            tags: instance.tags.clone(),
        };

        // Populate children/parent context for composition guards
        if !machine_def.children.is_empty() || instance.parent_id.is_some() {
            self.populate_composition_context(&mut ctx, &instance, &machine_def)
                .await;
        }

        // Build HookContext for this transition
        // Use actor_override identity (from JWT) when present, otherwise fall back to command's AS clause
        let effective_actor = actor_override
            .as_ref()
            .map(|a| a.id.clone())
            .or_else(|| cmd.as_actor.clone());
        let hook_ctx = HookContext {
            instance_id: cmd.instance_id.clone(),
            machine: instance.machine.clone(),
            from_state: instance.state.clone(),
            to_state: cmd.to_state.clone(),
            data: ctx.data.clone(),
            actor: effective_actor.clone(),
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
                recovery_options: vec![RecoveryOption {
                    action: RecoveryAction::Escalate,
                    field: None,
                    suggested_value: None,
                    reason: "A BEFORE hook rejected this transition. Escalating to a human or alternative path may be required.".to_string(),
                    example: Some(format!("TRANSITION {} {} TO awaiting_agent", instance.machine, cmd.instance_id)),
                }],
                llm_prompt: Some(format!(
                    "Transition {} -> {} for instance {} was rejected by a BEFORE hook: {}. Consider escalating to a human agent.",
                    instance.state, cmd.to_state, cmd.instance_id, e
                )),
            }));
        }

        // --- 2. Evaluate ALL guard conditions — collect ALL failures ---
        // First expand APPLY POLICY guards, then evaluate inline guards.
        // We collect both GuardFailure (for error reporting) and the original Expression AST
        // (for precise recovery option generation).
        let mut guard_failures = Vec::new();
        let mut failed_guard_exprs: Vec<smql_ast::Expression> = Vec::new();

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
                                failed_guard_exprs.push(guard.clone());
                            }
                            Err(e) => {
                                guard_failures.push(GuardFailure {
                                    guard_expr: format!("[POLICY {}] {}", policy_name, guard),
                                    actual_value: Some(e.to_string()),
                                    expected: None,
                                    hint: None,
                                });
                                failed_guard_exprs.push(guard.clone());
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
                    failed_guard_exprs.push(guard.clone());
                }
                Err(e) => {
                    guard_failures.push(GuardFailure {
                        guard_expr: guard.to_string(),
                        actual_value: Some(e.to_string()),
                        expected: None,
                        hint: None,
                    });
                    failed_guard_exprs.push(guard.clone());
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
                    failed_guard_exprs.push(rule.invariant.clone());
                }
                Err(e) => {
                    guard_failures.push(GuardFailure {
                        guard_expr: format!("[RULE {}] {}", rule.name, rule.invariant),
                        actual_value: Some(e.to_string()),
                        expected: None,
                        hint: None,
                    });
                    failed_guard_exprs.push(rule.invariant.clone());
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
                let or_stay_result = TransitionResult {
                    from_state: instance.state.clone(),
                    to_state: instance.state.clone(),
                    instance: updated,
                };
                // Store idempotency entry if key was provided
                if let Some(ref ikey) = cmd.idempotency_key {
                    if let Ok(serialized) = serde_json::to_vec(&or_stay_result) {
                        let expires_at = Utc::now() + chrono::Duration::hours(24);
                        if let Err(e) = self.storage.store_idempotency(ikey, &serialized, expires_at).await {
                            tracing::warn!(key = %ikey, error = %e, "Failed to store idempotency key after OR STAY");
                        }
                    }
                }
                return Ok(or_stay_result);
            }

            // Generate recovery options using AST-based analysis of failed guard expressions
            let recovery_options = Self::generate_recovery_options_from_ast(&failed_guard_exprs, &guard_failures, &cmd.instance_id, &instance.machine, &cmd.to_state);
            let llm_prompt = Self::generate_llm_prompt(&guard_failures, &cmd.instance_id, &instance.state, &cmd.to_state);

            return Err(SmqlError::TransitionDenied(TransitionDeniedError {
                instance_id: cmd.instance_id.clone(),
                from_state: instance.state.clone(),
                to_state: cmd.to_state.clone(),
                guard_failures,
                hint: None,
                recovery_options,
                llm_prompt,
            }));
        }

        // --- 3. Build mutations from WITH data and MUTATE clauses ---
        // Check field-level write permissions first
        // Use actor_override role (from JWT) when present, otherwise fall back to command's AS clause
        let actor_role = actor_override
            .as_ref()
            .and_then(|a| a.role.as_deref())
            .or_else(|| cmd.as_actor.as_deref());
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
                    recovery_options: vec![
                        RecoveryOption {
                            action: RecoveryAction::ChangeActor,
                            field: Some(field.clone()),
                            suggested_value: None,
                            reason: format!("Current role '{}' cannot write field '{}'. Retry as a role with write permission.", actor_role.unwrap_or("unknown"), field),
                            example: Some(format!("AS \"admin\" TRANSITION {} {} TO {}", instance.machine, cmd.instance_id, cmd.to_state)),
                        },
                        RecoveryOption {
                            action: RecoveryAction::Escalate,
                            field: None,
                            suggested_value: None,
                            reason: "Escalate to an agent with appropriate permissions.".to_string(),
                            example: Some(format!("TRANSITION {} {} TO awaiting_agent", instance.machine, cmd.instance_id)),
                        },
                    ],
                    llm_prompt: Some(format!(
                        "Transition {} -> {} for instance {} failed: role '{}' cannot write field '{}'. Retry as a different actor or escalate.",
                        instance.state, cmd.to_state, cmd.instance_id, actor_role.unwrap_or("unknown"), field
                    )),
                }));
            }
        }

        // Reject COMPUTED fields in WITH data — they are read-only
        {
            use smql_ast::types::Constraint;
            for (field, _) in &cmd.with_data {
                if let Some(field_def) = machine_def.data.iter().find(|d| d.name == *field) {
                    if field_def.constraints.iter().any(|c| matches!(c, Constraint::Computed(_))) {
                        return Err(SmqlError::ValidationError {
                            message: format!("Field '{}' is COMPUTED and cannot be set directly", field),
                            field: Some(field.clone()),
                            hint: Some("Remove this field from WITH data — it is auto-derived".to_string()),
                        });
                    }
                }
            }
        }

        let mut mutations = Vec::new();
        for (field, expr) in &cmd.with_data {
            let val = eval_expr(expr, &ctx)?;
            mutations.push(Mutation::SetField(field.clone(), val));
        }
        // Collect deferred spawn commands — these execute AFTER the version check
        // to avoid creating orphaned children if the transition conflicts.
        let mut deferred_spawns: Vec<(String, SpawnCommand)> = Vec::new();
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
                        // Build spawn command for deferred execution
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
                            idempotency_key: None,
                            tags: Vec::new(),
                            ttl: None,
                        };
                        deferred_spawns.push((mutate.field.clone(), child_cmd));
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

        // Apply tag mutations from command
        for (key, value) in &cmd.tags {
            mutations.push(Mutation::SetTag(key.clone(), value.clone()));
        }

        // Create trail entry — use authenticated actor identity when available
        let trail_entry = TrailEntry {
            instance_id: id.clone(),
            machine: instance.machine.clone(),
            sequence: instance.trail_length + 1,
            from_state: instance.state.clone(),
            to_state: cmd.to_state.clone(),
            transition_name: Some(format!("{} -> {}", instance.state, cmd.to_state)),
            actor: effective_actor,
            memo: cmd.memo.clone(),
            timestamp: Utc::now(),
            data_snapshot: None,
        };

        // --- 4. Atomic storage write ---
        let transitioned_instance = self
            .storage
            .transition_instance(
                &id,
                instance.version,
                &cmd.to_state,
                &mutations,
                trail_entry,
            )
            .await?;

        // --- 4a. Store durable transition event ---
        let transition_event = smql_storage::instance::StoredEvent {
            id: ulid::Ulid::new().to_string(),
            timestamp: Utc::now(),
            machine: cmd.machine.clone(),
            event_name: "transition".to_string(),
            instance_id: cmd.instance_id.clone(),
            payload: serde_json::json!({
                "from_state": instance.state,
                "to_state": cmd.to_state,
            }),
            actor: cmd.as_actor.clone(),
        };
        if let Err(e) = self.storage.store_event(&transition_event).await {
            tracing::error!(error = %e, "Failed to store transition event — event log has a gap");
        }

        // --- 4b. Execute deferred SPAWN commands (after version check succeeded) ---
        let has_deferred_spawns = !deferred_spawns.is_empty();
        if has_deferred_spawns {
            // Use the transitioned instance's actual version, not the stale pre-transition version
            let mut current_version = transitioned_instance.version;
            for (field, child_cmd) in deferred_spawns {
                match self.spawn(&child_cmd).await {
                    Ok(result) => {
                        let child_id = result.instance.id.as_str();
                        let child_machine = child_cmd.machine.clone();
                        let spawn_mutations =
                            vec![Mutation::SetField(field.clone(), Value::Ref(child_machine, child_id))];
                        match self
                            .storage
                            .update_instance(&id, current_version, &spawn_mutations)
                            .await
                        {
                            Ok(()) => {
                                current_version += 1;
                            }
                            Err(e) => {
                                tracing::error!(
                                    instance_id = %cmd.instance_id,
                                    field = %field,
                                    error = %e,
                                    "Failed to link deferred child to parent — child exists but parent reference is missing"
                                );
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!(
                            instance_id = %cmd.instance_id,
                            error = %e,
                            "Deferred MUTATE SPAWN failed (transition already committed)"
                        );
                    }
                }
            }
        }

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
        if let Err(e) = self.storage.remove_timer(&cmd.instance_id, &instance.state).await {
            tracing::warn!(error = %e, "Failed to remove old timer");
        }

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
                if let Err(e) = self.storage.store_timer(&stored).await {
                    tracing::warn!(error = %e, "Failed to persist timeout timer");
                }
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
            let cascade_failures = self.cascade_children(&id, &instance.machine).await;
            if cascade_failures > 0 {
                tracing::warn!(
                    instance_id = %cmd.instance_id,
                    failures = cascade_failures,
                    "CASCADE completed with failures — some children may still be in non-terminal states"
                );
            }
        }

        // Use the returned instance directly; only re-fetch if deferred spawns
        // mutated the stored instance further via update_instance.
        let updated = if has_deferred_spawns {
            self.storage
                .get_instance(&id)
                .await?
                .ok_or_else(|| SmqlError::not_found("Instance", &cmd.instance_id))?
        } else {
            transitioned_instance
        };

        tracing::info!(from = %instance.state, to = %cmd.to_state, "transition complete");
        let result = TransitionResult {
            from_state: instance.state,
            to_state: cmd.to_state.clone(),
            instance: updated,
        };

        // Notify any watchers waiting on this machine
        self.notify_watchers(&cmd.machine, &result.instance);

        // Store idempotency entry if key was provided
        if let Some(ref ikey) = cmd.idempotency_key {
            if let Ok(serialized) = serde_json::to_vec(&result) {
                let expires_at = Utc::now() + chrono::Duration::hours(24);
                if let Err(e) = self.storage.store_idempotency(ikey, &serialized, expires_at).await {
                    tracing::warn!(key = %ikey, error = %e, "Failed to store idempotency key after transition");
                }
            }
        }

        Ok(result)
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

    /// Try a transition with an explicit actor override (from JWT auth).
    pub async fn try_transition_with_actor(
        &self,
        cmd: &TransitionCommand,
        actor: ActorInfo,
    ) -> SmqlResult<Option<TransitionResult>> {
        match self.transition_with_actor(cmd, actor).await {
            Ok(result) => Ok(Some(result)),
            Err(SmqlError::TransitionDenied(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Batch spawn: create multiple instances in one call.
    /// Each entry in `cmd.batch_data` is a separate instance's data fields.
    /// Validation failures are collected per-index; valid instances are still created.
    pub async fn batch_spawn(&self, cmd: &SpawnCommand) -> SmqlResult<BatchSpawnResult> {
        if !cmd.batch || cmd.batch_data.is_empty() {
            return Ok(BatchSpawnResult {
                created: Vec::new(),
                failures: Vec::new(),
            });
        }

        let _machine_def = self.catalog.get(&cmd.machine)?;

        let mut created = Vec::new();
        let mut failures = Vec::new();

        for (index, data_fields) in cmd.batch_data.iter().enumerate() {
            let single = SpawnCommand {
                machine: cmd.machine.clone(),
                data: data_fields.clone(),
                then_transition: None,
                batch: false,
                batch_data: Vec::new(),
                parent_id: cmd.parent_id.clone(),
                parent_machine: cmd.parent_machine.clone(),
                as_actor: cmd.as_actor.clone(),
                idempotency_key: None,
                tags: cmd.tags.clone(),
                ttl: cmd.ttl.clone(),
            };
            match self.spawn(&single).await {
                Ok(result) => created.push(result.instance),
                Err(e) => failures.push(BatchSpawnFailure {
                    index,
                    error: e.to_string(),
                }),
            }
        }

        Ok(BatchSpawnResult { created, failures })
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
                idempotency_key: None,
                tags: Vec::new(),
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
    async fn transition_through(&self, cmd: &TransitionCommand, actor_override: Option<ActorInfo>) -> SmqlResult<TransitionResult> {
        // Validate THROUGH chain for cycles (repeated states)
        {
            let mut seen = std::collections::HashSet::new();
            for state in &cmd.through {
                if !seen.insert(state.as_str()) {
                    return Err(SmqlError::ValidationError {
                        message: format!(
                            "THROUGH chain contains duplicate state '{}' — this would create an infinite loop",
                            state
                        ),
                        field: Some("through".into()),
                        hint: Some("Remove the repeated state from the THROUGH list".into()),
                    });
                }
            }
            if !seen.insert(cmd.to_state.as_str()) {
                // Final target also duplicates a THROUGH state
                return Err(SmqlError::ValidationError {
                    message: format!(
                        "THROUGH chain target state '{}' duplicates an intermediate state — this would create an infinite loop",
                        cmd.to_state
                    ),
                    field: Some("through".into()),
                    hint: Some("The target state should not appear in the THROUGH list".into()),
                });
            }
        }

        let mut current_id = cmd.instance_id.clone();
        let mut all_states = cmd.through.clone();
        all_states.push(cmd.to_state.clone());

        let mut last_result = None;

        let last_idx = all_states.len().saturating_sub(1);
        for (i, target) in all_states.iter().enumerate() {
            let is_final_step = i == last_idx;
            let step_cmd = TransitionCommand {
                machine: cmd.machine.clone(),
                instance_id: current_id.clone(),
                to_state: target.clone(),
                with_data: if is_final_step { cmd.with_data.clone() } else { Vec::new() },
                memo: cmd.memo.clone(),
                as_actor: cmd.as_actor.clone(),
                through: Vec::new(),
                or_stay: false,
                cascade: false,
                idempotency_key: None,
                tags: Vec::new(),
            };

            let result = self.transition_inner(&step_cmd, actor_override.clone()).await?;
            current_id = result.instance.id.as_str();
            last_result = Some(result);
        }

        last_result.ok_or_else(|| SmqlError::internal("THROUGH transition had no steps"))
    }

    /// Maximum CASCADE recursion depth to prevent infinite loops with circular composition.
    const MAX_CASCADE_DEPTH: u32 = 16;

    /// Cascade: transition all children to their machine's first terminal state.
    /// Returns the number of children that failed to cascade.
    async fn cascade_children(&self, parent_id: &smql_storage::InstanceId, parent_machine: &str) -> usize {
        self.cascade_children_with_depth(parent_id, parent_machine, 0).await
    }

    /// Inner cascade with depth tracking to prevent infinite recursion.
    fn cascade_children_with_depth<'a>(
        &'a self,
        parent_id: &'a smql_storage::InstanceId,
        _parent_machine: &'a str,
        depth: u32,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = usize> + Send + 'a>> {
        Box::pin(async move {
            if depth >= Self::MAX_CASCADE_DEPTH {
                tracing::warn!(
                    parent_id = parent_id.as_str(),
                    depth,
                    "CASCADE depth limit reached — aborting to prevent infinite recursion"
                );
                return 1;
            }

            let children = match self.storage.find_children(parent_id, None).await {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!(parent_id = parent_id.as_str(), error = %e, "CASCADE failed to find children");
                    return 1;
                }
            };

            let mut failures = 0usize;

            for child in children {
                let child_machine_def = match self.catalog.get(&child.machine) {
                    Ok(m) => m,
                    Err(e) => {
                        tracing::error!(child_machine = %child.machine, error = %e, "CASCADE: child machine not found");
                        failures += 1;
                        continue;
                    }
                };

                if child_machine_def.terminal_states.contains(&child.state) {
                    continue;
                }

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
                        cascade: false,
                        idempotency_key: None,
                        tags: Vec::new(),
                    };
                    match self.try_transition(&cmd).await {
                        Ok(Some(_)) => {
                            failures += self.cascade_children_with_depth(&child.id, &child.machine, depth + 1).await;
                        }
                        Ok(None) => {
                            tracing::warn!(
                                child_id = child.id.as_str(),
                                child_machine = %child.machine,
                                "CASCADE: child transition denied by guards"
                            );
                            failures += 1;
                        }
                        Err(e) => {
                            tracing::error!(
                                child_id = child.id.as_str(),
                                child_machine = %child.machine,
                                error = %e,
                                "CASCADE: child transition failed"
                            );
                            failures += 1;
                        }
                    }
                } else {
                    tracing::warn!(
                        child_machine = %child.machine,
                        "CASCADE: child machine has no terminal states"
                    );
                    failures += 1;
                }
            }

            failures
        })
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
            recovery_options: vec![
                RecoveryOption {
                    action: RecoveryAction::Escalate,
                    field: None,
                    suggested_value: None,
                    reason: format!("No valid transition path from '{}' to '{}' exists. Escalate to review workflow.", from_state, to_state),
                    example: None,
                },
            ],
            llm_prompt: Some(format!(
                "Cannot transition from '{}' to '{}' in machine '{}': no such transition is defined. Check available transitions or escalate.",
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
            None => {
                // Instance deleted — clean up the orphaned persisted timer
                if let Err(e) = self.storage.remove_timer(instance_id, expected_from_state).await {
                    tracing::warn!(error = %e, "Failed to remove orphaned timer for deleted instance");
                }
                return Ok(None);
            }
        };

        // Race condition: instance already moved to a different state
        if instance.state != expected_from_state {
            // State changed — clean up the stale persisted timer
            if let Err(e) = self.storage.remove_timer(instance_id, expected_from_state).await {
                tracing::warn!(error = %e, "Failed to remove stale timer after state change");
            }
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

        // Atomic transition — if version conflict occurs, still clean up the stale timer
        let transition_result = self
            .storage
            .transition_instance(&id, instance.version, target_state, &mutations, trail_entry)
            .await;

        // Always remove the fired/stale timer from storage, even on version conflict.
        // Without this, a stale StoredTimer would persist and be reloaded on restart.
        if let Err(e) = self.storage.remove_timer(instance_id, expected_from_state).await {
            tracing::warn!(error = %e, "Failed to remove fired timer");
        }

        // Now propagate the transition error (if any)
        let transitioned_instance = transition_result?;

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
                            if let Err(e) = self.storage.store_timer(&stored).await {
                                tracing::warn!(error = %e, "Failed to persist timeout timer");
                            }
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
                                if let Err(e) = self.storage.store_timer(&stored).await {
                                    tracing::warn!(error = %e, "Failed to persist timeout timer");
                                }
                            }
                            break;
                        }
                    }
                }
            }
        }

        // No deferred spawns in timeout path — use returned instance directly
        let updated = transitioned_instance;

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

    // -----------------------------------------------------------------------
    // Instance Claiming
    // -----------------------------------------------------------------------

    /// Execute a CLAIM command: find an unclaimed instance matching the filter
    /// and atomically claim it for the given agent.
    pub async fn execute_claim(&self, cmd: &ClaimCommand) -> SmqlResult<ClaimResult> {
        // Validate machine exists
        let _machine_def = self.catalog.get(&cmd.machine)?;

        let now = Utc::now();
        let expires_at = now + chrono::Duration::seconds(cmd.lease_duration.seconds as i64);

        // Find candidates
        let filter = Filter::default();
        let instances = self.storage.find_instances(&cmd.machine, &filter).await?;

        // Apply expression filter and skip already-claimed
        let mut claimed_instance = None;
        for inst in &instances {
            // Apply WHERE filter if present
            if let Some(ref filter_expr) = cmd.filter {
                let ctx = EvalContext::new(inst.data.clone(), inst.state.clone());
                if !eval_guard(filter_expr, &ctx).unwrap_or(false) {
                    continue;
                }
            }

            // Try to claim this instance
            match self
                .storage
                .claim_instance(&inst.id, &cmd.agent_id, expires_at)
                .await
            {
                Ok(()) => {
                    // Re-fetch to get updated fields
                    let updated = self
                        .storage
                        .get_instance(&inst.id)
                        .await?
                        .ok_or_else(|| SmqlError::not_found("Instance", inst.id.as_str()))?;
                    claimed_instance = Some(updated);
                    break;
                }
                Err(SmqlError::Conflict { .. }) => continue, // already claimed, try next
                Err(e) => return Err(e),
            }
        }

        let mut instance = claimed_instance.ok_or_else(|| SmqlError::NotFound {
            entity_type: "Unclaimed instance".to_string(),
            id: cmd.machine.clone(),
        })?;

        // Optionally transition on claim
        if let Some(ref target_state) = cmd.transition_to {
            let t_cmd = TransitionCommand {
                machine: cmd.machine.clone(),
                instance_id: instance.id.as_str(),
                to_state: target_state.clone(),
                with_data: Vec::new(),
                memo: Some(format!("Claimed by agent '{}'", cmd.agent_id)),
                as_actor: Some(cmd.agent_id.clone()),
                through: Vec::new(),
                or_stay: false,
                cascade: false,
                idempotency_key: None,
                tags: Vec::new(),
            };
            let result = self.transition(&t_cmd).await?;
            instance = result.instance;
        }

        tracing::info!(
            instance_id = %instance.id,
            agent = %cmd.agent_id,
            "instance claimed"
        );

        Ok(ClaimResult {
            instance,
            agent_id: cmd.agent_id.clone(),
            lease_expires_at: expires_at,
        })
    }

    /// Execute a RELEASE command: release a claim held by an agent.
    pub async fn execute_release(&self, cmd: &ReleaseCommand) -> SmqlResult<ReleaseResult> {
        let id = smql_storage::instance::InstanceId::from_string(&cmd.instance_id)
            .map_err(|_| SmqlError::validation(format!("Invalid instance ID: {}", cmd.instance_id)))?;

        self.storage.release_claim(&id, &cmd.agent_id).await?;

        tracing::info!(
            instance_id = %cmd.instance_id,
            agent = %cmd.agent_id,
            "claim released"
        );

        Ok(ReleaseResult {
            instance_id: cmd.instance_id.clone(),
            agent_id: cmd.agent_id.clone(),
        })
    }

    /// Execute a WATCH command — block until a condition becomes true on an instance.
    pub async fn watch(&self, cmd: &WatchCommand) -> SmqlResult<WatchResult> {
        // Validate machine exists
        let _machine_def = self.catalog.get(&cmd.machine)?;

        let start = std::time::Instant::now();

        // Check condition immediately — if already true, return instantly
        if let Some(ref instance_id) = cmd.instance_id {
            let id = smql_storage::instance::InstanceId::from_string(instance_id)
                .map_err(|_| SmqlError::validation(format!("Invalid instance ID: {}", instance_id)))?;
            if let Some(instance) = self.storage.get_instance(&id).await? {
                if self.evaluate_watch_condition(&instance, &cmd.condition) {
                    return Ok(WatchResult {
                        instance,
                        waited_ms: 0,
                    });
                }
            } else {
                return Err(SmqlError::not_found("Instance", instance_id));
            }
        } else {
            // Check all matching instances
            let filter = Filter {
                state: None,
                states: None,
                predicate: None,
                limit: None,
                offset: None,
                after_id: None,
            };
            let instances = self.storage.find_instances(&cmd.machine, &filter).await?;
            for inst in &instances {
                let matches_filter = if let Some(ref f) = cmd.filter {
                    let ctx = EvalContext::new(inst.data.clone(), inst.state.clone());
                    eval_guard(f, &ctx).unwrap_or(false)
                } else {
                    true
                };
                if matches_filter && self.evaluate_watch_condition(inst, &cmd.condition) {
                    return Ok(WatchResult {
                        instance: inst.clone(),
                        waited_ms: 0,
                    });
                }
            }
        }

        // Condition not yet met — register a watcher and wait
        let (tx, rx) = tokio::sync::oneshot::channel();
        let watcher_id = ulid::Ulid::new().to_string();

        {
            let watcher = Watcher {
                id: watcher_id.clone(),
                condition: cmd.condition.clone(),
                instance_id: cmd.instance_id.clone(),
                filter: cmd.filter.clone(),
                sender: tx,
            };
            self.watchers
                .entry(cmd.machine.clone())
                .or_default()
                .push(watcher);
        }

        // Set up timeout
        let timeout_duration = cmd.timeout
            .as_ref()
            .map(|t| std::time::Duration::from_secs(t.seconds))
            .unwrap_or(std::time::Duration::from_secs(300)); // default 5 minute max

        match tokio::time::timeout(timeout_duration, rx).await {
            Ok(Ok(instance)) => {
                let waited = start.elapsed().as_millis() as u64;
                Ok(WatchResult {
                    instance,
                    waited_ms: waited,
                })
            }
            Ok(Err(_)) => {
                // oneshot was dropped (watcher removed) — treat as cancelled
                Err(SmqlError::validation("Watch was cancelled".to_string()))
            }
            Err(_) => {
                // Timeout — remove the watcher
                self.remove_watcher(&cmd.machine, &watcher_id);
                Err(SmqlError::TimeoutError {
                    message: format!(
                        "Watch timed out after {}s",
                        timeout_duration.as_secs()
                    ),
                    instance_id: cmd.instance_id.clone(),
                    state: None,
                })
            }
        }
    }

    /// Evaluate a watch UNTIL condition against an instance.
    fn evaluate_watch_condition(&self, instance: &Instance, condition: &Expression) -> bool {
        let ctx = EvalContext::new(instance.data.clone(), instance.state.clone());
        eval_guard(condition, &ctx).unwrap_or(false)
    }

    /// Notify all watchers for a machine after a state change.
    /// Called after successful spawn or transition.
    pub(crate) fn notify_watchers(&self, machine: &str, instance: &Instance) {
        let mut entry = match self.watchers.get_mut(machine) {
            Some(e) => e,
            None => return,
        };

        let watchers = entry.value_mut();
        let mut i = 0;
        while i < watchers.len() {
            let watcher = &watchers[i];

            // Check if watcher is interested in this instance
            let interested = if let Some(ref watcher_inst_id) = watcher.instance_id {
                instance.id.as_str() == watcher_inst_id.as_str()
            } else if let Some(ref filter) = watcher.filter {
                let ctx = EvalContext::new(instance.data.clone(), instance.state.clone());
                eval_guard(filter, &ctx).unwrap_or(false)
            } else {
                true
            };

            if interested && self.evaluate_watch_condition(instance, &watcher.condition) {
                let watcher = watchers.remove(i);
                let _ = watcher.sender.send(instance.clone());
            } else {
                i += 1;
            }
        }
    }

    /// Remove a watcher by ID (used on timeout/cancel).
    fn remove_watcher(&self, machine: &str, watcher_id: &str) {
        if let Some(mut entry) = self.watchers.get_mut(machine) {
            entry.value_mut().retain(|w| w.id != watcher_id);
        }
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
    // Transaction support
    // -----------------------------------------------------------------------

    /// Execute a transaction: run all statements atomically.
    /// On any failure, rollback all changes made so far.
    pub async fn execute_transaction(
        &self,
        statements: &[smql_ast::command::Statement],
    ) -> SmqlResult<Vec<TransactionStepResult>> {
        use smql_ast::command::{Command, Statement};
        use smql_storage::instance::InstanceId;

        if statements.is_empty() {
            return Ok(Vec::new());
        }

        // Track snapshots for rollback: (instance_id, Option<Instance>) where None means "was newly created"
        let mut snapshots: Vec<(InstanceId, Option<Instance>)> = Vec::new();
        let mut results: Vec<TransactionStepResult> = Vec::new();

        for (i, stmt) in statements.iter().enumerate() {
            let step_result = match stmt {
                Statement::Command(Command::Spawn(spawn_cmd)) => {
                    match self.spawn(spawn_cmd).await {
                        Ok(result) => {
                            // Track: this instance was newly created (rollback = delete)
                            snapshots.push((result.instance.id.clone(), None));
                            Ok(TransactionStepResult::Spawned {
                                instance_id: result.instance.id.as_str(),
                                state: result.instance.state.clone(),
                            })
                        }
                        Err(e) => Err(e),
                    }
                }
                Statement::Command(Command::Transition(t_cmd)) => {
                    let id_result = InstanceId::from_string(&t_cmd.instance_id)
                        .map_err(|_| SmqlError::validation(format!("Invalid instance ID: {}", t_cmd.instance_id)));
                    match id_result {
                        Err(e) => Err(e),
                        Ok(id) => {
                            let snapshot_result = self.storage.get_instance(&id).await;
                            match snapshot_result {
                                Err(e) => Err(e),
                                Ok(None) => Err(SmqlError::not_found("Instance", &t_cmd.instance_id)),
                                Ok(Some(snapshot)) => match self.transition(t_cmd).await {
                                    Ok(result) => {
                                        snapshots.push((id, Some(snapshot)));
                                        Ok(TransactionStepResult::Transitioned {
                                            instance_id: result.instance.id.as_str(),
                                            from_state: result.from_state,
                                            to_state: result.to_state,
                                        })
                                    }
                                    Err(e) => Err(e),
                                },
                            }
                        }
                    }
                }
                Statement::Command(Command::TryTransition(t_cmd)) => {
                    let id_result = InstanceId::from_string(&t_cmd.instance_id)
                        .map_err(|_| SmqlError::validation(format!("Invalid instance ID: {}", t_cmd.instance_id)));
                    match id_result {
                        Err(e) => Err(e),
                        Ok(id) => {
                            let snapshot_result = self.storage.get_instance(&id).await;
                            match snapshot_result {
                                Err(e) => Err(e),
                                Ok(None) => Err(SmqlError::not_found("Instance", &t_cmd.instance_id)),
                                Ok(Some(snapshot)) => match self.try_transition(t_cmd).await {
                                    Ok(Some(result)) => {
                                        snapshots.push((id, Some(snapshot)));
                                        Ok(TransactionStepResult::Transitioned {
                                            instance_id: result.instance.id.as_str(),
                                            from_state: result.from_state,
                                            to_state: result.to_state,
                                        })
                                    }
                                    Ok(None) => Ok(TransactionStepResult::Skipped),
                                    Err(e) => Err(e),
                                },
                            }
                        }
                    }
                }
                _ => Err(SmqlError::validation(
                    "Only SPAWN, TRANSITION, and TRY TRANSITION are allowed inside BEGIN...COMMIT".to_string(),
                )),
            };

            match step_result {
                Ok(sr) => results.push(sr),
                Err(e) => {
                    tracing::warn!(step = i, error = %e, "Transaction step failed — rolling back");
                    let mut rollback_errors = Vec::new();

                    for (id, snapshot) in snapshots.into_iter().rev() {
                        match snapshot {
                            None => {
                                if let Err(del_err) = self.storage.delete_instance(&id).await {
                                    tracing::error!(
                                        instance_id = id.as_str(),
                                        error = %del_err,
                                        "Transaction rollback: failed to delete spawned instance"
                                    );
                                    rollback_errors.push(format!("delete {}: {}", id.as_str(), del_err));
                                }
                            }
                            Some(old_instance) => {
                                if let Err(del_err) = self.storage.delete_instance(&id).await {
                                    tracing::error!(
                                        instance_id = id.as_str(),
                                        error = %del_err,
                                        "Transaction rollback: failed to delete modified instance before restore"
                                    );
                                    rollback_errors.push(format!("delete {}: {}", id.as_str(), del_err));
                                    continue;
                                }
                                if let Err(restore_err) = self.storage.store_instance(&old_instance).await {
                                    tracing::error!(
                                        instance_id = id.as_str(),
                                        error = %restore_err,
                                        "Transaction rollback: CRITICAL — deleted instance but failed to restore snapshot. Instance data may be lost."
                                    );
                                    rollback_errors.push(format!("restore {}: {}", id.as_str(), restore_err));
                                }
                            }
                        }
                    }

                    if !rollback_errors.is_empty() {
                        tracing::error!(
                            errors = rollback_errors.join("; "),
                            "Transaction rollback completed with errors — manual intervention may be required"
                        );
                    }

                    return Err(SmqlError::TransactionFailed {
                        message: format!("Transaction failed at step {}: {}", i, e),
                        step: i,
                        original_error: Box::new(e),
                    });
                }
            }
        }

        Ok(results)
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
                idempotency_key: None,
                tags: Vec::new(),
            };

            match self.transition_inner(&cmd, None).await {
                Ok(_) => {
                    tracing::info!(saga = saga_name, step = %step.name, "SAGA step completed");
                    completed_steps.push(i);
                }
                Err(e) => {
                    tracing::warn!(saga = saga_name, step = %step.name, error = %e, "SAGA step failed — compensating");

                    let mut compensation_failures = Vec::new();

                    for &ci in completed_steps.iter().rev() {
                        if let Some(comp) = &saga.steps[ci].compensate {
                            let comp_ctx = EvalContext::new(std::collections::HashMap::new(), String::new());
                            match eval_expr(&comp.instance_expr, &comp_ctx) {
                                Ok(comp_id_val) => {
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
                                        idempotency_key: None,
                                        tags: Vec::new(),
                                    };
                                    match self.transition_inner(&comp_cmd, None).await {
                                        Ok(_) => {
                                            tracing::info!(
                                                saga = saga_name,
                                                step = %saga.steps[ci].name,
                                                "SAGA compensation step succeeded"
                                            );
                                        }
                                        Err(comp_err) => {
                                            tracing::error!(
                                                saga = saga_name,
                                                step = %saga.steps[ci].name,
                                                error = %comp_err,
                                                "SAGA compensation FAILED — manual intervention required"
                                            );
                                            compensation_failures.push(format!(
                                                "step '{}': {}",
                                                saga.steps[ci].name, comp_err
                                            ));
                                        }
                                    }
                                }
                                Err(eval_err) => {
                                    tracing::error!(
                                        saga = saga_name,
                                        step = %saga.steps[ci].name,
                                        error = %eval_err,
                                        "SAGA compensation instance_expr evaluation failed"
                                    );
                                    compensation_failures.push(format!(
                                        "step '{}' (eval): {}",
                                        saga.steps[ci].name, eval_err
                                    ));
                                }
                            }
                        }
                    }

                    if compensation_failures.is_empty() {
                        return Err(format!(
                            "SAGA '{}' failed at step '{}': {} (all compensations succeeded)",
                            saga_name, step.name, e
                        ));
                    } else {
                        return Err(format!(
                            "SAGA '{}' failed at step '{}': {} — COMPENSATION FAILURES: {}",
                            saga_name,
                            step.name,
                            e,
                            compensation_failures.join("; ")
                        ));
                    }
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
                                idempotency_key: None,
                                tags: Vec::new(),
                            };
                            if let Ok(result) = self.transition_inner(&cmd, None).await {
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
    /// Called after spawn data validation, after each transition mutation, and in query read paths.
    pub(crate) fn evaluate_computed_fields(
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
            Action::Webhook {
                url,
                payload,
                response_field,
                on_failure_state,
            } => {
                let resolved_payload = payload
                    .as_ref()
                    .map(|expr| eval_expr(expr, ctx))
                    .transpose()?;
                Ok(ResolvedAction::Webhook {
                    url: url.clone(),
                    payload: resolved_payload,
                    response_field: response_field.clone(),
                    on_failure_state: on_failure_state.clone(),
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
            idempotency_key: None,
            tags: Vec::new(),
            ttl: None,
        };

        // Create a temporary engine to perform the spawn
        let engine = Engine {
            catalog: self.catalog.clone(),
            storage: self.storage.clone(),
            timer_manager: self.timer_manager.clone(),
            hook_executor: self.hook_executor.clone(),
            watchers: DashMap::new(),
        };
        let result = engine
            .spawn(&cmd)
            .await
            .map_err(|e| HookError::ActionFailed {
                message: format!("Failed to spawn child: {}", e),
            })?;
        Ok(result.instance.id.as_str())
    }

    async fn update_instance_field(
        &self,
        instance_id: &str,
        field: &str,
        value: Value,
    ) -> Result<(), HookError> {
        let id = smql_storage::InstanceId::from_string(instance_id).map_err(|_| {
            HookError::ActionFailed {
                message: format!("Invalid instance ID: {}", instance_id),
            }
        })?;
        let mut instance = self
            .storage
            .get_instance(&id)
            .await
            .map_err(|e| HookError::ActionFailed {
                message: format!("Failed to get instance: {}", e),
            })?
            .ok_or_else(|| HookError::ActionFailed {
                message: format!("Instance not found: {}", instance_id),
            })?;

        instance.data.insert(field.to_string(), value);
        self.storage
            .delete_instance(&id)
            .await
            .map_err(|e| HookError::ActionFailed {
                message: format!("Failed to delete instance for update: {}", e),
            })?;
        self.storage
            .store_instance(&instance)
            .await
            .map_err(|e| HookError::ActionFailed {
                message: format!("Failed to store updated instance: {}", e),
            })?;

        Ok(())
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
            watchers: DashMap::new(),
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
    pub(crate) async fn populate_composition_context(
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

    /// Generate actionable recovery options from guard failures for AI agents.
    /// Generate recovery options by walking the guard AST for precise analysis.
    /// Falls back to string-based heuristics when no AST is available.
    pub(crate) fn generate_recovery_options_from_ast(
        failed_exprs: &[smql_ast::Expression],
        guard_failures: &[GuardFailure],
        instance_id: &str,
        machine: &str,
        target_state: &str,
    ) -> Vec<RecoveryOption> {
        let mut options = Vec::new();

        for expr in failed_exprs {
            Self::collect_recovery_from_expr(&expr.kind, instance_id, machine, target_state, &mut options);
        }

        // Deduplicate by (action, field) pair
        options.dedup_by(|a, b| a.action == b.action && a.field == b.field);

        // If AST analysis produced nothing, fall back to string-based heuristics
        if options.is_empty() {
            return Self::generate_recovery_options(guard_failures, instance_id, machine, target_state);
        }

        options
    }

    /// Recursively walk an expression AST and collect recovery options.
    fn collect_recovery_from_expr(
        kind: &smql_ast::expression::ExpressionKind,
        instance_id: &str,
        machine: &str,
        target_state: &str,
        options: &mut Vec<RecoveryOption>,
    ) {
        use smql_ast::expression::{BinaryOperator, ExpressionKind::*};

        match kind {
            // field IS SET → SetField
            IsSet(inner) => {
                if let FieldAccess(parts) = &inner.kind {
                    let field = parts.first().cloned().unwrap_or_default();
                    options.push(RecoveryOption {
                        action: RecoveryAction::SetField,
                        field: Some(field.clone()),
                        suggested_value: Some("Provide a value for this field".to_string()),
                        reason: format!("Field '{}' must be set.", field),
                        example: Some(format!(
                            "TRANSITION {} {} TO {} WITH {{ {}: \"...\" }}",
                            machine, instance_id, target_state, field
                        )),
                    });
                }
            }

            // field IS NOT SET → inform that field must be absent
            IsNotSet(inner) => {
                if let FieldAccess(parts) = &inner.kind {
                    let field = parts.first().cloned().unwrap_or_default();
                    options.push(RecoveryOption {
                        action: RecoveryAction::SetField,
                        field: Some(field.clone()),
                        suggested_value: Some("Remove or clear this field".to_string()),
                        reason: format!("Field '{}' must not be set.", field),
                        example: None,
                    });
                }
            }

            // Binary comparisons: field > N, field == value, ACTOR.role == "admin", etc.
            BinaryOp { left, op, right } => {
                match op {
                    // Logical operators: recurse into both sides
                    BinaryOperator::And => {
                        Self::collect_recovery_from_expr(&left.kind, instance_id, machine, target_state, options);
                        Self::collect_recovery_from_expr(&right.kind, instance_id, machine, target_state, options);
                    }
                    BinaryOperator::Or => {
                        // For OR, collect from both sides (agent can satisfy either)
                        Self::collect_recovery_from_expr(&left.kind, instance_id, machine, target_state, options);
                        Self::collect_recovery_from_expr(&right.kind, instance_id, machine, target_state, options);
                    }

                    // Comparison: field == value, field > N, ACTOR == assignee, etc.
                    BinaryOperator::Eq | BinaryOperator::NotEq
                    | BinaryOperator::Gt | BinaryOperator::GtEq
                    | BinaryOperator::Lt | BinaryOperator::LtEq => {
                        // Check for ACTOR.role == "value" pattern
                        if let Some(role) = Self::extract_actor_role_comparison(left, right) {
                            options.push(RecoveryOption {
                                action: RecoveryAction::ChangeActor,
                                field: None,
                                suggested_value: Some(role.clone()),
                                reason: format!("Actor role must be '{}'.", role),
                                example: Some(format!(
                                    "TRANSITION {} {} TO {} AS \"{}\"",
                                    machine, instance_id, target_state, role
                                )),
                            });
                            return;
                        }

                        // Check for ACTOR == value pattern (any ACTOR reference)
                        if Self::expr_references_actor(&left.kind) || Self::expr_references_actor(&right.kind) {
                            let suggested = Self::extract_literal_str(right)
                                .or_else(|| Self::extract_literal_str(left));
                            options.push(RecoveryOption {
                                action: RecoveryAction::ChangeActor,
                                field: None,
                                suggested_value: suggested.map(|s| s.to_string()),
                                reason: "The current actor does not satisfy this guard.".to_string(),
                                example: Some(format!(
                                    "TRANSITION {} {} TO {} AS \"appropriate_actor\"",
                                    machine, instance_id, target_state
                                )),
                            });
                            return;
                        }

                        // Field comparison: field > N, field == value
                        if let FieldAccess(parts) = &left.kind {
                            let field = parts.first().cloned().unwrap_or_default();
                            let hint = match op {
                                BinaryOperator::Eq => {
                                    if let Some(v) = Self::extract_literal_str(right) {
                                        format!("must equal '{}'", v)
                                    } else {
                                        format!("must equal {}", right)
                                    }
                                }
                                BinaryOperator::Gt => format!("must be > {}", right),
                                BinaryOperator::GtEq => format!("must be >= {}", right),
                                BinaryOperator::Lt => format!("must be < {}", right),
                                BinaryOperator::LtEq => format!("must be <= {}", right),
                                BinaryOperator::NotEq => format!("must not equal {}", right),
                                _ => format!("must satisfy: {} {} {}", left, op, right),
                            };
                            options.push(RecoveryOption {
                                action: RecoveryAction::SetField,
                                field: Some(field.clone()),
                                suggested_value: Some(hint),
                                reason: format!("Field '{}' does not satisfy guard.", field),
                                example: Some(format!(
                                    "TRANSITION {} {} TO {} WITH {{ {}: ... }}",
                                    machine, instance_id, target_state, field
                                )),
                            });
                        }
                        if let FieldAccess(parts) = &right.kind {
                            let field = parts.first().cloned().unwrap_or_default();
                            // Only add if left is not also a field (avoid duplicate)
                            if !matches!(&left.kind, FieldAccess(_)) {
                                options.push(RecoveryOption {
                                    action: RecoveryAction::SetField,
                                    field: Some(field.clone()),
                                    suggested_value: Some(format!("must satisfy: {} {} {}", left, op, field)),
                                    reason: format!("Field '{}' does not satisfy guard.", field),
                                    example: Some(format!(
                                        "TRANSITION {} {} TO {} WITH {{ {}: ... }}",
                                        machine, instance_id, target_state, field
                                    )),
                                });
                            }
                        }
                    }

                    // Arithmetic operators: don't generate recovery for intermediate math
                    _ => {}
                }
            }

            // STATE IS state_name → Retry
            StateIs(state) => {
                options.push(RecoveryOption {
                    action: RecoveryAction::Retry,
                    field: None,
                    suggested_value: Some(state.clone()),
                    reason: format!("Instance must be in state '{}'.", state),
                    example: None,
                });
            }

            // STATE IN { states } → Retry
            StateIn(states) => {
                options.push(RecoveryOption {
                    action: RecoveryAction::Retry,
                    field: None,
                    suggested_value: Some(states.join(", ")),
                    reason: format!("Instance must be in one of states: [{}].", states.join(", ")),
                    example: None,
                });
            }

            // field IN { values } → SetField
            InSet { expr, values } | InList { expr, values } => {
                if let FieldAccess(parts) = &expr.kind {
                    let field = parts.first().cloned().unwrap_or_default();
                    let vals: Vec<String> = values.iter().map(|v| v.to_string()).collect();
                    options.push(RecoveryOption {
                        action: RecoveryAction::SetField,
                        field: Some(field.clone()),
                        suggested_value: Some(format!("must be one of [{}]", vals.join(", "))),
                        reason: format!("Field '{}' must be one of the allowed values.", field),
                        example: Some(format!(
                            "TRANSITION {} {} TO {} WITH {{ {}: ... }}",
                            machine, instance_id, target_state, field
                        )),
                    });
                }
                if Self::expr_references_actor(&expr.kind) {
                    let vals: Vec<String> = values.iter().map(|v| v.to_string()).collect();
                    options.push(RecoveryOption {
                        action: RecoveryAction::ChangeActor,
                        field: None,
                        suggested_value: Some(format!("must be one of [{}]", vals.join(", "))),
                        reason: "Actor must be one of the allowed values.".to_string(),
                        example: Some(format!(
                            "TRANSITION {} {} TO {} AS \"...\"",
                            machine, instance_id, target_state
                        )),
                    });
                }
            }

            // expr IN collection (dynamic membership, e.g. "approve" IN ACTOR.capabilities) → ChangeActor
            InCollection { expr, collection } => {
                if Self::expr_references_actor(&collection.kind) {
                    let hint = Self::extract_literal_str(expr)
                        .map(|s| s.to_string());
                    options.push(RecoveryOption {
                        action: RecoveryAction::ChangeActor,
                        field: None,
                        suggested_value: hint,
                        reason: format!("Actor's collection does not contain required value: {}.", expr),
                        example: Some(format!(
                            "TRANSITION {} {} TO {} AS \"actor_with_capability\"",
                            machine, instance_id, target_state
                        )),
                    });
                } else if let FieldAccess(parts) = &collection.kind {
                    let field = parts.first().cloned().unwrap_or_default();
                    options.push(RecoveryOption {
                        action: RecoveryAction::SetField,
                        field: Some(field.clone()),
                        suggested_value: Some(format!("must contain {}", expr)),
                        reason: format!("Collection field '{}' must contain the required value.", field),
                        example: Some(format!(
                            "TRANSITION {} {} TO {} WITH {{ {}: ... }}",
                            machine, instance_id, target_state, field
                        )),
                    });
                }
            }

            // ALL/ANY child guards → Escalate (agent can't easily fix child state)
            All { collection: _, predicate } => {
                options.push(RecoveryOption {
                    action: RecoveryAction::Escalate,
                    field: None,
                    suggested_value: None,
                    reason: format!("All children must satisfy: {}. Transition children first.", predicate),
                    example: None,
                });
            }
            Any { collection: _, predicate } => {
                options.push(RecoveryOption {
                    action: RecoveryAction::Escalate,
                    field: None,
                    suggested_value: None,
                    reason: format!("At least one child must satisfy: {}. Transition a child first.", predicate),
                    example: None,
                });
            }

            // Time-based functions → Wait
            FunctionCall { name, args: _ } => {
                let lower = name.to_lowercase();
                if lower.contains("elapsed") || lower.contains("timeout") || lower == "now" || lower == "today" {
                    options.push(RecoveryOption {
                        action: RecoveryAction::Wait,
                        field: None,
                        suggested_value: None,
                        reason: "A time-based condition is not yet met.".to_string(),
                        example: None,
                    });
                }
            }

            // NOT expr → recurse into the negated expression
            UnaryOp { op: _, operand } => {
                Self::collect_recovery_from_expr(&operand.kind, instance_id, machine, target_state, options);
            }

            // SIGNAL FROM → Escalate (external dependency)
            SignalFrom { machine: child_machine, condition } => {
                options.push(RecoveryOption {
                    action: RecoveryAction::Escalate,
                    field: None,
                    suggested_value: None,
                    reason: format!("Waiting for signal from '{}' where {}.", child_machine, condition),
                    example: None,
                });
            }

            // Qualified access like ACTOR.role → ChangeActor
            QualifiedAccess { root, path: _ } => {
                if Self::expr_references_actor(&root.kind) {
                    options.push(RecoveryOption {
                        action: RecoveryAction::ChangeActor,
                        field: None,
                        suggested_value: None,
                        reason: "Actor does not satisfy this guard.".to_string(),
                        example: Some(format!(
                            "TRANSITION {} {} TO {} AS \"appropriate_actor\"",
                            machine, instance_id, target_state
                        )),
                    });
                }
            }

            // Literals, SelfRef, Pattern, etc. — no actionable recovery
            _ => {}
        }
    }

    /// Check if an expression references the ACTOR.
    fn expr_references_actor(kind: &smql_ast::expression::ExpressionKind) -> bool {
        use smql_ast::expression::ExpressionKind::*;
        match kind {
            ActorRef => true,
            QualifiedAccess { root, .. } => Self::expr_references_actor(&root.kind),
            _ => false,
        }
    }

    /// Extract ACTOR.role == "value" pattern: returns the expected role.
    fn extract_actor_role_comparison(
        left: &smql_ast::Expression,
        right: &smql_ast::Expression,
    ) -> Option<String> {
        use smql_ast::expression::ExpressionKind::*;
        // Check left = ACTOR.role, right = literal
        if let QualifiedAccess { root, path } = &left.kind {
            if matches!(root.kind, ActorRef) && path.len() == 1 && path[0].eq_ignore_ascii_case("role") {
                if let Some(val) = Self::extract_literal_str(right) {
                    return Some(val.to_string());
                }
                return Some("*".to_string());
            }
        }
        // Check right = ACTOR.role, left = literal
        if let QualifiedAccess { root, path } = &right.kind {
            if matches!(root.kind, ActorRef) && path.len() == 1 && path[0].eq_ignore_ascii_case("role") {
                if let Some(val) = Self::extract_literal_str(left) {
                    return Some(val.to_string());
                }
                return Some("*".to_string());
            }
        }
        None
    }

    /// Extract a string literal from an expression (if it is one).
    fn extract_literal_str(expr: &smql_ast::Expression) -> Option<&str> {
        use smql_ast::expression::ExpressionKind::*;
        match &expr.kind {
            Literal(Value::Text(s)) => Some(s.as_str()),
            _ => None,
        }
    }

    /// Fallback: string-based recovery option generation (used when no AST is available).
    pub(crate) fn generate_recovery_options(
        guard_failures: &[GuardFailure],
        instance_id: &str,
        machine: &str,
        target_state: &str,
    ) -> Vec<RecoveryOption> {
        let mut options = Vec::new();

        for failure in guard_failures {
            if failure.guard_expr.contains("IS SET") {
                let field = failure.guard_expr.split_whitespace().next().map(|s| s.to_string());
                options.push(RecoveryOption {
                    action: RecoveryAction::SetField,
                    field: field.clone(),
                    suggested_value: Some("Provide a value for this field".to_string()),
                    reason: format!("Guard '{}' requires this field to be set.", failure.guard_expr),
                    example: field.map(|f| format!("TRANSITION {} {} TO {} WITH {{ {}: \"...\" }}", machine, instance_id, target_state, f)),
                });
            }

            if failure.guard_expr.contains("ACTOR") {
                options.push(RecoveryOption {
                    action: RecoveryAction::ChangeActor,
                    field: None,
                    suggested_value: None,
                    reason: "The current actor does not have permission for this transition.".to_string(),
                    example: Some(format!("AS \"appropriate_actor\" TRANSITION {} {} TO {}", machine, instance_id, target_state)),
                });
            }

            if failure.guard_expr.contains("elapsed") {
                options.push(RecoveryOption {
                    action: RecoveryAction::Wait,
                    field: None,
                    suggested_value: None,
                    reason: "A time-based condition is not yet met. Wait for the condition to become true.".to_string(),
                    example: None,
                });
            }
        }

        if options.is_empty() {
            options.push(RecoveryOption {
                action: RecoveryAction::Escalate,
                field: None,
                suggested_value: None,
                reason: "Unable to determine specific recovery action. Escalate for manual review.".to_string(),
                example: Some(format!("TRANSITION {} {} TO awaiting_agent", machine, instance_id)),
            });
        }

        options
    }

    /// Generate an LLM-friendly prompt describing the transition failure.
    fn generate_llm_prompt(
        guard_failures: &[GuardFailure],
        instance_id: &str,
        from_state: &str,
        to_state: &str,
    ) -> Option<String> {
        if guard_failures.is_empty() {
            return None;
        }

        let failures_summary: Vec<String> = guard_failures
            .iter()
            .map(|f| {
                let mut s = format!("Guard '{}' failed", f.guard_expr);
                if let Some(actual) = &f.actual_value {
                    s.push_str(&format!(" (actual: {})", actual));
                }
                if let Some(hint) = &f.hint {
                    s.push_str(&format!(". Hint: {}", hint));
                }
                s
            })
            .collect();

        Some(format!(
            "Transition {} -> {} for instance {} was denied. Failures: {}. Review the guard conditions and provide missing data or escalate to a human agent.",
            from_state, to_state, instance_id, failures_summary.join("; ")
        ))
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
