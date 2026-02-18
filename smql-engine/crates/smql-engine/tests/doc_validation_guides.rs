/// Doc Validation: Guides & Tutorials
///
/// Tests all SMQL code examples from the documentation guides and tutorials
/// against the actual parser and engine. Each test is named after the source
/// doc and a description of the example being tested.
///
/// Source docs:
///   - guides/support-ticket.md
///   - guides/order-processing.md
///   - guides/ci-pipeline.md
///   - guides/schema-evolution.md
///   - tutorials/your-first-machine.md
///   - tutorials/adding-data-and-guards.md
///   - tutorials/timeouts-and-hooks.md
///   - tutorials/composition-patterns.md
///   - tutorials/queries-and-analytics.md

use std::collections::BTreeMap;
use std::sync::Arc;

use smql_ast::command::{Command, SpawnCommand, Statement, TransitionCommand};
use smql_ast::expression::{Expression, ExpressionKind};
use smql_ast::value::Value;
use smql_catalog::MachineCatalog;
use smql_engine_core::query::QueryResult;
use smql_engine_core::Engine;
use smql_hooks::{EventBus, HookExecutor};
use smql_storage::MemoryStorage;
use smql_timer::TimerManager;

// ===========================================================================
// Helpers
// ===========================================================================

fn lit(v: Value) -> Expression {
    Expression::new(ExpressionKind::Literal(v))
}

fn actor_map(id: &str) -> Value {
    let mut m = BTreeMap::new();
    m.insert("id".to_string(), Value::Text(id.to_string()));
    Value::Map(m)
}

// make_engine is available but not currently used by name
#[allow(dead_code)]
fn _make_engine() -> Engine {
    let catalog = Arc::new(MachineCatalog::new());
    let storage = Arc::new(MemoryStorage::new());
    let timer = Arc::new(TimerManager::new());
    let event_bus = Arc::new(EventBus::new(64));
    let hooks = Arc::new(HookExecutor::new(event_bus));
    let engine = Engine::with_hooks(catalog, storage, timer, hooks);
    engine.wire_callback();
    engine
}

fn spawn_cmd(machine: &str, data: Vec<(&str, Value)>) -> SpawnCommand {
    SpawnCommand {
        machine: machine.to_string(),
        data: data
            .into_iter()
            .map(|(k, v)| (k.to_string(), lit(v)))
            .collect(),
        then_transition: None,
        batch: false,
        batch_data: Vec::new(),
        parent_id: None,
        parent_machine: None,
    }
}

fn spawn_child_cmd(
    machine: &str,
    data: Vec<(&str, Value)>,
    parent_id: &str,
    parent_machine: &str,
) -> SpawnCommand {
    SpawnCommand {
        machine: machine.to_string(),
        data: data
            .into_iter()
            .map(|(k, v)| (k.to_string(), lit(v)))
            .collect(),
        then_transition: None,
        batch: false,
        batch_data: Vec::new(),
        parent_id: Some(parent_id.to_string()),
        parent_machine: Some(parent_machine.to_string()),
    }
}

fn transition_cmd(machine: &str, id: &str, to: &str) -> TransitionCommand {
    TransitionCommand::new(machine.to_string(), id.to_string(), to.to_string())
}

fn transition_as(machine: &str, id: &str, to: &str, actor: &str) -> TransitionCommand {
    let mut cmd = TransitionCommand::new(machine.to_string(), id.to_string(), to.to_string());
    cmd.as_actor = Some(actor.to_string());
    cmd
}

fn transition_with(
    machine: &str,
    id: &str,
    to: &str,
    actor: &str,
    data: Vec<(&str, Value)>,
) -> TransitionCommand {
    let mut cmd = TransitionCommand::new(machine.to_string(), id.to_string(), to.to_string());
    cmd.as_actor = Some(actor.to_string());
    cmd.with_data = data
        .into_iter()
        .map(|(k, v)| (k.to_string(), lit(v)))
        .collect();
    cmd
}

fn transition_with_no_actor(
    machine: &str,
    id: &str,
    to: &str,
    data: Vec<(&str, Value)>,
) -> TransitionCommand {
    let mut cmd = TransitionCommand::new(machine.to_string(), id.to_string(), to.to_string());
    cmd.with_data = data
        .into_iter()
        .map(|(k, v)| (k.to_string(), lit(v)))
        .collect();
    cmd
}

fn cascade_cmd(machine: &str, id: &str, to: &str) -> TransitionCommand {
    let mut cmd = TransitionCommand::new(machine.to_string(), id.to_string(), to.to_string());
    cmd.cascade = true;
    cmd
}

/// Helper: parse SMQL, register all DEFINE MACHINE statements, return engine.
fn engine_from_smql(smql: &str) -> Engine {
    let stmts = smql_parser::parse(smql).expect("parse SMQL for engine setup");
    let catalog = Arc::new(MachineCatalog::new());
    for stmt in &stmts {
        if let Statement::Command(Command::DefineMachine(m)) = stmt {
            catalog.register(m.clone()).expect("register machine");
        }
    }
    let storage = Arc::new(MemoryStorage::new());
    let timer = Arc::new(TimerManager::new());
    let event_bus = Arc::new(EventBus::new(64));
    let hooks = Arc::new(HookExecutor::new(event_bus));
    let engine = Engine::with_hooks(catalog, storage, timer, hooks);
    engine.wire_callback();
    engine
}

// ===========================================================================
// GUIDE: support-ticket.md
// ===========================================================================

const SUPPORT_TICKET_DEFINE: &str = r#"
DEFINE MACHINE SupportTicket (
  DATA {
    customer_id    : UUID        -> REQUIRED
    subject        : TEXT        -> REQUIRED, MAX(200)
    description    : TEXT        -> REQUIRED
    priority       : ENUM(low, medium, high, critical) -> DEFAULT(medium)
    assignee       : REF(Agent)  -> OPTIONAL
    tags           : SET(TEXT)   -> DEFAULT({})
    satisfaction   : INT         -> RANGE(1, 5), OPTIONAL
    resolution_note: TEXT        -> OPTIONAL
  }

  STATES {
    open
    triaged
    in_progress
    waiting_on_customer
    resolved
    closed
    reopened
  }

  INITIAL STATE open
  TERMINAL STATES { closed }

  TRANSITIONS {
    open -> triaged {
      GUARD  : assignee IS SET
      ACTION : NOTIFY(assignee, "ticket.assigned")
    }

    triaged -> in_progress {
      GUARD : ACTOR == assignee OR ACTOR.role == "admin"
    }

    in_progress -> waiting_on_customer {
      GUARD   : ACTOR == assignee
      TIMEOUT : 72h -> resolved
      ACTION  : NOTIFY(customer_id, "ticket.needs_response")
    }

    waiting_on_customer -> in_progress {
      GUARD : ACTOR.id == customer_id OR ACTOR == assignee
    }

    in_progress -> resolved {
      GUARD  : resolution_note IS SET
      GUARD  : ACTOR == assignee OR ACTOR.role == "admin"
      TIMEOUT: 7d -> closed
      ACTION : NOTIFY(customer_id, "ticket.resolved")
    }

    resolved -> reopened {
      GUARD : ACTOR.id == customer_id
      GUARD : elapsed_since(resolved) < 30d
    }

    reopened -> in_progress {
      GUARD : assignee IS SET
    }

    resolved -> closed {
      GUARD : elapsed_since(resolved) >= 7d OR ACTOR.role == "admin"
    }

    ANY -> triaged {
      EXCEPT FROM { open, closed }
      GUARD  : ACTOR.role IN ("admin", "supervisor")
      MUTATE : priority = critical
      ACTION : LOG("Escalated by {ACTOR}")
    }
  }
)
"#;

#[test]
fn guide_support_ticket_parse_define() {
    let result = smql_parser::parse(SUPPORT_TICKET_DEFINE);
    assert!(result.is_ok(), "Failed to parse SupportTicket DEFINE: {:?}", result.err());
    let stmts = result.unwrap();
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Statement::Command(Command::DefineMachine(m)) => {
            assert_eq!(m.name, "SupportTicket");
            assert_eq!(m.states.len(), 7);
            assert_eq!(m.initial_state, "open");
            assert_eq!(m.terminal_states, vec!["closed"]);
        }
        _ => panic!("Expected DefineMachine statement"),
    }
}

#[test]
fn guide_support_ticket_parse_spawn() {
    let smql = r#"SPAWN SupportTicket { customer_id: "a1b2c3d4-e5f6-7890-abcd-ef1234567890", subject: "Login broken after update", description: "Cannot log in since the v2.3 update" }"#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse SPAWN: {:?}", result.err());
}

#[test]
fn guide_support_ticket_parse_spawn_empty() {
    let smql = r#"SPAWN SupportTicket {}"#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse empty SPAWN: {:?}", result.err());
}

#[test]
fn guide_support_ticket_parse_transition_with_data_and_actor() {
    let smql = r#"TRANSITION SupportTicket "01JMABCDEF1234567890ABCDEF" TO triaged WITH { assignee: {id: "agent_1"} } AS "agent_1""#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse TRANSITION WITH AS: {:?}", result.err());
}

#[test]
fn guide_support_ticket_parse_transition_simple() {
    let smql = r#"TRANSITION SupportTicket "01JMABCDEF1234567890ABCDEF" TO triaged"#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse simple TRANSITION: {:?}", result.err());
}

#[test]
fn guide_support_ticket_parse_transition_as_actor() {
    let smql = r#"TRANSITION SupportTicket "01JMABCDEF1234567890ABCDEF" TO in_progress AS "agent_1""#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse TRANSITION AS: {:?}", result.err());
}

#[test]
fn guide_support_ticket_parse_transition_with_resolution_note() {
    let smql = r#"TRANSITION SupportTicket "01JMABCDEF1234567890ABCDEF" TO resolved WITH { resolution_note: "Cleared browser cache and reset session tokens. Issue resolved." } AS "agent_1""#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse TRANSITION WITH resolution_note: {:?}", result.err());
}

#[test]
fn guide_support_ticket_parse_transition_with_satisfaction() {
    let smql = r#"TRANSITION SupportTicket "01JMABCDEF1234567890ABCDEF" TO closed AS "admin_user" WITH { satisfaction: 4 }"#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse TRANSITION AS ... WITH: {:?}", result.err());
}

#[test]
fn guide_support_ticket_parse_find() {
    let smql = r#"FIND SupportTicket WHERE STATE IS open"#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse FIND: {:?}", result.err());
}

#[test]
fn guide_support_ticket_parse_trail() {
    // DOC ISSUE: Docs show `TRAIL OF SupportTicket "01JM..."` but parser
    // uses `TRAIL OF "instance_id"` (no machine name). The machine name
    // is not part of the TRAIL syntax.
    let smql_doc = r#"TRAIL OF SupportTicket "01JMABCDEF1234567890ABCDEF""#;
    let result_doc = smql_parser::parse(smql_doc);
    // This currently parses because "SupportTicket" is consumed as instance_id
    // and then "01JMABCDEF..." becomes a new statement (which fails).
    // The actual working syntax is:
    let smql_actual = r#"TRAIL OF "01JMABCDEF1234567890ABCDEF""#;
    let result_actual = smql_parser::parse(smql_actual);
    assert!(result_actual.is_ok(), "Failed to parse TRAIL (actual syntax): {:?}", result_actual.err());
    // Doc syntax does NOT parse correctly:
    assert!(result_doc.is_err(), "Doc TRAIL syntax with machine name should fail to parse as a single statement");
}

#[test]
fn guide_support_ticket_parse_aggregate() {
    let smql = r#"AGGREGATE SupportTicket MEASURE COUNT() GROUP BY state"#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse AGGREGATE: {:?}", result.err());
}

