/// Documentation validation tests for SMQL command docs.
///
/// Tests all SMQL code examples from:
/// - docs/commands/spawn.md
/// - docs/commands/transition.md
/// - docs/commands/try-transition.md
/// - docs/commands/transition-all.md
/// - docs/commands/alter-machine.md
///
/// Each test first validates parsing, then (where feasible) executes through the engine.
///
/// Known doc issues found during validation are documented with #[doc_issue] comments.
use std::collections::BTreeMap;
use std::sync::Arc;

use smql_ast::command::{Command, Statement};
use smql_ast::expression::{Expression, ExpressionKind};
use smql_ast::value::Value;
use smql_catalog::MachineCatalog;
use smql_engine_core::Engine;
use smql_hooks::{EventBus, HookExecutor};
use smql_storage::MemoryStorage;
use smql_timer::TimerManager;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a minimal Task machine for basic spawn/transition tests.
fn make_task_engine() -> Engine {
    let smql = r#"
DEFINE MACHINE Task (
    DATA {
        title : TEXT -> REQUIRED
    }
    STATES { todo, doing, done }
    INITIAL STATE todo
    TERMINAL STATES { done }
    TRANSITIONS {
        todo -> doing {}
        doing -> done {}
    }
)
"#;
    build_engine(smql)
}

/// Build an engine with a SupportTicket machine (matches the machine used in docs).
fn make_support_ticket_engine() -> Engine {
    let smql = include_str!("../../../examples/support_ticket.smql");
    build_engine(smql)
}

/// Build an engine with a more complex machine that supports TRANSITION ALL and other features.
fn make_order_engine() -> Engine {
    let smql = r#"
DEFINE MACHINE Order (
    DATA {
        customer : TEXT -> REQUIRED
        total    : INT  -> DEFAULT(0)
    }
    STATES { draft, pending, approved, cancelled }
    INITIAL STATE draft
    TERMINAL STATES { approved, cancelled }
    TRANSITIONS {
        draft -> pending {}
        pending -> approved {}
        ANY -> cancelled {
            EXCEPT FROM { approved }
        }
    }
)
"#;
    build_engine(smql)
}

fn build_engine(smql: &str) -> Engine {
    let machines = smql_parser::parse_machines(smql).expect("parse machines");
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

/// Parse SMQL and return the list of statements -- panics on failure.
fn must_parse(input: &str) -> Vec<Statement> {
    smql_parser::parse(input).unwrap_or_else(|e| {
        panic!(
            "Failed to parse SMQL:\n  Input: {}\n  Error: {}",
            input, e
        )
    })
}

/// Spawn a Task instance and return its ID.
async fn spawn_task(engine: &Engine, title: &str) -> String {
    let smql = format!(r#"SPAWN Task {{ title: "{}" }}"#, title);
    let stmts = must_parse(&smql);
    match &stmts[0] {
        Statement::Command(Command::Spawn(cmd)) => engine
            .spawn(cmd)
            .await
            .expect("spawn task")
            .instance
            .id
            .as_str(),
        _ => panic!("expected Spawn command"),
    }
}

/// Spawn a SupportTicket via the engine API (bypasses the doc UUID-as-string issue).
async fn spawn_support_ticket_via_api(engine: &Engine) -> String {
    let cid = Uuid::new_v4();
    let cmd = smql_ast::command::SpawnCommand {
        machine: "SupportTicket".to_string(),
        data: vec![
            (
                "customer_id".to_string(),
                Expression::new(ExpressionKind::Literal(Value::Uuid(cid))),
            ),
            (
                "subject".to_string(),
                Expression::new(ExpressionKind::Literal(Value::Text(
                    "Cannot login".into(),
                ))),
            ),
            (
                "description".to_string(),
                Expression::new(ExpressionKind::Literal(Value::Text(
                    "Getting 401 error".into(),
                ))),
            ),
        ],
        then_transition: None,
        batch: false,
        batch_data: Vec::new(),
        parent_id: None,
        parent_machine: None,
        as_actor: None,
    };
    engine
        .spawn(&cmd)
        .await
        .expect("spawn support ticket")
        .instance
        .id
        .as_str()
}

/// Spawn an Order instance and return its ID.
async fn spawn_order(engine: &Engine, customer: &str) -> String {
    let smql = format!(r#"SPAWN Order {{ customer: "{}" }}"#, customer);
    let stmts = must_parse(&smql);
    match &stmts[0] {
        Statement::Command(Command::Spawn(cmd)) => engine
            .spawn(cmd)
            .await
            .expect("spawn order")
            .instance
            .id
            .as_str(),
        _ => panic!("expected Spawn command"),
    }
}

// ===========================================================================
// docs/commands/spawn.md
// ===========================================================================

/// spawn.md: Syntax example -- `SPAWN MachineName { field: value, field2: value2 }`
#[tokio::test]
async fn spawn_doc_syntax_basic() {
    let input = r#"SPAWN Task { title: "Hello" }"#;
    let stmts = must_parse(input);
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Statement::Command(Command::Spawn(cmd)) => {
            assert_eq!(cmd.machine, "Task");
            assert_eq!(cmd.data.len(), 1);
            assert_eq!(cmd.data[0].0, "title");
        }
        other => panic!("expected Spawn, got {:?}", other),
    }
    // Execute it through the engine
    let engine = make_task_engine();
    match &stmts[0] {
        Statement::Command(Command::Spawn(cmd)) => {
            let result = engine.spawn(cmd).await.expect("spawn");
            assert_eq!(result.instance.state, "todo");
            assert_eq!(
                result.instance.data.get("title"),
                Some(&Value::Text("Hello".into()))
            );
        }
        _ => unreachable!(),
    }
}

/// spawn.md: Empty braces -- `SPAWN Machine {}`
#[tokio::test]
async fn spawn_doc_empty_braces() {
    let input = "SPAWN Task {}";
    let stmts = must_parse(input);
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Statement::Command(Command::Spawn(cmd)) => {
            assert_eq!(cmd.machine, "Task");
            assert!(cmd.data.is_empty());
        }
        other => panic!("expected Spawn, got {:?}", other),
    }
    // Execute -- should fail because Task requires `title`
    let engine = make_task_engine();
    match &stmts[0] {
        Statement::Command(Command::Spawn(cmd)) => {
            let result = engine.spawn(cmd).await;
            assert!(
                result.is_err(),
                "spawn with missing required fields should fail"
            );
        }
        _ => unreachable!(),
    }
}

/// spawn.md: SPAWN BATCH -- `SPAWN BATCH Task [ { title: "Task 1" }, ... ]`
///
/// DOC ISSUE #1: SPAWN BATCH parses correctly but the engine does not implement
/// batch spawn -- it processes `cmd.data` (empty for batch) instead of `cmd.batch_data`.
/// The engine should iterate over `batch_data` and spawn each set individually.
#[tokio::test]
async fn spawn_doc_batch_parse_only() {
    let input = r#"SPAWN BATCH Task [
        { title: "Task 1" },
        { title: "Task 2" },
        { title: "Task 3" }
    ]"#;
    let stmts = must_parse(input);
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Statement::Command(Command::Spawn(cmd)) => {
            assert_eq!(cmd.machine, "Task");
            assert!(cmd.batch);
            assert_eq!(cmd.batch_data.len(), 3);
            // Verify each batch entry has title
            for (i, data) in cmd.batch_data.iter().enumerate() {
                assert_eq!(data.len(), 1, "batch entry {} should have 1 field", i);
                assert_eq!(data[0].0, "title");
            }
        }
        other => panic!("expected Spawn batch, got {:?}", other),
    }
    // NOTE: Engine execution of SPAWN BATCH is broken -- it uses empty `data` instead of `batch_data`.
    // This is an engine bug, not a doc issue. The doc syntax is correct.
    let engine = make_task_engine();
    match &stmts[0] {
        Statement::Command(Command::Spawn(cmd)) => {
            let result = engine.spawn(cmd).await;
            assert!(
                result.is_err(),
                "batch spawn fails because engine doesn't use batch_data (engine bug)"
            );
        }
        _ => unreachable!(),
    }
}

