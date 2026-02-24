/// Documentation validation tests for introduction pages.
///
/// Extracts SMQL code examples from:
///   - docs/introduction/quick-start.md
///   - docs/introduction/key-concepts.md
///   - docs/introduction/what-is-smql.md
///   - docs/introduction/why-smql.md
///
/// Tests each example against the parser and, where applicable, the engine.
///
/// DOC ISSUES FOUND:
///   - quick-start.md: DATA block uses comma between field definitions
///     (`title: TEXT -> REQUIRED, assignee: TEXT -> OPTIONAL`).
///     The parser uses commas as constraint separators within a field,
///     so this is ambiguous and fails to parse. Fields should be on
///     separate lines or the comma between them should be removed.
///     Affects: Examples 1-3 (curl, REPL, SDK DEFINE MACHINE) and
///     all engine tests that use that definition.
use std::sync::Arc;

use smql_ast::command::{Command, Statement};
use smql_catalog::MachineCatalog;
use smql_engine_core::Engine;
use smql_hooks::{EventBus, HookExecutor};
use smql_storage::MemoryStorage;
use smql_timer::TimerManager;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a fresh engine from a machine definition string, parse + register all machines.
fn engine_from_smql(machine_defs: &str) -> Engine {
    let machines = smql_parser::parse_machines(machine_defs).expect("parse machine definitions");
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

/// Execute a single SMQL statement string against the engine.
/// Parses, then dispatches to the appropriate engine method.
async fn execute_smql(engine: &Engine, smql: &str) -> Result<String, String> {
    let stmts = smql_parser::parse(smql).map_err(|e| format!("Parse error: {}", e))?;
    let stmt = stmts
        .into_iter()
        .next()
        .ok_or_else(|| "Empty SMQL input".to_string())?;

    match stmt {
        Statement::Command(cmd) => match cmd {
            Command::DefineMachine(def) => {
                engine
                    .catalog
                    .register(def)
                    .map_err(|e| format!("Register error: {}", e))?;
                Ok("machine_defined".to_string())
            }
            Command::Spawn(spawn_cmd) => {
                let result = engine
                    .spawn(&spawn_cmd)
                    .await
                    .map_err(|e| format!("Spawn error: {}", e))?;
                Ok(format!(
                    "spawned:{},state:{}",
                    result.instance.id.as_str(),
                    result.instance.state
                ))
            }
            Command::Transition(t_cmd) => {
                let result = engine
                    .transition(&t_cmd)
                    .await
                    .map_err(|e| format!("Transition error: {}", e))?;
                Ok(format!(
                    "transitioned:{},from:{},to:{}",
                    result.instance.id.as_str(),
                    result.from_state,
                    result.to_state
                ))
            }
            Command::TryTransition(t_cmd) => {
                let result = engine
                    .try_transition(&t_cmd)
                    .await
                    .map_err(|e| format!("TryTransition error: {}", e))?;
                Ok(format!("try_transitioned:{:?}", result))
            }
            Command::BatchTransition(b_cmd) => {
                let result = engine
                    .batch_transition(&b_cmd)
                    .await
                    .map_err(|e| format!("BatchTransition error: {}", e))?;
                Ok(format!(
                    "batch:matched={},transitioned={}",
                    result.matched, result.transitioned
                ))
            }
            Command::AlterMachine(a_cmd) => {
                Ok(format!("alter_machine:{}", a_cmd.machine))
            }
            Command::DefinePolicy(policy) => {
                engine.catalog.register_policy(policy);
                Ok("policy_defined".to_string())
            }
            Command::DefineView(view) => {
                engine.catalog.register_view(view);
                Ok("view_defined".to_string())
            }
            Command::DefineProjection(proj) => {
                engine.catalog.register_projection(proj);
                Ok("projection_defined".to_string())
            }
            Command::DefineRule(rule) => {
                engine.catalog.register_rule(rule);
                Ok("rule_defined".to_string())
            }
            Command::DefineSubscription(sub) => {
                engine.catalog.register_subscription(sub);
                Ok("subscription_defined".to_string())
            }
            Command::DefineSaga(saga) => {
                engine.catalog.register_saga(saga);
                Ok("saga_defined".to_string())
            }
            Command::DefineTemplate(def) => {
                engine.catalog.register_template(def);
                Ok("template_defined".to_string())
            }
            Command::Claim(claim_cmd) => {
                let result = engine
                    .execute_claim(&claim_cmd)
                    .await
                    .map_err(|e| format!("Claim error: {}", e))?;
                Ok(format!("claimed:{}", result.instance.id.as_str()))
            }
            Command::Release(release_cmd) => {
                let result = engine
                    .execute_release(&release_cmd)
                    .await
                    .map_err(|e| format!("Release error: {}", e))?;
                Ok(format!("released:{}", result.instance_id))
            }
            Command::Watch(watch_cmd) => {
                let result = engine
                    .watch(&watch_cmd)
                    .await
                    .map_err(|e| format!("Watch error: {}", e))?;
                Ok(format!("watched:{},waited_ms:{}", result.instance.id.as_str(), result.waited_ms))
            }
        },
        Statement::Query(query) => {
            let result = engine
                .execute_query(&query)
                .await
                .map_err(|e| format!("Query error: {}", e))?;
            Ok(format!("{:?}", result))
        }
        Statement::Transaction(stmts) => {
            let results = engine
                .execute_transaction(&stmts)
                .await
                .map_err(|e| format!("Transaction error: {}", e))?;
            Ok(format!("transaction:{} steps", results.len()))
        }
    }
}

// ===========================================================================
// The corrected Task machine definition (fields on separate lines)
// Used by engine tests that need a valid machine from quick-start.md
// ===========================================================================

const TASK_MACHINE: &str = r#"DEFINE MACHINE Task (
    DATA {
        title: TEXT -> REQUIRED
        assignee: TEXT -> OPTIONAL
    }
    STATES { todo, doing, done }
    INITIAL STATE todo
    TERMINAL STATES { done }
    TRANSITIONS {
        todo -> doing {}
        doing -> done {}
    }
)"#;