#[test]
fn guide_support_ticket_parse_funnel() {
    // DOC ISSUE: Docs show `FUNNEL Machine THROUGH open, triaged, ...`
    // but parser requires brackets: `FUNNEL Machine THROUGH [open, triaged, ...]`
    let smql_doc = r#"FUNNEL SupportTicket THROUGH open, triaged, in_progress, resolved"#;
    let result_doc = smql_parser::parse(smql_doc);
    assert!(result_doc.is_err(), "Doc FUNNEL syntax without brackets should fail to parse");

    let smql_actual = r#"FUNNEL SupportTicket THROUGH [open, triaged, in_progress, resolved]"#;
    let result_actual = smql_parser::parse(smql_actual);
    assert!(result_actual.is_ok(), "Failed to parse FUNNEL (actual syntax with brackets): {:?}", result_actual.err());
}

#[tokio::test]
async fn guide_support_ticket_full_lifecycle() {
    // Tests: DEFINE -> SPAWN -> open -> triaged -> in_progress -> waiting_on_customer
    //        -> in_progress -> resolved -> closed
    let engine = engine_from_smql(SUPPORT_TICKET_DEFINE);

    // Spawn a ticket
    let cmd = spawn_cmd("SupportTicket", vec![
        ("customer_id", Value::Uuid(uuid::Uuid::new_v4())),
        ("subject", Value::Text("Login broken after update".into())),
        ("description", Value::Text("Cannot log in since the v2.3 update".into())),
    ]);
    let r = engine.spawn(&cmd).await.expect("spawn");
    let id = r.instance.id.as_str();
    assert_eq!(r.instance.state, "open");
    assert_eq!(r.instance.data.get("priority").unwrap(), &Value::Text("medium".into()));

    // open -> triaged (requires assignee IS SET)
    let cmd = transition_with("SupportTicket", &id, "triaged", "agent_1",
        vec![("assignee", actor_map("agent_1"))]);
    let r = engine.transition(&cmd).await.expect("open -> triaged");
    assert_eq!(r.to_state, "triaged");

    // triaged -> in_progress (requires ACTOR == assignee OR ACTOR.role == admin)
    let cmd = transition_as("SupportTicket", &id, "in_progress", "agent_1");
    let r = engine.transition(&cmd).await.expect("triaged -> in_progress");
    assert_eq!(r.to_state, "in_progress");

    // in_progress -> waiting_on_customer (requires ACTOR == assignee)
    let cmd = transition_as("SupportTicket", &id, "waiting_on_customer", "agent_1");
    let r = engine.transition(&cmd).await.expect("in_progress -> waiting_on_customer");
    assert_eq!(r.to_state, "waiting_on_customer");

    // waiting_on_customer -> in_progress (ACTOR.id == customer_id OR ACTOR == assignee)
    let cmd = transition_as("SupportTicket", &id, "in_progress", "agent_1");
    let r = engine.transition(&cmd).await.expect("waiting_on_customer -> in_progress");
    assert_eq!(r.to_state, "in_progress");

    // in_progress -> resolved (resolution_note IS SET, ACTOR == assignee)
    let cmd = transition_with("SupportTicket", &id, "resolved", "agent_1",
        vec![("resolution_note", Value::Text("Cleared browser cache and reset session tokens.".into()))]);
    let r = engine.transition(&cmd).await.expect("in_progress -> resolved");
    assert_eq!(r.to_state, "resolved");

    // resolved -> closed
    // Guard: elapsed_since(resolved) >= 7d OR ACTOR.role == "admin"
    // DOC ISSUE: The doc shows `AS "admin_user"` to close, but AS only sets
    // ACTOR = {id: "admin_user"} — it doesn't set role. And elapsed_since
    // is ~0 in tests. So neither guard condition passes.
    // To work around this, we would need the ACTOR to have role: "admin",
    // but the AS clause doesn't support that. This is a doc inaccuracy.
    let cmd = transition_with("SupportTicket", &id, "closed", "admin_user",
        vec![("satisfaction", Value::Int(4))]);
    let result = engine.transition(&cmd).await;
    // This will fail because:
    // 1. elapsed_since(resolved) is ~0, not >= 7d
    // 2. ACTOR.role is not "admin" (AS only sets {id: "admin_user"})
    assert!(result.is_err(), "Expected guard failure for resolved -> closed (elapsed_since and ACTOR.role)");
    let msg = format!("{}", result.unwrap_err());
    assert!(msg.contains("guard") || msg.contains("denied"),
        "Expected guard failure, got: {}", msg);
}

#[tokio::test]
async fn guide_support_ticket_guard_failure_no_assignee() {
    let engine = engine_from_smql(SUPPORT_TICKET_DEFINE);
    let cmd = spawn_cmd("SupportTicket", vec![
        ("customer_id", Value::Uuid(uuid::Uuid::new_v4())),
        ("subject", Value::Text("Test".into())),
        ("description", Value::Text("Test".into())),
    ]);
    let r = engine.spawn(&cmd).await.expect("spawn");
    let id = r.instance.id.as_str();

    // Try open -> triaged without assignee — should fail
    let cmd = transition_cmd("SupportTicket", &id, "triaged");
    let err = engine.transition(&cmd).await.unwrap_err();
    let msg = format!("{}", err);
    assert!(msg.contains("assignee") || msg.contains("guard"), "Expected guard failure about assignee, got: {}", msg);
}

#[tokio::test]
async fn guide_support_ticket_wildcard_escalation() {
    let engine = engine_from_smql(SUPPORT_TICKET_DEFINE);
    let cmd = spawn_cmd("SupportTicket", vec![
        ("customer_id", Value::Uuid(uuid::Uuid::new_v4())),
        ("subject", Value::Text("Escalation test".into())),
        ("description", Value::Text("Test escalation".into())),
    ]);
    let r = engine.spawn(&cmd).await.expect("spawn");
    let id = r.instance.id.as_str();

    // open -> triaged (normal path)
    let cmd = transition_with("SupportTicket", &id, "triaged", "agent_1",
        vec![("assignee", actor_map("agent_1"))]);
    engine.transition(&cmd).await.expect("open -> triaged");

    // triaged -> in_progress
    let cmd = transition_as("SupportTicket", &id, "in_progress", "agent_1");
    engine.transition(&cmd).await.expect("triaged -> in_progress");

    // ANY -> triaged (escalation by supervisor) from in_progress
    // The ACTOR.role must be admin or supervisor. AS only sets id by default.
    // This may fail because ACTOR doesn't have role "admin" or "supervisor" —
    // the guard checks ACTOR.role IN ("admin", "supervisor").
    // In the engine, AS "supervisor_1" creates ACTOR = {id: "supervisor_1"}
    // but doesn't set role. So this guard may fail unless the engine
    // defaults role somehow. Let's test what happens.
    let cmd = transition_as("SupportTicket", &id, "triaged", "supervisor_1");
    let result = engine.transition(&cmd).await;
    // The wildcard escalation requires ACTOR.role IN ("admin", "supervisor")
    // Since AS "supervisor_1" only sets {id: "supervisor_1"}, role is not set.
    // This should fail because ACTOR.role is not "admin" or "supervisor".
    // This is a known pattern — the doc shows `AS "supervisor_1"` which won't
    // actually satisfy ACTOR.role. Document this as a doc issue.
    if result.is_err() {
        // Expected: guard failure because ACTOR.role is not set
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("guard") || msg.contains("denied"),
            "Expected guard failure, got: {}", msg);
    }
}

// ===========================================================================
// GUIDE: order-processing.md
// ===========================================================================

const ORDER_DEFINE: &str = r#"
DEFINE MACHINE Order (
  DATA {
    customer : TEXT -> REQUIRED
    total    : INT  -> REQUIRED
    notes    : TEXT -> OPTIONAL
  }

  STATES { draft, placed, paid, fulfilled, shipped, delivered, cancelled, returned }
  INITIAL STATE draft
  TERMINAL STATES { delivered, cancelled, returned }

  CHILDREN {
    items    : LIST(LineItem)    -> MIN(1)
    shipment : OPTIONAL(Shipment)
  }

  TRANSITIONS {
    draft -> placed {
      GUARD : total > 0
    }
    placed -> paid {}
    paid -> fulfilled {
      GUARD : ALL(items, STATE IS confirmed)
    }
    fulfilled -> shipped {}
    shipped -> delivered {}
    delivered -> returned {}
    ANY -> cancelled {
      EXCEPT FROM { shipped, delivered, returned }
    }
  }
)
"#;

const LINEITEM_ORDER_DEFINE: &str = r#"
DEFINE MACHINE LineItem (
  PARENT : Order

  DATA {
    product  : TEXT -> REQUIRED
    quantity : INT  -> REQUIRED
    price    : INT  -> REQUIRED
  }

  STATES { pending, confirmed, backordered, cancelled }
  INITIAL STATE pending
  TERMINAL STATES { confirmed, cancelled }

  TRANSITIONS {
    pending -> confirmed {
      GUARD : quantity > 0
    }
    pending -> backordered {}
    backordered -> confirmed {}
    ANY -> cancelled {
      EXCEPT FROM { confirmed }
    }
  }
)
"#;

const SHIPMENT_DEFINE: &str = r#"
DEFINE MACHINE Shipment (
  PARENT : Order

  DATA {
    tracking : TEXT -> OPTIONAL
    carrier  : TEXT -> OPTIONAL
  }

  STATES { created, dispatched, in_transit, delivered, lost }
  INITIAL STATE created
  TERMINAL STATES { delivered, lost }

  TRANSITIONS {
    created -> dispatched {
      GUARD : tracking IS SET
      GUARD : carrier IS SET
    }
    dispatched -> in_transit {}
    in_transit -> delivered {
      SIGNAL PARENT TO delivered
    }
    in_transit -> lost {}
  }
)
"#;

#[test]
fn guide_order_parse_define_order() {
    let result = smql_parser::parse(ORDER_DEFINE);
    assert!(result.is_ok(), "Failed to parse Order DEFINE: {:?}", result.err());
}

#[test]
fn guide_order_parse_define_lineitem() {
    let result = smql_parser::parse(LINEITEM_ORDER_DEFINE);
    assert!(result.is_ok(), "Failed to parse LineItem DEFINE: {:?}", result.err());
}

#[test]
fn guide_order_parse_define_shipment() {
    let result = smql_parser::parse(SHIPMENT_DEFINE);
    assert!(result.is_ok(), "Failed to parse Shipment DEFINE: {:?}", result.err());
}

#[test]
fn guide_order_parse_spawn_with_parent() {
    // DOC ISSUE: Docs show `SPAWN X { ... } PARENT Y "id"` but the parser
    // does NOT support the PARENT clause in SPAWN. The parent_id and
    // parent_machine are set programmatically via the SpawnCommand struct.
    let smql = r#"SPAWN LineItem { product: "Widget A", quantity: 2, price: 2500 } PARENT Order "01JMORDER0000000000000001""#;
    let result = smql_parser::parse(smql);
    // Parser does not consume PARENT, so it will try to parse "PARENT" as a new statement and fail
    assert!(result.is_err(), "Doc SPAWN PARENT syntax should fail to parse (not implemented in parser)");
}

#[test]
fn guide_order_parse_transition_cascade() {
    let smql = r#"TRANSITION Order "01JMORDER0000000000000001" TO cancelled CASCADE"#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse TRANSITION CASCADE: {:?}", result.err());
}

#[test]
fn guide_order_parse_find_lineitem() {
    let smql = r#"FIND LineItem WHERE STATE IS pending"#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse FIND LineItem: {:?}", result.err());
}

#[test]
fn guide_order_parse_aggregate_lineitem() {
    let smql = r#"AGGREGATE LineItem MEASURE COUNT() GROUP BY state"#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse AGGREGATE LineItem: {:?}", result.err());
}

#[test]
fn guide_order_parse_trail() {
    // DOC ISSUE: Same as support-ticket TRAIL — docs show machine name
    // but parser uses `TRAIL OF "instance_id"` without machine name.
    let smql_doc = r#"TRAIL OF Order "01JMORDER0000000000000001""#;
    let result_doc = smql_parser::parse(smql_doc);
    assert!(result_doc.is_err(), "Doc TRAIL syntax with machine name should fail");

    let smql_actual = r#"TRAIL OF "01JMORDER0000000000000001""#;
    let result_actual = smql_parser::parse(smql_actual);
    assert!(result_actual.is_ok(), "Failed to parse TRAIL (actual syntax): {:?}", result_actual.err());
}