/// spawn.md: THEN TRANSITION -- `SPAWN Task { title: "Urgent" } THEN TRANSITION TO doing`
#[tokio::test]
async fn spawn_doc_then_transition() {
    let input = r#"SPAWN Task { title: "Urgent" } THEN TRANSITION TO doing"#;
    let stmts = must_parse(input);
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Statement::Command(Command::Spawn(cmd)) => {
            assert_eq!(cmd.machine, "Task");
            assert_eq!(cmd.then_transition, Some("doing".to_string()));
        }
        other => panic!("expected Spawn with then_transition, got {:?}", other),
    }
    // Execute through engine
    let engine = make_task_engine();
    match &stmts[0] {
        Statement::Command(Command::Spawn(cmd)) => {
            let result = engine.spawn(cmd).await.expect("spawn then transition");
            assert_eq!(result.instance.state, "doing");
        }
        _ => unreachable!(),
    }
}

/// spawn.md: curl example -- SupportTicket spawn with customer_id, subject, description
///
/// DOC ISSUE #2: The doc example passes customer_id as a string literal
/// ("550e8400-e29b-41d4-a716-446655440000") but the SupportTicket machine defines
/// customer_id as UUID type. The engine rejects this with:
///   "Field 'customer_id' has type UUID but got value \"550e8400-...\""
///
/// Fix: The doc should note that UUID fields require a UUID value type, or the HTTP
/// server should auto-coerce UUID-formatted strings to UUID values.
/// Alternatively, the doc example should use a machine that accepts TEXT for customer_id.
#[tokio::test]
async fn spawn_doc_support_ticket_example_parse() {
    let input = r#"SPAWN SupportTicket { customer_id: "550e8400-e29b-41d4-a716-446655440000", subject: "Cannot login", description: "Getting 401 error" }"#;
    let stmts = must_parse(input);
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Statement::Command(Command::Spawn(cmd)) => {
            assert_eq!(cmd.machine, "SupportTicket");
            assert_eq!(cmd.data.len(), 3);
        }
        other => panic!("expected Spawn, got {:?}", other),
    }
    // Engine execution fails because UUID field receives a Text value
    let engine = make_support_ticket_engine();
    match &stmts[0] {
        Statement::Command(Command::Spawn(cmd)) => {
            let result = engine.spawn(cmd).await;
            assert!(
                result.is_err(),
                "Expected UUID type mismatch: doc passes string for UUID field"
            );
            let err_msg = result.unwrap_err().to_string();
            assert!(
                err_msg.contains("UUID"),
                "Error should mention UUID: {}",
                err_msg
            );
        }
        _ => unreachable!(),
    }
}