// ===========================================================================
// quick-start.md — Example 1: DEFINE MACHINE Task (curl one-liner)
// DOC BUG: comma between DATA field definitions is parsed as constraint separator
// ===========================================================================

#[test]
fn quickstart_define_task_curl_oneliner_doc_bug() {
    // EXACT text from quick-start.md curl example (JSON value, single line).
    // This FAILS because the comma between `REQUIRED` and `assignee` is
    // interpreted as a constraint separator, not a field separator.
    let smql = r#"DEFINE MACHINE Task ( DATA { title: TEXT -> REQUIRED, assignee: TEXT -> OPTIONAL } STATES { todo, doing, done } INITIAL STATE todo TERMINAL STATES { done } TRANSITIONS { todo -> doing {} doing -> done {} } )"#;
    let result = smql_parser::parse(smql);
    assert!(
        result.is_err(),
        "DOC BUG: comma between DATA fields is ambiguous with constraint separator. \
         Parser treats the comma after REQUIRED as another constraint, then fails on 'assignee'. \
         Fix: put each field on a separate line or remove the comma."
    );
}

#[test]
fn quickstart_define_task_curl_oneliner_corrected() {
    // CORRECTED version: no comma between field definitions (newline separated)
    let smql = r#"DEFINE MACHINE Task ( DATA { title: TEXT -> REQUIRED  assignee: TEXT -> OPTIONAL } STATES { todo, doing, done } INITIAL STATE todo TERMINAL STATES { done } TRANSITIONS { todo -> doing {} doing -> done {} } )"#;
    let stmts = smql_parser::parse(smql).expect("corrected parse should succeed");
    assert_eq!(stmts.len(), 1, "should produce exactly one statement");
    match &stmts[0] {
        Statement::Command(Command::DefineMachine(def)) => {
            assert_eq!(def.name, "Task");
            assert_eq!(def.states.len(), 3);
            assert_eq!(def.initial_state, "todo");
            assert!(def.terminal_states.contains(&"done".to_string()));
            assert_eq!(def.data.len(), 2, "should have 2 data fields");
        }
        other => panic!("expected DefineMachine, got {:?}", other),
    }
}

// ===========================================================================
// quick-start.md — Example 2: DEFINE MACHINE Task (REPL multiline)
// DOC BUG: same comma issue
// ===========================================================================

#[test]
fn quickstart_define_task_repl_multiline_doc_bug() {
    // EXACT text from quick-start.md REPL example
    let smql = r#"DEFINE MACHINE Task (
    DATA { title: TEXT -> REQUIRED, assignee: TEXT -> OPTIONAL }
    STATES { todo, doing, done }
    INITIAL STATE todo
    TERMINAL STATES { done }
    TRANSITIONS {
      todo -> doing {}
      doing -> done {}
    }
  )"#;
    let result = smql_parser::parse(smql);
    assert!(
        result.is_err(),
        "DOC BUG: same comma-between-fields issue as curl example"
    );
}