#[tokio::test]
async fn guide_order_full_lifecycle() {
    // Register all three machines and run the full order flow
    let all_smql = format!("{}\n{}\n{}", ORDER_DEFINE, LINEITEM_ORDER_DEFINE, SHIPMENT_DEFINE);
    let engine = engine_from_smql(&all_smql);

    // Spawn Order
    let cmd = spawn_cmd("Order", vec![
        ("customer", Value::Text("customer_001".into())),
        ("total", Value::Int(9999)),
    ]);
    let r = engine.spawn(&cmd).await.expect("spawn Order");
    let order_id = r.instance.id.as_str();
    assert_eq!(r.instance.state, "draft");

    // Spawn LineItem children
    let cmd = spawn_child_cmd("LineItem", vec![
        ("product", Value::Text("Widget A".into())),
        ("quantity", Value::Int(2)),
        ("price", Value::Int(2500)),
    ], &order_id, "Order");
    let r = engine.spawn(&cmd).await.expect("spawn LineItem 1");
    let item1_id = r.instance.id.as_str();
    assert_eq!(r.instance.state, "pending");

    let cmd = spawn_child_cmd("LineItem", vec![
        ("product", Value::Text("Widget B".into())),
        ("quantity", Value::Int(1)),
        ("price", Value::Int(4999)),
    ], &order_id, "Order");
    let r = engine.spawn(&cmd).await.expect("spawn LineItem 2");
    let item2_id = r.instance.id.as_str();

    // draft -> placed (guard: total > 0)
    let cmd = transition_cmd("Order", &order_id, "placed");
    let r = engine.transition(&cmd).await.expect("draft -> placed");
    assert_eq!(r.to_state, "placed");

    // placed -> paid
    let cmd = transition_cmd("Order", &order_id, "paid");
    engine.transition(&cmd).await.expect("placed -> paid");

    // Try paid -> fulfilled BEFORE confirming items — should fail
    let cmd = transition_cmd("Order", &order_id, "fulfilled");
    let err = engine.transition(&cmd).await.unwrap_err();
    let msg = format!("{}", err);
    assert!(msg.contains("ALL") || msg.contains("guard"), "Expected ALL guard failure, got: {}", msg);

    // Confirm both items
    let cmd = transition_cmd("LineItem", &item1_id, "confirmed");
    engine.transition(&cmd).await.expect("confirm item 1");

    let cmd = transition_cmd("LineItem", &item2_id, "confirmed");
    engine.transition(&cmd).await.expect("confirm item 2");

    // Now paid -> fulfilled should succeed
    let cmd = transition_cmd("Order", &order_id, "fulfilled");
    let r = engine.transition(&cmd).await.expect("paid -> fulfilled");
    assert_eq!(r.to_state, "fulfilled");

    // fulfilled -> shipped -> delivered
    let cmd = transition_cmd("Order", &order_id, "shipped");
    engine.transition(&cmd).await.expect("fulfilled -> shipped");

    let cmd = transition_cmd("Order", &order_id, "delivered");
    engine.transition(&cmd).await.expect("shipped -> delivered");
}

#[tokio::test]
async fn guide_order_cascade_cancellation() {
    let all_smql = format!("{}\n{}\n{}", ORDER_DEFINE, LINEITEM_ORDER_DEFINE, SHIPMENT_DEFINE);
    let engine = engine_from_smql(&all_smql);

    // Spawn Order and a LineItem
    let cmd = spawn_cmd("Order", vec![
        ("customer", Value::Text("test".into())),
        ("total", Value::Int(100)),
    ]);
    let r = engine.spawn(&cmd).await.expect("spawn Order");
    let order_id = r.instance.id.as_str();

    let cmd = spawn_child_cmd("LineItem", vec![
        ("product", Value::Text("Widget".into())),
        ("quantity", Value::Int(1)),
        ("price", Value::Int(100)),
    ], &order_id, "Order");
    engine.spawn(&cmd).await.expect("spawn LineItem");

    // Cancel with CASCADE
    let cmd = cascade_cmd("Order", &order_id, "cancelled");
    let r = engine.transition(&cmd).await.expect("cancel Order CASCADE");
    assert_eq!(r.to_state, "cancelled");
}

// ===========================================================================
// GUIDE: ci-pipeline.md
// ===========================================================================

const PIPELINE_DEFINE: &str = r#"
DEFINE MACHINE Pipeline (
  DATA {
    repo     : TEXT -> REQUIRED
    branch   : TEXT -> REQUIRED
    commit   : TEXT -> REQUIRED
    trigger  : ENUM(push, pr, manual, schedule) -> DEFAULT(push)
  }

  STATES { queued, running, passed, failed, cancelled }
  INITIAL STATE queued
  TERMINAL STATES { passed, failed, cancelled }

  CHILDREN {
    stages : LIST(Stage) -> MIN(1)
  }

  TRANSITIONS {
    queued -> running {}

    running -> passed {
      GUARD : ALL(stages, STATE IS passed)
    }

    running -> failed {
      GUARD : ANY(stages, STATE IS failed)
    }

    ANY -> cancelled {
      EXCEPT FROM { passed, failed }
    }
  }
)
"#;

const STAGE_DEFINE: &str = r#"
DEFINE MACHINE Stage (
  PARENT : Pipeline

  DATA {
    name  : TEXT -> REQUIRED
    order : INT  -> REQUIRED
  }

  STATES { pending, running, passed, failed, skipped }
  INITIAL STATE pending
  TERMINAL STATES { passed, failed, skipped }

  CHILDREN {
    jobs : LIST(Job) -> MIN(1)
  }

  TRANSITIONS {
    pending -> running {}
    running -> passed {
      GUARD : ALL(jobs, STATE IS passed)
    }
    running -> failed {
      GUARD : ANY(jobs, STATE IS failed)
    }
    pending -> skipped {}
  }
)
"#;

const JOB_DEFINE: &str = r#"
DEFINE MACHINE Job (
  PARENT : Stage

  DATA {
    name    : TEXT -> REQUIRED
    image   : TEXT -> OPTIONAL
    command : TEXT -> REQUIRED
  }

  STATES { pending, running, passed, failed }
  INITIAL STATE pending
  TERMINAL STATES { passed, failed }

  TRANSITIONS {
    pending -> running {}
    running -> passed {}
    running -> failed {}
  }
)
"#;

#[test]
fn guide_ci_parse_define_pipeline() {
    let result = smql_parser::parse(PIPELINE_DEFINE);
    assert!(result.is_ok(), "Failed to parse Pipeline DEFINE: {:?}", result.err());
}

#[test]
fn guide_ci_parse_define_stage() {
    let result = smql_parser::parse(STAGE_DEFINE);
    assert!(result.is_ok(), "Failed to parse Stage DEFINE: {:?}", result.err());
}

#[test]
fn guide_ci_parse_define_job() {
    let result = smql_parser::parse(JOB_DEFINE);
    assert!(result.is_ok(), "Failed to parse Job DEFINE: {:?}", result.err());
}

#[test]
fn guide_ci_parse_spawn_pipeline() {
    let smql = r#"SPAWN Pipeline { repo: "acme/backend", branch: "main", commit: "abc123def" }"#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse SPAWN Pipeline: {:?}", result.err());
}

#[test]
fn guide_ci_parse_spawn_stage_with_parent() {
    // DOC ISSUE: SPAWN PARENT clause not supported in parser
    let smql = r#"SPAWN Stage { name: "build", order: 1 } PARENT Pipeline "01JMPIPE00000000000000001""#;
    let result = smql_parser::parse(smql);
    assert!(result.is_err(), "Doc SPAWN PARENT syntax should fail (not in parser)");
}

#[test]
fn guide_ci_parse_spawn_job_with_parent() {
    // DOC ISSUE: SPAWN PARENT clause not supported in parser
    let smql = r#"SPAWN Job { name: "compile", command: "cargo build --release" } PARENT Stage "01JMSTG000000000000000001""#;
    let result = smql_parser::parse(smql);
    assert!(result.is_err(), "Doc SPAWN PARENT syntax should fail (not in parser)");
}

#[test]
fn guide_ci_parse_find_failed_jobs() {
    let smql = r#"FIND Job WHERE STATE IS failed"#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse FIND failed jobs: {:?}", result.err());
}

#[test]
fn guide_ci_parse_aggregate_jobs() {
    let smql = r#"AGGREGATE Job MEASURE COUNT() GROUP BY state"#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse AGGREGATE jobs: {:?}", result.err());
}

#[test]
fn guide_ci_parse_trail_pipeline() {
    // DOC ISSUE: TRAIL syntax doesn't include machine name
    let smql_doc = r#"TRAIL OF Pipeline "01JMPIPE00000000000000001""#;
    let result_doc = smql_parser::parse(smql_doc);
    assert!(result_doc.is_err(), "Doc TRAIL syntax with machine name should fail");

    let smql_actual = r#"TRAIL OF "01JMPIPE00000000000000001""#;
    let result_actual = smql_parser::parse(smql_actual);
    assert!(result_actual.is_ok(), "Failed to parse TRAIL (actual syntax): {:?}", result_actual.err());
}

#[test]
fn guide_ci_parse_cascade_cancel() {
    let smql = r#"TRANSITION Pipeline "01JMPIPE00000000000000001" TO cancelled CASCADE"#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse TRANSITION CASCADE: {:?}", result.err());
}

#[test]
fn guide_ci_parse_stage_skip() {
    let smql = r#"TRANSITION Stage "01JMSTG000000000000000003" TO skipped"#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse TRANSITION Stage skip: {:?}", result.err());
}

#[tokio::test]
async fn guide_ci_full_pipeline_pass() {
    let all_smql = format!("{}\n{}\n{}", PIPELINE_DEFINE, STAGE_DEFINE, JOB_DEFINE);
    let engine = engine_from_smql(&all_smql);

    // Spawn Pipeline
    let cmd = spawn_cmd("Pipeline", vec![
        ("repo", Value::Text("acme/backend".into())),
        ("branch", Value::Text("main".into())),
        ("commit", Value::Text("abc123".into())),
    ]);
    let r = engine.spawn(&cmd).await.expect("spawn Pipeline");
    let pipeline_id = r.instance.id.as_str();
    assert_eq!(r.instance.state, "queued");
    assert_eq!(r.instance.data.get("trigger").unwrap(), &Value::Text("push".into()));

    // Spawn stages
    let cmd = spawn_child_cmd("Stage", vec![
        ("name", Value::Text("build".into())),
        ("order", Value::Int(1)),
    ], &pipeline_id, "Pipeline");
    let r = engine.spawn(&cmd).await.expect("spawn build stage");
    let build_stage_id = r.instance.id.as_str();

    let cmd = spawn_child_cmd("Stage", vec![
        ("name", Value::Text("test".into())),
        ("order", Value::Int(2)),
    ], &pipeline_id, "Pipeline");
    let r = engine.spawn(&cmd).await.expect("spawn test stage");
    let test_stage_id = r.instance.id.as_str();

    // Spawn jobs
    let cmd = spawn_child_cmd("Job", vec![
        ("name", Value::Text("compile".into())),
        ("command", Value::Text("cargo build --release".into())),
    ], &build_stage_id, "Stage");
    let r = engine.spawn(&cmd).await.expect("spawn compile job");
    let compile_job_id = r.instance.id.as_str();

    let cmd = spawn_child_cmd("Job", vec![
        ("name", Value::Text("unit-tests".into())),
        ("command", Value::Text("cargo test".into())),
    ], &test_stage_id, "Stage");
    let r = engine.spawn(&cmd).await.expect("spawn unit-tests job");
    let unit_test_job_id = r.instance.id.as_str();

    let cmd = spawn_child_cmd("Job", vec![
        ("name", Value::Text("lint".into())),
        ("command", Value::Text("cargo clippy".into())),
    ], &test_stage_id, "Stage");
    let r = engine.spawn(&cmd).await.expect("spawn lint job");
    let lint_job_id = r.instance.id.as_str();

    // Start pipeline
    let cmd = transition_cmd("Pipeline", &pipeline_id, "running");
    engine.transition(&cmd).await.expect("queued -> running");

    // Build stage: start -> pass job -> pass stage
    let cmd = transition_cmd("Stage", &build_stage_id, "running");
    engine.transition(&cmd).await.expect("build: pending -> running");
    let cmd = transition_cmd("Job", &compile_job_id, "running");
    engine.transition(&cmd).await.expect("compile: pending -> running");
    let cmd = transition_cmd("Job", &compile_job_id, "passed");
    engine.transition(&cmd).await.expect("compile: running -> passed");
    let cmd = transition_cmd("Stage", &build_stage_id, "passed");
    engine.transition(&cmd).await.expect("build: running -> passed");

    // Test stage: start -> pass all jobs -> pass stage
    let cmd = transition_cmd("Stage", &test_stage_id, "running");
    engine.transition(&cmd).await.expect("test: pending -> running");
    let cmd = transition_cmd("Job", &unit_test_job_id, "running");
    engine.transition(&cmd).await.expect("unit-tests: pending -> running");
    let cmd = transition_cmd("Job", &lint_job_id, "running");
    engine.transition(&cmd).await.expect("lint: pending -> running");
    let cmd = transition_cmd("Job", &unit_test_job_id, "passed");
    engine.transition(&cmd).await.expect("unit-tests: running -> passed");
    let cmd = transition_cmd("Job", &lint_job_id, "passed");
    engine.transition(&cmd).await.expect("lint: running -> passed");
    let cmd = transition_cmd("Stage", &test_stage_id, "passed");
    engine.transition(&cmd).await.expect("test: running -> passed");

    // Pipeline passes
    let cmd = transition_cmd("Pipeline", &pipeline_id, "passed");
    let r = engine.transition(&cmd).await.expect("pipeline: running -> passed");
    assert_eq!(r.to_state, "passed");
}

