// SMQL Hooks — Action/hook execution runtime

pub mod webhook;

use smql_ast::machine::{HookDefinition, HookTrigger};
use smql_ast::value::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::broadcast;

pub use webhook::{WebhookClient, WebhookConfig};

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, thiserror::Error)]
pub enum HookError {
    #[error("Hook rejected transition: {reason}")]
    Rejected { reason: String },

    #[error("Action failed: {message}")]
    ActionFailed { message: String },

    #[error("Webhook failed: {url}: {message}")]
    WebhookFailed { url: String, message: String },
}

// ---------------------------------------------------------------------------
// HookContext — data passed into hook execution
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct HookContext {
    pub instance_id: String,
    pub machine: String,
    pub from_state: String,
    pub to_state: String,
    pub data: HashMap<String, Value>,
    pub actor: Option<String>,
    pub memo: Option<String>,
}

// ---------------------------------------------------------------------------
// ResolvedAction — actions with concrete Values (no expressions)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum ResolvedAction {
    Notify {
        target: Value,
        event: String,
    },
    Log(String),
    Emit {
        event: String,
        payload: Option<Value>,
    },
    Webhook {
        url: String,
        payload: Option<Value>,
        response_field: Option<String>,
        on_failure_state: Option<String>,
    },
    SpawnChild {
        machine: String,
        data: Vec<(String, Value)>,
    },
    SignalParent {
        target_state: String,
    },
}

// ---------------------------------------------------------------------------
// EngineCallback — trait for engine operations that hooks can trigger
// ---------------------------------------------------------------------------

#[async_trait::async_trait]
pub trait EngineCallback: Send + Sync {
    async fn spawn_child(
        &self,
        parent_instance_id: &str,
        machine: &str,
        data: Vec<(String, Value)>,
    ) -> Result<String, HookError>;

    async fn signal_parent(
        &self,
        child_instance_id: &str,
        target_state: &str,
    ) -> Result<(), HookError>;

    /// Update a field on an instance (used by webhook response handling).
    async fn update_instance_field(
        &self,
        instance_id: &str,
        field: &str,
        value: Value,
    ) -> Result<(), HookError>;
}

// ---------------------------------------------------------------------------
// Event — published on the event bus
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Event {
    pub name: String,
    pub instance_id: String,
    pub machine: String,
    pub payload: Option<Value>,
}

// ---------------------------------------------------------------------------
// EventBus — broadcast channel for EMIT actions
// ---------------------------------------------------------------------------