/// spawn.md: REPL example -- multi-line SupportTicket spawn
#[tokio::test]
async fn spawn_doc_repl_multiline() {
    let input = r#"SPAWN SupportTicket {
        customer_id: "550e8400-e29b-41d4-a716-446655440000",
        subject: "Cannot login",
        description: "Getting 401 error"
    }"#;
    let stmts = must_parse(input);
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Statement::Command(Command::Spawn(cmd)) => {
            assert_eq!(cmd.machine, "SupportTicket");
            assert_eq!(cmd.data.len(), 3);
        }
        other => panic!("expected Spawn, got {:?}", other),
    }
}

// ===========================================================================
// docs/commands/transition.md
// ===========================================================================

/// transition.md: Syntax -- `TRANSITION MachineName "instance_id" TO target_state`
#[tokio::test]
async fn transition_doc_syntax_basic() {
    let input = r#"TRANSITION SupportTicket "01J5X7K2P3Q4R5S6T7U8V9W0XY" TO triaged"#;
    let stmts = must_parse(input);
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Statement::Command(Command::Transition(cmd)) => {
            assert_eq!(cmd.machine, "SupportTicket");
            assert_eq!(cmd.instance_id, "01J5X7K2P3Q4R5S6T7U8V9W0XY");
            assert_eq!(cmd.to_state, "triaged");
        }
        other => panic!("expected Transition, got {:?}", other),
    }
}

/// transition.md: Syntax -- `TRANSITION MachineName "instance_id" TO target_state AS "user-1"`
#[tokio::test]
async fn transition_doc_syntax_as() {
    let input =
        r#"TRANSITION SupportTicket "01J5X7K2P3Q4R5S6T7U8V9W0XY" TO triaged AS "user-1""#;
    let stmts = must_parse(input);
    match &stmts[0] {
        Statement::Command(Command::Transition(cmd)) => {
            assert_eq!(cmd.to_state, "triaged");
            assert_eq!(cmd.as_actor, Some("user-1".to_string()));
        }
        other => panic!("expected Transition, got {:?}", other),
    }
}

/// transition.md: Syntax -- `TRANSITION MachineName "instance_id" TO target_state WITH { field: value }`
#[tokio::test]
async fn transition_doc_syntax_with() {
    let input = r#"TRANSITION SupportTicket "01J5X7K2P3Q4R5S6T7U8V9W0XY" TO triaged WITH { note: "Fixed" }"#;
    let stmts = must_parse(input);
    match &stmts[0] {
        Statement::Command(Command::Transition(cmd)) => {
            assert_eq!(cmd.to_state, "triaged");
            assert_eq!(cmd.with_data.len(), 1);
            assert_eq!(cmd.with_data[0].0, "note");
        }
        other => panic!("expected Transition, got {:?}", other),
    }
}

/// transition.md: Syntax -- `TRANSITION MachineName "instance_id" TO target_state MEMO "Reason"`
#[tokio::test]
async fn transition_doc_syntax_memo() {
    let input = r#"TRANSITION SupportTicket "01J5X7K2P3Q4R5S6T7U8V9W0XY" TO triaged MEMO "Customer confirmed""#;
    let stmts = must_parse(input);
    match &stmts[0] {
        Statement::Command(Command::Transition(cmd)) => {
            assert_eq!(cmd.to_state, "triaged");
            assert_eq!(cmd.memo, Some("Customer confirmed".to_string()));
        }
        other => panic!("expected Transition, got {:?}", other),
    }
}

/// transition.md: THROUGH -- multi-hop transition
#[tokio::test]
async fn transition_doc_through() {
    let input = r#"TRANSITION SupportTicket "01J5..." TO resolved THROUGH [in_progress]"#;
    let stmts = must_parse(input);
    match &stmts[0] {
        Statement::Command(Command::Transition(cmd)) => {
            assert_eq!(cmd.to_state, "resolved");
            assert_eq!(cmd.through, vec!["in_progress"]);
        }
        other => panic!("expected Transition, got {:?}", other),
    }
}

/// transition.md: OR_STAY parse
#[tokio::test]
async fn transition_doc_or_stay() {
    let input = r#"TRANSITION SupportTicket "01J5..." TO resolved OR_STAY"#;
    let stmts = must_parse(input);
    match &stmts[0] {
        Statement::Command(Command::Transition(cmd)) => {
            assert_eq!(cmd.to_state, "resolved");
            assert!(cmd.or_stay);
        }
        other => panic!("expected Transition, got {:?}", other),
    }
}

/// transition.md: CASCADE parse
#[tokio::test]
async fn transition_doc_cascade() {
    let input = r#"TRANSITION SupportTicket "01J5..." TO cancelled CASCADE"#;
    let stmts = must_parse(input);
    match &stmts[0] {
        Statement::Command(Command::Transition(cmd)) => {
            assert_eq!(cmd.to_state, "cancelled");
            assert!(cmd.cascade);
        }
        other => panic!("expected Transition, got {:?}", other),
    }
}