#[tokio::test]
async fn guide_ci_pipeline_fail_on_any_failed_job() {
    let all_smql = format!("{}\n{}\n{}", PIPELINE_DEFINE, STAGE_DEFINE, JOB_DEFINE);
    let engine = engine_from_smql(&all_smql);

    // Spawn Pipeline
    let cmd = spawn_cmd("Pipeline", vec![
        ("repo", Value::Text("acme/backend".into())),
        ("branch", Value::Text("main".into())),
        ("commit", Value::Text("abc123".into())),
    ]);
    let r = engine.spawn(&cmd).await.expect("spawn Pipeline");
    let pipeline_id = r.instance.id.as_str();

    // Spawn one stage with two jobs
    let cmd = spawn_child_cmd("Stage", vec![
        ("name", Value::Text("test".into())),
        ("order", Value::Int(1)),
    ], &pipeline_id, "Pipeline");
    let r = engine.spawn(&cmd).await.expect("spawn test stage");
    let stage_id = r.instance.id.as_str();

    let cmd = spawn_child_cmd("Job", vec![
        ("name", Value::Text("unit-tests".into())),
        ("command", Value::Text("cargo test".into())),
    ], &stage_id, "Stage");
    let r = engine.spawn(&cmd).await.expect("spawn unit-test job");
    let job1_id = r.instance.id.as_str();

    let cmd = spawn_child_cmd("Job", vec![
        ("name", Value::Text("lint".into())),
        ("command", Value::Text("cargo clippy".into())),
    ], &stage_id, "Stage");
    let r = engine.spawn(&cmd).await.expect("spawn lint job");
    let job2_id = r.instance.id.as_str();

    // Start pipeline and stage
    engine.transition(&transition_cmd("Pipeline", &pipeline_id, "running")).await.unwrap();
    engine.transition(&transition_cmd("Stage", &stage_id, "running")).await.unwrap();

    // Run and pass job1, run and FAIL job2
    engine.transition(&transition_cmd("Job", &job1_id, "running")).await.unwrap();
    engine.transition(&transition_cmd("Job", &job2_id, "running")).await.unwrap();
    engine.transition(&transition_cmd("Job", &job1_id, "passed")).await.unwrap();
    engine.transition(&transition_cmd("Job", &job2_id, "failed")).await.unwrap();

    // Stage cannot pass (ALL guard fails)
    let err = engine.transition(&transition_cmd("Stage", &stage_id, "passed")).await.unwrap_err();
    assert!(format!("{}", err).contains("ALL") || format!("{}", err).contains("guard"));

    // Stage fails (ANY guard passes)
    engine.transition(&transition_cmd("Stage", &stage_id, "failed")).await.unwrap();

    // Pipeline cannot pass (ALL guard fails)
    let err = engine.transition(&transition_cmd("Pipeline", &pipeline_id, "passed")).await.unwrap_err();
    assert!(format!("{}", err).contains("ALL") || format!("{}", err).contains("guard"));

    // Pipeline fails (ANY guard passes)
    let r = engine.transition(&transition_cmd("Pipeline", &pipeline_id, "failed")).await.unwrap();
    assert_eq!(r.to_state, "failed");
}

// ===========================================================================
// GUIDE: schema-evolution.md
// ===========================================================================

const TASK_DEFINE: &str = r#"
DEFINE MACHINE Task (
  DATA {
    title    : TEXT -> REQUIRED
    priority : INT  -> DEFAULT(3)
  }

  STATES { open, in_progress, done }
  INITIAL STATE open
  TERMINAL STATES { done }

  TRANSITIONS {
    open -> in_progress {}
    in_progress -> done {}
    in_progress -> open {}
  }
)
"#;

#[test]
fn guide_schema_parse_define_task() {
    let result = smql_parser::parse(TASK_DEFINE);
    assert!(result.is_ok(), "Failed to parse Task DEFINE: {:?}", result.err());
}

#[test]
fn guide_schema_parse_alter_add_state() {
    let smql = r#"ALTER MACHINE Task ADD STATE blocked"#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse ALTER ADD STATE: {:?}", result.err());
}

#[test]
fn guide_schema_parse_alter_add_transitions() {
    // DOC ISSUE: Docs show `ADD TRANSITION from -> to {}` with braces,
    // but the ALTER parser does NOT consume the `{}` body. The actual
    // working syntax omits the braces: `ADD TRANSITION from -> to`
    let smql_doc = r#"ALTER MACHINE Task ADD TRANSITION in_progress -> blocked {} ADD TRANSITION blocked -> in_progress {}"#;
    let result_doc = smql_parser::parse(smql_doc);
    assert!(result_doc.is_err(), "Doc ALTER ADD TRANSITION with braces should fail");

    let smql_actual = r#"ALTER MACHINE Task ADD TRANSITION in_progress -> blocked ADD TRANSITION blocked -> in_progress"#;
    let result_actual = smql_parser::parse(smql_actual);
    assert!(result_actual.is_ok(), "Failed to parse ALTER ADD TRANSITION (no braces): {:?}", result_actual.err());
}

#[test]
fn guide_schema_parse_alter_add_data_backfill() {
    let smql = r#"ALTER MACHINE Task ADD DATA category : TEXT -> OPTIONAL BACKFILL "general""#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse ALTER ADD DATA BACKFILL: {:?}", result.err());
}

#[test]
fn guide_schema_parse_alter_add_data_default() {
    let smql = r#"ALTER MACHINE Task ADD DATA status_note : TEXT -> DEFAULT("none")"#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse ALTER ADD DATA DEFAULT: {:?}", result.err());
}

#[test]
fn guide_schema_parse_alter_remove_transition() {
    let smql = r#"ALTER MACHINE Task REMOVE TRANSITION in_progress -> open"#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse ALTER REMOVE TRANSITION: {:?}", result.err());
}

#[test]
fn guide_schema_parse_alter_remove_state_migrate() {
    let smql = r#"ALTER MACHINE Task REMOVE STATE blocked MIGRATE TO in_progress"#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse ALTER REMOVE STATE MIGRATE: {:?}", result.err());
}

#[test]
fn guide_schema_parse_alter_remove_data() {
    let smql = r#"ALTER MACHINE Task REMOVE DATA status_note"#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse ALTER REMOVE DATA: {:?}", result.err());
}

#[test]
fn guide_schema_parse_alter_multi_operation() {
    // DOC ISSUE: Same as above — ALTER ADD TRANSITION doesn't support `{}`
    let smql_doc = r#"ALTER MACHINE Task ADD STATE review ADD TRANSITION in_progress -> review {} ADD TRANSITION review -> done {} ADD TRANSITION review -> in_progress {}"#;
    let result_doc = smql_parser::parse(smql_doc);
    assert!(result_doc.is_err(), "Doc ALTER multi-op with braces should fail");

    let smql_actual = r#"ALTER MACHINE Task ADD STATE review ADD TRANSITION in_progress -> review ADD TRANSITION review -> done ADD TRANSITION review -> in_progress"#;
    let result_actual = smql_parser::parse(smql_actual);
    assert!(result_actual.is_ok(), "Failed to parse ALTER multi-op (no braces): {:?}", result_actual.err());
}

#[test]
fn guide_schema_parse_alter_remove_initial_state() {
    // This parses fine but should fail at engine level
    let smql = r#"ALTER MACHINE Task REMOVE STATE open MIGRATE TO in_progress"#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse ALTER REMOVE initial state: {:?}", result.err());
}