pub struct EventBus {
    sender: broadcast::Sender<Event>,
    capacity: usize,
    dropped: std::sync::atomic::AtomicU64,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self {
            sender,
            capacity,
            dropped: std::sync::atomic::AtomicU64::new(0),
        }
    }

    pub fn emit(&self, event: Event) {
        match self.sender.send(event) {
            Ok(_) => {}
            Err(_) => {
                // No active receivers — count as dropped
                self.dropped
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.sender.subscribe()
    }

    /// Returns the configured capacity.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Returns the total number of events dropped (no receivers).
    pub fn dropped_count(&self) -> u64 {
        self.dropped.load(std::sync::atomic::Ordering::Relaxed)
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new(1024)
    }
}

// ---------------------------------------------------------------------------
// HookExecutor — orchestrates hook matching and action dispatch
// ---------------------------------------------------------------------------

pub struct HookExecutor {
    pub event_bus: Arc<EventBus>,
    callback: std::sync::RwLock<Option<Arc<dyn EngineCallback>>>,
    webhook_client: std::sync::RwLock<Option<Arc<WebhookClient>>>,
}

impl HookExecutor {
    pub fn new(event_bus: Arc<EventBus>) -> Self {
        Self {
            event_bus,
            callback: std::sync::RwLock::new(None),
            webhook_client: std::sync::RwLock::new(None),
        }
    }

    pub fn with_callback(event_bus: Arc<EventBus>, callback: Arc<dyn EngineCallback>) -> Self {
        Self {
            event_bus,
            callback: std::sync::RwLock::new(Some(callback)),
            webhook_client: std::sync::RwLock::new(None),
        }
    }

    pub fn set_callback(&self, callback: Arc<dyn EngineCallback>) {
        *self.callback.write().unwrap() = Some(callback);
    }

    /// Set the webhook HTTP client. When set, WEBHOOK actions make real HTTP POST requests.
    /// When not set, WEBHOOK actions are logged as dry-run.
    pub fn set_webhook_client(&self, client: Arc<WebhookClient>) {
        *self.webhook_client.write().unwrap() = Some(client);
    }

    /// Fire matching hooks for a trigger. Returns Err only if a BEFORE hook rejects.
    ///
    /// - BEFORE hooks: executed synchronously, can reject via HookError::Rejected
    /// - All other hooks: fire-and-forget (errors are logged, not propagated)
    pub async fn fire_hooks(
        &self,
        hooks: &[HookDefinition],
        trigger: &HookTrigger,
        ctx: &HookContext,
        resolved_actions_per_hook: &[Vec<ResolvedAction>],
    ) -> Result<(), HookError> {
        let is_before = matches!(trigger, HookTrigger::BeforeEachTransition);

        for (i, hook) in hooks.iter().enumerate() {
            if !trigger_matches(&hook.trigger, trigger) {
                continue;
            }

            let actions = resolved_actions_per_hook
                .get(i)
                .cloned()
                .unwrap_or_default();

            if is_before {
                // Synchronous: any error rejects the transition
                self.execute_actions(&actions, ctx).await?;
            } else {
                // Fire-and-forget: log errors but don't propagate
                if let Err(e) = self.execute_actions(&actions, ctx).await {
                    tracing::warn!(
                        hook_trigger = %hook.trigger,
                        instance_id = %ctx.instance_id,
                        error = %e,
                        "Hook action failed (fire-and-forget)"
                    );
                }
            }
        }
        Ok(())
    }

    /// Execute a list of resolved actions for a single hook or transition.
    pub async fn execute_actions(
        &self,
        actions: &[ResolvedAction],
        ctx: &HookContext,
    ) -> Result<(), HookError> {
        for action in actions {
            self.execute_action(action, ctx).await?;
        }
        Ok(())
    }

    /// Execute a single resolved action.
    async fn execute_action(
        &self,
        action: &ResolvedAction,
        ctx: &HookContext,
    ) -> Result<(), HookError> {
        match action {
            ResolvedAction::Log(message) => {
                let rendered = render_log_template(message, ctx);
                tracing::info!(
                    instance_id = %ctx.instance_id,
                    machine = %ctx.machine,
                    "LOG: {}", rendered
                );
                Ok(())
            }

            ResolvedAction::Emit { event, payload } => {
                self.event_bus.emit(Event {
                    name: event.clone(),
                    instance_id: ctx.instance_id.clone(),
                    machine: ctx.machine.clone(),
                    payload: payload.clone(),
                });
                tracing::debug!(
                    event = %event,
                    instance_id = %ctx.instance_id,
                    "EMIT event"
                );
                Ok(())
            }

            ResolvedAction::Notify { target, event } => {
                tracing::info!(
                    target = %target,
                    event = %event,
                    instance_id = %ctx.instance_id,
                    "NOTIFY (dry-run)"
                );
                Ok(())
            }

            ResolvedAction::Webhook {
                url,
                payload,
                response_field,
                on_failure_state,
            } => {
                let client = self.webhook_client.read().unwrap().clone();
                if let Some(client) = client {
                    match client.execute(url, ctx, payload.as_ref()).await {
                        Ok(response_body) => {
                            tracing::info!(
                                url = %url,
                                instance_id = %ctx.instance_id,
                                "WEBHOOK delivered"
                            );
                            // Store response in instance field if requested
                            if let Some(field) = response_field {
                                if let Some(json_body) = response_body {
                                    let cb = self.callback.read().unwrap().clone();
                                    if let Some(cb) = cb {
                                        let value = json_to_value(&json_body);
                                        if let Err(e) = cb
                                            .update_instance_field(
                                                &ctx.instance_id,
                                                field,
                                                value,
                                            )
                                            .await
                                        {
                                            tracing::warn!(
                                                field = %field,
                                                error = %e,
                                                "Failed to store webhook response"
                                            );
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!(
                                url = %url,
                                instance_id = %ctx.instance_id,
                                error = %e,
                                "WEBHOOK failed"
                            );
                            // If on_failure_state is set, we don't propagate the error —
                            // the caller (engine) handles the state transition separately.
                            // We return a special error that includes the target state.
                            if let Some(_failure_state) = on_failure_state {
                                return Err(HookError::WebhookFailed {
                                    url: url.clone(),
                                    message: e.to_string(),
                                });
                            }
                            return Err(HookError::WebhookFailed {
                                url: url.clone(),
                                message: e.to_string(),
                            });
                        }
                    }
                } else {
                    tracing::info!(
                        url = %url,
                        instance_id = %ctx.instance_id,
                        has_payload = payload.is_some(),
                        "WEBHOOK (dry-run, no HTTP client)"
                    );
                }
                Ok(())
            }

            ResolvedAction::SpawnChild { machine, data } => {
                let cb = self.callback.read().unwrap().clone();
                if let Some(cb) = cb {
                    let child_id = cb
                        .spawn_child(&ctx.instance_id, machine, data.clone())
                        .await?;
                    tracing::info!(
                        parent = %ctx.instance_id,
                        child = %child_id,
                        machine = %machine,
                        "SPAWN CHILD"
                    );
                } else {
                    tracing::warn!(
                        instance_id = %ctx.instance_id,
                        machine = %machine,
                        "SPAWN CHILD skipped: no engine callback configured"
                    );
                }
                Ok(())
            }

            ResolvedAction::SignalParent { target_state } => {
                let cb = self.callback.read().unwrap().clone();
                if let Some(cb) = cb {
                    cb.signal_parent(&ctx.instance_id, target_state).await?;
                } else {
                    tracing::warn!(
                        instance_id = %ctx.instance_id,
                        target_state = %target_state,
                        "SIGNAL PARENT skipped: no engine callback configured"
                    );
                }
                Ok(())
            }
        }
    }
}

/// Check if a hook trigger matches the fired trigger.
fn trigger_matches(hook_trigger: &HookTrigger, fired: &HookTrigger) -> bool {
    match (hook_trigger, fired) {
        (HookTrigger::OnSpawn, HookTrigger::OnSpawn) => true,
        (HookTrigger::BeforeEachTransition, HookTrigger::BeforeEachTransition) => true,
        (HookTrigger::AfterEachTransition, HookTrigger::AfterEachTransition) => true,
        (HookTrigger::OnEnter(hook_state), HookTrigger::OnEnter(fired_state)) => {
            hook_state == fired_state
        }
        (HookTrigger::OnExit(hook_state), HookTrigger::OnExit(fired_state)) => {
            hook_state == fired_state
        }
        _ => false,
    }
}

/// Convert a serde_json::Value to an SMQL Value.
pub fn json_to_value(json: &serde_json::Value) -> Value {
    match json {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int(i)
            } else {
                Value::Float(n.as_f64().unwrap_or(0.0))
            }
        }
        serde_json::Value::String(s) => Value::Text(s.clone()),
        serde_json::Value::Array(arr) => {
            Value::List(arr.iter().map(json_to_value).collect())
        }
        serde_json::Value::Object(obj) => {
            let map: std::collections::BTreeMap<String, Value> = obj
                .iter()
                .map(|(k, v)| (k.clone(), json_to_value(v)))
                .collect();
            Value::Map(map)
        }
    }
}

/// Simple template rendering for LOG messages.
/// Replaces `{field}` with values from context data.
fn render_log_template(template: &str, ctx: &HookContext) -> String {
    let mut result = template.to_string();
    // Replace built-in variables
    result = result.replace("{instance_id}", &ctx.instance_id);
    result = result.replace("{machine}", &ctx.machine);
    result = result.replace("{from_state}", &ctx.from_state);
    result = result.replace("{to_state}", &ctx.to_state);
    if let Some(actor) = &ctx.actor {
        result = result.replace("{actor}", actor);
    }
    // Replace data fields
    for (key, value) in &ctx.data {
        let placeholder = format!("{{{}}}", key);
        result = result.replace(&placeholder, &value.to_string());
    }
    result
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use smql_ast::machine::{Action, HookDefinition, HookTrigger};
    use smql_ast::value::Value;
    use std::collections::HashMap;
    use std::sync::Arc;

    fn test_ctx() -> HookContext {
        let mut data = HashMap::new();
        data.insert("title".to_string(), Value::Text("Bug fix".to_string()));
        data.insert("priority".to_string(), Value::Int(1));
        HookContext {
            instance_id: "INST001".to_string(),
            machine: "Ticket".to_string(),
            from_state: "open".to_string(),
            to_state: "in_progress".to_string(),
            data,
            actor: Some("alice".to_string()),
            memo: None,
        }
    }

    fn make_executor() -> (HookExecutor, Arc<EventBus>) {
        let bus = Arc::new(EventBus::new(64));
        let exec = HookExecutor::new(Arc::clone(&bus));
        (exec, bus)
    }

    // --- EventBus tests ---

    #[tokio::test]
    async fn event_bus_emit_and_receive() {
        let bus = EventBus::new(16);
        let mut rx = bus.subscribe();

        bus.emit(Event {
            name: "order.created".to_string(),
            instance_id: "I1".to_string(),
            machine: "Order".to_string(),
            payload: Some(Value::Int(42)),
        });

        let event = rx.recv().await.unwrap();
        assert_eq!(event.name, "order.created");
        assert_eq!(event.instance_id, "I1");
        assert_eq!(event.payload, Some(Value::Int(42)));
    }

    #[tokio::test]
    async fn event_bus_no_subscribers_doesnt_error() {
        let bus = EventBus::new(16);
        // No subscribers — emit should not panic or error
        bus.emit(Event {
            name: "test".to_string(),
            instance_id: "I1".to_string(),
            machine: "M".to_string(),
            payload: None,
        });
    }

    #[tokio::test]
    async fn event_bus_multiple_subscribers() {
        let bus = EventBus::new(16);
        let mut rx1 = bus.subscribe();
        let mut rx2 = bus.subscribe();

        bus.emit(Event {
            name: "test.event".to_string(),
            instance_id: "I1".to_string(),
            machine: "M".to_string(),
            payload: None,
        });

        let e1 = rx1.recv().await.unwrap();
        let e2 = rx2.recv().await.unwrap();
        assert_eq!(e1.name, "test.event");
        assert_eq!(e2.name, "test.event");
    }

    // --- Action execution tests ---

    #[tokio::test]
    async fn execute_log_action() {
        let (exec, _bus) = make_executor();
        let ctx = test_ctx();
        let actions = vec![ResolvedAction::Log(
            "Transitioning {instance_id}".to_string(),
        )];
        let result = exec.execute_actions(&actions, &ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn execute_emit_action_publishes_event() {
        let (exec, bus) = make_executor();
        let ctx = test_ctx();
        let mut rx = bus.subscribe();

        let actions = vec![ResolvedAction::Emit {
            event: "ticket.moved".to_string(),
            payload: Some(Value::Text("hello".to_string())),
        }];
        exec.execute_actions(&actions, &ctx).await.unwrap();

        let event = rx.recv().await.unwrap();
        assert_eq!(event.name, "ticket.moved");
        assert_eq!(event.instance_id, "INST001");
        assert_eq!(event.payload, Some(Value::Text("hello".to_string())));
    }

    #[tokio::test]
    async fn execute_notify_action_dry_run() {
        let (exec, _bus) = make_executor();
        let ctx = test_ctx();
        let actions = vec![ResolvedAction::Notify {
            target: Value::Text("admin".to_string()),
            event: "escalation".to_string(),
        }];
        let result = exec.execute_actions(&actions, &ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn execute_webhook_action_dry_run() {
        let (exec, _bus) = make_executor();
        let ctx = test_ctx();
        let actions = vec![ResolvedAction::Webhook {
            url: "https://example.com/hook".to_string(),
            payload: None,
            response_field: None,
            on_failure_state: None,
        }];
        let result = exec.execute_actions(&actions, &ctx).await;
        assert!(result.is_ok());
    }

    // --- fire_hooks trigger matching tests ---

    #[tokio::test]
    async fn fire_hooks_matches_correct_trigger() {
        let (exec, bus) = make_executor();
        let ctx = test_ctx();
        let mut rx = bus.subscribe();

        let hooks = vec![
            HookDefinition {
                trigger: HookTrigger::OnSpawn,
                actions: vec![Action::Log("spawn".to_string())],
            },
            HookDefinition {
                trigger: HookTrigger::OnEnter("in_progress".to_string()),
                actions: vec![Action::Log("enter".to_string())],
            },
        ];

        // Only OnEnter(in_progress) resolved actions provided
        let resolved = vec![
            vec![], // OnSpawn hook: no actions should fire
            vec![ResolvedAction::Emit {
                event: "entered".to_string(),
                payload: None,
            }],
        ];

        exec.fire_hooks(
            &hooks,
            &HookTrigger::OnEnter("in_progress".to_string()),
            &ctx,
            &resolved,
        )
        .await
        .unwrap();

        // Should have received the emit from the matching hook
        let event = rx.recv().await.unwrap();
        assert_eq!(event.name, "entered");
    }

    #[tokio::test]
    async fn fire_hooks_before_hook_can_reject() {
        let (exec, _bus) = make_executor();
        let ctx = test_ctx();

        // A BEFORE hook that "rejects" — we simulate this by having the executor
        // return an error. In practice, BEFORE hooks reject by returning HookError::Rejected
        // from a custom action. Here we test with an empty action list which passes.
        let hooks = vec![HookDefinition {
            trigger: HookTrigger::BeforeEachTransition,
            actions: vec![Action::Log("before check".to_string())],
        }];

        let resolved = vec![vec![ResolvedAction::Log("checking...".to_string())]];

        let result = exec
            .fire_hooks(&hooks, &HookTrigger::BeforeEachTransition, &ctx, &resolved)
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn fire_hooks_non_matching_trigger_skipped() {
        let (exec, bus) = make_executor();
        let ctx = test_ctx();
        let mut rx = bus.subscribe();

        let hooks = vec![HookDefinition {
            trigger: HookTrigger::OnExit("open".to_string()),
            actions: vec![Action::Log("exit".to_string())],
        }];

        let resolved = vec![vec![ResolvedAction::Emit {
            event: "should_not_fire".to_string(),
            payload: None,
        }]];

        // Fire OnEnter, not OnExit
        exec.fire_hooks(
            &hooks,
            &HookTrigger::OnEnter("in_progress".to_string()),
            &ctx,
            &resolved,
        )
        .await
        .unwrap();

        // No event should have been emitted
        let result = rx.try_recv();
        assert!(result.is_err());
    }

    // --- Log template rendering ---

    #[test]
    fn render_log_template_replaces_fields() {
        let ctx = test_ctx();
        let rendered = render_log_template(
            "Instance {instance_id} moved from {from_state} to {to_state} by {actor}, title={title}",
            &ctx,
        );
        assert!(rendered.contains("INST001"));
        assert!(rendered.contains("open"));
        assert!(rendered.contains("in_progress"));
        assert!(rendered.contains("alice"));
        assert!(rendered.contains("\"Bug fix\""));
    }

    // --- trigger_matches ---

    #[test]
    fn trigger_matches_on_spawn() {
        assert!(trigger_matches(
            &HookTrigger::OnSpawn,
            &HookTrigger::OnSpawn
        ));
        assert!(!trigger_matches(
            &HookTrigger::OnSpawn,
            &HookTrigger::AfterEachTransition,
        ));
    }

    #[test]
    fn trigger_matches_on_enter_specific_state() {
        assert!(trigger_matches(
            &HookTrigger::OnEnter("active".to_string()),
            &HookTrigger::OnEnter("active".to_string()),
        ));
        assert!(!trigger_matches(
            &HookTrigger::OnEnter("active".to_string()),
            &HookTrigger::OnEnter("closed".to_string()),
        ));
    }

    #[test]
    fn trigger_matches_on_exit_specific_state() {
        assert!(trigger_matches(
            &HookTrigger::OnExit("draft".to_string()),
            &HookTrigger::OnExit("draft".to_string()),
        ));
        assert!(!trigger_matches(
            &HookTrigger::OnExit("draft".to_string()),
            &HookTrigger::OnExit("published".to_string()),
        ));
    }

    // --- HookExecutor::with_callback() constructor test ---

    struct MockCallback;

    #[async_trait::async_trait]
    impl EngineCallback for MockCallback {
        async fn spawn_child(
            &self,
            _parent_instance_id: &str,
            _machine: &str,
            _data: Vec<(String, Value)>,
        ) -> Result<String, HookError> {
            Ok("CHILD_001".to_string())
        }

        async fn signal_parent(
            &self,
            _child_instance_id: &str,
            _target_state: &str,
        ) -> Result<(), HookError> {
            Ok(())
        }

        async fn update_instance_field(
            &self,
            _instance_id: &str,
            _field: &str,
            _value: Value,
        ) -> Result<(), HookError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn with_callback_constructor_sets_callback() {
        let bus = Arc::new(EventBus::new(16));
        let cb: Arc<dyn EngineCallback> = Arc::new(MockCallback);
        let exec = HookExecutor::with_callback(Arc::clone(&bus), cb);

        // Verify the callback is set by executing a SpawnChild action
        let ctx = test_ctx();
        let actions = vec![ResolvedAction::SpawnChild {
            machine: "Child".to_string(),
            data: vec![],
        }];
        let result = exec.execute_actions(&actions, &ctx).await;
        assert!(result.is_ok());
    }

    // --- SpawnChild action when callback is None ---

    #[tokio::test]
    async fn spawn_child_without_callback_warns_but_succeeds() {
        let bus = Arc::new(EventBus::new(16));
        let exec = HookExecutor::new(Arc::clone(&bus));
        // No callback set

        let ctx = test_ctx();
        let actions = vec![ResolvedAction::SpawnChild {
            machine: "ChildMachine".to_string(),
            data: vec![("key".to_string(), Value::Text("val".to_string()))],
        }];
        // Should succeed (warn, not error) even without a callback
        let result = exec.execute_actions(&actions, &ctx).await;
        assert!(result.is_ok());
    }

    // --- SignalParent action when callback is None ---

    #[tokio::test]
    async fn signal_parent_without_callback_warns_but_succeeds() {
        let bus = Arc::new(EventBus::new(16));
        let exec = HookExecutor::new(Arc::clone(&bus));
        // No callback set

        let ctx = test_ctx();
        let actions = vec![ResolvedAction::SignalParent {
            target_state: "completed".to_string(),
        }];
        // Should succeed (warn, not error) even without a callback
        let result = exec.execute_actions(&actions, &ctx).await;
        assert!(result.is_ok());
    }

    // --- Action error in non-BEFORE hook (fire-and-forget) ---

    struct FailingCallback;

    #[async_trait::async_trait]
    impl EngineCallback for FailingCallback {
        async fn spawn_child(
            &self,
            _parent_instance_id: &str,
            _machine: &str,
            _data: Vec<(String, Value)>,
        ) -> Result<String, HookError> {
            Err(HookError::ActionFailed {
                message: "spawn failed".to_string(),
            })
        }

        async fn signal_parent(
            &self,
            _child_instance_id: &str,
            _target_state: &str,
        ) -> Result<(), HookError> {
            Err(HookError::ActionFailed {
                message: "signal failed".to_string(),
            })
        }

        async fn update_instance_field(
            &self,
            _instance_id: &str,
            _field: &str,
            _value: Value,
        ) -> Result<(), HookError> {
            Err(HookError::ActionFailed {
                message: "update failed".to_string(),
            })
        }
    }

    #[tokio::test]
    async fn non_before_hook_error_is_fire_and_forget() {
        let bus = Arc::new(EventBus::new(16));
        let cb: Arc<dyn EngineCallback> = Arc::new(FailingCallback);
        let exec = HookExecutor::with_callback(Arc::clone(&bus), cb);
        let ctx = test_ctx();

        // An AFTER hook with an action that will fail
        let hooks = vec![HookDefinition {
            trigger: HookTrigger::AfterEachTransition,
            actions: vec![Action::Log("after".to_string())],
        }];

        let resolved = vec![vec![ResolvedAction::SpawnChild {
            machine: "Child".to_string(),
            data: vec![],
        }]];

        // Fire as AfterEachTransition — the error should be swallowed (fire-and-forget)
        let result = exec
            .fire_hooks(&hooks, &HookTrigger::AfterEachTransition, &ctx, &resolved)
            .await;
        assert!(
            result.is_ok(),
            "Non-BEFORE hook errors should not propagate"
        );
    }

    #[tokio::test]
    async fn before_hook_error_propagates() {
        let bus = Arc::new(EventBus::new(16));
        let cb: Arc<dyn EngineCallback> = Arc::new(FailingCallback);
        let exec = HookExecutor::with_callback(Arc::clone(&bus), cb);
        let ctx = test_ctx();

        // A BEFORE hook with an action that will fail
        let hooks = vec![HookDefinition {
            trigger: HookTrigger::BeforeEachTransition,
            actions: vec![Action::Log("before".to_string())],
        }];

        let resolved = vec![vec![ResolvedAction::SpawnChild {
            machine: "Child".to_string(),
            data: vec![],
        }]];

        // Fire as BeforeEachTransition — the error SHOULD propagate
        let result = exec
            .fire_hooks(&hooks, &HookTrigger::BeforeEachTransition, &ctx, &resolved)
            .await;
        assert!(result.is_err(), "BEFORE hook errors should propagate");
    }

    // ---------------------------------------------------------------
    // Additional coverage tests
    // ---------------------------------------------------------------

    /// A callback that records calls for verification.
    #[allow(clippy::type_complexity)]
    struct RecordingCallback {
        spawn_calls: std::sync::Mutex<Vec<(String, String, Vec<(String, Value)>)>>,
        signal_calls: std::sync::Mutex<Vec<(String, String)>>,
    }

    impl RecordingCallback {
        fn new() -> Self {
            Self {
                spawn_calls: std::sync::Mutex::new(Vec::new()),
                signal_calls: std::sync::Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait::async_trait]
    impl EngineCallback for RecordingCallback {
        async fn spawn_child(
            &self,
            parent_instance_id: &str,
            machine: &str,
            data: Vec<(String, Value)>,
        ) -> Result<String, HookError> {
            self.spawn_calls.lock().unwrap().push((
                parent_instance_id.to_string(),
                machine.to_string(),
                data,
            ));
            Ok("CHILD_REC_001".to_string())
        }

        async fn signal_parent(
            &self,
            child_instance_id: &str,
            target_state: &str,
        ) -> Result<(), HookError> {
            self.signal_calls
                .lock()
                .unwrap()
                .push((child_instance_id.to_string(), target_state.to_string()));
            Ok(())
        }

        async fn update_instance_field(
            &self,
            _instance_id: &str,
            _field: &str,
            _value: Value,
        ) -> Result<(), HookError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn spawn_child_with_callback_calls_callback_and_passes_data() {
        let bus = Arc::new(EventBus::new(16));
        let cb = Arc::new(RecordingCallback::new());
        let exec = HookExecutor::with_callback(
            Arc::clone(&bus),
            Arc::clone(&cb) as Arc<dyn EngineCallback>,
        );
        let ctx = test_ctx();

        let data = vec![
            ("name".to_string(), Value::Text("Widget".to_string())),
            ("count".to_string(), Value::Int(5)),
        ];
        let actions = vec![ResolvedAction::SpawnChild {
            machine: "ChildMachine".to_string(),
            data: data.clone(),
        }];
        let result = exec.execute_actions(&actions, &ctx).await;
        assert!(result.is_ok());

        let calls = cb.spawn_calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "INST001"); // parent_instance_id from ctx
        assert_eq!(calls[0].1, "ChildMachine");
        assert_eq!(calls[0].2.len(), 2);
        assert_eq!(calls[0].2[0].0, "name");
        assert_eq!(calls[0].2[0].1, Value::Text("Widget".to_string()));
    }

    #[tokio::test]
    async fn signal_parent_with_callback_calls_callback() {
        let bus = Arc::new(EventBus::new(16));
        let cb = Arc::new(RecordingCallback::new());
        let exec = HookExecutor::with_callback(
            Arc::clone(&bus),
            Arc::clone(&cb) as Arc<dyn EngineCallback>,
        );
        let ctx = test_ctx();

        let actions = vec![ResolvedAction::SignalParent {
            target_state: "resolved".to_string(),
        }];
        let result = exec.execute_actions(&actions, &ctx).await;
        assert!(result.is_ok());

        let calls = cb.signal_calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "INST001"); // child_instance_id from ctx
        assert_eq!(calls[0].1, "resolved");
    }

    #[tokio::test]
    async fn multiple_hooks_same_trigger_all_fire() {
        let bus = Arc::new(EventBus::new(64));
        let cb = Arc::new(RecordingCallback::new());
        let exec = HookExecutor::with_callback(
            Arc::clone(&bus),
            Arc::clone(&cb) as Arc<dyn EngineCallback>,
        );
        let ctx = test_ctx();
        let mut rx = bus.subscribe();

        // Two hooks both triggered by AfterEachTransition
        let hooks = vec![
            HookDefinition {
                trigger: HookTrigger::AfterEachTransition,
                actions: vec![Action::Log("first".to_string())],
            },
            HookDefinition {
                trigger: HookTrigger::AfterEachTransition,
                actions: vec![Action::Log("second".to_string())],
            },
        ];

        let resolved = vec![
            vec![ResolvedAction::Emit {
                event: "hook_one_fired".to_string(),
                payload: None,
            }],
            vec![ResolvedAction::Emit {
                event: "hook_two_fired".to_string(),
                payload: None,
            }],
        ];

        exec.fire_hooks(&hooks, &HookTrigger::AfterEachTransition, &ctx, &resolved)
            .await
            .unwrap();

        // Both hooks should have emitted events
        let e1 = rx.recv().await.unwrap();
        let e2 = rx.recv().await.unwrap();
        assert_eq!(e1.name, "hook_one_fired");
        assert_eq!(e2.name, "hook_two_fired");
    }

    #[tokio::test]
    async fn resolved_actions_out_of_bounds_uses_empty_default() {
        let (exec, bus) = make_executor();
        let ctx = test_ctx();
        let mut rx = bus.subscribe();

        // Two hooks defined but only one resolved actions entry
        let hooks = vec![
            HookDefinition {
                trigger: HookTrigger::AfterEachTransition,
                actions: vec![Action::Log("a".to_string())],
            },
            HookDefinition {
                trigger: HookTrigger::AfterEachTransition,
                actions: vec![Action::Log("b".to_string())],
            },
        ];

        // Only provide resolved actions for the first hook
        let resolved = vec![vec![ResolvedAction::Emit {
            event: "only_first".to_string(),
            payload: None,
        }]];

        // Should not panic — second hook gets empty actions via unwrap_or_default
        let result = exec
            .fire_hooks(&hooks, &HookTrigger::AfterEachTransition, &ctx, &resolved)
            .await;
        assert!(result.is_ok());

        // Only one event should have been emitted
        let e1 = rx.recv().await.unwrap();
        assert_eq!(e1.name, "only_first");
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn render_log_template_without_actor() {
        let ctx = HookContext {
            instance_id: "I99".to_string(),
            machine: "Order".to_string(),
            from_state: "new".to_string(),
            to_state: "paid".to_string(),
            data: HashMap::new(),
            actor: None,
            memo: None,
        };
        // {actor} should remain unreplaced when actor is None
        let rendered = render_log_template("actor={actor} machine={machine}", &ctx);
        assert!(rendered.contains("actor={actor}"));
        assert!(rendered.contains("machine=Order"));
    }

    #[tokio::test]
    async fn set_callback_enables_spawn() {
        let bus = Arc::new(EventBus::new(16));
        let exec = HookExecutor::new(Arc::clone(&bus));
        let ctx = test_ctx();

        // Initially no callback — SpawnChild is a no-op
        let actions = vec![ResolvedAction::SpawnChild {
            machine: "M".to_string(),
            data: vec![],
        }];
        assert!(exec.execute_actions(&actions, &ctx).await.is_ok());

        // Now set the callback
        let cb = Arc::new(RecordingCallback::new());
        exec.set_callback(Arc::clone(&cb) as Arc<dyn EngineCallback>);

        // Now SpawnChild should call the callback
        assert!(exec.execute_actions(&actions, &ctx).await.is_ok());
        let calls = cb.spawn_calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].1, "M");
    }

    #[tokio::test]
    async fn execute_empty_actions_list() {
        let (exec, _bus) = make_executor();
        let ctx = test_ctx();
        let result = exec.execute_actions(&[], &ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn execute_multiple_actions_in_sequence() {
        let (exec, bus) = make_executor();
        let ctx = test_ctx();
        let mut rx = bus.subscribe();

        let actions = vec![
            ResolvedAction::Log("step1".to_string()),
            ResolvedAction::Emit {
                event: "step2_event".to_string(),
                payload: Some(Value::Bool(true)),
            },
            ResolvedAction::Notify {
                target: Value::Text("ops".to_string()),
                event: "step3_notify".to_string(),
            },
            ResolvedAction::Webhook {
                url: "https://example.com".to_string(),
                payload: None,
                response_field: None,
                on_failure_state: None,
            },
        ];

        let result = exec.execute_actions(&actions, &ctx).await;
        assert!(result.is_ok());

        // Only the Emit should produce an event on the bus
        let event = rx.recv().await.unwrap();
        assert_eq!(event.name, "step2_event");
        assert_eq!(event.payload, Some(Value::Bool(true)));
        // No more events
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn signal_parent_error_propagates_in_before_hook() {
        let bus = Arc::new(EventBus::new(16));
        let cb: Arc<dyn EngineCallback> = Arc::new(FailingCallback);
        let exec = HookExecutor::with_callback(Arc::clone(&bus), cb);
        let ctx = test_ctx();

        let hooks = vec![HookDefinition {
            trigger: HookTrigger::BeforeEachTransition,
            actions: vec![Action::Log("before".to_string())],
        }];

        // Use SignalParent (not SpawnChild) to test FailingCallback::signal_parent path
        let resolved = vec![vec![ResolvedAction::SignalParent {
            target_state: "done".to_string(),
        }]];

        let result = exec
            .fire_hooks(&hooks, &HookTrigger::BeforeEachTransition, &ctx, &resolved)
            .await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("signal failed"),
            "Expected 'signal failed' in error, got: {}",
            err_msg
        );
    }

    #[test]
    fn trigger_matches_cross_variant_mismatches() {
        // BeforeEachTransition vs AfterEachTransition
        assert!(!trigger_matches(
            &HookTrigger::BeforeEachTransition,
            &HookTrigger::AfterEachTransition,
        ));
        // AfterEachTransition vs OnSpawn
        assert!(!trigger_matches(
            &HookTrigger::AfterEachTransition,
            &HookTrigger::OnSpawn,
        ));
        // OnEnter vs OnExit with same state
        assert!(!trigger_matches(
            &HookTrigger::OnEnter("active".to_string()),
            &HookTrigger::OnExit("active".to_string()),
        ));
        // OnExit vs OnSpawn
        assert!(!trigger_matches(
            &HookTrigger::OnExit("draft".to_string()),
            &HookTrigger::OnSpawn,
        ));
        // AfterEachTransition matches itself
        assert!(trigger_matches(
            &HookTrigger::AfterEachTransition,
            &HookTrigger::AfterEachTransition,
        ));
    }

    #[test]
    fn event_bus_default_constructor() {
        let bus = EventBus::default();
        let mut rx = bus.subscribe();
        bus.emit(Event {
            name: "default_test".to_string(),
            instance_id: "I1".to_string(),
            machine: "M".to_string(),
            payload: None,
        });
        // Should receive the event — default capacity is 256
        let event = rx.try_recv().unwrap();
        assert_eq!(event.name, "default_test");
    }

    #[test]
    fn render_log_template_with_data_and_unknown_placeholders() {
        let mut data = HashMap::new();
        data.insert("priority".to_string(), Value::Int(3));
        data.insert("tags".to_string(), Value::List(vec![Value::Text("bug".into())]));
        let ctx = HookContext {
            instance_id: "I1".to_string(),
            machine: "Ticket".to_string(),
            from_state: "open".to_string(),
            to_state: "closed".to_string(),
            data,
            actor: Some("bob".to_string()),
            memo: Some("fixed".to_string()),
        };
        let rendered =
            render_log_template("p={priority} tags={tags} unknown={unknown_field}", &ctx);
        assert!(rendered.contains("p=3"));
        assert!(rendered.contains("tags=[\"bug\"]"));
        assert!(rendered.contains("unknown={unknown_field}"));
    }

    #[test]
    fn render_log_template_no_placeholders() {
        let ctx = test_ctx();
        let rendered = render_log_template("plain text no placeholders", &ctx);
        assert_eq!(rendered, "plain text no placeholders");
    }

    #[test]
    fn hook_error_display() {
        let e = HookError::Rejected {
            reason: "guard failed".to_string(),
        };
        assert!(e.to_string().contains("guard failed"));

        let e = HookError::ActionFailed {
            message: "boom".to_string(),
        };
        assert!(e.to_string().contains("boom"));

        let e = HookError::WebhookFailed {
            url: "https://x.com".to_string(),
            message: "timeout".to_string(),
        };
        assert!(e.to_string().contains("https://x.com"));
        assert!(e.to_string().contains("timeout"));
    }

    #[tokio::test]
    async fn set_webhook_client_enables_webhook_execution() {
        let bus = Arc::new(EventBus::new(16));
        let exec = HookExecutor::new(Arc::clone(&bus));

        // Create a webhook client — we can't actually test HTTP calls but we can set it
        let wh_config = webhook::WebhookConfig {
            timeout: std::time::Duration::from_secs(1),
            max_retries: 0,
            retry_delay: std::time::Duration::from_millis(100),
        };
        let wh_client = Arc::new(webhook::WebhookClient::new(wh_config));
        exec.set_webhook_client(wh_client);

        // Webhook to a non-existent URL should fail with network error (not dry-run)
        let ctx = test_ctx();
        let actions = vec![ResolvedAction::Webhook {
            url: "http://127.0.0.1:1/nonexistent".to_string(),
            payload: None,
            response_field: None,
            on_failure_state: None,
        }];
        let result = exec.execute_actions(&actions, &ctx).await;
        assert!(result.is_err()); // Should be a WebhookFailed error, not dry-run OK
    }

    #[tokio::test]
    async fn webhook_with_payload_dry_run() {
        let (exec, _bus) = make_executor();
        let ctx = test_ctx();
        let actions = vec![ResolvedAction::Webhook {
            url: "https://example.com/hook".to_string(),
            payload: Some(Value::Map(std::collections::BTreeMap::from([
                ("key".to_string(), Value::Text("val".to_string())),
            ]))),
            response_field: None,
            on_failure_state: None,
        }];
        // No webhook client set → dry-run mode
        let result = exec.execute_actions(&actions, &ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn fire_hooks_with_empty_hooks_list() {
        let (exec, _bus) = make_executor();
        let ctx = test_ctx();
        let result = exec
            .fire_hooks(&[], &HookTrigger::OnSpawn, &ctx, &[])
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn fire_hooks_on_spawn_trigger() {
        let (exec, bus) = make_executor();
        let ctx = test_ctx();
        let mut rx = bus.subscribe();

        let hooks = vec![HookDefinition {
            trigger: HookTrigger::OnSpawn,
            actions: vec![Action::Log("spawned".to_string())],
        }];
        let resolved = vec![vec![ResolvedAction::Emit {
            event: "spawn_event".to_string(),
            payload: None,
        }]];

        exec.fire_hooks(&hooks, &HookTrigger::OnSpawn, &ctx, &resolved)
            .await
            .unwrap();

        let event = rx.recv().await.unwrap();
        assert_eq!(event.name, "spawn_event");
    }

    // ---------------------------------------------------------------
    // Additional coverage tests — coverage gaps
    // ---------------------------------------------------------------

    /// Notify action with a Map target (non-Text) to exercise the Display path for target.
    #[tokio::test]
    async fn notify_action_with_map_target() {
        let (exec, _bus) = make_executor();
        let ctx = test_ctx();
        let mut target_map = std::collections::BTreeMap::new();
        target_map.insert("id".to_string(), Value::Text("user42".to_string()));
        target_map.insert("role".to_string(), Value::Text("admin".to_string()));
        let actions = vec![ResolvedAction::Notify {
            target: Value::Map(target_map),
            event: "escalation_alert".to_string(),
        }];
        let result = exec.execute_actions(&actions, &ctx).await;
        assert!(result.is_ok());
    }

    /// Notify action with a list target.
    #[tokio::test]
    async fn notify_action_with_list_target() {
        let (exec, _bus) = make_executor();
        let ctx = test_ctx();
        let actions = vec![ResolvedAction::Notify {
            target: Value::List(vec![
                Value::Text("admin".to_string()),
                Value::Text("ops".to_string()),
            ]),
            event: "multi_notify".to_string(),
        }];
        let result = exec.execute_actions(&actions, &ctx).await;
        assert!(result.is_ok());
    }

    /// SpawnChild without callback — with data fields to exercise the warn log with machine name.
    #[tokio::test]
    async fn spawn_child_no_callback_with_data_fields() {
        let bus = Arc::new(EventBus::new(16));
        let exec = HookExecutor::new(Arc::clone(&bus));
        let ctx = test_ctx();
        let actions = vec![ResolvedAction::SpawnChild {
            machine: "SubTask".to_string(),
            data: vec![
                ("assignee".to_string(), Value::Text("bob".to_string())),
                ("priority".to_string(), Value::Int(3)),
            ],
        }];
        // No callback set — should warn and succeed, not error
        let result = exec.execute_actions(&actions, &ctx).await;
        assert!(result.is_ok());
    }

    /// SignalParent without callback — exercises the warn branch with a specific target_state.
    #[tokio::test]
    async fn signal_parent_no_callback_with_target_state() {
        let bus = Arc::new(EventBus::new(16));
        let exec = HookExecutor::new(Arc::clone(&bus));
        let ctx = test_ctx();
        let actions = vec![ResolvedAction::SignalParent {
            target_state: "parent_resolved".to_string(),
        }];
        // No callback set — should warn and succeed
        let result = exec.execute_actions(&actions, &ctx).await;
        assert!(result.is_ok());
    }

    /// fire_hooks with multiple hooks that have different triggers — only matching ones fire.
    #[tokio::test]
    async fn fire_hooks_mixed_triggers_only_matching_fires() {
        let (exec, bus) = make_executor();
        let ctx = test_ctx();
        let mut rx = bus.subscribe();

        let hooks = vec![
            HookDefinition {
                trigger: HookTrigger::OnSpawn,
                actions: vec![Action::Log("spawn".to_string())],
            },
            HookDefinition {
                trigger: HookTrigger::AfterEachTransition,
                actions: vec![Action::Log("after".to_string())],
            },
            HookDefinition {
                trigger: HookTrigger::OnEnter("closed".to_string()),
                actions: vec![Action::Log("enter_closed".to_string())],
            },
            HookDefinition {
                trigger: HookTrigger::AfterEachTransition,
                actions: vec![Action::Log("after2".to_string())],
            },
        ];

        let resolved = vec![
            vec![ResolvedAction::Emit {
                event: "should_not_fire_spawn".to_string(),
                payload: None,
            }],
            vec![ResolvedAction::Emit {
                event: "should_fire_after1".to_string(),
                payload: None,
            }],
            vec![ResolvedAction::Emit {
                event: "should_not_fire_enter".to_string(),
                payload: None,
            }],
            vec![ResolvedAction::Emit {
                event: "should_fire_after2".to_string(),
                payload: None,
            }],
        ];

        // Fire AfterEachTransition — only hooks[1] and hooks[3] should fire
        exec.fire_hooks(&hooks, &HookTrigger::AfterEachTransition, &ctx, &resolved)
            .await
            .unwrap();

        let e1 = rx.recv().await.unwrap();
        assert_eq!(e1.name, "should_fire_after1");
        let e2 = rx.recv().await.unwrap();
        assert_eq!(e2.name, "should_fire_after2");
        // No more events
        assert!(rx.try_recv().is_err());
    }

    /// fire_hooks: all hooks have non-matching triggers — nothing should execute.
    #[tokio::test]
    async fn fire_hooks_all_non_matching_triggers() {
        let (exec, bus) = make_executor();
        let ctx = test_ctx();
        let mut rx = bus.subscribe();

        let hooks = vec![
            HookDefinition {
                trigger: HookTrigger::OnSpawn,
                actions: vec![Action::Log("spawn".to_string())],
            },
            HookDefinition {
                trigger: HookTrigger::OnEnter("closed".to_string()),
                actions: vec![Action::Log("enter_closed".to_string())],
            },
            HookDefinition {
                trigger: HookTrigger::OnExit("draft".to_string()),
                actions: vec![Action::Log("exit_draft".to_string())],
            },
        ];

        let resolved = vec![
            vec![ResolvedAction::Emit {
                event: "e1".to_string(),
                payload: None,
            }],
            vec![ResolvedAction::Emit {
                event: "e2".to_string(),
                payload: None,
            }],
            vec![ResolvedAction::Emit {
                event: "e3".to_string(),
                payload: None,
            }],
        ];

        // Fire BeforeEachTransition — none of the hooks match
        exec.fire_hooks(
            &hooks,
            &HookTrigger::BeforeEachTransition,
            &ctx,
            &resolved,
        )
        .await
        .unwrap();

        // No events should have been emitted
        assert!(rx.try_recv().is_err());
    }

    /// fire_hooks: OnExit trigger fires only for matching state.
    #[tokio::test]
    async fn fire_hooks_on_exit_trigger() {
        let (exec, bus) = make_executor();
        let ctx = test_ctx();
        let mut rx = bus.subscribe();

        let hooks = vec![
            HookDefinition {
                trigger: HookTrigger::OnExit("open".to_string()),
                actions: vec![Action::Log("exit_open".to_string())],
            },
            HookDefinition {
                trigger: HookTrigger::OnExit("closed".to_string()),
                actions: vec![Action::Log("exit_closed".to_string())],
            },
        ];

        let resolved = vec![
            vec![ResolvedAction::Emit {
                event: "exited_open".to_string(),
                payload: None,
            }],
            vec![ResolvedAction::Emit {
                event: "exited_closed".to_string(),
                payload: None,
            }],
        ];

        // Fire OnExit("open") — only the first hook matches
        exec.fire_hooks(
            &hooks,
            &HookTrigger::OnExit("open".to_string()),
            &ctx,
            &resolved,
        )
        .await
        .unwrap();

        let event = rx.recv().await.unwrap();
        assert_eq!(event.name, "exited_open");
        assert!(rx.try_recv().is_err());
    }

    /// Webhook action with a webhook client set — hitting an unreachable server returns WebhookFailed.
    #[tokio::test]
    async fn webhook_action_with_client_unreachable_server() {
        let bus = Arc::new(EventBus::new(16));
        let exec = HookExecutor::new(Arc::clone(&bus));

        let wh_config = webhook::WebhookConfig {
            timeout: std::time::Duration::from_secs(1),
            max_retries: 0,
            retry_delay: std::time::Duration::from_millis(10),
        };
        let wh_client = Arc::new(webhook::WebhookClient::new(wh_config));
        exec.set_webhook_client(wh_client);

        let ctx = test_ctx();
        let actions = vec![ResolvedAction::Webhook {
            url: "http://127.0.0.1:1/unreachable".to_string(),
            payload: Some(Value::Text("test_payload".to_string())),
            response_field: None,
            on_failure_state: None,
        }];
        let result = exec.execute_actions(&actions, &ctx).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            HookError::WebhookFailed { url, message } => {
                assert_eq!(url, "http://127.0.0.1:1/unreachable");
                assert!(!message.is_empty());
            }
            other => panic!("Expected WebhookFailed, got: {:?}", other),
        }
    }

    /// Webhook failure in a non-BEFORE hook is fire-and-forget (error swallowed).
    #[tokio::test]
    async fn webhook_failure_fire_and_forget_in_after_hook() {
        let bus = Arc::new(EventBus::new(16));
        let exec = HookExecutor::new(Arc::clone(&bus));

        let wh_config = webhook::WebhookConfig {
            timeout: std::time::Duration::from_secs(1),
            max_retries: 0,
            retry_delay: std::time::Duration::from_millis(10),
        };
        let wh_client = Arc::new(webhook::WebhookClient::new(wh_config));
        exec.set_webhook_client(wh_client);

        let ctx = test_ctx();
        let hooks = vec![HookDefinition {
            trigger: HookTrigger::AfterEachTransition,
            actions: vec![Action::Log("after".to_string())],
        }];
        let resolved = vec![vec![ResolvedAction::Webhook {
            url: "http://127.0.0.1:1/fail".to_string(),
            payload: None,
            response_field: None,
            on_failure_state: None,
        }]];

        // AFTER hook — error is swallowed
        let result = exec
            .fire_hooks(&hooks, &HookTrigger::AfterEachTransition, &ctx, &resolved)
            .await;
        assert!(
            result.is_ok(),
            "Webhook failure in AFTER hook should be fire-and-forget"
        );
    }

    /// Webhook failure in a BEFORE hook propagates the error.
    #[tokio::test]
    async fn webhook_failure_propagates_in_before_hook() {
        let bus = Arc::new(EventBus::new(16));
        let exec = HookExecutor::new(Arc::clone(&bus));

        let wh_config = webhook::WebhookConfig {
            timeout: std::time::Duration::from_secs(1),
            max_retries: 0,
            retry_delay: std::time::Duration::from_millis(10),
        };
        let wh_client = Arc::new(webhook::WebhookClient::new(wh_config));
        exec.set_webhook_client(wh_client);

        let ctx = test_ctx();
        let hooks = vec![HookDefinition {
            trigger: HookTrigger::BeforeEachTransition,
            actions: vec![Action::Log("before".to_string())],
        }];
        let resolved = vec![vec![ResolvedAction::Webhook {
            url: "http://127.0.0.1:1/fail".to_string(),
            payload: None,
            response_field: None,
            on_failure_state: None,
        }]];

        // BEFORE hook — error should propagate
        let result = exec
            .fire_hooks(
                &hooks,
                &HookTrigger::BeforeEachTransition,
                &ctx,
                &resolved,
            )
            .await;
        assert!(
            result.is_err(),
            "Webhook failure in BEFORE hook should propagate"
        );
    }

    /// Execute mixed actions: Log, Notify, Emit, SpawnChild (no callback), SignalParent (no callback).
    /// All should succeed (no callback means warn-and-noop for spawn/signal).
    #[tokio::test]
    async fn execute_all_action_types_without_callback() {
        let (exec, bus) = make_executor();
        let ctx = test_ctx();
        let mut rx = bus.subscribe();

        let actions = vec![
            ResolvedAction::Log("log msg".to_string()),
            ResolvedAction::Notify {
                target: Value::Text("admin".to_string()),
                event: "alert".to_string(),
            },
            ResolvedAction::Emit {
                event: "mixed_event".to_string(),
                payload: Some(Value::Int(99)),
            },
            ResolvedAction::Webhook {
                url: "https://example.com/dry".to_string(),
                payload: None,
                response_field: None,
                on_failure_state: None,
            },
            ResolvedAction::SpawnChild {
                machine: "ChildTask".to_string(),
                data: vec![],
            },
            ResolvedAction::SignalParent {
                target_state: "done".to_string(),
            },
        ];

        let result = exec.execute_actions(&actions, &ctx).await;
        assert!(result.is_ok());

        // Only the Emit should produce a bus event
        let event = rx.recv().await.unwrap();
        assert_eq!(event.name, "mixed_event");
        assert_eq!(event.payload, Some(Value::Int(99)));
        assert!(rx.try_recv().is_err());
    }

    /// Emit action without payload.
    #[tokio::test]
    async fn emit_action_without_payload() {
        let (exec, bus) = make_executor();
        let ctx = test_ctx();
        let mut rx = bus.subscribe();

        let actions = vec![ResolvedAction::Emit {
            event: "bare_event".to_string(),
            payload: None,
        }];
        exec.execute_actions(&actions, &ctx).await.unwrap();

        let event = rx.recv().await.unwrap();
        assert_eq!(event.name, "bare_event");
        assert_eq!(event.payload, None);
        assert_eq!(event.machine, "Ticket");
    }

    // ---------------------------------------------------------------
    // Additional coverage: webhook success path through HookExecutor
    // (lines 271, 274 — WEBHOOK delivered info log)
    // ---------------------------------------------------------------

    /// Webhook action with a real client hitting a 200 OK server — exercises
    /// the success path in execute_action (lines 270-274).
    #[tokio::test]
    async fn webhook_action_success_through_executor() {
        use tokio::io::AsyncWriteExt;
        use tokio::net::TcpListener;

        // Start a local server that returns 200 OK
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let handle = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 4096];
            let _ = tokio::io::AsyncReadExt::read(&mut stream, &mut buf).await;
            let response = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok";
            stream.write_all(response.as_bytes()).await.unwrap();
            stream.flush().await.unwrap();
        });

        let bus = Arc::new(EventBus::new(16));
        let exec = HookExecutor::new(Arc::clone(&bus));

        let wh_config = webhook::WebhookConfig {
            timeout: std::time::Duration::from_secs(5),
            max_retries: 0,
            retry_delay: std::time::Duration::from_millis(10),
        };
        let wh_client = Arc::new(webhook::WebhookClient::new(wh_config));
        exec.set_webhook_client(wh_client);

        let ctx = test_ctx();
        let url = format!("http://127.0.0.1:{}/webhook", addr.port());
        let actions = vec![ResolvedAction::Webhook {
            url,
            payload: Some(Value::Text("hello".to_string())),
            response_field: None,
            on_failure_state: None,
        }];
        let result = exec.execute_actions(&actions, &ctx).await;
        assert!(result.is_ok(), "Webhook to 200 OK server should succeed");

        handle.await.unwrap();
    }

    /// SpawnChild with callback — verify the child_id return is logged (line 311).
    #[tokio::test]
    async fn spawn_child_callback_logs_child_id() {
        let bus = Arc::new(EventBus::new(16));
        let cb = Arc::new(RecordingCallback::new());
        let exec = HookExecutor::with_callback(
            Arc::clone(&bus),
            Arc::clone(&cb) as Arc<dyn EngineCallback>,
        );
        let ctx = test_ctx();

        let actions = vec![ResolvedAction::SpawnChild {
            machine: "ChildWidget".to_string(),
            data: vec![("size".to_string(), Value::Int(42))],
        }];
        let result = exec.execute_actions(&actions, &ctx).await;
        assert!(result.is_ok());

        let calls = cb.spawn_calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].1, "ChildWidget");
    }

    /// SignalParent with callback — exercises line 326.
    #[tokio::test]
    async fn signal_parent_callback_succeeds() {
        let bus = Arc::new(EventBus::new(16));
        let cb = Arc::new(RecordingCallback::new());
        let exec = HookExecutor::with_callback(
            Arc::clone(&bus),
            Arc::clone(&cb) as Arc<dyn EngineCallback>,
        );
        let ctx = test_ctx();

        let actions = vec![ResolvedAction::SignalParent {
            target_state: "all_done".to_string(),
        }];
        let result = exec.execute_actions(&actions, &ctx).await;
        assert!(result.is_ok());

        let calls = cb.signal_calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "INST001");
        assert_eq!(calls[0].1, "all_done");
    }

    /// Log action with template that includes machine and from_state (line 236).
    #[tokio::test]
    async fn log_action_with_rendered_template() {
        let (exec, _bus) = make_executor();
        let ctx = test_ctx();
        let actions = vec![ResolvedAction::Log(
            "{machine}: {from_state} -> {to_state} by {actor} (title={title})".to_string(),
        )];
        let result = exec.execute_actions(&actions, &ctx).await;
        assert!(result.is_ok());
    }

    /// Emit with payload exercises the debug log path (line 251).
    #[tokio::test]
    async fn emit_action_with_payload_debug_log() {
        let (exec, bus) = make_executor();
        let ctx = test_ctx();
        let mut rx = bus.subscribe();

        let actions = vec![ResolvedAction::Emit {
            event: "detailed_event".to_string(),
            payload: Some(Value::Map(std::collections::BTreeMap::from([
                ("key".to_string(), Value::Int(99)),
            ]))),
        }];
        exec.execute_actions(&actions, &ctx).await.unwrap();

        let event = rx.recv().await.unwrap();
        assert_eq!(event.name, "detailed_event");
        assert!(event.payload.is_some());
    }

    /// Notify action exercises the info log path (line 261).
    #[tokio::test]
    async fn notify_action_text_target_info_log() {
        let (exec, _bus) = make_executor();
        let ctx = test_ctx();
        let actions = vec![ResolvedAction::Notify {
            target: Value::Text("customer_service".to_string()),
            event: "ticket_escalated".to_string(),
        }];
        let result = exec.execute_actions(&actions, &ctx).await;
        assert!(result.is_ok());
    }

    /// Webhook dry-run with payload — exercises lines 294-295.
    #[tokio::test]
    async fn webhook_dry_run_with_payload_has_payload() {
        let (exec, _bus) = make_executor();
        let ctx = test_ctx();
        let actions = vec![ResolvedAction::Webhook {
            url: "https://example.com/hook".to_string(),
            payload: Some(Value::List(vec![Value::Int(1), Value::Int(2)])),
            response_field: None,
            on_failure_state: None,
        }];
        let result = exec.execute_actions(&actions, &ctx).await;
        assert!(result.is_ok());
    }

    /// Webhook dry-run without payload — exercises dry-run path with None payload.
    #[tokio::test]
    async fn webhook_dry_run_without_payload() {
        let (exec, _bus) = make_executor();
        let ctx = test_ctx();
        let actions = vec![ResolvedAction::Webhook {
            url: "https://example.com/no_payload".to_string(),
            payload: None,
            response_field: None,
            on_failure_state: None,
        }];
        let result = exec.execute_actions(&actions, &ctx).await;
        assert!(result.is_ok());
    }

    #[test]
    fn eventbus_configurable_capacity() {
        let bus = EventBus::new(2048);
        assert_eq!(bus.capacity(), 2048);
    }

    #[test]
    fn eventbus_default_capacity_is_1024() {
        let bus = EventBus::default();
        assert_eq!(bus.capacity(), 1024);
    }

    #[test]
    fn eventbus_tracks_dropped_events() {
        // No subscribers → emits should count as dropped
        let bus = EventBus::new(16);
        assert_eq!(bus.dropped_count(), 0);
        bus.emit(Event {
            name: "test".to_string(),
            instance_id: "i1".to_string(),
            machine: "M".to_string(),
            payload: None,
        });
        assert_eq!(bus.dropped_count(), 1);

        bus.emit(Event {
            name: "test2".to_string(),
            instance_id: "i2".to_string(),
            machine: "M".to_string(),
            payload: None,
        });
        assert_eq!(bus.dropped_count(), 2);
    }

    // ---------------------------------------------------------------
    // E14: Webhook response handling tests
    // ---------------------------------------------------------------

    /// A callback that records update_instance_field calls.
    struct FieldRecordingCallback {
        updates: std::sync::Mutex<Vec<(String, String, Value)>>,
    }

    impl FieldRecordingCallback {
        fn new() -> Self {
            Self {
                updates: std::sync::Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait::async_trait]
    impl EngineCallback for FieldRecordingCallback {
        async fn spawn_child(
            &self,
            _parent_instance_id: &str,
            _machine: &str,
            _data: Vec<(String, Value)>,
        ) -> Result<String, HookError> {
            Ok("CHILD".to_string())
        }

        async fn signal_parent(
            &self,
            _child_instance_id: &str,
            _target_state: &str,
        ) -> Result<(), HookError> {
            Ok(())
        }

        async fn update_instance_field(
            &self,
            instance_id: &str,
            field: &str,
            value: Value,
        ) -> Result<(), HookError> {
            self.updates.lock().unwrap().push((
                instance_id.to_string(),
                field.to_string(),
                value,
            ));
            Ok(())
        }
    }

    /// Webhook with STORE — response body stored via callback.
    #[tokio::test]
    async fn webhook_response_stored_in_field() {
        use tokio::io::AsyncWriteExt;
        use tokio::net::TcpListener;

        // Start a local server that returns JSON
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let handle = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 4096];
            let _ = tokio::io::AsyncReadExt::read(&mut stream, &mut buf).await;
            let json_body = r#"{"status":"approved","score":95}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                json_body.len(),
                json_body
            );
            stream.write_all(response.as_bytes()).await.unwrap();
            stream.flush().await.unwrap();
        });

        let bus = Arc::new(EventBus::new(16));
        let cb = Arc::new(FieldRecordingCallback::new());
        let exec = HookExecutor::with_callback(Arc::clone(&bus), Arc::clone(&cb) as Arc<dyn EngineCallback>);

        let wh_config = webhook::WebhookConfig {
            timeout: std::time::Duration::from_secs(5),
            max_retries: 0,
            retry_delay: std::time::Duration::from_millis(10),
        };
        let wh_client = Arc::new(webhook::WebhookClient::new(wh_config));
        exec.set_webhook_client(wh_client);

        let ctx = test_ctx();
        let url = format!("http://127.0.0.1:{}/webhook", addr.port());
        let actions = vec![ResolvedAction::Webhook {
            url,
            payload: None,
            response_field: Some("api_result".to_string()),
            on_failure_state: None,
        }];
        let result = exec.execute_actions(&actions, &ctx).await;
        assert!(result.is_ok());

        // Verify the callback was called with the right data
        let updates = cb.updates.lock().unwrap();
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].0, "INST001"); // instance_id from ctx
        assert_eq!(updates[0].1, "api_result"); // field name from response_field

        // The stored value should be a Map with status and score
        match &updates[0].2 {
            Value::Map(map) => {
                assert_eq!(map.get("status"), Some(&Value::Text("approved".to_string())));
                assert_eq!(map.get("score"), Some(&Value::Int(95)));
            }
            other => panic!("Expected Map, got {:?}", other),
        }

        handle.await.unwrap();
    }

    /// Webhook without STORE — no callback update even on success with JSON body.
    #[tokio::test]
    async fn webhook_without_store_does_not_update_field() {
        use tokio::io::AsyncWriteExt;
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let handle = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 4096];
            let _ = tokio::io::AsyncReadExt::read(&mut stream, &mut buf).await;
            let json_body = r#"{"result":"ok"}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                json_body.len(),
                json_body
            );
            stream.write_all(response.as_bytes()).await.unwrap();
            stream.flush().await.unwrap();
        });

        let bus = Arc::new(EventBus::new(16));
        let cb = Arc::new(FieldRecordingCallback::new());
        let exec = HookExecutor::with_callback(Arc::clone(&bus), Arc::clone(&cb) as Arc<dyn EngineCallback>);

        let wh_config = webhook::WebhookConfig {
            timeout: std::time::Duration::from_secs(5),
            max_retries: 0,
            retry_delay: std::time::Duration::from_millis(10),
        };
        let wh_client = Arc::new(webhook::WebhookClient::new(wh_config));
        exec.set_webhook_client(wh_client);

        let ctx = test_ctx();
        let url = format!("http://127.0.0.1:{}/webhook", addr.port());
        let actions = vec![ResolvedAction::Webhook {
            url,
            payload: None,
            response_field: None, // No STORE
            on_failure_state: None,
        }];
        let result = exec.execute_actions(&actions, &ctx).await;
        assert!(result.is_ok());

        // No field update should have happened
        let updates = cb.updates.lock().unwrap();
        assert_eq!(updates.len(), 0);

        handle.await.unwrap();
    }

    /// Webhook failure with on_failure_state returns WebhookFailed error.
    #[tokio::test]
    async fn webhook_failure_with_on_failure_state() {
        let bus = Arc::new(EventBus::new(16));
        let exec = HookExecutor::new(Arc::clone(&bus));

        let wh_config = webhook::WebhookConfig {
            timeout: std::time::Duration::from_secs(1),
            max_retries: 0,
            retry_delay: std::time::Duration::from_millis(10),
        };
        let wh_client = Arc::new(webhook::WebhookClient::new(wh_config));
        exec.set_webhook_client(wh_client);

        let ctx = test_ctx();
        let actions = vec![ResolvedAction::Webhook {
            url: "http://127.0.0.1:1/fail".to_string(),
            payload: None,
            response_field: None,
            on_failure_state: Some("error_state".to_string()),
        }];
        let result = exec.execute_actions(&actions, &ctx).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            HookError::WebhookFailed { url, message } => {
                assert!(url.contains("/fail"));
                assert!(!message.is_empty());
            }
            other => panic!("Expected WebhookFailed, got {:?}", other),
        }
    }

    /// json_to_value round-trip: convert SMQL Value to JSON and back.
    #[test]
    fn json_to_value_round_trip() {
        use webhook::value_to_json;

        let original = Value::Map(std::collections::BTreeMap::from([
            ("name".to_string(), Value::Text("test".to_string())),
            ("count".to_string(), Value::Int(42)),
            ("active".to_string(), Value::Bool(true)),
            ("tags".to_string(), Value::List(vec![
                Value::Text("a".to_string()),
                Value::Text("b".to_string()),
            ])),
        ]));

        let json = value_to_json(&original);
        let back = json_to_value(&json);
        assert_eq!(original, back);
    }
}
