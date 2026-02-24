/// Tests for the EXPLAIN TRANSITIONS feature.
///
/// Covers schema-level and instance-level explain queries, guard evaluation,
/// actor context, terminal-state behaviour, and requires_data extraction.
use std::sync::Arc;

use smql_ast::query::{ExplainTransitionsQuery, Query};
use smql_ast::value::Value;
use smql_catalog::MachineCatalog;
use smql_engine_core::query::QueryResult;
use smql_engine_core::Engine;
use smql_hooks::{EventBus, HookExecutor};
use smql_storage::MemoryStorage;
use smql_timer::TimerManager;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Ticket machine definition used across all tests.
///
/// States: open -> in_progress -> resolved -> closed
/// Guards:
///   open -> in_progress  : assignee IS SET
///   in_progress -> resolved : resolution IS SET
///   resolved -> closed   : (no guard)
///   ANY -> closed        : (no guard, escape hatch)
const TICKET_SMQL: &str = r#"
DEFINE MACHINE Ticket (
    DATA {
        title      : TEXT -> REQUIRED
        assignee   : TEXT -> OPTIONAL
        resolution : TEXT -> OPTIONAL
    }
    STATES { open, in_progress, resolved, closed }
    INITIAL STATE open
    TERMINAL STATES { closed }
    TRANSITIONS {
        open -> in_progress {
            GUARD : assignee IS SET
        }
        in_progress -> resolved {
            GUARD : resolution IS SET
        }
        resolved -> closed {}
        ANY -> closed {
            EXCEPT FROM { closed }
        }
    }
)
"#;

fn make_engine() -> Engine {
    let machines = smql_parser::parse_machines(TICKET_SMQL).expect("parse Ticket machine");
    let catalog = Arc::new(MachineCatalog::new());
    for m in machines {
        catalog.register(m).expect("register machine");
    }
    let storage = Arc::new(MemoryStorage::new());
    let timer = Arc::new(TimerManager::new());
    let event_bus = Arc::new(EventBus::new(64));
    let hooks = Arc::new(HookExecutor::new(event_bus));
    let engine = Engine::with_hooks(catalog, storage, timer, hooks);
    engine.wire_callback();
    engine
}

fn lit(v: Value) -> smql_ast::expression::Expression {
    smql_ast::expression::Expression::new(smql_ast::expression::ExpressionKind::Literal(v))
}

/// Spawn a Ticket instance and return its ID string.
async fn spawn_ticket(engine: &Engine, title: &str, assignee: Option<&str>, resolution: Option<&str>) -> String {
    let mut data = vec![
        ("title".to_string(), lit(Value::Text(title.into()))),
    ];
    if let Some(a) = assignee {
        data.push(("assignee".to_string(), lit(Value::Text(a.into()))));
    }
    if let Some(r) = resolution {
        data.push(("resolution".to_string(), lit(Value::Text(r.into()))));
    }

    let cmd = smql_ast::command::SpawnCommand {
        machine: "Ticket".to_string(),
        data,
        then_transition: None,
        batch: false,
        batch_data: Vec::new(),
        parent_id: None,
        parent_machine: None,
        as_actor: None,
        idempotency_key: None,
        tags: Vec::new(),
        ttl: None,
    };
    engine.spawn(&cmd).await.expect("spawn ticket").instance.id.as_str()
}