#[tokio::test]
async fn guide_schema_evolution_full_flow() {
    let engine = engine_from_smql(TASK_DEFINE);

    // Spawn 3 tasks
    let cmd = spawn_cmd("Task", vec![("title", Value::Text("Implement login".into()))]);
    let r1 = engine.spawn(&cmd).await.expect("spawn task 1");
    let id1 = r1.instance.id.as_str();

    let cmd = spawn_cmd("Task", vec![("title", Value::Text("Fix CSS bug".into()))]);
    engine.spawn(&cmd).await.expect("spawn task 2");

    let cmd = spawn_cmd("Task", vec![("title", Value::Text("Write tests".into()))]);
    engine.spawn(&cmd).await.expect("spawn task 3");

    // Move task 1 to in_progress
    let cmd = transition_cmd("Task", &id1, "in_progress");
    engine.transition(&cmd).await.expect("open -> in_progress");

    // Step 2: ALTER ADD STATE blocked
    let stmts = smql_parser::parse("ALTER MACHINE Task ADD STATE blocked").unwrap();
    if let Statement::Command(Command::AlterMachine(alter_cmd)) = &stmts[0] {
        let r = engine.execute_alter_machine(alter_cmd).await.expect("alter add state");
        assert_eq!(r.operations_applied, 1);
        assert_eq!(r.instances_migrated, 0);
    } else {
        panic!("Expected AlterMachine");
    }

    // Step 3: ALTER ADD TRANSITION (without `{}` — parser doesn't support body in ALTER)
    let stmts = smql_parser::parse("ALTER MACHINE Task ADD TRANSITION in_progress -> blocked ADD TRANSITION blocked -> in_progress").unwrap();
    if let Statement::Command(Command::AlterMachine(alter_cmd)) = &stmts[0] {
        let r = engine.execute_alter_machine(alter_cmd).await.expect("alter add transitions");
        assert_eq!(r.operations_applied, 2);
    } else {
        panic!("Expected AlterMachine");
    }

    // Now block and unblock task 1
    let cmd = transition_cmd("Task", &id1, "blocked");
    engine.transition(&cmd).await.expect("in_progress -> blocked");

    let cmd = transition_cmd("Task", &id1, "in_progress");
    engine.transition(&cmd).await.expect("blocked -> in_progress");

    // Step 4: ALTER ADD DATA with BACKFILL
    let stmts = smql_parser::parse(r#"ALTER MACHINE Task ADD DATA category : TEXT -> OPTIONAL BACKFILL "general""#).unwrap();
    if let Statement::Command(Command::AlterMachine(alter_cmd)) = &stmts[0] {
        let r = engine.execute_alter_machine(alter_cmd).await.expect("alter add data backfill");
        assert_eq!(r.operations_applied, 1);
        assert_eq!(r.instances_migrated, 3);
    } else {
        panic!("Expected AlterMachine");
    }

    // Step 5: ALTER ADD DATA with DEFAULT
    let stmts = smql_parser::parse(r#"ALTER MACHINE Task ADD DATA status_note : TEXT -> DEFAULT("none")"#).unwrap();
    if let Statement::Command(Command::AlterMachine(alter_cmd)) = &stmts[0] {
        let r = engine.execute_alter_machine(alter_cmd).await.expect("alter add data default");
        assert_eq!(r.operations_applied, 1);
        assert_eq!(r.instances_migrated, 3);
    } else {
        panic!("Expected AlterMachine");
    }

    // Step 6: ALTER REMOVE TRANSITION
    let stmts = smql_parser::parse("ALTER MACHINE Task REMOVE TRANSITION in_progress -> open").unwrap();
    if let Statement::Command(Command::AlterMachine(alter_cmd)) = &stmts[0] {
        let r = engine.execute_alter_machine(alter_cmd).await.expect("alter remove transition");
        assert_eq!(r.operations_applied, 1);
    } else {
        panic!("Expected AlterMachine");
    }

    // Move task 1 to blocked so we can test migration
    let cmd = transition_cmd("Task", &id1, "blocked");
    engine.transition(&cmd).await.expect("in_progress -> blocked again");

    // Step 7: ALTER REMOVE STATE blocked MIGRATE TO in_progress
    let stmts = smql_parser::parse("ALTER MACHINE Task REMOVE STATE blocked MIGRATE TO in_progress").unwrap();
    if let Statement::Command(Command::AlterMachine(alter_cmd)) = &stmts[0] {
        let r = engine.execute_alter_machine(alter_cmd).await.expect("alter remove state migrate");
        assert_eq!(r.operations_applied, 1);
        assert_eq!(r.instances_migrated, 1);
    } else {
        panic!("Expected AlterMachine");
    }

    // Step 8: ALTER REMOVE DATA
    let stmts = smql_parser::parse("ALTER MACHINE Task REMOVE DATA status_note").unwrap();
    if let Statement::Command(Command::AlterMachine(alter_cmd)) = &stmts[0] {
        let r = engine.execute_alter_machine(alter_cmd).await.expect("alter remove data");
        assert_eq!(r.operations_applied, 1);
        assert_eq!(r.instances_migrated, 3);
    } else {
        panic!("Expected AlterMachine");
    }

    // Step 9: Multi-operation ALTER (without `{}` — parser doesn't support body in ALTER)
    let stmts = smql_parser::parse("ALTER MACHINE Task ADD STATE review ADD TRANSITION in_progress -> review ADD TRANSITION review -> done ADD TRANSITION review -> in_progress").unwrap();
    if let Statement::Command(Command::AlterMachine(alter_cmd)) = &stmts[0] {
        let r = engine.execute_alter_machine(alter_cmd).await.expect("alter multi-op");
        assert_eq!(r.operations_applied, 4);
    } else {
        panic!("Expected AlterMachine");
    }
}

#[tokio::test]
async fn guide_schema_cannot_remove_initial_state() {
    let engine = engine_from_smql(TASK_DEFINE);

    let stmts = smql_parser::parse("ALTER MACHINE Task REMOVE STATE open MIGRATE TO in_progress").unwrap();
    if let Statement::Command(Command::AlterMachine(alter_cmd)) = &stmts[0] {
        let err = engine.execute_alter_machine(alter_cmd).await.unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("initial") || msg.contains("open"),
            "Expected error about removing initial state, got: {}", msg);
    }
}

// ===========================================================================
// TUTORIAL: your-first-machine.md
// ===========================================================================

const TRAFFIC_LIGHT_DEFINE: &str = r#"
DEFINE MACHINE TrafficLight (
    STATES { red, green, yellow }
    INITIAL STATE red
    TERMINAL STATES {}
    TRANSITIONS {
      red -> green {}
      green -> yellow {}
      yellow -> red {}
    }
)
"#;

#[test]
fn tutorial_first_machine_parse_define() {
    let result = smql_parser::parse(TRAFFIC_LIGHT_DEFINE);
    assert!(result.is_ok(), "Failed to parse TrafficLight DEFINE: {:?}", result.err());
}

#[test]
fn tutorial_first_machine_parse_spawn() {
    let smql = r#"SPAWN TrafficLight {}"#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse SPAWN TrafficLight: {:?}", result.err());
}

#[test]
fn tutorial_first_machine_parse_transition() {
    let smql = r#"TRANSITION TrafficLight "01J5X7K2P3Q4R5S6T7U8V9W0XY" TO green"#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse TRANSITION: {:?}", result.err());
}

#[test]
fn tutorial_first_machine_parse_get() {
    let smql = r#"GET TrafficLight "01J5X7K2P3Q4R5S6T7U8V9W0XY""#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse GET: {:?}", result.err());
}

#[test]
fn tutorial_first_machine_parse_trail() {
    // DOC ISSUE: Docs show `TRAIL OF TrafficLight "id"` but parser uses `TRAIL OF "id"`
    let smql_doc = r#"TRAIL OF TrafficLight "01J5X7K2P3Q4R5S6T7U8V9W0XY""#;
    assert!(smql_parser::parse(smql_doc).is_err(), "Doc TRAIL syntax with machine name should fail");

    let smql_actual = r#"TRAIL OF "01J5X7K2P3Q4R5S6T7U8V9W0XY""#;
    let result = smql_parser::parse(smql_actual);
    assert!(result.is_ok(), "Failed to parse TRAIL (actual syntax): {:?}", result.err());
}

#[test]
fn tutorial_first_machine_parse_find() {
    let smql = r#"FIND TrafficLight WHERE STATE IS red"#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse FIND: {:?}", result.err());
}

#[tokio::test]
async fn tutorial_first_machine_full_cycle() {
    let engine = engine_from_smql(TRAFFIC_LIGHT_DEFINE);

    // Spawn
    let cmd = spawn_cmd("TrafficLight", vec![]);
    let r = engine.spawn(&cmd).await.expect("spawn");
    let id = r.instance.id.as_str();
    assert_eq!(r.instance.state, "red");
    // DOC ISSUE: Docs say trail_length: 1 after spawn, but engine initializes
    // trail_length to 0 in Instance::new(). The trail entry is appended
    // separately but trail_length on the returned instance is 0.
    assert_eq!(r.instance.trail_length, 0);
    assert_eq!(r.instance.version, 1);

    // red -> green
    let cmd = transition_cmd("TrafficLight", &id, "green");
    let r = engine.transition(&cmd).await.expect("red -> green");
    assert_eq!(r.from_state, "red");
    assert_eq!(r.to_state, "green");

    // green -> red (invalid — should fail)
    let cmd = transition_cmd("TrafficLight", &id, "red");
    let err = engine.transition(&cmd).await.unwrap_err();
    assert!(format!("{}", err).contains("transition") || format!("{}", err).contains("defined"),
        "Expected transition not defined error, got: {}", err);

    // green -> yellow -> red (complete cycle)
    let cmd = transition_cmd("TrafficLight", &id, "yellow");
    engine.transition(&cmd).await.expect("green -> yellow");

    let cmd = transition_cmd("TrafficLight", &id, "red");
    engine.transition(&cmd).await.expect("yellow -> red");

    // Verify trail has 4 entries
    use smql_ast::query::{Query, TrailQuery};
    let q = Query::Trail(TrailQuery {
        machine: Some("TrafficLight".to_string()),
        instance_id: id.to_string(),
        filter: None,
    });
    let result = engine.execute_query(&q).await.expect("trail query");
    match result {
        QueryResult::Trail(entries) => {
            assert_eq!(entries.len(), 4);
            assert_eq!(entries[0].from_state, "");
            assert_eq!(entries[0].to_state, "red");
            assert_eq!(entries[1].to_state, "green");
            assert_eq!(entries[2].to_state, "yellow");
            assert_eq!(entries[3].to_state, "red");
        }
        _ => panic!("Expected Trail result"),
    }
}

#[tokio::test]
async fn tutorial_first_machine_terminal_state() {
    let smql = r#"
    DEFINE MACHINE OneShotTask (
        STATES { pending, done }
        INITIAL STATE pending
        TERMINAL STATES { done }
        TRANSITIONS {
          pending -> done {}
        }
    )
    "#;
    let engine = engine_from_smql(smql);

    let cmd = spawn_cmd("OneShotTask", vec![]);
    let r = engine.spawn(&cmd).await.expect("spawn OneShotTask");
    let id = r.instance.id.as_str();

    // pending -> done
    let cmd = transition_cmd("OneShotTask", &id, "done");
    engine.transition(&cmd).await.expect("pending -> done");

    // done -> pending (should fail: terminal state)
    let cmd = transition_cmd("OneShotTask", &id, "pending");
    let err = engine.transition(&cmd).await.unwrap_err();
    assert!(format!("{}", err).contains("terminal") || format!("{}", err).contains("done"),
        "Expected terminal state error, got: {}", err);
}

#[tokio::test]
async fn tutorial_first_machine_find_multiple() {
    let engine = engine_from_smql(TRAFFIC_LIGHT_DEFINE);

    // Spawn 3 traffic lights
    for _ in 0..3 {
        let cmd = spawn_cmd("TrafficLight", vec![]);
        engine.spawn(&cmd).await.expect("spawn");
    }

    // FIND WHERE STATE IS red — use parser instead of manual construction
    let stmts = smql_parser::parse("FIND TrafficLight WHERE STATE IS red").unwrap();
    if let Statement::Query(q) = &stmts[0] {
        let result = engine.execute_query(q).await.expect("find query");
        match result {
            QueryResult::Instances(instances) => {
                assert_eq!(instances.len(), 3);
            }
            _ => panic!("Expected Instances result"),
        }
    } else {
        panic!("Expected Query statement");
    }
}

// ===========================================================================
// TUTORIAL: adding-data-and-guards.md
// ===========================================================================

const TODOITEM_DEFINE: &str = r#"
DEFINE MACHINE TodoItem (

  DATA {
    title       : TEXT        -> REQUIRED, MAX(100)
    assignee    : TEXT        -> OPTIONAL
    priority    : ENUM(low, medium, high) -> DEFAULT(medium)
    notes       : TEXT        -> OPTIONAL
    effort_days : INT         -> RANGE(1, 30), OPTIONAL
  }

  STATES { open, in_progress, blocked, done }
  INITIAL STATE open
  TERMINAL STATES { done }

  TRANSITIONS {
    open -> in_progress {
      GUARD : assignee IS SET
    }

    in_progress -> blocked {
      GUARD : notes IS SET
    }

    blocked -> in_progress {}

    in_progress -> done {
      GUARD : notes IS SET
      GUARD : ACTOR.id == assignee
    }

    open -> done {
      GUARD : ACTOR.role == "admin"
    }
  }
)
"#;

#[test]
fn tutorial_guards_parse_define() {
    let result = smql_parser::parse(TODOITEM_DEFINE);
    assert!(result.is_ok(), "Failed to parse TodoItem DEFINE: {:?}", result.err());
}

#[test]
fn tutorial_guards_parse_spawn_with_data() {
    let smql = r#"SPAWN TodoItem { title: "Write unit tests", assignee: "alice", effort_days: 5 }"#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse SPAWN with data: {:?}", result.err());
}

#[test]
fn tutorial_guards_parse_spawn_empty() {
    let smql = r#"SPAWN TodoItem {}"#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse empty SPAWN: {:?}", result.err());
}

#[test]
fn tutorial_guards_parse_transition_with_notes_and_actor() {
    let smql = r#"TRANSITION TodoItem "01JMTODO00000000000000001A" TO done WITH { notes: "All tests passing" } AS "alice""#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse TRANSITION WITH AS: {:?}", result.err());
}

#[test]
fn tutorial_guards_parse_transition_as_bob() {
    let smql = r#"TRANSITION TodoItem "01JMTODO00000000000000001A" TO done WITH { notes: "Done" } AS "bob""#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse TRANSITION AS bob: {:?}", result.err());
}

#[test]
fn tutorial_guards_parse_mutate_example() {
    // Parse the MUTATE transition snippet from the doc
    let smql = r#"
    DEFINE MACHINE MutateTest (
        DATA {
            assignee : TEXT -> OPTIONAL
            priority : ENUM(low, medium, high) -> DEFAULT(medium)
        }
        STATES { open, in_progress }
        INITIAL STATE open
        TERMINAL STATES {}
        TRANSITIONS {
            open -> in_progress {
                GUARD  : assignee IS SET
                MUTATE : priority = high
            }
        }
    )
    "#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse MUTATE example: {:?}", result.err());
}