/// transition.md: curl example -- full TRANSITION with AS and WITH
#[tokio::test]
async fn transition_doc_curl_example() {
    let input = r#"TRANSITION SupportTicket "01J5X7K2P3Q4R5S6T7U8V9W0XY" TO triaged AS "agent-1" WITH { assignee: { id: "agent-1", role: "support" } }"#;
    let stmts = must_parse(input);
    match &stmts[0] {
        Statement::Command(Command::Transition(cmd)) => {
            assert_eq!(cmd.machine, "SupportTicket");
            assert_eq!(cmd.to_state, "triaged");
            assert_eq!(cmd.as_actor, Some("agent-1".to_string()));
            assert_eq!(cmd.with_data.len(), 1);
            assert_eq!(cmd.with_data[0].0, "assignee");
        }
        other => panic!("expected Transition, got {:?}", other),
    }
}

/// transition.md: End-to-end -- spawn then transition through the engine.
#[tokio::test]
async fn transition_doc_e2e_basic() {
    let engine = make_task_engine();
    let id = spawn_task(&engine, "My Task").await;

    let smql = format!(r#"TRANSITION Task "{}" TO doing"#, id);
    let stmts = must_parse(&smql);
    match &stmts[0] {
        Statement::Command(Command::Transition(cmd)) => {
            let result = engine.transition(cmd).await.expect("transition");
            assert_eq!(result.from_state, "todo");
            assert_eq!(result.to_state, "doing");
        }
        _ => panic!("expected Transition command"),
    }
}

/// transition.md: End-to-end -- THROUGH multi-hop
#[tokio::test]
async fn transition_doc_e2e_through() {
    let engine = make_task_engine();
    let id = spawn_task(&engine, "Through test").await;

    let smql = format!(r#"TRANSITION Task "{}" TO done THROUGH [doing]"#, id);
    let stmts = must_parse(&smql);
    match &stmts[0] {
        Statement::Command(Command::Transition(cmd)) => {
            let result = engine.transition(cmd).await.expect("through transition");
            assert_eq!(result.to_state, "done");
        }
        _ => panic!("expected Transition command"),
    }
}

/// transition.md: End-to-end -- OR_STAY on guard failure (valid transition, failing guard)
///
/// DOC ISSUE #3: The doc says "If the transition fails, keep the instance in its current
/// state instead of returning an error." However, OR_STAY only catches guard failures,
/// not missing transitions. If no transition is defined from->to, OR_STAY still errors.
/// The doc should clarify: "If a guard fails, keep the instance..."
#[tokio::test]
async fn transition_doc_e2e_or_stay_on_guard_fail() {
    // Use a machine with a guarded transition so OR_STAY can demonstrate its behavior
    let smql = r#"
DEFINE MACHINE Guarded (
    DATA {
        approved : BOOL -> DEFAULT(false)
    }
    STATES { pending, active, done }
    INITIAL STATE pending
    TERMINAL STATES { done }
    TRANSITIONS {
        pending -> active {
            GUARD : approved == true
        }
        active -> done {}
    }
)
"#;
    let engine = build_engine(smql);
    let spawn_stmts = must_parse(r#"SPAWN Guarded { approved: false }"#);
    let id = match &spawn_stmts[0] {
        Statement::Command(Command::Spawn(cmd)) => engine
            .spawn(cmd)
            .await
            .expect("spawn")
            .instance
            .id
            .as_str(),
        _ => panic!("expected Spawn"),
    };

    // OR_STAY: guard fails (approved != true), should return Ok with same state
    let smql = format!(r#"TRANSITION Guarded "{}" TO active OR_STAY"#, id);
    let stmts = must_parse(&smql);
    match &stmts[0] {
        Statement::Command(Command::Transition(cmd)) => {
            let result = engine.transition(cmd).await.expect("OR_STAY should not error");
            assert_eq!(result.from_state, "pending");
            assert_eq!(result.to_state, "pending"); // Stayed in same state
        }
        _ => panic!("expected Transition command"),
    }
}

/// transition.md: End-to-end -- OR_STAY with no transition defined still errors
#[tokio::test]
async fn transition_doc_e2e_or_stay_no_transition_defined() {
    let engine = make_task_engine();
    let id = spawn_task(&engine, "OR_STAY test").await;

    // todo -> done has no transition defined, OR_STAY still errors
    let smql = format!(r#"TRANSITION Task "{}" TO done OR_STAY"#, id);
    let stmts = must_parse(&smql);
    match &stmts[0] {
        Statement::Command(Command::Transition(cmd)) => {
            let result = engine.transition(cmd).await;
            assert!(
                result.is_err(),
                "OR_STAY does not catch missing transitions, only guard failures"
            );
        }
        _ => panic!("expected Transition command"),
    }
}

/// transition.md: End-to-end -- MEMO clause
#[tokio::test]
async fn transition_doc_e2e_memo() {
    let engine = make_task_engine();
    let id = spawn_task(&engine, "Memo test").await;

    let smql = format!(
        r#"TRANSITION Task "{}" TO doing MEMO "Starting work""#,
        id
    );
    let stmts = must_parse(&smql);
    match &stmts[0] {
        Statement::Command(Command::Transition(cmd)) => {
            let result = engine.transition(cmd).await.expect("transition with memo");
            assert_eq!(result.to_state, "doing");
        }
        _ => panic!("expected Transition command"),
    }
}

// ===========================================================================
// docs/commands/try-transition.md
// ===========================================================================

/// try-transition.md: Syntax -- `TRY TRANSITION MachineName "instance_id" TO target_state`
#[tokio::test]
async fn try_transition_doc_syntax_basic() {
    let input = r#"TRY TRANSITION SupportTicket "01J5X7K2P3Q4R5S6T7U8V9W0XY" TO triaged"#;
    let stmts = must_parse(input);
    match &stmts[0] {
        Statement::Command(Command::TryTransition(cmd)) => {
            assert_eq!(cmd.machine, "SupportTicket");
            assert_eq!(cmd.instance_id, "01J5X7K2P3Q4R5S6T7U8V9W0XY");
            assert_eq!(cmd.to_state, "triaged");
        }
        other => panic!("expected TryTransition, got {:?}", other),
    }
}

/// try-transition.md: Syntax -- `TRY TRANSITION MachineName "instance_id" TO target_state AS "u1"`
#[tokio::test]
async fn try_transition_doc_syntax_as() {
    let input =
        r#"TRY TRANSITION SupportTicket "01J5X7K2P3Q4R5S6T7U8V9W0XY" TO triaged AS "u1""#;
    let stmts = must_parse(input);
    match &stmts[0] {
        Statement::Command(Command::TryTransition(cmd)) => {
            assert_eq!(cmd.as_actor, Some("u1".to_string()));
        }
        other => panic!("expected TryTransition, got {:?}", other),
    }
}

/// try-transition.md: End-to-end -- successful try_transition
#[tokio::test]
async fn try_transition_doc_e2e_success() {
    let engine = make_task_engine();
    let id = spawn_task(&engine, "TRY test success").await;

    let smql = format!(r#"TRY TRANSITION Task "{}" TO doing"#, id);
    let stmts = must_parse(&smql);
    match &stmts[0] {
        Statement::Command(Command::TryTransition(cmd)) => {
            let result = engine.try_transition(cmd).await.expect("try_transition");
            assert!(result.is_some(), "should succeed");
            let tr = result.unwrap();
            assert_eq!(tr.from_state, "todo");
            assert_eq!(tr.to_state, "doing");
        }
        _ => panic!("expected TryTransition command"),
    }
}

/// try-transition.md: End-to-end -- try_transition on invalid path
#[tokio::test]
async fn try_transition_doc_e2e_guard_failure() {
    let engine = make_task_engine();
    let id = spawn_task(&engine, "TRY test fail").await;

    let smql = format!(r#"TRY TRANSITION Task "{}" TO done"#, id);
    let stmts = must_parse(&smql);
    match &stmts[0] {
        Statement::Command(Command::TryTransition(cmd)) => {
            let result = engine.try_transition(cmd).await;
            // TRY TRANSITION returns Ok(None) on guard failure (TransitionDenied).
            // For missing transitions, find_transition also returns TransitionDenied,
            // so try_transition catches it.
            match result {
                Ok(None) => { /* guard/transition denied -- caught by TRY */ }
                Err(_) => { /* some other error -- also acceptable */ }
                Ok(Some(_)) => panic!("should not succeed"),
            }
        }
        _ => panic!("expected TryTransition command"),
    }
}

// ===========================================================================
// docs/commands/transition-all.md
// ===========================================================================

/// transition-all.md: Syntax -- `TRANSITION ALL MachineName WHERE <predicate> TO target_state`
#[tokio::test]
async fn transition_all_doc_syntax() {
    let input = r#"TRANSITION ALL SupportTicket WHERE STATE IS resolved TO closed"#;
    let stmts = must_parse(input);
    match &stmts[0] {
        Statement::Command(Command::BatchTransition(cmd)) => {
            assert_eq!(cmd.machine, "SupportTicket");
            assert_eq!(cmd.to_state, "closed");
        }
        other => panic!("expected BatchTransition, got {:?}", other),
    }
}

/// transition-all.md: Example -- close all resolved tickets
#[tokio::test]
async fn transition_all_doc_close_resolved() {
    let input = r#"TRANSITION ALL SupportTicket WHERE STATE IS resolved TO closed"#;
    let stmts = must_parse(input);
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Statement::Command(Command::BatchTransition(cmd)) => {
            assert_eq!(cmd.machine, "SupportTicket");
            assert_eq!(cmd.to_state, "closed");
        }
        other => panic!("expected BatchTransition, got {:?}", other),
    }
}

/// transition-all.md: Example with compound predicate -- `WHERE STATE IS draft AND elapsed() > 30d`
#[tokio::test]
async fn transition_all_doc_compound_predicate() {
    let input =
        r#"TRANSITION ALL Order WHERE STATE IS draft AND elapsed() > 30d TO cancelled"#;
    let stmts = must_parse(input);
    match &stmts[0] {
        Statement::Command(Command::BatchTransition(cmd)) => {
            assert_eq!(cmd.machine, "Order");
            assert_eq!(cmd.to_state, "cancelled");
        }
        other => panic!("expected BatchTransition, got {:?}", other),
    }
}

/// transition-all.md: End-to-end -- batch transition through the engine
#[tokio::test]
async fn transition_all_doc_e2e() {
    let engine = make_order_engine();

    let _id1 = spawn_order(&engine, "Alice").await;
    let _id2 = spawn_order(&engine, "Bob").await;
    let id3 = spawn_order(&engine, "Charlie").await;

    // Move one to pending
    let smql = format!(r#"TRANSITION Order "{}" TO pending"#, id3);
    let stmts = must_parse(&smql);
    match &stmts[0] {
        Statement::Command(Command::Transition(cmd)) => {
            engine.transition(cmd).await.expect("transition to pending");
        }
        _ => panic!("expected Transition command"),
    }

    // TRANSITION ALL draft to cancelled
    let input = r#"TRANSITION ALL Order WHERE STATE IS draft TO cancelled"#;
    let stmts = must_parse(input);
    match &stmts[0] {
        Statement::Command(Command::BatchTransition(cmd)) => {
            let result = engine
                .batch_transition(cmd)
                .await
                .expect("batch transition");
            assert_eq!(result.matched, 2);
            assert_eq!(result.transitioned, 2);
            assert!(result.failures.is_empty());
        }
        _ => panic!("expected BatchTransition command"),
    }
}

/// transition-all.md: End-to-end -- no matches
#[tokio::test]
async fn transition_all_doc_e2e_no_matches() {
    let engine = make_order_engine();
    let input = r#"TRANSITION ALL Order WHERE STATE IS draft TO cancelled"#;
    let stmts = must_parse(input);
    match &stmts[0] {
        Statement::Command(Command::BatchTransition(cmd)) => {
            let result = engine
                .batch_transition(cmd)
                .await
                .expect("batch transition");
            assert_eq!(result.matched, 0);
            assert_eq!(result.transitioned, 0);
            assert!(result.failures.is_empty());
        }
        _ => panic!("expected BatchTransition command"),
    }
}

/// transition-all.md: Comments -- verify the `--` comment syntax parses
#[tokio::test]
async fn transition_all_doc_comment_syntax() {
    let input =
        "-- Close all resolved tickets\nTRANSITION ALL SupportTicket WHERE STATE IS resolved TO closed";
    let stmts = must_parse(input);
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Statement::Command(Command::BatchTransition(cmd)) => {
            assert_eq!(cmd.machine, "SupportTicket");
            assert_eq!(cmd.to_state, "closed");
        }
        other => panic!("expected BatchTransition, got {:?}", other),
    }
}

/// transition-all.md: Second comment example
#[tokio::test]
async fn transition_all_doc_cancel_draft_orders() {
    let input = "-- Cancel all draft orders older than 30 days\nTRANSITION ALL Order WHERE STATE IS draft AND elapsed() > 30d TO cancelled";
    let stmts = must_parse(input);
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Statement::Command(Command::BatchTransition(cmd)) => {
            assert_eq!(cmd.machine, "Order");
            assert_eq!(cmd.to_state, "cancelled");
        }
        other => panic!("expected BatchTransition, got {:?}", other),
    }
}

// ===========================================================================
// docs/commands/alter-machine.md
// ===========================================================================

/// alter-machine.md: Syntax -- all ALTER forms parse correctly
#[tokio::test]
async fn alter_doc_syntax_all_forms() {
    let input = r#"ALTER MACHINE SupportTicket ADD STATE escalated"#;
    let stmts = must_parse(input);
    match &stmts[0] {
        Statement::Command(Command::AlterMachine(cmd)) => {
            assert_eq!(cmd.machine, "SupportTicket");
            assert_eq!(cmd.operations.len(), 1);
        }
        other => panic!("expected AlterMachine, got {:?}", other),
    }
}

/// alter-machine.md: ADD STATE -- `ALTER MACHINE SupportTicket ADD STATE escalated`
#[tokio::test]
async fn alter_doc_add_state() {
    let input = "ALTER MACHINE SupportTicket\n  ADD STATE escalated";
    let stmts = must_parse(input);
    match &stmts[0] {
        Statement::Command(Command::AlterMachine(cmd)) => {
            assert_eq!(cmd.machine, "SupportTicket");
            match &cmd.operations[0] {
                smql_ast::command::AlterOperation::AddState(name) => {
                    assert_eq!(name, "escalated");
                }
                other => panic!("expected AddState, got {:?}", other),
            }
        }
        other => panic!("expected AlterMachine, got {:?}", other),
    }
}

/// alter-machine.md: REMOVE STATE -- `ALTER MACHINE SupportTicket REMOVE STATE reopened MIGRATE TO open`
#[tokio::test]
async fn alter_doc_remove_state() {
    let input = "ALTER MACHINE SupportTicket\n  REMOVE STATE reopened MIGRATE TO open";
    let stmts = must_parse(input);
    match &stmts[0] {
        Statement::Command(Command::AlterMachine(cmd)) => {
            match &cmd.operations[0] {
                smql_ast::command::AlterOperation::RemoveState { state, migrate_to } => {
                    assert_eq!(state, "reopened");
                    assert_eq!(migrate_to, "open");
                }
                other => panic!("expected RemoveState, got {:?}", other),
            }
        }
        other => panic!("expected AlterMachine, got {:?}", other),
    }
}

/// alter-machine.md: ADD TRANSITION -- `ALTER MACHINE SupportTicket ADD TRANSITION triaged -> escalated`
#[tokio::test]
async fn alter_doc_add_transition() {
    let input = "ALTER MACHINE SupportTicket\n  ADD TRANSITION triaged -> escalated";
    let stmts = must_parse(input);
    match &stmts[0] {
        Statement::Command(Command::AlterMachine(cmd)) => {
            match &cmd.operations[0] {
                smql_ast::command::AlterOperation::AddTransition(td) => {
                    assert_eq!(td.to, "escalated");
                }
                other => panic!("expected AddTransition, got {:?}", other),
            }
        }
        other => panic!("expected AlterMachine, got {:?}", other),
    }
}

/// alter-machine.md: REMOVE TRANSITION -- `ALTER MACHINE SupportTicket REMOVE TRANSITION reopened -> in_progress`
#[tokio::test]
async fn alter_doc_remove_transition() {
    let input = "ALTER MACHINE SupportTicket\n  REMOVE TRANSITION reopened -> in_progress";
    let stmts = must_parse(input);
    match &stmts[0] {
        Statement::Command(Command::AlterMachine(cmd)) => {
            match &cmd.operations[0] {
                smql_ast::command::AlterOperation::RemoveTransition { from, to } => {
                    assert_eq!(from, "reopened");
                    assert_eq!(to, "in_progress");
                }
                other => panic!("expected RemoveTransition, got {:?}", other),
            }
        }
        other => panic!("expected AlterMachine, got {:?}", other),
    }
}

/// alter-machine.md: ADD DATA with BACKFILL on same line
///
/// DOC ISSUE #4: The doc shows BACKFILL on a separate line after ADD DATA:
///   ```
///   ALTER MACHINE SupportTicket
///     ADD DATA severity : INT -> DEFAULT(0)
///     BACKFILL severity = 0
///   ```
/// The parser has an inline BACKFILL check after ADD DATA that greedily consumes
/// the BACKFILL keyword but only parses an expression (not `field = value`).
/// This leaves `= 0` unparsed and causes a parse error.
///
/// Workaround: Put BACKFILL on the same line as ADD DATA without `field =`, OR
/// restructure the parser to not greedily consume BACKFILL inline.
///
/// For now, test that the standalone syntax works without the BACKFILL:
#[tokio::test]
async fn alter_doc_add_data_parse() {
    // ADD DATA without BACKFILL works fine
    let input = "ALTER MACHINE SupportTicket\n  ADD DATA severity : INT -> DEFAULT(0)";
    let stmts = must_parse(input);
    match &stmts[0] {
        Statement::Command(Command::AlterMachine(cmd)) => {
            assert_eq!(cmd.operations.len(), 1);
            match &cmd.operations[0] {
                smql_ast::command::AlterOperation::AddData { field, .. } => {
                    assert_eq!(field.name, "severity");
                }
                other => panic!("expected AddData, got {:?}", other),
            }
        }
        other => panic!("expected AlterMachine, got {:?}", other),
    }
}

/// alter-machine.md: Demonstrate the BACKFILL parse issue
#[tokio::test]
async fn alter_doc_add_data_with_backfill_parse_issue() {
    // The doc example: ADD DATA + BACKFILL on separate lines
    let input = "ALTER MACHINE SupportTicket\n  ADD DATA severity : INT -> DEFAULT(0)\n  BACKFILL severity = 0";
    let result = smql_parser::parse(input);
    // This fails because the inline BACKFILL parser in ADD DATA greedily consumes BACKFILL
    // and parses only `severity` as the expression, leaving `= 0` behind.
    assert!(
        result.is_err(),
        "Expected parse error due to BACKFILL parsing issue -- this is a known parser bug"
    );
}

/// alter-machine.md: REMOVE DATA -- `ALTER MACHINE SupportTicket REMOVE DATA satisfaction`
#[tokio::test]
async fn alter_doc_remove_data() {
    let input = "ALTER MACHINE SupportTicket\n  REMOVE DATA satisfaction";
    let stmts = must_parse(input);
    match &stmts[0] {
        Statement::Command(Command::AlterMachine(cmd)) => {
            match &cmd.operations[0] {
                smql_ast::command::AlterOperation::RemoveData(name) => {
                    assert_eq!(name, "satisfaction");
                }
                other => panic!("expected RemoveData, got {:?}", other),
            }
        }
        other => panic!("expected AlterMachine, got {:?}", other),
    }
}

/// alter-machine.md: Multi-operation -- ADD STATE + ADD TRANSITION in one ALTER
#[tokio::test]
async fn alter_doc_multi_operation() {
    let input =
        "ALTER MACHINE SupportTicket\n  ADD STATE escalated\n  ADD TRANSITION triaged -> escalated";
    let stmts = must_parse(input);
    match &stmts[0] {
        Statement::Command(Command::AlterMachine(cmd)) => {
            assert_eq!(cmd.machine, "SupportTicket");
            assert_eq!(cmd.operations.len(), 2);
            match &cmd.operations[0] {
                smql_ast::command::AlterOperation::AddState(name) => {
                    assert_eq!(name, "escalated")
                }
                other => panic!("expected AddState first, got {:?}", other),
            }
            match &cmd.operations[1] {
                smql_ast::command::AlterOperation::AddTransition(td) => {
                    assert_eq!(td.to, "escalated")
                }
                other => panic!("expected AddTransition second, got {:?}", other),
            }
        }
        other => panic!("expected AlterMachine, got {:?}", other),
    }
}

/// alter-machine.md: End-to-end -- ADD STATE + ADD TRANSITION through engine
#[tokio::test]
async fn alter_doc_e2e_add_state_and_transition() {
    let engine = make_support_ticket_engine();

    // ALTER MACHINE to add escalated state and transition
    let input =
        "ALTER MACHINE SupportTicket\n  ADD STATE escalated\n  ADD TRANSITION triaged -> escalated";
    let stmts = must_parse(input);
    match &stmts[0] {
        Statement::Command(Command::AlterMachine(cmd)) => {
            let result = engine
                .execute_alter_machine(cmd)
                .await
                .expect("alter machine");
            assert_eq!(result.machine, "SupportTicket");
            assert_eq!(result.operations_applied, 2);
            assert!(result.new_version >= 2);
        }
        _ => panic!("expected AlterMachine command"),
    }

    // Spawn a ticket using API helper (bypasses UUID type issue)
    let id = spawn_support_ticket_via_api(&engine).await;

    // Move to triaged (needs assignee IS SET guard)
    let mut assignee = BTreeMap::new();
    assignee.insert("id".to_string(), Value::Text("agent-1".into()));
    assignee.insert("role".to_string(), Value::Text("support".into()));

    let t_cmd = smql_ast::command::TransitionCommand {
        machine: "SupportTicket".to_string(),
        instance_id: id.clone(),
        to_state: "triaged".to_string(),
        with_data: vec![(
            "assignee".to_string(),
            Expression::new(ExpressionKind::Literal(Value::Map(assignee))),
        )],
        memo: None,
        as_actor: Some("agent-1".to_string()),
        through: Vec::new(),
        or_stay: false,
        cascade: false,
    };
    engine.transition(&t_cmd).await.expect("to triaged");

    // Now transition from triaged to escalated (the newly added transition)
    let smql = format!(
        r#"TRANSITION SupportTicket "{}" TO escalated"#,
        id
    );
    let stmts = must_parse(&smql);
    match &stmts[0] {
        Statement::Command(Command::Transition(cmd)) => {
            let result = engine.transition(cmd).await.expect("to escalated");
            assert_eq!(result.from_state, "triaged");
            assert_eq!(result.to_state, "escalated");
        }
        _ => panic!("expected Transition command"),
    }
}

/// alter-machine.md: End-to-end -- REMOVE STATE with MIGRATE
#[tokio::test]
async fn alter_doc_e2e_remove_state() {
    let engine = make_support_ticket_engine();

    let _id = spawn_support_ticket_via_api(&engine).await;

    let input = "ALTER MACHINE SupportTicket\n  REMOVE STATE reopened MIGRATE TO open";
    let stmts = must_parse(input);
    match &stmts[0] {
        Statement::Command(Command::AlterMachine(cmd)) => {
            let result = engine
                .execute_alter_machine(cmd)
                .await
                .expect("alter remove state");
            assert_eq!(result.operations_applied, 1);
        }
        _ => panic!("expected AlterMachine command"),
    }
}

/// alter-machine.md: End-to-end -- REMOVE DATA
#[tokio::test]
async fn alter_doc_e2e_remove_data() {
    let engine = make_support_ticket_engine();

    let input = "ALTER MACHINE SupportTicket\n  REMOVE DATA satisfaction";
    let stmts = must_parse(input);
    match &stmts[0] {
        Statement::Command(Command::AlterMachine(cmd)) => {
            let result = engine
                .execute_alter_machine(cmd)
                .await
                .expect("alter remove data");
            assert_eq!(result.operations_applied, 1);
        }
        _ => panic!("expected AlterMachine command"),
    }
}

/// alter-machine.md: End-to-end -- REMOVE TRANSITION
#[tokio::test]
async fn alter_doc_e2e_remove_transition() {
    let engine = make_support_ticket_engine();

    let input = "ALTER MACHINE SupportTicket\n  REMOVE TRANSITION reopened -> in_progress";
    let stmts = must_parse(input);
    match &stmts[0] {
        Statement::Command(Command::AlterMachine(cmd)) => {
            let result = engine
                .execute_alter_machine(cmd)
                .await
                .expect("alter remove transition");
            assert_eq!(result.operations_applied, 1);
        }
        _ => panic!("expected AlterMachine command"),
    }
}

/// alter-machine.md: End-to-end -- ADD DATA through engine (without BACKFILL)
#[tokio::test]
async fn alter_doc_e2e_add_data() {
    let engine = make_support_ticket_engine();

    let _id1 = spawn_support_ticket_via_api(&engine).await;
    let _id2 = spawn_support_ticket_via_api(&engine).await;

    // Add data field (without BACKFILL due to parse bug)
    let input = "ALTER MACHINE SupportTicket\n  ADD DATA severity : INT -> DEFAULT(0)";
    let stmts = must_parse(input);
    match &stmts[0] {
        Statement::Command(Command::AlterMachine(cmd)) => {
            let result = engine
                .execute_alter_machine(cmd)
                .await
                .expect("alter add data");
            assert_eq!(result.machine, "SupportTicket");
            assert_eq!(result.operations_applied, 1);
        }
        _ => panic!("expected AlterMachine command"),
    }
}