/// Convenience: execute EXPLAIN TRANSITIONS FOR Ticket [instance_id] [AS actor].
async fn explain(
    engine: &Engine,
    instance_id: Option<&str>,
    as_actor: Option<&str>,
) -> smql_engine_core::query::ExplainTransitionsResult {
    let query = Query::ExplainTransitions(ExplainTransitionsQuery {
        machine: "Ticket".to_string(),
        instance_id: instance_id.map(|s| s.to_string()),
        as_actor: as_actor.map(|s| s.to_string()),
    });
    match engine.execute_query(&query).await.expect("execute_query") {
        QueryResult::ExplainTransitions(r) => r,
        other => panic!("Expected ExplainTransitions result, got {:?}", other),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Schema-level explain returns all transitions defined on the machine
/// without performing any guard evaluation.
#[tokio::test]
async fn explain_transitions_schema_level() {
    let engine = make_engine();
    let result = explain(&engine, None, None).await;

    assert_eq!(result.machine, "Ticket");
    assert!(result.current_state.is_none(), "No current_state for schema-level");
    assert!(result.instance_id.is_none(), "No instance_id for schema-level");

    // The Ticket machine has 4 transitions in its definition
    assert_eq!(result.available.len(), 4, "Expected 4 transitions, got {:?}", result.available.iter().map(|t| format!("{} -> {}", t.from_state, t.to_state)).collect::<Vec<_>>());

    // At schema level, guards_met is always false (unknown without instance context)
    for t in &result.available {
        assert!(!t.guards_met, "guards_met should be false at schema level for {} -> {}", t.from_state, t.to_state);
    }

    // Verify the set of transitions present by to_state/from_state patterns
    let has_open_to_in_progress = result
        .available
        .iter()
        .any(|t| t.from_state == "open" && t.to_state == "in_progress");
    assert!(has_open_to_in_progress, "Missing open -> in_progress");

    let has_in_progress_to_resolved = result
        .available
        .iter()
        .any(|t| t.from_state == "in_progress" && t.to_state == "resolved");
    assert!(has_in_progress_to_resolved, "Missing in_progress -> resolved");

    let has_resolved_to_closed = result
        .available
        .iter()
        .any(|t| t.from_state == "resolved" && t.to_state == "closed");
    assert!(has_resolved_to_closed, "Missing resolved -> closed");

    // The ANY -> closed transition uses TransitionSource::Any which serializes as
    // "ANY" or "ANY EXCEPT FROM {...}" depending on whether except list is empty.
    let has_any_to_closed = result
        .available
        .iter()
        .any(|t| t.from_state.starts_with("ANY") && t.to_state == "closed");
    assert!(has_any_to_closed, "Missing ANY -> closed");
}

/// Instance-level explain where the guard IS met (assignee is provided).
/// The `open -> in_progress` transition must have guards_met=true.
#[tokio::test]
async fn explain_transitions_instance_guards_met() {
    let engine = make_engine();
    // Spawn with assignee set — the `assignee IS SET` guard should pass
    let id = spawn_ticket(&engine, "Bug report", Some("alice"), None).await;

    let result = explain(&engine, Some(&id), None).await;

    assert_eq!(result.machine, "Ticket");
    assert_eq!(result.current_state.as_deref(), Some("open"));
    assert_eq!(result.instance_id.as_deref(), Some(id.as_str()));

    // From `open`, there are two applicable transitions:
    //   open -> in_progress  (guard: assignee IS SET)
    //   ANY  -> closed       (no guard, except from closed)
    let open_to_in_progress = result
        .available
        .iter()
        .find(|t| t.from_state == "open" && t.to_state == "in_progress")
        .expect("open -> in_progress transition must be present");

    assert!(
        open_to_in_progress.guards_met,
        "guards_met should be true when assignee IS SET: {:?}",
        open_to_in_progress.blocking_guards
    );
    assert!(
        open_to_in_progress.blocking_guards.is_empty(),
        "No blocking guards expected when guard passes"
    );

    // The ANY -> closed transition has no guards, so guards_met=true
    let any_to_closed = result
        .available
        .iter()
        .find(|t| t.to_state == "closed")
        .expect("ANY -> closed transition must be present");
    assert!(any_to_closed.guards_met, "ANY -> closed has no guards, must be met");
}

/// Instance-level explain where the guard is NOT met (no assignee).
/// The `open -> in_progress` transition must have guards_met=false and
/// blocking_guards populated.
#[tokio::test]
async fn explain_transitions_instance_guards_blocked() {
    let engine = make_engine();
    // Spawn without assignee — the `assignee IS SET` guard should fail
    let id = spawn_ticket(&engine, "Bug report", None, None).await;

    let result = explain(&engine, Some(&id), None).await;

    assert_eq!(result.current_state.as_deref(), Some("open"));

    let open_to_in_progress = result
        .available
        .iter()
        .find(|t| t.from_state == "open" && t.to_state == "in_progress")
        .expect("open -> in_progress must be present");

    assert!(
        !open_to_in_progress.guards_met,
        "guards_met should be false when assignee is not set"
    );
    assert!(
        !open_to_in_progress.blocking_guards.is_empty(),
        "blocking_guards must be non-empty when guard fails"
    );

    // The blocking guard should reference the assignee IS SET expression
    let blocker = &open_to_in_progress.blocking_guards[0];
    assert!(
        blocker.guard_expr.contains("assignee"),
        "Blocking guard expr should mention 'assignee', got: {}",
        blocker.guard_expr
    );

    // recovery_options should be populated since the guard is blocked
    assert!(
        !open_to_in_progress.recovery_options.is_empty(),
        "Recovery options should be populated for a blocked guard"
    );
}

/// Instance-level explain with an AS actor context.
/// Guard evaluation context includes the actor; the result must include
/// the actor's influence (no crash, correct machine/state).
#[tokio::test]
async fn explain_transitions_instance_with_actor() {
    let engine = make_engine();
    let id = spawn_ticket(&engine, "Feature request", Some("bob"), None).await;

    let result = explain(&engine, Some(&id), Some("admin")).await;

    assert_eq!(result.machine, "Ticket");
    assert_eq!(result.current_state.as_deref(), Some("open"));
    assert_eq!(result.instance_id.as_deref(), Some(id.as_str()));

    // The open -> in_progress guard (assignee IS SET) should still pass
    // because the instance data has assignee set, regardless of actor
    let open_to_in_progress = result
        .available
        .iter()
        .find(|t| t.from_state == "open" && t.to_state == "in_progress")
        .expect("open -> in_progress must be present");

    assert!(
        open_to_in_progress.guards_met,
        "Guard should pass (assignee is set) even with actor context"
    );
}

/// When an instance is in a terminal state, no outgoing transitions apply
/// (or the only one is the ANY -> closed which is excepted from closed itself).
/// The available list must be empty because closed is in the EXCEPT FROM list.
#[tokio::test]
async fn explain_transitions_terminal_state() {
    let engine = make_engine();
    // Spawn with assignee set so we can transition to in_progress
    let id = spawn_ticket(&engine, "Done ticket", Some("alice"), Some("Fixed it")).await;

    // Drive the ticket all the way to closed via: open -> in_progress -> resolved -> closed
    let t = |to: &str| smql_ast::command::TransitionCommand::new(
        "Ticket".to_string(),
        id.clone(),
        to.to_string(),
    );
    engine.transition(&t("in_progress")).await.expect("open -> in_progress");
    engine.transition(&t("resolved")).await.expect("in_progress -> resolved");
    engine.transition(&t("closed")).await.expect("resolved -> closed");

    let result = explain(&engine, Some(&id), None).await;

    assert_eq!(result.current_state.as_deref(), Some("closed"));

    // `closed` is terminal and is listed in the EXCEPT FROM of the ANY -> closed
    // transition, so no transitions should be applicable.
    assert!(
        result.available.is_empty(),
        "No transitions should be available from a terminal state, got: {:?}",
        result.available.iter().map(|t| format!("{} -> {}", t.from_state, t.to_state)).collect::<Vec<_>>()
    );
}

// ---------------------------------------------------------------------------
// Recovery option tests (AST-based generate_recovery_options_from_ast)
// ---------------------------------------------------------------------------

/// Attempt a transition blocked by an `assignee IS SET` guard without providing
/// assignee.  The engine must return TransitionDenied with a SetField recovery
/// option that names "assignee" and whose example contains "WITH".
#[tokio::test]
async fn recovery_options_is_set_guard() {
    use smql_ast::{RecoveryAction, SmqlError};

    let engine = make_engine();
    // Spawn without assignee so the IS SET guard will fail.
    let id = spawn_ticket(&engine, "IS SET test", None, None).await;

    let cmd = smql_ast::command::TransitionCommand::new(
        "Ticket".to_string(),
        id.clone(),
        "in_progress".to_string(),
    );

    match engine.transition(&cmd).await {
        Err(SmqlError::TransitionDenied(denied)) => {
            assert!(
                !denied.recovery_options.is_empty(),
                "Expected at least one recovery option for IS SET guard failure"
            );

            let opt = denied
                .recovery_options
                .iter()
                .find(|o| o.action == RecoveryAction::SetField && o.field.as_deref() == Some("assignee"))
                .expect("Expected a SetField recovery option for field 'assignee'");

            assert_eq!(
                opt.action,
                RecoveryAction::SetField,
                "Recovery action must be SetField"
            );
            assert_eq!(
                opt.field.as_deref(),
                Some("assignee"),
                "Recovery option field must be 'assignee'"
            );

            // The example should show how to supply the field via WITH clause.
            let example = opt.example.as_deref().unwrap_or("");
            assert!(
                example.contains("WITH"),
                "Example should contain 'WITH' to show how to set the field, got: {:?}",
                example
            );
        }
        other => panic!("Expected SmqlError::TransitionDenied, got {:?}", other),
    }
}

/// Attempt a transition blocked by a comparison guard `priority > 0` when the
/// instance was spawned with priority = 0.  The engine must return
/// TransitionDenied with a SetField recovery option for "priority" whose
/// suggested_value encodes the "> 0" constraint.
#[tokio::test]
async fn recovery_options_comparison_guard() {
    use smql_ast::{RecoveryAction, SmqlError};

    // Define a minimal machine with a numeric comparison guard.
    let smql = r#"
DEFINE MACHINE PriorityTask (
    DATA {
        title    : TEXT -> REQUIRED
        priority : INT  -> REQUIRED
    }
    STATES { pending, active }
    INITIAL STATE pending
    TERMINAL STATES { active }
    TRANSITIONS {
        pending -> active {
            GUARD : priority > 0
        }
    }
)
"#;

    let machines = smql_parser::parse_machines(smql).expect("parse PriorityTask");
    let catalog = std::sync::Arc::new(smql_catalog::MachineCatalog::new());
    for m in machines {
        catalog.register(m).expect("register PriorityTask");
    }
    let storage = std::sync::Arc::new(smql_storage::MemoryStorage::new());
    let timer = std::sync::Arc::new(smql_timer::TimerManager::new());
    let event_bus = std::sync::Arc::new(smql_hooks::EventBus::new(64));
    let hooks = std::sync::Arc::new(smql_hooks::HookExecutor::new(event_bus));
    let engine = smql_engine_core::Engine::with_hooks(catalog, storage, timer, hooks);
    engine.wire_callback();

    // Spawn with priority = 0 so the `priority > 0` guard will fail.
    let spawn_cmd = smql_ast::command::SpawnCommand {
        machine: "PriorityTask".to_string(),
        data: vec![
            ("title".to_string(), lit(smql_ast::value::Value::Text("Low priority".into()))),
            ("priority".to_string(), lit(smql_ast::value::Value::Int(0))),
        ],
        then_transition: None,
        batch: false,
        batch_data: Vec::new(),
        parent_id: None,
        parent_machine: None,
        as_actor: None,
        idempotency_key: None,
        tags: Vec::new(),
        ttl: None,
    };
    let id = engine.spawn(&spawn_cmd).await.expect("spawn PriorityTask").instance.id;

    let cmd = smql_ast::command::TransitionCommand::new(
        "PriorityTask".to_string(),
        id.as_str(),
        "active".to_string(),
    );

    match engine.transition(&cmd).await {
        Err(SmqlError::TransitionDenied(denied)) => {
            assert!(
                !denied.recovery_options.is_empty(),
                "Expected recovery options for comparison guard failure"
            );

            let opt = denied
                .recovery_options
                .iter()
                .find(|o| o.action == RecoveryAction::SetField && o.field.as_deref() == Some("priority"))
                .expect("Expected a SetField recovery option for field 'priority'");

            assert_eq!(
                opt.action,
                RecoveryAction::SetField,
                "Recovery action must be SetField"
            );
            assert_eq!(
                opt.field.as_deref(),
                Some("priority"),
                "Recovery option field must be 'priority'"
            );

            // suggested_value should encode the "> 0" constraint.
            let sv = opt.suggested_value.as_deref().unwrap_or("");
            assert!(
                sv.contains("> 0"),
                "suggested_value should describe the '>  0' constraint, got: {:?}",
                sv
            );
        }
        other => panic!("Expected SmqlError::TransitionDenied, got {:?}", other),
    }
}

/// Attempt a transition blocked by an `ACTOR.role == "admin"` guard when the
/// transition is issued without an actor (or with a non-admin actor).  The
/// engine must return TransitionDenied with a ChangeActor recovery option whose
/// suggested_value is "admin".
#[tokio::test]
async fn recovery_options_actor_role_guard() {
    use smql_ast::{RecoveryAction, SmqlError};

    // Define a machine whose only transition requires an admin actor.
    let smql = r#"
DEFINE MACHINE AdminGate (
    DATA {
        title : TEXT -> REQUIRED
    }
    STATES { open, closed }
    INITIAL STATE open
    TERMINAL STATES { closed }
    TRANSITIONS {
        open -> closed {
            GUARD : ACTOR.role == "admin"
        }
    }
)
"#;

    let machines = smql_parser::parse_machines(smql).expect("parse AdminGate");
    let catalog = std::sync::Arc::new(smql_catalog::MachineCatalog::new());
    for m in machines {
        catalog.register(m).expect("register AdminGate");
    }
    let storage = std::sync::Arc::new(smql_storage::MemoryStorage::new());
    let timer = std::sync::Arc::new(smql_timer::TimerManager::new());
    let event_bus = std::sync::Arc::new(smql_hooks::EventBus::new(64));
    let hooks = std::sync::Arc::new(smql_hooks::HookExecutor::new(event_bus));
    let engine = smql_engine_core::Engine::with_hooks(catalog, storage, timer, hooks);
    engine.wire_callback();

    let spawn_cmd = smql_ast::command::SpawnCommand {
        machine: "AdminGate".to_string(),
        data: vec![
            ("title".to_string(), lit(smql_ast::value::Value::Text("Gate item".into()))),
        ],
        then_transition: None,
        batch: false,
        batch_data: Vec::new(),
        parent_id: None,
        parent_machine: None,
        as_actor: None,
        idempotency_key: None,
        tags: Vec::new(),
        ttl: None,
    };
    let id = engine.spawn(&spawn_cmd).await.expect("spawn AdminGate").instance.id;

    // Transition as "non-admin" — as_actor sets the actor id only, not role,
    // so ACTOR.role evaluates to Null and the guard fails.
    let mut cmd = smql_ast::command::TransitionCommand::new(
        "AdminGate".to_string(),
        id.as_str(),
        "closed".to_string(),
    );
    cmd.as_actor = Some("non-admin".to_string());

    match engine.transition(&cmd).await {
        Err(SmqlError::TransitionDenied(denied)) => {
            assert!(
                !denied.recovery_options.is_empty(),
                "Expected recovery options for ACTOR.role guard failure"
            );

            let opt = denied
                .recovery_options
                .iter()
                .find(|o| o.action == RecoveryAction::ChangeActor)
                .expect("Expected a ChangeActor recovery option for ACTOR.role guard");

            assert_eq!(
                opt.action,
                RecoveryAction::ChangeActor,
                "Recovery action must be ChangeActor"
            );

            // The suggested_value must identify the required role.
            let sv = opt.suggested_value.as_deref().unwrap_or("");
            assert_eq!(
                sv, "admin",
                "suggested_value must be 'admin' (the required role), got: {:?}",
                sv
            );
        }
        other => panic!("Expected SmqlError::TransitionDenied, got {:?}", other),
    }
}

/// Attempt a transition blocked by a compound `assignee IS SET AND resolution IS SET`
/// guard when neither field is present.  The AST walker must recurse through the
/// AND node and produce TWO SetField recovery options — one per field.
#[tokio::test]
async fn recovery_options_compound_and_guard() {
    use smql_ast::{RecoveryAction, SmqlError};

    // Define a machine with a compound AND guard.
    let smql = r#"
DEFINE MACHINE CompoundGuard (
    DATA {
        assignee   : TEXT -> OPTIONAL
        resolution : TEXT -> OPTIONAL
    }
    STATES { open, closed }
    INITIAL STATE open
    TERMINAL STATES { closed }
    TRANSITIONS {
        open -> closed {
            GUARD : assignee IS SET AND resolution IS SET
        }
    }
)
"#;

    let machines = smql_parser::parse_machines(smql).expect("parse CompoundGuard");
    let catalog = std::sync::Arc::new(smql_catalog::MachineCatalog::new());
    for m in machines {
        catalog.register(m).expect("register CompoundGuard");
    }
    let storage = std::sync::Arc::new(smql_storage::MemoryStorage::new());
    let timer = std::sync::Arc::new(smql_timer::TimerManager::new());
    let event_bus = std::sync::Arc::new(smql_hooks::EventBus::new(64));
    let hooks = std::sync::Arc::new(smql_hooks::HookExecutor::new(event_bus));
    let engine = smql_engine_core::Engine::with_hooks(catalog, storage, timer, hooks);
    engine.wire_callback();

    // Spawn without either field set so both halves of the AND guard fail.
    let spawn_cmd = smql_ast::command::SpawnCommand {
        machine: "CompoundGuard".to_string(),
        data: vec![],
        then_transition: None,
        batch: false,
        batch_data: Vec::new(),
        parent_id: None,
        parent_machine: None,
        as_actor: None,
        idempotency_key: None,
        tags: Vec::new(),
        ttl: None,
    };
    let id = engine.spawn(&spawn_cmd).await.expect("spawn CompoundGuard").instance.id;

    let cmd = smql_ast::command::TransitionCommand::new(
        "CompoundGuard".to_string(),
        id.as_str(),
        "closed".to_string(),
    );

    match engine.transition(&cmd).await {
        Err(SmqlError::TransitionDenied(denied)) => {
            // Both sides of the AND guard must produce recovery options.
            let set_field_opts: Vec<_> = denied
                .recovery_options
                .iter()
                .filter(|o| o.action == RecoveryAction::SetField)
                .collect();

            assert!(
                set_field_opts.len() >= 2,
                "Expected at least 2 SetField recovery options for compound AND guard, got: {:?}",
                set_field_opts.iter().map(|o| o.field.as_deref()).collect::<Vec<_>>()
            );

            let fields: Vec<&str> = set_field_opts
                .iter()
                .filter_map(|o| o.field.as_deref())
                .collect();

            assert!(
                fields.contains(&"assignee"),
                "Recovery options must include a SetField for 'assignee', got: {:?}",
                fields
            );
            assert!(
                fields.contains(&"resolution"),
                "Recovery options must include a SetField for 'resolution', got: {:?}",
                fields
            );
        }
        other => panic!("Expected SmqlError::TransitionDenied, got {:?}", other),
    }
}

/// Verify that requires_data is correctly populated from guard expressions.
/// The `open -> in_progress` guard is `assignee IS SET`, so `requires_data`
/// must contain `"assignee"`.
/// The `in_progress -> resolved` guard is `resolution IS SET`, so it must
/// contain `"resolution"`.
#[tokio::test]
async fn explain_transitions_extracts_field_refs() {
    let engine = make_engine();
    // Schema-level: requires_data is extracted regardless of instance
    let result = explain(&engine, None, None).await;

    let open_to_in_progress = result
        .available
        .iter()
        .find(|t| t.from_state == "open" && t.to_state == "in_progress")
        .expect("open -> in_progress must be present");

    assert!(
        open_to_in_progress.requires_data.contains(&"assignee".to_string()),
        "requires_data must include 'assignee' for guard 'assignee IS SET', got: {:?}",
        open_to_in_progress.requires_data
    );

    let in_progress_to_resolved = result
        .available
        .iter()
        .find(|t| t.from_state == "in_progress" && t.to_state == "resolved")
        .expect("in_progress -> resolved must be present");

    assert!(
        in_progress_to_resolved.requires_data.contains(&"resolution".to_string()),
        "requires_data must include 'resolution' for guard 'resolution IS SET', got: {:?}",
        in_progress_to_resolved.requires_data
    );

    // Transitions with no guards should have empty requires_data
    let resolved_to_closed = result
        .available
        .iter()
        .find(|t| t.from_state == "resolved" && t.to_state == "closed")
        .expect("resolved -> closed must be present");

    assert!(
        resolved_to_closed.requires_data.is_empty(),
        "No requires_data for guard-free transition, got: {:?}",
        resolved_to_closed.requires_data
    );
}