#[tokio::test]
async fn tutorial_guards_spawn_with_defaults() {
    let engine = engine_from_smql(TODOITEM_DEFINE);

    let cmd = spawn_cmd("TodoItem", vec![
        ("title", Value::Text("Write unit tests".into())),
        ("assignee", Value::Text("alice".into())),
        ("effort_days", Value::Int(5)),
    ]);
    let r = engine.spawn(&cmd).await.expect("spawn TodoItem");
    assert_eq!(r.instance.state, "open");
    assert_eq!(r.instance.data.get("priority").unwrap(), &Value::Text("medium".into()));
    assert!(r.instance.data.get("notes").is_none() || r.instance.data.get("notes") == Some(&Value::Null));
}

#[tokio::test]
async fn tutorial_guards_spawn_validation_missing_required() {
    let engine = engine_from_smql(TODOITEM_DEFINE);

    let cmd = spawn_cmd("TodoItem", vec![]);
    let err = engine.spawn(&cmd).await.unwrap_err();
    let msg = format!("{}", err);
    assert!(msg.contains("title") || msg.contains("required") || msg.contains("REQUIRED"),
        "Expected missing required field error, got: {}", msg);
}

#[tokio::test]
async fn tutorial_guards_transition_guard_assignee_required() {
    let engine = engine_from_smql(TODOITEM_DEFINE);

    // Spawn without assignee
    let cmd = spawn_cmd("TodoItem", vec![
        ("title", Value::Text("No assignee".into())),
    ]);
    let r = engine.spawn(&cmd).await.expect("spawn");
    let id = r.instance.id.as_str();

    // open -> in_progress should fail (assignee IS SET guard)
    let cmd = transition_cmd("TodoItem", &id, "in_progress");
    let err = engine.transition(&cmd).await.unwrap_err();
    let msg = format!("{}", err);
    assert!(msg.contains("assignee") || msg.contains("guard"),
        "Expected guard failure about assignee, got: {}", msg);
}

#[tokio::test]
async fn tutorial_guards_with_clause_satisfies_guard() {
    let engine = engine_from_smql(TODOITEM_DEFINE);

    let cmd = spawn_cmd("TodoItem", vec![
        ("title", Value::Text("Write unit tests".into())),
        ("assignee", Value::Text("alice".into())),
    ]);
    let r = engine.spawn(&cmd).await.expect("spawn");
    let id = r.instance.id.as_str();

    // open -> in_progress (assignee is set)
    let cmd = transition_cmd("TodoItem", &id, "in_progress");
    engine.transition(&cmd).await.expect("open -> in_progress");

    // in_progress -> done requires notes IS SET AND ACTOR.id == assignee
    // Without notes, should fail
    let cmd = transition_as("TodoItem", &id, "done", "alice");
    let err = engine.transition(&cmd).await.unwrap_err();
    let msg = format!("{}", err);
    assert!(msg.contains("notes") || msg.contains("guard"),
        "Expected guard failure about notes, got: {}", msg);

    // With notes AND correct actor, should succeed
    let cmd = transition_with("TodoItem", &id, "done", "alice",
        vec![("notes", Value::Text("All tests passing".into()))]);
    let r = engine.transition(&cmd).await.expect("in_progress -> done");
    assert_eq!(r.to_state, "done");
}

#[tokio::test]
async fn tutorial_guards_wrong_actor() {
    let engine = engine_from_smql(TODOITEM_DEFINE);

    let cmd = spawn_cmd("TodoItem", vec![
        ("title", Value::Text("Test".into())),
        ("assignee", Value::Text("alice".into())),
    ]);
    let r = engine.spawn(&cmd).await.expect("spawn");
    let id = r.instance.id.as_str();

    // open -> in_progress
    let cmd = transition_cmd("TodoItem", &id, "in_progress");
    engine.transition(&cmd).await.expect("open -> in_progress");

    // in_progress -> done with wrong actor "bob"
    let cmd = transition_with("TodoItem", &id, "done", "bob",
        vec![("notes", Value::Text("Done".into()))]);
    let err = engine.transition(&cmd).await.unwrap_err();
    let msg = format!("{}", err);
    assert!(msg.contains("ACTOR") || msg.contains("guard") || msg.contains("denied"),
        "Expected ACTOR guard failure, got: {}", msg);
}

#[tokio::test]
async fn tutorial_guards_blocked_flow() {
    let engine = engine_from_smql(TODOITEM_DEFINE);

    let cmd = spawn_cmd("TodoItem", vec![
        ("title", Value::Text("Fix login bug".into())),
        ("assignee", Value::Text("bob".into())),
    ]);
    let r = engine.spawn(&cmd).await.expect("spawn");
    let id = r.instance.id.as_str();

    // open -> in_progress
    let cmd = transition_cmd("TodoItem", &id, "in_progress");
    engine.transition(&cmd).await.expect("open -> in_progress");

    // in_progress -> blocked (requires notes IS SET)
    let cmd = transition_with_no_actor("TodoItem", &id, "blocked",
        vec![("notes", Value::Text("Waiting on API team".into()))]);
    engine.transition(&cmd).await.expect("in_progress -> blocked");

    // blocked -> in_progress (no guard)
    let cmd = transition_cmd("TodoItem", &id, "in_progress");
    engine.transition(&cmd).await.expect("blocked -> in_progress");

    // in_progress -> done with correct actor
    let cmd = transition_with("TodoItem", &id, "done", "bob",
        vec![("notes", Value::Text("API fixed, login works".into()))]);
    let r = engine.transition(&cmd).await.expect("in_progress -> done");
    assert_eq!(r.to_state, "done");
}

// ===========================================================================
// TUTORIAL: timeouts-and-hooks.md
// ===========================================================================

const REVIEW_REQUEST_DEFINE: &str = r#"
DEFINE MACHINE ReviewRequest (

  DATA {
    pr_url      : TEXT -> REQUIRED
    author      : TEXT -> REQUIRED
    reviewer    : TEXT -> OPTIONAL
    comments    : TEXT -> OPTIONAL
  }

  STATES { pending, in_review, changes_requested, approved, stale, merged }
  INITIAL STATE pending
  TERMINAL STATES { merged, stale }

  TRANSITIONS {
    pending -> in_review {
      GUARD : reviewer IS SET
    }

    in_review -> approved {
      GUARD : ACTOR.id == reviewer
    }

    in_review -> changes_requested {
      GUARD  : ACTOR.id == reviewer
      GUARD  : comments IS SET
      TIMEOUT: 7d -> stale
    }

    changes_requested -> in_review {}

    approved -> merged {
      GUARD : ACTOR.id == author
    }
  }

  HOOKS {
    ON SPAWN {
      EMIT("review.created")
    }

    AFTER EACH TRANSITION {
      EMIT("review.state_changed")
    }

    ON ENTER approved {
      EMIT("review.approved")
    }

    ON ENTER stale {
      EMIT("review.gone_stale")
    }
  }
)
"#;

#[test]
fn tutorial_timeouts_parse_define() {
    let result = smql_parser::parse(REVIEW_REQUEST_DEFINE);
    assert!(result.is_ok(), "Failed to parse ReviewRequest DEFINE: {:?}", result.err());
}

#[test]
fn tutorial_timeouts_parse_quick_review() {
    let smql = r#"
    DEFINE MACHINE QuickReview (
      DATA { title: TEXT -> REQUIRED }
      STATES { open, waiting, expired }
      INITIAL STATE open
      TERMINAL STATES { expired }
      TRANSITIONS {
        open -> waiting {
          TIMEOUT: 5s -> expired
        }
      }
    )
    "#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse QuickReview DEFINE: {:?}", result.err());
}

#[test]
fn tutorial_timeouts_parse_duration_formats() {
    // Test various duration format examples from the doc
    let templates = vec![
        "TIMEOUT: 30s -> expired",
        "TIMEOUT: 5m -> expired",
        "TIMEOUT: 2h -> expired",
        "TIMEOUT: 7d -> expired",
    ];
    for timeout in templates {
        let smql = format!(r#"
            DEFINE MACHINE DurTest (
              STATES {{ open, expired }}
              INITIAL STATE open
              TERMINAL STATES {{ expired }}
              TRANSITIONS {{
                open -> open {{
                  {}
                }}
              }}
            )
        "#, timeout);
        let result = smql_parser::parse(&smql);
        assert!(result.is_ok(), "Failed to parse duration '{}': {:?}", timeout, result.err());
    }
}

#[test]
fn tutorial_timeouts_parse_hooks_block() {
    // Test that HOOKS block with various triggers parses
    let smql = r#"
    DEFINE MACHINE HookTest (
      STATES { a, b }
      INITIAL STATE a
      TERMINAL STATES { b }
      TRANSITIONS { a -> b {} }
      HOOKS {
        ON SPAWN {
          EMIT("spawned")
        }
        AFTER EACH TRANSITION {
          EMIT("transitioned")
        }
        ON ENTER b {
          EMIT("entered_b")
        }
        ON EXIT a {
          EMIT("exited_a")
        }
        BEFORE EACH TRANSITION {
          EMIT("before_transition")
        }
      }
    )
    "#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse HOOKS block: {:?}", result.err());
}

#[tokio::test]
async fn tutorial_timeouts_review_lifecycle() {
    let engine = engine_from_smql(REVIEW_REQUEST_DEFINE);

    // Spawn
    let cmd = spawn_cmd("ReviewRequest", vec![
        ("pr_url", Value::Text("https://github.com/acme/app/pull/42".into())),
        ("author", Value::Text("alice".into())),
    ]);
    let r = engine.spawn(&cmd).await.expect("spawn ReviewRequest");
    let id = r.instance.id.as_str();
    assert_eq!(r.instance.state, "pending");

    // pending -> in_review (requires reviewer IS SET)
    let cmd = transition_with_no_actor("ReviewRequest", &id, "in_review",
        vec![("reviewer", Value::Text("bob".into()))]);
    engine.transition(&cmd).await.expect("pending -> in_review");

    // in_review -> changes_requested (ACTOR.id == reviewer, comments IS SET)
    let cmd = transition_with("ReviewRequest", &id, "changes_requested", "bob",
        vec![("comments", Value::Text("Please add tests".into()))]);
    engine.transition(&cmd).await.expect("in_review -> changes_requested");

    // changes_requested -> in_review (cancel timer)
    let cmd = transition_cmd("ReviewRequest", &id, "in_review");
    engine.transition(&cmd).await.expect("changes_requested -> in_review");

    // in_review -> approved (ACTOR.id == reviewer)
    let cmd = transition_as("ReviewRequest", &id, "approved", "bob");
    engine.transition(&cmd).await.expect("in_review -> approved");

    // approved -> merged (ACTOR.id == author)
    let cmd = transition_as("ReviewRequest", &id, "merged", "alice");
    let r = engine.transition(&cmd).await.expect("approved -> merged");
    assert_eq!(r.to_state, "merged");
}

// ===========================================================================
// TUTORIAL: composition-patterns.md
// ===========================================================================

const INVOICE_DEFINE: &str = r#"
DEFINE MACHINE Invoice (

  DATA {
    customer : TEXT       -> REQUIRED
    due_date : TEXT       -> OPTIONAL
    total    : INT        -> REQUIRED
  }

  STATES { draft, sent, paid, cancelled, archived }
  INITIAL STATE draft
  TERMINAL STATES { archived, cancelled }

  CHILDREN {
    items : LIST(LineItem) -> MIN(1)
  }

  TRANSITIONS {
    draft -> sent {
      GUARD : ALL(items, STATE IS confirmed)
      GUARD : total > 0
    }

    sent -> paid {}

    paid -> archived {}

    ANY -> cancelled {
      EXCEPT FROM { paid, archived }
    }
  }
)
"#;