#[test]
fn quickstart_define_task_repl_multiline_corrected() {
    // CORRECTED: fields on separate lines, no comma between them
    let smql = r#"DEFINE MACHINE Task (
    DATA {
        title: TEXT -> REQUIRED
        assignee: TEXT -> OPTIONAL
    }
    STATES { todo, doing, done }
    INITIAL STATE todo
    TERMINAL STATES { done }
    TRANSITIONS {
      todo -> doing {}
      doing -> done {}
    }
  )"#;
    let stmts = smql_parser::parse(smql).expect("corrected parse should succeed");
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Statement::Command(Command::DefineMachine(def)) => {
            assert_eq!(def.name, "Task");
            assert_eq!(def.data.len(), 2);
            assert_eq!(def.transitions.len(), 2);
        }
        other => panic!("expected DefineMachine, got {:?}", other),
    }
}

// ===========================================================================
// quick-start.md — Example 3: DEFINE MACHINE Task (SDK, same content)
// DOC BUG: same comma issue
// ===========================================================================

#[test]
fn quickstart_define_task_sdk_doc_bug() {
    // EXACT text from quick-start.md SDK example
    let smql = r#"
  DEFINE MACHINE Task (
    DATA { title: TEXT -> REQUIRED, assignee: TEXT -> OPTIONAL }
    STATES { todo, doing, done }
    INITIAL STATE todo
    TERMINAL STATES { done }
    TRANSITIONS {
      todo -> doing {}
      doing -> done {}
    }
  )
"#;
    let result = smql_parser::parse(smql);
    assert!(
        result.is_err(),
        "DOC BUG: same comma-between-fields issue as curl/REPL examples"
    );
}

#[test]
fn quickstart_define_task_sdk_corrected() {
    // CORRECTED: fields on separate lines
    let smql = r#"
  DEFINE MACHINE Task (
    DATA {
        title: TEXT -> REQUIRED
        assignee: TEXT -> OPTIONAL
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
    let stmts = smql_parser::parse(smql).expect("corrected parse should succeed");
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Statement::Command(Command::DefineMachine(def)) => {
            assert_eq!(def.name, "Task");
        }
        other => panic!("expected DefineMachine, got {:?}", other),
    }
}

// ===========================================================================
// quick-start.md — Example 4: SPAWN Task
// ===========================================================================

#[test]
fn quickstart_spawn_task_parse() {
    let smql = r#"SPAWN Task { title: "Write docs", assignee: "alice" }"#;
    let stmts = smql_parser::parse(smql).expect("parse should succeed");
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Statement::Command(Command::Spawn(spawn)) => {
            assert_eq!(spawn.machine, "Task");
            assert_eq!(spawn.data.len(), 2);
        }
        other => panic!("expected Spawn, got {:?}", other),
    }
}