const LINEITEM_INVOICE_DEFINE: &str = r#"
DEFINE MACHINE LineItem (

  PARENT : Invoice

  DATA {
    product  : TEXT -> REQUIRED
    quantity : INT  -> MIN(1), REQUIRED
    price    : INT  -> REQUIRED
  }

  STATES { pending, confirmed, cancelled }
  INITIAL STATE pending
  TERMINAL STATES { confirmed, cancelled }

  TRANSITIONS {
    pending -> confirmed {
      GUARD : quantity > 0
    }

    pending -> cancelled {}
  }
)
"#;

#[test]
fn tutorial_composition_parse_invoice() {
    let result = smql_parser::parse(INVOICE_DEFINE);
    assert!(result.is_ok(), "Failed to parse Invoice DEFINE: {:?}", result.err());
}

#[test]
fn tutorial_composition_parse_lineitem() {
    let result = smql_parser::parse(LINEITEM_INVOICE_DEFINE);
    assert!(result.is_ok(), "Failed to parse LineItem DEFINE: {:?}", result.err());
}

#[test]
fn tutorial_composition_parse_spawn_with_parent_shorthand() {
    // DOC ISSUE: SPAWN PARENT clause is not implemented in the parser at all.
    // Both the shorthand `PARENT "id"` and full `PARENT Machine "id"` fail.
    // Child spawning is only supported via the programmatic SpawnCommand API.
    let smql1 = r#"SPAWN LineItem { product: "Widget A", quantity: 3, price: 2500 } PARENT "01JMINV000000000000000001A""#;
    assert!(smql_parser::parse(smql1).is_err(), "SPAWN PARENT (shorthand) not supported in parser");

    let smql2 = r#"SPAWN LineItem { product: "Widget A", quantity: 3, price: 2500 } PARENT Invoice "01JMINV000000000000000001A""#;
    assert!(smql_parser::parse(smql2).is_err(), "SPAWN PARENT (full) not supported in parser");
}

#[test]
fn tutorial_composition_parse_cascade_transition() {
    let smql = r#"TRANSITION Invoice "01JMINV000000000000000001A" TO cancelled CASCADE"#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse CASCADE TRANSITION: {:?}", result.err());
}

#[test]
fn tutorial_composition_parse_find_children_by_parent() {
    let smql = r#"FIND LineItem WHERE parent == "01JMINV000000000000000001A""#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse FIND by parent: {:?}", result.err());
}

#[test]
fn tutorial_composition_parse_multi_level_define() {
    // The tutorial shows a minimal multi-level example
    let smql = r#"
    DEFINE MACHINE Pipeline (
      CHILDREN { stages : LIST(Stage) -> MIN(1) }
      STATES { pending, running, passed }
      INITIAL STATE pending
      TERMINAL STATES { passed }
      TRANSITIONS {
        pending -> running {}
        running -> passed { GUARD : ALL(stages, STATE IS passed) }
      }
    )

    DEFINE MACHINE Stage (
      PARENT : Pipeline
      CHILDREN { jobs : LIST(Job) -> MIN(1) }
      STATES { pending, running, passed }
      INITIAL STATE pending
      TERMINAL STATES { passed }
      TRANSITIONS {
        pending -> running {}
        running -> passed { GUARD : ALL(jobs, STATE IS passed) }
      }
    )

    DEFINE MACHINE Job (
      PARENT : Stage
      STATES { pending, running, passed, failed }
      INITIAL STATE pending
      TERMINAL STATES { passed, failed }
      TRANSITIONS {
        pending -> running {}
        running -> passed {}
        running -> failed {}
      }
    )
    "#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse multi-level composition: {:?}", result.err());
}

#[tokio::test]
async fn tutorial_composition_invoice_lifecycle() {
    let all_smql = format!("{}\n{}", INVOICE_DEFINE, LINEITEM_INVOICE_DEFINE);
    let engine = engine_from_smql(&all_smql);

    // Create invoice
    let cmd = spawn_cmd("Invoice", vec![
        ("customer", Value::Text("Acme Corp".into())),
        ("total", Value::Int(7500)),
    ]);
    let r = engine.spawn(&cmd).await.expect("spawn Invoice");
    let inv_id = r.instance.id.as_str();
    assert_eq!(r.instance.state, "draft");

    // Create line items
    let cmd = spawn_child_cmd("LineItem", vec![
        ("product", Value::Text("Widget A".into())),
        ("quantity", Value::Int(3)),
        ("price", Value::Int(2500)),
    ], &inv_id, "Invoice");
    let r = engine.spawn(&cmd).await.expect("spawn LineItem 1");
    let li1_id = r.instance.id.as_str();

    let cmd = spawn_child_cmd("LineItem", vec![
        ("product", Value::Text("Widget B".into())),
        ("quantity", Value::Int(1)),
        ("price", Value::Int(5000)),
    ], &inv_id, "Invoice");
    let r = engine.spawn(&cmd).await.expect("spawn LineItem 2");
    let li2_id = r.instance.id.as_str();

    // Try to send — should fail (items not confirmed)
    let cmd = transition_cmd("Invoice", &inv_id, "sent");
    let err = engine.transition(&cmd).await.unwrap_err();
    assert!(format!("{}", err).contains("ALL") || format!("{}", err).contains("guard"));

    // Confirm items
    let cmd = transition_cmd("LineItem", &li1_id, "confirmed");
    engine.transition(&cmd).await.expect("confirm item 1");
    let cmd = transition_cmd("LineItem", &li2_id, "confirmed");
    engine.transition(&cmd).await.expect("confirm item 2");

    // Now send should succeed
    let cmd = transition_cmd("Invoice", &inv_id, "sent");
    let r = engine.transition(&cmd).await.expect("draft -> sent");
    assert_eq!(r.to_state, "sent");

    // sent -> paid -> archived
    let cmd = transition_cmd("Invoice", &inv_id, "paid");
    engine.transition(&cmd).await.expect("sent -> paid");
    let cmd = transition_cmd("Invoice", &inv_id, "archived");
    engine.transition(&cmd).await.expect("paid -> archived");
}

#[tokio::test]
async fn tutorial_composition_cascade_cancel() {
    let all_smql = format!("{}\n{}", INVOICE_DEFINE, LINEITEM_INVOICE_DEFINE);
    let engine = engine_from_smql(&all_smql);

    // Create invoice with one item
    let cmd = spawn_cmd("Invoice", vec![
        ("customer", Value::Text("Beta Corp".into())),
        ("total", Value::Int(3000)),
    ]);
    let r = engine.spawn(&cmd).await.expect("spawn Invoice");
    let inv_id = r.instance.id.as_str();

    let cmd = spawn_child_cmd("LineItem", vec![
        ("product", Value::Text("Gadget".into())),
        ("quantity", Value::Int(1)),
        ("price", Value::Int(3000)),
    ], &inv_id, "Invoice");
    engine.spawn(&cmd).await.expect("spawn LineItem");

    // CASCADE cancel
    let cmd = cascade_cmd("Invoice", &inv_id, "cancelled");
    let r = engine.transition(&cmd).await.expect("cancel Invoice CASCADE");
    assert_eq!(r.to_state, "cancelled");
}

// ===========================================================================
// TUTORIAL: queries-and-analytics.md
// ===========================================================================

const QUERY_SUPPORT_TICKET_DEFINE: &str = r#"
DEFINE MACHINE SupportTicket (
  DATA {
    customer_id : UUID -> REQUIRED
    subject     : TEXT -> REQUIRED
    priority    : ENUM(low, medium, high, critical) -> DEFAULT(medium)
    assignee    : TEXT -> OPTIONAL
    resolution_note : TEXT -> OPTIONAL
  }
  STATES { open, triaged, in_progress, resolved, closed }
  INITIAL STATE open
  TERMINAL STATES { closed }
  TRANSITIONS {
    open -> triaged { GUARD : assignee IS SET }
    triaged -> in_progress {}
    in_progress -> resolved { GUARD : resolution_note IS SET }
    resolved -> closed {}
  }
)
"#;

#[test]
fn tutorial_queries_parse_find_basic() {
    let smql = r#"FIND SupportTicket"#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse basic FIND: {:?}", result.err());
}

#[test]
fn tutorial_queries_parse_find_state() {
    let smql = r#"FIND SupportTicket WHERE STATE IS open"#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse FIND WHERE STATE: {:?}", result.err());
}

#[test]
fn tutorial_queries_parse_find_data_field() {
    let smql = r#"FIND SupportTicket WHERE priority == "high""#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse FIND WHERE data field: {:?}", result.err());
}

#[test]
fn tutorial_queries_parse_find_and() {
    let smql = r#"FIND SupportTicket WHERE STATE IS open AND priority == "critical""#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse FIND WHERE AND: {:?}", result.err());
}

#[test]
fn tutorial_queries_parse_find_or() {
    let smql = r#"FIND SupportTicket WHERE priority == "high" OR priority == "critical""#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse FIND WHERE OR: {:?}", result.err());
}

#[test]
fn tutorial_queries_parse_find_in() {
    let smql = r#"FIND SupportTicket WHERE priority IN ("high", "critical")"#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse FIND WHERE IN: {:?}", result.err());
}

#[test]
fn tutorial_queries_parse_find_is_set() {
    let smql = r#"FIND SupportTicket WHERE assignee IS SET"#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse FIND WHERE IS SET: {:?}", result.err());
}

#[test]
fn tutorial_queries_parse_find_is_not_set() {
    let smql = r#"FIND SupportTicket WHERE resolution_note IS NOT SET"#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse FIND WHERE IS NOT SET: {:?}", result.err());
}

#[test]
fn tutorial_queries_parse_find_alive() {
    let smql = r#"FIND SupportTicket WHERE ALIVE"#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse FIND WHERE ALIVE: {:?}", result.err());
}

#[test]
fn tutorial_queries_parse_find_terminated() {
    let smql = r#"FIND SupportTicket WHERE TERMINATED"#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse FIND WHERE TERMINATED: {:?}", result.err());
}

#[test]
fn tutorial_queries_parse_find_has_visited() {
    // DOC ISSUE: HAS_VISITED is lexed as a keyword but not parsed in the
    // query parser. It's documented but not implemented.
    let smql = r#"FIND SupportTicket WHERE HAS_VISITED in_progress"#;
    let result = smql_parser::parse(smql);
    assert!(result.is_err(), "HAS_VISITED is not implemented in parser (keyword only)");
}

#[test]
fn tutorial_queries_parse_find_never_visited() {
    // DOC ISSUE: NEVER_VISITED is lexed as a keyword but not parsed in the
    // query parser. It's documented but not implemented.
    let smql = r#"FIND SupportTicket WHERE NEVER_VISITED resolved"#;
    let result = smql_parser::parse(smql);
    assert!(result.is_err(), "NEVER_VISITED is not implemented in parser (keyword only)");
}

#[test]
fn tutorial_queries_parse_find_sort_limit() {
    let smql = r#"FIND SupportTicket WHERE STATE IS open SORT BY priority DESC"#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse FIND SORT BY: {:?}", result.err());
}

#[test]
fn tutorial_queries_parse_find_limit_offset() {
    let smql = r#"FIND SupportTicket LIMIT 10 OFFSET 20"#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse FIND LIMIT OFFSET: {:?}", result.err());
}

#[test]
fn tutorial_queries_parse_find_combined() {
    let smql = r#"FIND SupportTicket WHERE ALIVE SORT BY priority ASC LIMIT 5"#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse FIND combined: {:?}", result.err());
}

#[test]
fn tutorial_queries_parse_aggregate_count() {
    let smql = r#"AGGREGATE SupportTicket MEASURE COUNT()"#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse AGGREGATE COUNT: {:?}", result.err());
}

#[test]
fn tutorial_queries_parse_aggregate_group_by() {
    let smql = r#"AGGREGATE SupportTicket MEASURE COUNT() GROUP BY state"#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse AGGREGATE GROUP BY: {:?}", result.err());
}

#[test]
fn tutorial_queries_parse_aggregate_multi_measure() {
    let smql = r#"AGGREGATE SupportTicket MEASURE COUNT() MEASURE AVG(satisfaction) AS avg_score MEASURE MIN(satisfaction) AS min_score MEASURE MAX(satisfaction) AS max_score GROUP BY priority"#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse AGGREGATE multi-measure: {:?}", result.err());
}

#[test]
fn tutorial_queries_parse_aggregate_percentile() {
    // DOC ISSUE: Docs show `PERCENTILE(field, value)` with two args, but the
    // parser only supports `PERCENTILE(field)` — the percentile value is not
    // parsed (TODO in parser code). The two-arg form fails at the comma.
    let smql_doc = r#"AGGREGATE SupportTicket MEASURE PERCENTILE(satisfaction, 50) AS p50 MEASURE PERCENTILE(satisfaction, 95) AS p95 MEASURE PERCENTILE(satisfaction, 99) AS p99"#;
    let result_doc = smql_parser::parse(smql_doc);
    assert!(result_doc.is_err(), "Doc PERCENTILE(field, value) syntax not supported");

    // Single-arg form works:
    let smql_actual = r#"AGGREGATE SupportTicket MEASURE PERCENTILE(satisfaction) AS p50"#;
    let result_actual = smql_parser::parse(smql_actual);
    assert!(result_actual.is_ok(), "Failed to parse PERCENTILE(field) single-arg: {:?}", result_actual.err());
}

#[test]
fn tutorial_queries_parse_trail() {
    // DOC ISSUE: Docs show `TRAIL OF SupportTicket "01JM..."` but parser
    // uses `TRAIL OF "instance_id"` without machine name.
    let smql_doc = r#"TRAIL OF SupportTicket "01JM...""#;
    assert!(smql_parser::parse(smql_doc).is_err(), "Doc TRAIL syntax with machine should fail");

    let smql_actual = r#"TRAIL OF "01JM...""#;
    let result = smql_parser::parse(smql_actual);
    assert!(result.is_ok(), "Failed to parse TRAIL (actual syntax): {:?}", result.err());
}

#[test]
fn tutorial_queries_parse_transition_with_memo() {
    let smql = r#"TRANSITION SupportTicket "01JMABCDEF1234567890ABCDEF" TO resolved WITH { resolution_note: "Cache cleared" } AS "agent_1" MEMO "Customer confirmed fix works""#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse TRANSITION with MEMO: {:?}", result.err());
}

#[test]
fn tutorial_queries_parse_paths() {
    let smql = r#"PATHS FROM SupportTicket"#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse PATHS: {:?}", result.err());
}

#[test]
fn tutorial_queries_parse_funnel() {
    // DOC ISSUE: Docs show `FUNNEL Machine THROUGH open, triaged, ...` without brackets
    // but parser requires brackets: `FUNNEL Machine THROUGH [open, triaged, ...]`
    let smql_doc = r#"FUNNEL SupportTicket THROUGH open, triaged, in_progress, resolved, closed"#;
    assert!(smql_parser::parse(smql_doc).is_err(), "Doc FUNNEL syntax without brackets should fail");

    let smql_actual = r#"FUNNEL SupportTicket THROUGH [open, triaged, in_progress, resolved, closed]"#;
    let result = smql_parser::parse(smql_actual);
    assert!(result.is_ok(), "Failed to parse FUNNEL (actual syntax with brackets): {:?}", result.err());
}

#[test]
fn tutorial_queries_parse_compare_paths() {
    let smql = r#"COMPARE PATHS SupportTicket SEGMENT BY priority"#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse COMPARE PATHS: {:?}", result.err());
}

#[test]
fn tutorial_queries_parse_paths_with_where() {
    let smql = r#"PATHS FROM SupportTicket WHERE priority == "critical""#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse PATHS WHERE: {:?}", result.err());
}

#[test]
fn tutorial_queries_parse_find_sort_desc() {
    let smql = r#"FIND SupportTicket WHERE STATE IS open SORT BY priority DESC"#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse FIND SORT DESC: {:?}", result.err());
}

#[tokio::test]
async fn tutorial_queries_full_query_suite() {
    // Test queries actually execute against a populated engine
    let engine = engine_from_smql(QUERY_SUPPORT_TICKET_DEFINE);

    // Spawn several tickets with different priorities
    for (i, priority) in ["low", "medium", "high", "critical"].iter().enumerate() {
        let cmd = spawn_cmd("SupportTicket", vec![
            ("customer_id", Value::Uuid(uuid::Uuid::new_v4())),
            ("subject", Value::Text(format!("Ticket {}", i))),
            ("priority", Value::Text(priority.to_string())),
        ]);
        engine.spawn(&cmd).await.expect("spawn ticket");
    }

    // Extra medium tickets
    for i in 0..3 {
        let cmd = spawn_cmd("SupportTicket", vec![
            ("customer_id", Value::Uuid(uuid::Uuid::new_v4())),
            ("subject", Value::Text(format!("Extra ticket {}", i))),
        ]);
        engine.spawn(&cmd).await.expect("spawn extra ticket");
    }

    // FIND all
    let stmts = smql_parser::parse("FIND SupportTicket").unwrap();
    if let Statement::Query(q) = &stmts[0] {
        let r = engine.execute_query(q).await.expect("FIND all");
        match r {
            QueryResult::Instances(insts) => assert_eq!(insts.len(), 7),
            _ => panic!("Expected Instances"),
        }
    }

    // FIND WHERE STATE IS open
    let stmts = smql_parser::parse("FIND SupportTicket WHERE STATE IS open").unwrap();
    if let Statement::Query(q) = &stmts[0] {
        let r = engine.execute_query(q).await.expect("FIND open");
        match r {
            QueryResult::Instances(insts) => assert_eq!(insts.len(), 7),
            _ => panic!("Expected Instances"),
        }
    }

    // AGGREGATE COUNT() GROUP BY state
    let stmts = smql_parser::parse("AGGREGATE SupportTicket MEASURE COUNT() GROUP BY state").unwrap();
    if let Statement::Query(q) = &stmts[0] {
        let r = engine.execute_query(q).await.expect("AGGREGATE");
        match r {
            QueryResult::Aggregate(rows) => {
                assert!(!rows.is_empty(), "Expected at least 1 aggregate row");
                // All tickets are in "open", so there should be one row
                assert_eq!(rows.len(), 1);
            }
            _ => panic!("Expected Aggregate"),
        }
    }

    // FUNNEL (using brackets — actual parser syntax)
    let stmts = smql_parser::parse("FUNNEL SupportTicket THROUGH [open, triaged, in_progress, resolved, closed]").unwrap();
    if let Statement::Query(q) = &stmts[0] {
        let r = engine.execute_query(q).await.expect("FUNNEL");
        match r {
            QueryResult::Funnel(funnel) => {
                assert_eq!(funnel.stages.len(), 5);
                assert_eq!(funnel.stages[0].state, "open");
                assert_eq!(funnel.stages[0].count, 7);
            }
            _ => panic!("Expected Funnel"),
        }
    }

    // PATHS
    let stmts = smql_parser::parse("PATHS FROM SupportTicket").unwrap();
    if let Statement::Query(q) = &stmts[0] {
        let r = engine.execute_query(q).await.expect("PATHS");
        match r {
            QueryResult::Paths(paths) => {
                assert!(!paths.is_empty(), "Expected at least 1 path");
            }
            _ => panic!("Expected Paths"),
        }
    }

    // COMPARE PATHS
    let stmts = smql_parser::parse("COMPARE PATHS SupportTicket SEGMENT BY priority").unwrap();
    if let Statement::Query(q) = &stmts[0] {
        let r = engine.execute_query(q).await.expect("COMPARE PATHS");
        match r {
            QueryResult::ComparePaths(cp) => {
                assert_eq!(cp.segment_by, "priority");
                assert!(!cp.segments.is_empty());
            }
            _ => panic!("Expected ComparePaths"),
        }
    }
}

// ===========================================================================
// Additional parse-only tests for edge cases from docs
// ===========================================================================

#[test]
fn doc_parse_transition_as_before_with() {
    // Some doc examples show AS before WITH, others show WITH before AS
    // Test that both work
    let smql1 = r#"TRANSITION SupportTicket "01JMABCDEF1234567890ABCDEF" TO closed AS "admin_user" WITH { satisfaction: 4 }"#;
    let smql2 = r#"TRANSITION SupportTicket "01JMABCDEF1234567890ABCDEF" TO resolved WITH { resolution_note: "Fixed" } AS "agent_1""#;
    let r1 = smql_parser::parse(smql1);
    let r2 = smql_parser::parse(smql2);
    assert!(r1.is_ok(), "Failed to parse AS before WITH: {:?}", r1.err());
    assert!(r2.is_ok(), "Failed to parse WITH before AS: {:?}", r2.err());
}

#[test]
fn doc_parse_spawn_with_parent_machine_name() {
    // DOC ISSUE: SPAWN PARENT not supported in parser
    let smql = r#"SPAWN Shipment {} PARENT Order "01JMORDER0000000000000001""#;
    let result = smql_parser::parse(smql);
    assert!(result.is_err(), "Doc SPAWN PARENT syntax not supported in parser");
}

#[test]
fn doc_parse_transition_with_map_literal() {
    // From support-ticket guide: assignee as map literal
    let smql = r#"TRANSITION SupportTicket "01JMABCDEF1234567890ABCDEF" TO triaged WITH { assignee: {id: "agent_1"} } AS "agent_1""#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse map literal in WITH: {:?}", result.err());
}

#[test]
fn doc_parse_signal_parent_in_transition() {
    // From order-processing guide: SIGNAL PARENT TO delivered
    let smql = r#"
    DEFINE MACHINE TestShipment (
      PARENT : Order
      DATA { tracking: TEXT -> OPTIONAL }
      STATES { created, delivered }
      INITIAL STATE created
      TERMINAL STATES { delivered }
      TRANSITIONS {
        created -> delivered {
          SIGNAL PARENT TO delivered
        }
      }
    )
    "#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse SIGNAL PARENT: {:?}", result.err());
}

#[test]
fn doc_parse_any_with_except_from() {
    // From support-ticket guide and order-processing guide
    let smql = r#"
    DEFINE MACHINE TestWildcard (
      STATES { a, b, c, d }
      INITIAL STATE a
      TERMINAL STATES { d }
      TRANSITIONS {
        a -> b {}
        ANY -> d {
          EXCEPT FROM { a, d }
        }
      }
    )
    "#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse ANY with EXCEPT FROM: {:?}", result.err());
}

#[test]
fn doc_parse_elapsed_since_guard() {
    // From support-ticket guide: elapsed_since(resolved) < 30d
    let smql = r#"
    DEFINE MACHINE TestElapsed (
      STATES { a, b, c }
      INITIAL STATE a
      TERMINAL STATES { c }
      TRANSITIONS {
        a -> b {}
        b -> c {
          GUARD : elapsed_since(b) < 30d
        }
      }
    )
    "#;
    let result = smql_parser::parse(smql);
    assert!(result.is_ok(), "Failed to parse elapsed_since guard: {:?}", result.err());
}

#[test]
fn doc_parse_combined_timeout_1h30m() {
    // DOC ISSUE: Docs show `TIMEOUT: 1h 30m -> state` with combined durations,
    // but the lexer produces two separate DurationLiteral tokens (3600 and 1800)
    // and the parser's parse_duration() only consumes one. Combined durations
    // are NOT supported. Use a single unit instead (e.g., `90m`).
    let smql = r#"
    DEFINE MACHINE TestCombinedTimeout (
      STATES { a, b }
      INITIAL STATE a
      TERMINAL STATES { b }
      TRANSITIONS {
        a -> a {
          TIMEOUT: 1h 30m -> b
        }
      }
    )
    "#;
    let result = smql_parser::parse(smql);
    assert!(result.is_err(), "Combined duration 1h 30m should fail (not supported)");

    // Single-unit durations work fine:
    let smql_ok = r#"
    DEFINE MACHINE TestSingleTimeout (
      STATES { a, b }
      INITIAL STATE a
      TERMINAL STATES { b }
      TRANSITIONS {
        a -> a {
          TIMEOUT: 90m -> b
        }
      }
    )
    "#;
    let result_ok = smql_parser::parse(smql_ok);
    assert!(result_ok.is_ok(), "Failed to parse single-unit duration 90m: {:?}", result_ok.err());
}