#[tokio::test]
async fn quickstart_spawn_task_engine() {
    // Uses corrected machine definition
    let engine = engine_from_smql(TASK_MACHINE);
    let result = execute_smql(&engine, r#"SPAWN Task { title: "Write docs", assignee: "alice" }"#)
        .await
        .expect("spawn should succeed");
    assert!(result.starts_with("spawned:"), "result = {}", result);
    assert!(result.contains("state:todo"), "result = {}", result);
}

// ===========================================================================
// quick-start.md — Example 5: TRANSITION Task ... TO doing
// ===========================================================================

#[tokio::test]
async fn quickstart_transition_task_parse_and_engine() {
    let engine = engine_from_smql(TASK_MACHINE);

    // First, spawn an instance to get a real ID
    let spawn_result =
        execute_smql(&engine, r#"SPAWN Task { title: "Write docs", assignee: "alice" }"#)
            .await
            .expect("spawn should succeed");
    // Extract the ID from "spawned:<ID>,state:todo"
    let id = spawn_result
        .strip_prefix("spawned:")
        .unwrap()
        .split(',')
        .next()
        .unwrap();

    // Now test the TRANSITION command with the real ID
    let transition_smql = format!(r#"TRANSITION Task "{}" TO doing"#, id);
    let result = execute_smql(&engine, &transition_smql)
        .await
        .expect("transition should succeed");
    assert!(result.contains("from:todo"), "result = {}", result);
    assert!(result.contains("to:doing"), "result = {}", result);

    // Also verify the doc example parses (even though its ID is fake)
    let doc_smql = r#"TRANSITION Task "01J5X7K2P3Q4R5S6T7U8V9W0XY" TO doing"#;
    let stmts = smql_parser::parse(doc_smql).expect("parse should succeed");
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Statement::Command(Command::Transition(t)) => {
            assert_eq!(t.machine, "Task");
            assert_eq!(t.instance_id, "01J5X7K2P3Q4R5S6T7U8V9W0XY");
            assert_eq!(t.to_state, "doing");
        }
        other => panic!("expected Transition, got {:?}", other),
    }
}

// ===========================================================================
// quick-start.md — Example 6: FIND Task WHERE STATE IS doing
// ===========================================================================

#[test]
fn quickstart_find_task_parse() {
    let smql = "FIND Task WHERE STATE IS doing";
    let stmts = smql_parser::parse(smql).expect("parse should succeed");
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Statement::Query(_q) => {
            // Good -- parsed as a query (FindQuery)
        }
        other => panic!("expected Query, got {:?}", other),
    }
}

#[tokio::test]
async fn quickstart_find_task_engine() {
    let engine = engine_from_smql(TASK_MACHINE);

    // Spawn a task and transition it to doing
    let spawn_result =
        execute_smql(&engine, r#"SPAWN Task { title: "Write docs", assignee: "alice" }"#)
            .await
            .unwrap();
    let id = spawn_result
        .strip_prefix("spawned:")
        .unwrap()
        .split(',')
        .next()
        .unwrap();
    let transition_smql = format!(r#"TRANSITION Task "{}" TO doing"#, id);
    execute_smql(&engine, &transition_smql).await.unwrap();

    // Now test the FIND query
    let find_result = execute_smql(&engine, "FIND Task WHERE STATE IS doing")
        .await
        .expect("FIND should succeed");
    // The result should contain at least one instance
    assert!(
        !find_result.contains("Instances([])"),
        "should find at least one instance in 'doing': {}",
        find_result
    );
}

// ===========================================================================
// key-concepts.md — Example 7: DEFINE MACHINE SupportTicket (partial with ...)
// SKIP: Contains `...` placeholders, not valid SMQL
// ===========================================================================

#[test]
fn keyconcepts_define_support_ticket_partial_skip() {
    // From key-concepts.md:
    // ```sql
    // DEFINE MACHINE SupportTicket (
    //   DATA { ... }
    //   STATES { open, triaged, resolved, closed }
    //   INITIAL STATE open
    //   TERMINAL STATES { closed }
    //   TRANSITIONS { ... }
    // )
    // ```
    // This is pseudo-code with `...` placeholders -- not valid SMQL.
    let smql = r#"DEFINE MACHINE SupportTicket (
  DATA { ... }
  STATES { open, triaged, resolved, closed }
  INITIAL STATE open
  TERMINAL STATES { closed }
  TRANSITIONS { ... }
)"#;
    let result = smql_parser::parse(smql);
    // This SHOULD fail because `...` is not valid syntax
    assert!(
        result.is_err(),
        "pseudo-code with `...` should not parse as valid SMQL"
    );
}

// ===========================================================================
// key-concepts.md — Example 8: STATES / INITIAL STATE / TERMINAL STATES fragment
// SKIP: Not a complete statement, just a fragment
// ===========================================================================

#[test]
fn keyconcepts_states_fragment_skip() {
    // From key-concepts.md:
    // ```sql
    // STATES { open, triaged, in_progress, resolved, closed }
    // INITIAL STATE open
    // TERMINAL STATES { closed }
    // ```
    // This is a fragment inside a DEFINE MACHINE body, not a standalone statement.
    let smql = r#"STATES { open, triaged, in_progress, resolved, closed }
INITIAL STATE open
TERMINAL STATES { closed }"#;
    let result = smql_parser::parse(smql);
    assert!(
        result.is_err(),
        "fragment (not a full statement) should not parse standalone"
    );
}

// ===========================================================================
// key-concepts.md — Example 9: Transition body fragment
// SKIP: Not a complete statement
// ===========================================================================

#[test]
fn keyconcepts_transition_body_fragment_skip() {
    // From key-concepts.md:
    // ```sql
    // in_progress -> resolved {
    //   GUARD   : resolution_note IS SET
    //   GUARD   : ACTOR == assignee OR ACTOR.role == "admin"
    //   TIMEOUT : 7d -> closed
    //   ACTION  : NOTIFY(customer_id, "ticket.resolved")
    // }
    // ```
    // This is a transition body fragment, not a standalone statement.
    let smql = r#"in_progress -> resolved {
  GUARD   : resolution_note IS SET
  GUARD   : ACTOR == assignee OR ACTOR.role == "admin"
  TIMEOUT : 7d -> closed
  ACTION  : NOTIFY(customer_id, "ticket.resolved")
}"#;
    let result = smql_parser::parse(smql);
    assert!(
        result.is_err(),
        "transition body fragment should not parse as standalone"
    );
}

// ===========================================================================
// key-concepts.md — Example 10: TRAIL OF "..."
// ===========================================================================

#[test]
fn keyconcepts_trail_of_parse() {
    let smql = r#"TRAIL OF "01J5X7K2P3Q4R5S6T7U8V9W0XY""#;
    let stmts = smql_parser::parse(smql).expect("parse should succeed");
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Statement::Query(_) => {
            // Good -- parsed as a query (TrailQuery)
        }
        other => panic!("expected Query (TrailQuery), got {:?}", other),
    }
}

// ===========================================================================
// key-concepts.md — Example 11: TRANSITION with AS actor
// ===========================================================================

#[test]
fn keyconcepts_transition_with_actor_parse() {
    // From key-concepts.md:
    // TRANSITION SupportTicket "01J5..." TO resolved AS "user-42"
    // Note: "01J5..." is not a real ULID but should still parse as a string.
    let smql = r#"TRANSITION SupportTicket "01J5..." TO resolved AS "user-42""#;
    let stmts = smql_parser::parse(smql).expect("parse should succeed");
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Statement::Command(Command::Transition(t)) => {
            assert_eq!(t.machine, "SupportTicket");
            assert_eq!(t.instance_id, "01J5...");
            assert_eq!(t.to_state, "resolved");
            assert_eq!(t.as_actor.as_deref(), Some("user-42"));
        }
        other => panic!("expected Transition, got {:?}", other),
    }
}

// ===========================================================================
// key-concepts.md — Example 12: CHILDREN block fragment
// SKIP: Not a complete statement
// ===========================================================================

#[test]
fn keyconcepts_children_fragment_skip() {
    // From key-concepts.md:
    // ```sql
    // CHILDREN {
    //   items    : LIST(LineItem) -> MIN(1)
    //   shipment : OPTIONAL(Shipment)
    // }
    // ```
    // This is a fragment inside DEFINE MACHINE, not a standalone statement.
    let smql = r#"CHILDREN {
  items    : LIST(LineItem) -> MIN(1)
  shipment : OPTIONAL(Shipment)
}"#;
    let result = smql_parser::parse(smql);
    assert!(
        result.is_err(),
        "CHILDREN block fragment should not parse as standalone"
    );
}

// ===========================================================================
// what-is-smql.md — Example 13: DEFINE MACHINE SupportTicket (complete)
// ===========================================================================

#[test]
fn whatissmql_define_support_ticket_partial() {
    // From what-is-smql.md:
    // DEFINE MACHINE SupportTicket with DATA, one TRANSITION
    // This should be a complete, parseable machine definition.
    let smql = r#"DEFINE MACHINE SupportTicket (
  DATA {
    subject  : TEXT -> REQUIRED
    assignee : REF(Agent) -> OPTIONAL
  }

  STATES { open, triaged, in_progress, resolved, closed }
  INITIAL STATE open
  TERMINAL STATES { closed }

  TRANSITIONS {
    open -> triaged {
      GUARD  : assignee IS SET
      ACTION : NOTIFY(assignee, "ticket.assigned")
    }
  }
)"#;
    let stmts = smql_parser::parse(smql).expect("parse should succeed");
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Statement::Command(Command::DefineMachine(def)) => {
            assert_eq!(def.name, "SupportTicket");
            assert_eq!(def.states.len(), 5);
            assert_eq!(def.initial_state, "open");
            assert!(def.terminal_states.contains(&"closed".to_string()));
            assert_eq!(def.transitions.len(), 1);
            assert_eq!(def.data.len(), 2);
        }
        other => panic!("expected DefineMachine, got {:?}", other),
    }
}

#[tokio::test]
async fn whatissmql_define_and_spawn_support_ticket() {
    // Use the machine definition from what-is-smql.md and test engine round-trip
    let machine_def = r#"DEFINE MACHINE SupportTicket (
  DATA {
    subject  : TEXT -> REQUIRED
    assignee : REF(Agent) -> OPTIONAL
  }
  STATES { open, triaged, in_progress, resolved, closed }
  INITIAL STATE open
  TERMINAL STATES { closed }
  TRANSITIONS {
    open -> triaged {
      GUARD  : assignee IS SET
      ACTION : NOTIFY(assignee, "ticket.assigned")
    }
  }
)"#;
    let engine = engine_from_smql(machine_def);

    // Spawn an instance
    let spawn_result =
        execute_smql(&engine, r#"SPAWN SupportTicket { subject: "Login broken" }"#)
            .await
            .expect("spawn should succeed");
    assert!(spawn_result.starts_with("spawned:"));
    assert!(spawn_result.contains("state:open"));

    // Extract ID
    let id = spawn_result
        .strip_prefix("spawned:")
        .unwrap()
        .split(',')
        .next()
        .unwrap();

    // Transition without assignee should fail (guard: assignee IS SET)
    let t_smql = format!(r#"TRANSITION SupportTicket "{}" TO triaged"#, id);
    let t_result = execute_smql(&engine, &t_smql).await;
    assert!(
        t_result.is_err(),
        "transition without assignee should fail due to GUARD"
    );

    // Transition with assignee should succeed
    let t_smql_with_data = format!(
        r#"TRANSITION SupportTicket "{}" TO triaged WITH {{ assignee: "agent-1" }}"#,
        id
    );
    let t_result = execute_smql(&engine, &t_smql_with_data).await;
    assert!(
        t_result.is_ok(),
        "transition with assignee should succeed: {:?}",
        t_result
    );
}

// ===========================================================================
// what-is-smql.md — Example 14: Timeout transition fragment
// SKIP: Not a complete statement
// ===========================================================================

#[test]
fn whatissmql_timeout_fragment_skip() {
    // From what-is-smql.md:
    // ```sql
    // in_progress -> resolved {
    //   TIMEOUT: 7d -> closed
    // }
    // ```
    // This is a transition body fragment, not a standalone statement.
    let smql = r#"in_progress -> resolved {
  TIMEOUT: 7d -> closed
}"#;
    let result = smql_parser::parse(smql);
    assert!(
        result.is_err(),
        "transition body fragment should not parse as standalone"
    );
}

// ===========================================================================
// why-smql.md — No SMQL code examples
// ===========================================================================

#[test]
fn whysmql_no_code_examples() {
    // why-smql.md contains only prose descriptions of use cases,
    // no SMQL code blocks. Nothing to test.
    // This test exists to document the coverage gap.
}

// ===========================================================================
// Full round-trip: quick-start.md workflow
// Define -> Spawn -> Transition -> Query (using corrected machine def)
// ===========================================================================

#[tokio::test]
async fn quickstart_full_workflow_roundtrip() {
    // This test follows the exact quick-start.md workflow end-to-end,
    // using the corrected machine definition (fields on separate lines).
    let engine = engine_from_smql(TASK_MACHINE);

    // Step 1: Spawn an instance (quick-start.md example)
    let spawn_result =
        execute_smql(&engine, r#"SPAWN Task { title: "Write docs", assignee: "alice" }"#)
            .await
            .expect("spawn should succeed");
    let id = spawn_result
        .strip_prefix("spawned:")
        .unwrap()
        .split(',')
        .next()
        .unwrap();

    // Step 2: Transition to doing (quick-start.md example)
    let transition_smql = format!(r#"TRANSITION Task "{}" TO doing"#, id);
    let t_result = execute_smql(&engine, &transition_smql)
        .await
        .expect("transition should succeed");
    assert!(t_result.contains("from:todo"), "result = {}", t_result);
    assert!(t_result.contains("to:doing"), "result = {}", t_result);

    // Step 3: Query (quick-start.md example)
    let find_result = execute_smql(&engine, "FIND Task WHERE STATE IS doing")
        .await
        .expect("find should succeed");
    // Verify at least one instance was found
    assert!(
        !find_result.contains("Instances([])"),
        "should find doing instances: {}",
        find_result
    );

    // Step 4: Continue to done
    let done_smql = format!(r#"TRANSITION Task "{}" TO done"#, id);
    let done_result = execute_smql(&engine, &done_smql)
        .await
        .expect("transition to done should succeed");
    assert!(done_result.contains("to:done"));

    // Step 5: Verify terminal state -- no more transitions allowed
    let bad_transition = format!(r#"TRANSITION Task "{}" TO todo"#, id);
    let bad_result = execute_smql(&engine, &bad_transition).await;
    assert!(
        bad_result.is_err(),
        "transition from terminal state should fail"
    );
}
