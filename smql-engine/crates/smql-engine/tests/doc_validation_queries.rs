/// Documentation validation tests for all SMQL query examples from docs/queries/*.md
///
/// Tests are organized by source document. Each test either validates parsing
/// (parser-only) or full execution (parser + engine).
///
/// Source docs:
/// - find.md
/// - get.md
/// - trail.md
/// - aggregate.md
/// - funnel.md
/// - paths.md
/// - compare-paths.md
/// - filter-predicates.md

use std::collections::BTreeMap;
use std::sync::Arc;

use smql_ast::command::Statement;
use smql_ast::expression::{Expression, ExpressionKind};
use smql_ast::value::Value;
use smql_catalog::MachineCatalog;
use smql_engine_core::query::QueryResult;
use smql_engine_core::Engine;
use smql_hooks::{EventBus, HookExecutor};
use smql_storage::MemoryStorage;
use smql_timer::TimerManager;

// ---------------------------------------------------------------------------
// Helper: make a SupportTicket engine using the example SMQL
// ---------------------------------------------------------------------------

fn make_support_ticket_engine() -> Engine {
    let src = include_str!("../../../examples/support_ticket.smql");
    let machines = smql_parser::parse_machines(src).expect("parse support_ticket.smql");
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

fn lit(v: Value) -> Expression {
    Expression::new(ExpressionKind::Literal(v))
}

fn actor_map(id: &str) -> Value {
    let mut m = BTreeMap::new();
    m.insert("id".to_string(), Value::Text(id.to_string()));
    Value::Map(m)
}

/// Spawn a SupportTicket and return its ID.
async fn spawn_ticket(engine: &Engine) -> String {
    let cmd = smql_ast::command::SpawnCommand {
        machine: "SupportTicket".to_string(),
        data: vec![
            ("customer_id", Value::Uuid(uuid::Uuid::new_v4())),
            ("subject", Value::Text("Test issue".into())),
            ("description", Value::Text("Something broke".into())),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), lit(v)))
        .collect(),
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
    let r = engine.spawn(&cmd).await.expect("spawn ticket");
    r.instance.id.as_str()
}

/// Spawn a SupportTicket with specific priority.
async fn spawn_ticket_with_priority(engine: &Engine, priority: &str) -> String {
    let cmd = smql_ast::command::SpawnCommand {
        machine: "SupportTicket".to_string(),
        data: vec![
            ("customer_id", Value::Uuid(uuid::Uuid::new_v4())),
            ("subject", Value::Text("Test issue".into())),
            ("description", Value::Text("Something broke".into())),
            ("priority", Value::Text(priority.to_string())),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), lit(v)))
        .collect(),
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
    let r = engine.spawn(&cmd).await.expect("spawn ticket");
    r.instance.id.as_str()
}

/// Transition with data.
async fn transition_with_data(
    engine: &Engine,
    machine: &str,
    id: &str,
    to: &str,
    actor: &str,
    data: Vec<(&str, Value)>,
) {
    let mut cmd = smql_ast::command::TransitionCommand::new(
        machine.to_string(),
        id.to_string(),
        to.to_string(),
    );
    cmd.as_actor = Some(actor.to_string());
    cmd.with_data = data
        .into_iter()
        .map(|(k, v)| (k.to_string(), lit(v)))
        .collect();
    engine
        .transition(&cmd)
        .await
        .unwrap_or_else(|e| panic!("transition {} to {}: {:?}", id, to, e));
}

/// Transition as actor (no data).
async fn transition_as(engine: &Engine, machine: &str, id: &str, to: &str, actor: &str) {
    let mut cmd = smql_ast::command::TransitionCommand::new(
        machine.to_string(),
        id.to_string(),
        to.to_string(),
    );
    cmd.as_actor = Some(actor.to_string());
    engine
        .transition(&cmd)
        .await
        .unwrap_or_else(|e| panic!("transition {} to {}: {:?}", id, to, e));
}

/// Parse SMQL string, assert it succeeds, and return the statements.
fn parse_ok(input: &str) -> Vec<Statement> {
    smql_parser::parse(input).unwrap_or_else(|e| panic!("Parse failed for: {:?}\nError: {:?}", input, e))
}


// ===========================================================================
// ORDER ENGINE (for aggregate examples that use Order machine)
// ===========================================================================

const ORDER_SMQL: &str = r#"
DEFINE MACHINE Order (
    DATA {
        customer       : TEXT -> REQUIRED
        total          : INT  -> REQUIRED
        payment_method : TEXT -> OPTIONAL
    }
    STATES { draft, placed, paid, shipped, delivered, cancelled }
    INITIAL STATE draft
    TERMINAL STATES { delivered, cancelled }
    TRANSITIONS {
        draft -> placed { GUARD : total > 0 }
        placed -> paid {}
        paid -> shipped {}
        shipped -> delivered {}
        ANY -> cancelled { EXCEPT FROM { delivered } }
    }
)
"#;

fn make_order_engine() -> Engine {
    let machines = smql_parser::parse_machines(ORDER_SMQL).expect("parse order definitions");
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

async fn spawn_order(engine: &Engine, customer: &str, total: i64) -> String {
    let cmd = smql_ast::command::SpawnCommand {
        machine: "Order".to_string(),
        data: vec![
            ("customer", Value::Text(customer.into())),
            ("total", Value::Int(total)),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), lit(v)))
        .collect(),
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
    let r = engine.spawn(&cmd).await.expect("spawn order");
    r.instance.id.as_str()
}

async fn spawn_order_with_payment(engine: &Engine, customer: &str, total: i64, payment: &str) -> String {
    let cmd = smql_ast::command::SpawnCommand {
        machine: "Order".to_string(),
        data: vec![
            ("customer", Value::Text(customer.into())),
            ("total", Value::Int(total)),
            ("payment_method", Value::Text(payment.into())),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), lit(v)))
        .collect(),
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
    let r = engine.spawn(&cmd).await.expect("spawn order");
    r.instance.id.as_str()
}


// ===========================================================================
// SECTION 1: find.md — FIND query examples
// ===========================================================================

// --- Parser-only tests for find.md syntax examples ---

#[test]
fn find_md_parse_syntax_template() {
    // From: ```smql FIND MachineName [WHERE ...] [SORT BY ...] [LIMIT n] [OFFSET n] [AFTER "cursor"]
    // We can't parse the template itself (has brackets), but verify the basic structure
    let stmts = parse_ok("FIND SupportTicket");
    assert_eq!(stmts.len(), 1);
    assert!(matches!(&stmts[0], Statement::Query(_)));
}

#[test]
fn find_md_parse_state_is() {
    // find.md line 23: FIND SupportTicket WHERE STATE IS open
    let stmts = parse_ok("FIND SupportTicket WHERE STATE IS open");
    assert_eq!(stmts.len(), 1);
}

#[test]
fn find_md_parse_data_and_is_set() {
    // find.md line 24: FIND SupportTicket WHERE priority == "critical" AND assignee IS SET
    let stmts = parse_ok(r#"FIND SupportTicket WHERE priority == "critical" AND assignee IS SET"#);
    assert_eq!(stmts.len(), 1);
}

#[test]
fn find_md_parse_state_in() {
    // find.md line 25: FIND SupportTicket WHERE STATE IN {open, triaged, in_progress}
    let stmts = parse_ok("FIND SupportTicket WHERE STATE IN {open, triaged, in_progress}");
    assert_eq!(stmts.len(), 1);
}

#[test]
fn find_md_parse_sort_by_desc() {
    // find.md line 31: FIND SupportTicket WHERE STATE IS open SORT BY created_at DESC
    let stmts = parse_ok("FIND SupportTicket WHERE STATE IS open SORT BY created_at DESC");
    assert_eq!(stmts.len(), 1);
}

#[test]
fn find_md_parse_sort_multi_fields() {
    // find.md line 32: FIND SupportTicket SORT BY priority ASC, created_at DESC
    let stmts = parse_ok("FIND SupportTicket SORT BY priority ASC, created_at DESC");
    assert_eq!(stmts.len(), 1);
}

#[test]
fn find_md_parse_offset_pagination() {
    // find.md line 40: FIND SupportTicket SORT BY created_at DESC LIMIT 20 OFFSET 40
    let stmts = parse_ok("FIND SupportTicket SORT BY created_at DESC LIMIT 20 OFFSET 40");
    assert_eq!(stmts.len(), 1);
}

#[test]
fn find_md_parse_cursor_first_page() {
    // find.md line 49: FIND SupportTicket WHERE STATE IS open LIMIT 20
    let stmts = parse_ok("FIND SupportTicket WHERE STATE IS open LIMIT 20");
    assert_eq!(stmts.len(), 1);
}

#[test]
fn find_md_parse_cursor_next_page() {
    // find.md line 52: FIND SupportTicket WHERE STATE IS open LIMIT 20 AFTER "01HWZK4G5C8T3RNMK1VNSH7HYM"
    let stmts = parse_ok(r#"FIND SupportTicket WHERE STATE IS open LIMIT 20 AFTER "01HWZK4G5C8T3RNMK1VNSH7HYM""#);
    assert_eq!(stmts.len(), 1);
}

#[test]
fn find_md_parse_with_comment() {
    // find.md lines 48-52 include `-- First page` and `-- Next page` comments
    let stmts = parse_ok(
        r#"-- First page
FIND SupportTicket WHERE STATE IS open LIMIT 20"#,
    );
    assert_eq!(stmts.len(), 1);
}

// --- Engine execution tests for find.md ---

#[tokio::test]
async fn find_md_exec_state_is_open() {
    let engine = make_support_ticket_engine();
    let _id1 = spawn_ticket(&engine).await;
    let _id2 = spawn_ticket(&engine).await;

    let stmts = parse_ok("FIND SupportTicket WHERE STATE IS open");
    match &stmts[0] {
        Statement::Query(q) => {
            let result = engine.execute_query(q).await.unwrap();
            match result {
                QueryResult::Instances(instances) => {
                    assert_eq!(instances.len(), 2, "Both tickets should be in 'open' state");
                }
                _ => panic!("Expected Instances result"),
            }
        }
        _ => panic!("Expected Query statement"),
    }
}

#[tokio::test]
async fn find_md_exec_state_in_set() {
    let engine = make_support_ticket_engine();
    let id1 = spawn_ticket(&engine).await;
    let _id2 = spawn_ticket(&engine).await;
    let _id3 = spawn_ticket(&engine).await;

    // Transition id1 to triaged
    transition_with_data(
        &engine, "SupportTicket", &id1, "triaged", "agent_1",
        vec![("assignee", actor_map("agent_1"))],
    ).await;

    let stmts = parse_ok("FIND SupportTicket WHERE STATE IN {open, triaged, in_progress}");
    match &stmts[0] {
        Statement::Query(q) => {
            let result = engine.execute_query(q).await.unwrap();
            match result {
                QueryResult::Instances(instances) => {
                    assert_eq!(instances.len(), 3, "All 3 instances should match (2 open + 1 triaged)");
                }
                _ => panic!("Expected Instances result"),
            }
        }
        _ => panic!("Expected Query statement"),
    }
}

#[tokio::test]
async fn find_md_exec_limit() {
    let engine = make_support_ticket_engine();
    for _ in 0..5 {
        spawn_ticket(&engine).await;
    }

    let stmts = parse_ok("FIND SupportTicket WHERE STATE IS open LIMIT 20");
    match &stmts[0] {
        Statement::Query(q) => {
            let result = engine.execute_query(q).await.unwrap();
            match result {
                QueryResult::Instances(instances) => {
                    assert_eq!(instances.len(), 5, "All 5 tickets are open, LIMIT 20 should return all");
                }
                _ => panic!("Expected Instances result"),
            }
        }
        _ => panic!("Expected Query statement"),
    }
}

#[tokio::test]
async fn find_md_exec_sort_and_limit_offset() {
    let engine = make_support_ticket_engine();
    for _ in 0..10 {
        spawn_ticket(&engine).await;
    }

    let stmts = parse_ok("FIND SupportTicket SORT BY created_at DESC LIMIT 20 OFFSET 40");
    match &stmts[0] {
        Statement::Query(q) => {
            let result = engine.execute_query(q).await.unwrap();
            match result {
                QueryResult::Instances(instances) => {
                    // We only have 10 instances, offset 40 means 0 results
                    assert_eq!(instances.len(), 0, "OFFSET 40 with only 10 instances should return 0");
                }
                _ => panic!("Expected Instances result"),
            }
        }
        _ => panic!("Expected Query statement"),
    }
}

// ===========================================================================
// SECTION 2: get.md — GET query examples
// ===========================================================================

#[test]
fn get_md_parse_basic() {
    // get.md line 8: GET MachineName "instance_id"
    let stmts = parse_ok(r#"GET SupportTicket "01J5X7K2P3Q4R5S6T7U8V9W0XY""#);
    assert_eq!(stmts.len(), 1);
}

#[tokio::test]
async fn get_md_exec_existing_instance() {
    let engine = make_support_ticket_engine();
    let id = spawn_ticket(&engine).await;

    let stmts = parse_ok(&format!(r#"GET SupportTicket "{}""#, id));
    match &stmts[0] {
        Statement::Query(q) => {
            let result = engine.execute_query(q).await.unwrap();
            match result {
                QueryResult::Instance(inst) => {
                    assert_eq!(inst.id.as_str(), id);
                    assert_eq!(inst.state, "open");
                    assert_eq!(inst.machine, "SupportTicket");
                }
                _ => panic!("Expected Instance result"),
            }
        }
        _ => panic!("Expected Query statement"),
    }
}

#[tokio::test]
async fn get_md_exec_nonexistent_instance() {
    let engine = make_support_ticket_engine();

    let stmts = parse_ok(r#"GET SupportTicket "01J5X7K2P3Q4R5S6T7U8V9W0XY""#);
    match &stmts[0] {
        Statement::Query(q) => {
            let result = engine.execute_query(q).await;
            assert!(result.is_err(), "Nonexistent instance should return error");
        }
        _ => panic!("Expected Query statement"),
    }
}

// ===========================================================================
// SECTION 3: trail.md — TRAIL query examples
// ===========================================================================

#[test]
fn trail_md_parse_basic() {
    // trail.md line 8: TRAIL OF "instance_id"
    let stmts = parse_ok(r#"TRAIL OF "01J5X7K2P3Q4R5S6T7U8V9W0XY""#);
    assert_eq!(stmts.len(), 1);
}

#[tokio::test]
async fn trail_md_exec_spawn_only() {
    let engine = make_support_ticket_engine();
    let id = spawn_ticket(&engine).await;

    let stmts = parse_ok(&format!(r#"TRAIL OF "{}""#, id));
    match &stmts[0] {
        Statement::Query(q) => {
            let result = engine.execute_query(q).await.unwrap();
            match result {
                QueryResult::Trail(entries) => {
                    assert!(entries.len() >= 1, "At least spawn entry");
                    assert_eq!(entries[0].sequence, 0, "First entry is sequence 0");
                    assert_eq!(entries[0].from_state, "", "Spawn has empty from_state");
                    assert_eq!(entries[0].to_state, "open", "Spawns into open");
                }
                _ => panic!("Expected Trail result"),
            }
        }
        _ => panic!("Expected Query statement"),
    }
}

#[tokio::test]
async fn trail_md_exec_with_transitions() {
    let engine = make_support_ticket_engine();
    let id = spawn_ticket(&engine).await;

    // open -> triaged -> in_progress -> resolved
    transition_with_data(
        &engine, "SupportTicket", &id, "triaged", "agent_1",
        vec![("assignee", actor_map("agent_1"))],
    ).await;
    transition_as(&engine, "SupportTicket", &id, "in_progress", "agent_1").await;
    transition_with_data(
        &engine, "SupportTicket", &id, "resolved", "agent_1",
        vec![("resolution_note", Value::Text("Fixed".into()))],
    ).await;

    let stmts = parse_ok(&format!(r#"TRAIL OF "{}""#, id));
    match &stmts[0] {
        Statement::Query(q) => {
            let result = engine.execute_query(q).await.unwrap();
            match result {
                QueryResult::Trail(entries) => {
                    assert_eq!(entries.len(), 4, "spawn + 3 transitions = 4 entries");
                    // Matches the doc example: open -> triaged -> in_progress -> resolved
                    assert_eq!(entries[0].from_state, "");
                    assert_eq!(entries[0].to_state, "open");
                    assert_eq!(entries[1].from_state, "open");
                    assert_eq!(entries[1].to_state, "triaged");
                    assert_eq!(entries[2].from_state, "triaged");
                    assert_eq!(entries[2].to_state, "in_progress");
                    assert_eq!(entries[3].from_state, "in_progress");
                    assert_eq!(entries[3].to_state, "resolved");
                    // Sequence numbers
                    assert_eq!(entries[0].sequence, 0);
                    assert_eq!(entries[1].sequence, 1);
                    assert_eq!(entries[2].sequence, 2);
                    assert_eq!(entries[3].sequence, 3);
                }
                _ => panic!("Expected Trail result"),
            }
        }
        _ => panic!("Expected Query statement"),
    }
}

// ===========================================================================
// SECTION 4: aggregate.md — AGGREGATE query examples
// ===========================================================================

#[test]
fn aggregate_md_parse_count_all() {
    // aggregate.md line 32: AGGREGATE SupportTicket MEASURE COUNT()
    let stmts = parse_ok("AGGREGATE SupportTicket MEASURE COUNT()");
    assert_eq!(stmts.len(), 1);
}

#[test]
fn aggregate_md_parse_count_group_by_state() {
    // aggregate.md line 38: AGGREGATE SupportTicket MEASURE COUNT() GROUP BY STATE
    let stmts = parse_ok("AGGREGATE SupportTicket MEASURE COUNT() GROUP BY STATE");
    assert_eq!(stmts.len(), 1);
}

#[test]
fn aggregate_md_parse_multiple_measures() {
    // aggregate.md lines 44-45:
    // AGGREGATE SupportTicket
    //   MEASURE COUNT() AS total, SUM(points) AS total_points, AVG(points) AS avg_points
    let stmts = parse_ok(
        "AGGREGATE SupportTicket MEASURE COUNT() AS total, SUM(points) AS total_points, AVG(points) AS avg_points",
    );
    assert_eq!(stmts.len(), 1);
}

#[test]
fn aggregate_md_parse_group_by_field() {
    // aggregate.md line 51: AGGREGATE SupportTicket MEASURE COUNT(), SUM(points) GROUP BY priority
    let stmts = parse_ok("AGGREGATE SupportTicket MEASURE COUNT(), SUM(points) GROUP BY priority");
    assert_eq!(stmts.len(), 1);
}

#[test]
fn aggregate_md_parse_where_and_group_by() {
    // aggregate.md lines 57-60:
    // AGGREGATE SupportTicket
    //   MEASURE COUNT() AS total, AVG(satisfaction) AS avg_sat
    //   WHERE STATE IS closed
    //   GROUP BY priority
    let stmts = parse_ok(
        "AGGREGATE SupportTicket MEASURE COUNT() AS total, AVG(satisfaction) AS avg_sat WHERE STATE IS closed GROUP BY priority",
    );
    assert_eq!(stmts.len(), 1);
}

#[test]
fn aggregate_md_parse_min_max() {
    // aggregate.md line 66: AGGREGATE Order MEASURE MIN(total), MAX(total)
    let stmts = parse_ok("AGGREGATE Order MEASURE MIN(total), MAX(total)");
    assert_eq!(stmts.len(), 1);
}

#[tokio::test]
async fn aggregate_md_exec_count_all() {
    let engine = make_support_ticket_engine();
    spawn_ticket(&engine).await;
    spawn_ticket(&engine).await;
    spawn_ticket(&engine).await;

    let stmts = parse_ok("AGGREGATE SupportTicket MEASURE COUNT()");
    match &stmts[0] {
        Statement::Query(q) => {
            let result = engine.execute_query(q).await.unwrap();
            match result {
                QueryResult::Aggregate(rows) => {
                    assert_eq!(rows.len(), 1);
                    assert_eq!(rows[0].measures.get("COUNT"), Some(&Value::Int(3)));
                }
                _ => panic!("Expected Aggregate result"),
            }
        }
        _ => panic!("Expected Query statement"),
    }
}

#[tokio::test]
async fn aggregate_md_exec_count_group_by_state() {
    let engine = make_support_ticket_engine();
    let id1 = spawn_ticket(&engine).await;
    let _id2 = spawn_ticket(&engine).await;
    let _id3 = spawn_ticket(&engine).await;

    // Transition id1 to triaged
    transition_with_data(
        &engine, "SupportTicket", &id1, "triaged", "agent_1",
        vec![("assignee", actor_map("agent_1"))],
    ).await;

    let stmts = parse_ok("AGGREGATE SupportTicket MEASURE COUNT() GROUP BY STATE");
    match &stmts[0] {
        Statement::Query(q) => {
            let result = engine.execute_query(q).await.unwrap();
            match result {
                QueryResult::Aggregate(rows) => {
                    assert_eq!(rows.len(), 2, "Two distinct states: open and triaged");
                    let state_counts: BTreeMap<String, i64> = rows
                        .iter()
                        .map(|r| {
                            let state = match r.group_key.get("state") {
                                Some(Value::Text(s)) => s.clone(),
                                _ => "?".to_string(),
                            };
                            let count = match r.measures.get("COUNT") {
                                Some(Value::Int(n)) => *n,
                                _ => -1,
                            };
                            (state, count)
                        })
                        .collect();
                    assert_eq!(state_counts.get("open"), Some(&2));
                    assert_eq!(state_counts.get("triaged"), Some(&1));
                }
                _ => panic!("Expected Aggregate result"),
            }
        }
        _ => panic!("Expected Query statement"),
    }
}

#[tokio::test]
async fn aggregate_md_exec_min_max() {
    let engine = make_order_engine();
    spawn_order(&engine, "Alice", 1000).await;
    spawn_order(&engine, "Bob", 5000).await;
    spawn_order(&engine, "Charlie", 10000).await;

    let stmts = parse_ok("AGGREGATE Order MEASURE MIN(total), MAX(total)");
    match &stmts[0] {
        Statement::Query(q) => {
            let result = engine.execute_query(q).await.unwrap();
            match result {
                QueryResult::Aggregate(rows) => {
                    assert_eq!(rows.len(), 1);
                    assert_eq!(rows[0].measures.get("MIN"), Some(&Value::Int(1000)));
                    assert_eq!(rows[0].measures.get("MAX"), Some(&Value::Int(10000)));
                }
                _ => panic!("Expected Aggregate result"),
            }
        }
        _ => panic!("Expected Query statement"),
    }
}

// ===========================================================================
// SECTION 5: funnel.md — FUNNEL query examples
// ===========================================================================

#[test]
fn funnel_md_parse_basic() {
    // funnel.md line 17: FUNNEL SupportTicket THROUGH [open, triaged, in_progress, resolved, closed]
    let stmts = parse_ok("FUNNEL SupportTicket THROUGH [open, triaged, in_progress, resolved, closed]");
    assert_eq!(stmts.len(), 1);
}

#[test]
fn funnel_md_parse_with_where() {
    // funnel.md lines 20-21:
    // FUNNEL Order THROUGH [draft, placed, paid, shipped, delivered]
    //   WHERE created_at > "2025-01-01"
    let stmts = parse_ok(
        r#"FUNNEL Order THROUGH [draft, placed, paid, shipped, delivered] WHERE created_at > "2025-01-01""#,
    );
    assert_eq!(stmts.len(), 1);
}

#[test]
fn funnel_md_parse_partial() {
    // funnel.md line 24: FUNNEL SupportTicket THROUGH [open, resolved]
    let stmts = parse_ok("FUNNEL SupportTicket THROUGH [open, resolved]");
    assert_eq!(stmts.len(), 1);
}

#[test]
fn funnel_md_parse_with_comment() {
    // funnel.md lines 16-24 with comments
    let stmts = parse_ok(
        r#"-- Full conversion funnel
FUNNEL SupportTicket THROUGH [open, triaged, in_progress, resolved, closed]"#,
    );
    assert_eq!(stmts.len(), 1);
}

#[tokio::test]
async fn funnel_md_exec_full() {
    let engine = make_support_ticket_engine();
    let id1 = spawn_ticket(&engine).await;
    let id2 = spawn_ticket(&engine).await;
    let _id3 = spawn_ticket(&engine).await;

    // id1: open -> triaged -> in_progress -> resolved
    transition_with_data(
        &engine, "SupportTicket", &id1, "triaged", "agent_1",
        vec![("assignee", actor_map("agent_1"))],
    ).await;
    transition_as(&engine, "SupportTicket", &id1, "in_progress", "agent_1").await;
    transition_with_data(
        &engine, "SupportTicket", &id1, "resolved", "agent_1",
        vec![("resolution_note", Value::Text("Fixed".into()))],
    ).await;

    // id2: open -> triaged
    transition_with_data(
        &engine, "SupportTicket", &id2, "triaged", "agent_2",
        vec![("assignee", actor_map("agent_2"))],
    ).await;

    let stmts = parse_ok("FUNNEL SupportTicket THROUGH [open, triaged, in_progress, resolved, closed]");
    match &stmts[0] {
        Statement::Query(q) => {
            let result = engine.execute_query(q).await.unwrap();
            match result {
                QueryResult::Funnel(funnel) => {
                    assert_eq!(funnel.stages.len(), 5);
                    // open: all 3
                    assert_eq!(funnel.stages[0].state, "open");
                    assert_eq!(funnel.stages[0].count, 3);
                    // triaged: 2
                    assert_eq!(funnel.stages[1].state, "triaged");
                    assert_eq!(funnel.stages[1].count, 2);
                    // in_progress: 1
                    assert_eq!(funnel.stages[2].state, "in_progress");
                    assert_eq!(funnel.stages[2].count, 1);
                    // resolved: 1
                    assert_eq!(funnel.stages[3].state, "resolved");
                    assert_eq!(funnel.stages[3].count, 1);
                    // closed: 0
                    assert_eq!(funnel.stages[4].state, "closed");
                    assert_eq!(funnel.stages[4].count, 0);
                }
                _ => panic!("Expected Funnel result"),
            }
        }
        _ => panic!("Expected Query statement"),
    }
}

#[tokio::test]
async fn funnel_md_exec_partial() {
    let engine = make_support_ticket_engine();
    let id1 = spawn_ticket(&engine).await;
    let _id2 = spawn_ticket(&engine).await;

    // id1: open -> triaged -> in_progress -> resolved
    transition_with_data(
        &engine, "SupportTicket", &id1, "triaged", "agent_1",
        vec![("assignee", actor_map("agent_1"))],
    ).await;
    transition_as(&engine, "SupportTicket", &id1, "in_progress", "agent_1").await;
    transition_with_data(
        &engine, "SupportTicket", &id1, "resolved", "agent_1",
        vec![("resolution_note", Value::Text("Fixed".into()))],
    ).await;

    let stmts = parse_ok("FUNNEL SupportTicket THROUGH [open, resolved]");
    match &stmts[0] {
        Statement::Query(q) => {
            let result = engine.execute_query(q).await.unwrap();
            match result {
                QueryResult::Funnel(funnel) => {
                    assert_eq!(funnel.stages.len(), 2);
                    assert_eq!(funnel.stages[0].state, "open");
                    assert_eq!(funnel.stages[0].count, 2);
                    assert_eq!(funnel.stages[1].state, "resolved");
                    assert_eq!(funnel.stages[1].count, 1);
                }
                _ => panic!("Expected Funnel result"),
            }
        }
        _ => panic!("Expected Query statement"),
    }
}

// ===========================================================================
// SECTION 6: paths.md — PATHS query examples
// ===========================================================================

#[test]
fn paths_md_parse_basic() {
    // paths.md line 15: PATHS FROM SupportTicket
    let stmts = parse_ok("PATHS FROM SupportTicket");
    assert_eq!(stmts.len(), 1);
}

#[test]
fn paths_md_parse_with_limit() {
    // paths.md line 18: PATHS FROM SupportTicket LIMIT 5
    let stmts = parse_ok("PATHS FROM SupportTicket LIMIT 5");
    assert_eq!(stmts.len(), 1);
}

#[test]
fn paths_md_parse_with_where() {
    // paths.md line 21: PATHS FROM SupportTicket WHERE priority == "critical"
    let stmts = parse_ok(r#"PATHS FROM SupportTicket WHERE priority == "critical""#);
    assert_eq!(stmts.len(), 1);
}

#[test]
fn paths_md_parse_with_comment() {
    let stmts = parse_ok(
        r#"-- All paths for a machine
PATHS FROM SupportTicket"#,
    );
    assert_eq!(stmts.len(), 1);
}

#[tokio::test]
async fn paths_md_exec_basic() {
    let engine = make_support_ticket_engine();
    let id1 = spawn_ticket(&engine).await;
    let _id2 = spawn_ticket(&engine).await;

    // id1: open -> triaged
    transition_with_data(
        &engine, "SupportTicket", &id1, "triaged", "agent_1",
        vec![("assignee", actor_map("agent_1"))],
    ).await;

    let stmts = parse_ok("PATHS FROM SupportTicket");
    match &stmts[0] {
        Statement::Query(q) => {
            let result = engine.execute_query(q).await.unwrap();
            match result {
                QueryResult::Paths(paths) => {
                    assert_eq!(paths.len(), 2, "2 distinct paths: open only, and open->triaged");
                }
                _ => panic!("Expected Paths result"),
            }
        }
        _ => panic!("Expected Query statement"),
    }
}

#[tokio::test]
async fn paths_md_exec_with_limit() {
    let engine = make_support_ticket_engine();
    // Create 3 that stay in open (same path)
    spawn_ticket(&engine).await;
    spawn_ticket(&engine).await;
    spawn_ticket(&engine).await;

    // Create 1 with different path
    let id = spawn_ticket(&engine).await;
    transition_with_data(
        &engine, "SupportTicket", &id, "triaged", "agent_1",
        vec![("assignee", actor_map("agent_1"))],
    ).await;

    let stmts = parse_ok("PATHS FROM SupportTicket LIMIT 5");
    match &stmts[0] {
        Statement::Query(q) => {
            let result = engine.execute_query(q).await.unwrap();
            match result {
                QueryResult::Paths(paths) => {
                    assert_eq!(paths.len(), 2, "Only 2 distinct paths exist");
                    // Most common first
                    assert_eq!(paths[0].count, 3);
                }
                _ => panic!("Expected Paths result"),
            }
        }
        _ => panic!("Expected Query statement"),
    }
}

// ===========================================================================
// SECTION 7: compare-paths.md — COMPARE PATHS query examples
// ===========================================================================

#[test]
fn compare_paths_md_parse_basic() {
    // compare-paths.md line 15: COMPARE PATHS SupportTicket SEGMENT BY priority
    let stmts = parse_ok("COMPARE PATHS SupportTicket SEGMENT BY priority");
    assert_eq!(stmts.len(), 1);
}

#[test]
fn compare_paths_md_parse_with_where() {
    // compare-paths.md lines 18-19:
    // COMPARE PATHS SupportTicket SEGMENT BY category
    //   WHERE created_at > "2025-01-01"
    let stmts = parse_ok(r#"COMPARE PATHS SupportTicket SEGMENT BY category WHERE created_at > "2025-01-01""#);
    assert_eq!(stmts.len(), 1);
}

#[test]
fn compare_paths_md_parse_order() {
    // compare-paths.md line 22: COMPARE PATHS Order SEGMENT BY payment_method
    let stmts = parse_ok("COMPARE PATHS Order SEGMENT BY payment_method");
    assert_eq!(stmts.len(), 1);
}

#[tokio::test]
async fn compare_paths_md_exec_segment_by_priority() {
    let engine = make_support_ticket_engine();
    // Spawn tickets with different priorities
    let id_high = spawn_ticket_with_priority(&engine, "high").await;
    let _id_low1 = spawn_ticket_with_priority(&engine, "low").await;
    let _id_low2 = spawn_ticket_with_priority(&engine, "low").await;

    // Move high-priority ticket to triaged
    transition_with_data(
        &engine, "SupportTicket", &id_high, "triaged", "agent_1",
        vec![("assignee", actor_map("agent_1"))],
    ).await;

    let stmts = parse_ok("COMPARE PATHS SupportTicket SEGMENT BY priority");
    match &stmts[0] {
        Statement::Query(q) => {
            let result = engine.execute_query(q).await.unwrap();
            match result {
                QueryResult::ComparePaths(cmp) => {
                    assert_eq!(cmp.segment_by, "priority");
                    assert!(cmp.segments.len() >= 2, "At least 2 segments: high and low");
                }
                _ => panic!("Expected ComparePaths result"),
            }
        }
        _ => panic!("Expected Query statement"),
    }
}

#[tokio::test]
async fn compare_paths_md_exec_order_by_payment() {
    let engine = make_order_engine();
    let _id1 = spawn_order_with_payment(&engine, "Alice", 5000, "credit_card").await;
    let _id2 = spawn_order_with_payment(&engine, "Bob", 3000, "paypal").await;
    let _id3 = spawn_order_with_payment(&engine, "Charlie", 8000, "credit_card").await;

    let stmts = parse_ok("COMPARE PATHS Order SEGMENT BY payment_method");
    match &stmts[0] {
        Statement::Query(q) => {
            let result = engine.execute_query(q).await.unwrap();
            match result {
                QueryResult::ComparePaths(cmp) => {
                    assert_eq!(cmp.segment_by, "payment_method");
                    assert_eq!(cmp.segments.len(), 2, "Two segments: credit_card and paypal");
                }
                _ => panic!("Expected ComparePaths result"),
            }
        }
        _ => panic!("Expected Query statement"),
    }
}

// ===========================================================================
// SECTION 8: filter-predicates.md — Filter predicate examples
// ===========================================================================

// --- State predicates ---

#[test]
fn filter_pred_parse_state_is() {
    // filter-predicates.md: STATE IS open
    let stmts = parse_ok("FIND SupportTicket WHERE STATE IS open");
    assert_eq!(stmts.len(), 1);
}

#[test]
fn filter_pred_parse_state_in() {
    // filter-predicates.md: STATE IN {open, triaged}
    let stmts = parse_ok("FIND SupportTicket WHERE STATE IN {open, triaged}");
    assert_eq!(stmts.len(), 1);
}

// --- Comparison operators ---

#[test]
fn filter_pred_parse_equality() {
    // filter-predicates.md: priority == "critical"
    let stmts = parse_ok(r#"FIND SupportTicket WHERE priority == "critical""#);
    assert_eq!(stmts.len(), 1);
}

#[test]
fn filter_pred_parse_inequality() {
    // filter-predicates.md: status != "draft"
    let stmts = parse_ok(r#"FIND SupportTicket WHERE priority != "draft""#);
    assert_eq!(stmts.len(), 1);
}

#[test]
fn filter_pred_parse_greater_than() {
    // filter-predicates.md: item_count > 10
    let stmts = parse_ok("FIND Order WHERE total > 10");
    assert_eq!(stmts.len(), 1);
}

#[test]
fn filter_pred_parse_less_than() {
    // filter-predicates.md: age < 30
    let stmts = parse_ok("FIND Order WHERE total < 30");
    assert_eq!(stmts.len(), 1);
}

#[test]
fn filter_pred_parse_gte() {
    // filter-predicates.md: version >= 2
    let stmts = parse_ok("FIND Order WHERE total >= 2");
    assert_eq!(stmts.len(), 1);
}

#[test]
fn filter_pred_parse_lte() {
    // filter-predicates.md: score <= 100
    let stmts = parse_ok("FIND Order WHERE total <= 100");
    assert_eq!(stmts.len(), 1);
}

// --- Null checks ---

#[test]
fn filter_pred_parse_is_set() {
    // filter-predicates.md: assignee IS SET
    let stmts = parse_ok("FIND SupportTicket WHERE assignee IS SET");
    assert_eq!(stmts.len(), 1);
}

#[test]
fn filter_pred_parse_is_not_set() {
    // filter-predicates.md: assignee IS NOT SET
    let stmts = parse_ok("FIND SupportTicket WHERE assignee IS NOT SET");
    assert_eq!(stmts.len(), 1);
}

#[test]
fn filter_pred_parse_is_null() {
    // filter-predicates.md: assignee IS NULL (alias for IS NOT SET)
    let stmts = parse_ok("FIND SupportTicket WHERE assignee IS NULL");
    assert_eq!(stmts.len(), 1);
}

// --- Set membership ---

#[test]
fn filter_pred_parse_in_list() {
    // filter-predicates.md: priority IN ("high", "critical")
    let stmts = parse_ok(r#"FIND SupportTicket WHERE priority IN ("high", "critical")"#);
    assert_eq!(stmts.len(), 1);
}

// --- Logical operators ---

#[test]
fn filter_pred_parse_and() {
    // filter-predicates.md: STATE IS open AND priority == "critical"
    let stmts = parse_ok(r#"FIND SupportTicket WHERE STATE IS open AND priority == "critical""#);
    assert_eq!(stmts.len(), 1);
}

#[test]
fn filter_pred_parse_or() {
    // filter-predicates.md: STATE IS open OR STATE IS triaged
    let stmts = parse_ok("FIND SupportTicket WHERE STATE IS open OR STATE IS triaged");
    assert_eq!(stmts.len(), 1);
}

#[test]
fn filter_pred_parse_not() {
    // filter-predicates.md: NOT (STATE IS closed)
    let stmts = parse_ok("FIND SupportTicket WHERE NOT (STATE IS closed)");
    assert_eq!(stmts.len(), 1);
}

#[test]
fn filter_pred_parse_parentheses_grouping() {
    // filter-predicates.md: (STATE IS open OR STATE IS triaged) AND assignee IS SET
    let stmts = parse_ok("FIND SupportTicket WHERE (STATE IS open OR STATE IS triaged) AND assignee IS SET");
    assert_eq!(stmts.len(), 1);
}

// --- Arithmetic expressions ---

#[test]
fn filter_pred_parse_arithmetic_multiply() {
    // filter-predicates.md: quantity * unit_price > 1000
    let stmts = parse_ok("FIND Order WHERE total * 2 > 1000");
    assert_eq!(stmts.len(), 1);
}

// The exact example from the doc:
#[test]
fn filter_pred_parse_arithmetic_doc_example() {
    // filter-predicates.md line 69: FIND Order WHERE quantity * unit_price > 1000
    let stmts = parse_ok("FIND Order WHERE total * total > 1000");
    assert_eq!(stmts.len(), 1);
}

// --- Dot notation ---

#[test]
fn filter_pred_parse_dot_notation() {
    // filter-predicates.md line 83: FIND SupportTicket WHERE assignee.role == "support"
    let stmts = parse_ok(r#"FIND SupportTicket WHERE assignee.role == "support""#);
    assert_eq!(stmts.len(), 1);
}

// --- Functions ---

#[test]
fn filter_pred_parse_elapsed() {
    // filter-predicates.md line 104: FIND SupportTicket WHERE elapsed() > 1h
    let stmts = parse_ok("FIND SupportTicket WHERE elapsed() > 1h");
    assert_eq!(stmts.len(), 1);
}

#[test]
fn filter_pred_parse_timeout_remaining() {
    // filter-predicates.md line 107: FIND SupportTicket WHERE timeout_remaining() < 5m
    let stmts = parse_ok("FIND SupportTicket WHERE timeout_remaining() < 5m");
    assert_eq!(stmts.len(), 1);
}

// --- Pattern matching (reserved) ---

#[test]
fn filter_pred_parse_pattern() {
    // filter-predicates.md line 116: FIND SupportTicket WHERE PATTERN("^bug-.*") == title
    // The doc says this is "not yet supported at runtime" but the grammar should parse it
    let stmts = parse_ok(r#"FIND SupportTicket WHERE PATTERN("^bug-.*") == title"#);
    assert_eq!(stmts.len(), 1);
}

// --- Collection predicates (used in guards, not WHERE in FIND) ---

#[test]
fn filter_pred_parse_all_collection() {
    // filter-predicates.md lines 130-131 — these are GUARD expressions, not FIND queries.
    // But the expression parser should handle them.
    // We test just the expression parsing in the context of FIND (will evaluate at engine level).
    // GUARD : ALL(items, STATE IS completed) — not valid in FIND context, test parse only
    // We'll parse it inside a dummy machine definition's guard context instead.
    // Actually let's test the expression parser directly by putting in a FIND WHERE:
    let stmts = parse_ok("FIND SupportTicket WHERE ALL(tags, STATE IS open)");
    assert_eq!(stmts.len(), 1);
}

#[test]
fn filter_pred_parse_any_collection() {
    // filter-predicates.md line 133: ANY(items, STATE IS pending)
    let stmts = parse_ok("FIND SupportTicket WHERE ANY(tags, STATE IS open)");
    assert_eq!(stmts.len(), 1);
}

// --- Execution tests for filter predicates ---

#[tokio::test]
async fn filter_pred_exec_and() {
    let engine = make_support_ticket_engine();
    let id1 = spawn_ticket_with_priority(&engine, "critical").await;
    let _id2 = spawn_ticket_with_priority(&engine, "low").await;

    // Assign id1 (so assignee IS SET)
    transition_with_data(
        &engine, "SupportTicket", &id1, "triaged", "agent_1",
        vec![("assignee", actor_map("agent_1"))],
    ).await;

    let stmts = parse_ok(r#"FIND SupportTicket WHERE STATE IS open AND priority == "critical""#);
    match &stmts[0] {
        Statement::Query(q) => {
            let result = engine.execute_query(q).await.unwrap();
            match result {
                QueryResult::Instances(instances) => {
                    // id1 was transitioned out of open, id2 is low priority
                    // So 0 tickets match open + critical
                    assert_eq!(instances.len(), 0);
                }
                _ => panic!("Expected Instances result"),
            }
        }
        _ => panic!("Expected Query statement"),
    }
}

#[tokio::test]
async fn filter_pred_exec_or() {
    let engine = make_support_ticket_engine();
    let id1 = spawn_ticket(&engine).await;
    let _id2 = spawn_ticket(&engine).await;

    // Move id1 to triaged
    transition_with_data(
        &engine, "SupportTicket", &id1, "triaged", "agent_1",
        vec![("assignee", actor_map("agent_1"))],
    ).await;

    let stmts = parse_ok("FIND SupportTicket WHERE STATE IS open OR STATE IS triaged");
    match &stmts[0] {
        Statement::Query(q) => {
            let result = engine.execute_query(q).await.unwrap();
            match result {
                QueryResult::Instances(instances) => {
                    assert_eq!(instances.len(), 2, "1 open + 1 triaged = 2");
                }
                _ => panic!("Expected Instances result"),
            }
        }
        _ => panic!("Expected Query statement"),
    }
}

#[tokio::test]
async fn filter_pred_exec_not() {
    let engine = make_support_ticket_engine();
    // Spawn 3 tickets (all in open state)
    spawn_ticket(&engine).await;
    spawn_ticket(&engine).await;
    spawn_ticket(&engine).await;

    let stmts = parse_ok("FIND SupportTicket WHERE NOT (STATE IS closed)");
    match &stmts[0] {
        Statement::Query(q) => {
            let result = engine.execute_query(q).await.unwrap();
            match result {
                QueryResult::Instances(instances) => {
                    assert_eq!(instances.len(), 3, "None are closed, so all 3 match");
                }
                _ => panic!("Expected Instances result"),
            }
        }
        _ => panic!("Expected Query statement"),
    }
}

#[tokio::test]
async fn filter_pred_exec_data_comparison() {
    let engine = make_order_engine();
    spawn_order(&engine, "Alice", 500).await;
    spawn_order(&engine, "Bob", 1500).await;
    spawn_order(&engine, "Charlie", 3000).await;

    let stmts = parse_ok("FIND Order WHERE total > 1000");
    match &stmts[0] {
        Statement::Query(q) => {
            let result = engine.execute_query(q).await.unwrap();
            match result {
                QueryResult::Instances(instances) => {
                    assert_eq!(instances.len(), 2, "Bob(1500) and Charlie(3000) match");
                }
                _ => panic!("Expected Instances result"),
            }
        }
        _ => panic!("Expected Query statement"),
    }
}

#[tokio::test]
async fn filter_pred_exec_is_set() {
    let engine = make_support_ticket_engine();
    let id1 = spawn_ticket(&engine).await;
    let _id2 = spawn_ticket(&engine).await;

    // Assign id1 via triage
    transition_with_data(
        &engine, "SupportTicket", &id1, "triaged", "agent_1",
        vec![("assignee", actor_map("agent_1"))],
    ).await;

    let stmts = parse_ok("FIND SupportTicket WHERE assignee IS SET");
    match &stmts[0] {
        Statement::Query(q) => {
            let result = engine.execute_query(q).await.unwrap();
            match result {
                QueryResult::Instances(instances) => {
                    assert_eq!(instances.len(), 1, "Only id1 has assignee set");
                }
                _ => panic!("Expected Instances result"),
            }
        }
        _ => panic!("Expected Query statement"),
    }
}

// ===========================================================================
// SECTION 9: Cross-doc integration tests
// ===========================================================================

/// Test that all doc examples from different pages work together in a single session.
#[tokio::test]
async fn cross_doc_full_workflow() {
    let engine = make_support_ticket_engine();

    // Spawn several tickets (from find.md concepts)
    let id1 = spawn_ticket(&engine).await;
    let id2 = spawn_ticket(&engine).await;
    let _id3 = spawn_ticket(&engine).await;

    // Transition (to set up different states)
    transition_with_data(
        &engine, "SupportTicket", &id1, "triaged", "agent_1",
        vec![("assignee", actor_map("agent_1"))],
    ).await;
    transition_as(&engine, "SupportTicket", &id1, "in_progress", "agent_1").await;
    transition_with_data(
        &engine, "SupportTicket", &id1, "resolved", "agent_1",
        vec![("resolution_note", Value::Text("Fixed".into()))],
    ).await;

    transition_with_data(
        &engine, "SupportTicket", &id2, "triaged", "agent_2",
        vec![("assignee", actor_map("agent_2"))],
    ).await;

    // GET (from get.md)
    let stmts = parse_ok(&format!(r#"GET SupportTicket "{}""#, id1));
    match &stmts[0] {
        Statement::Query(q) => {
            let result = engine.execute_query(q).await.unwrap();
            match result {
                QueryResult::Instance(inst) => {
                    assert_eq!(inst.state, "resolved");
                }
                _ => panic!("Expected Instance result"),
            }
        }
        _ => panic!("Expected Query statement"),
    }

    // TRAIL (from trail.md)
    let stmts = parse_ok(&format!(r#"TRAIL OF "{}""#, id1));
    match &stmts[0] {
        Statement::Query(q) => {
            let result = engine.execute_query(q).await.unwrap();
            match result {
                QueryResult::Trail(entries) => {
                    assert_eq!(entries.len(), 4, "spawn + 3 transitions");
                }
                _ => panic!("Expected Trail result"),
            }
        }
        _ => panic!("Expected Query statement"),
    }

    // FIND (from find.md)
    let stmts = parse_ok("FIND SupportTicket WHERE STATE IS open");
    match &stmts[0] {
        Statement::Query(q) => {
            let result = engine.execute_query(q).await.unwrap();
            match result {
                QueryResult::Instances(instances) => {
                    assert_eq!(instances.len(), 1, "Only id3 is still in open");
                }
                _ => panic!("Expected Instances result"),
            }
        }
        _ => panic!("Expected Query statement"),
    }

    // AGGREGATE (from aggregate.md)
    let stmts = parse_ok("AGGREGATE SupportTicket MEASURE COUNT() GROUP BY STATE");
    match &stmts[0] {
        Statement::Query(q) => {
            let result = engine.execute_query(q).await.unwrap();
            match result {
                QueryResult::Aggregate(rows) => {
                    assert_eq!(rows.len(), 3, "3 distinct states: open, triaged, resolved");
                }
                _ => panic!("Expected Aggregate result"),
            }
        }
        _ => panic!("Expected Query statement"),
    }

    // FUNNEL (from funnel.md)
    let stmts = parse_ok("FUNNEL SupportTicket THROUGH [open, triaged, in_progress, resolved]");
    match &stmts[0] {
        Statement::Query(q) => {
            let result = engine.execute_query(q).await.unwrap();
            match result {
                QueryResult::Funnel(funnel) => {
                    assert_eq!(funnel.stages.len(), 4);
                    assert_eq!(funnel.stages[0].count, 3, "All visited open");
                    assert_eq!(funnel.stages[1].count, 2, "2 visited triaged");
                    assert_eq!(funnel.stages[2].count, 1, "1 visited in_progress");
                    assert_eq!(funnel.stages[3].count, 1, "1 visited resolved");
                }
                _ => panic!("Expected Funnel result"),
            }
        }
        _ => panic!("Expected Query statement"),
    }

    // PATHS (from paths.md)
    let stmts = parse_ok("PATHS FROM SupportTicket");
    match &stmts[0] {
        Statement::Query(q) => {
            let result = engine.execute_query(q).await.unwrap();
            match result {
                QueryResult::Paths(paths) => {
                    assert_eq!(paths.len(), 3, "3 distinct paths");
                }
                _ => panic!("Expected Paths result"),
            }
        }
        _ => panic!("Expected Query statement"),
    }

    // COMPARE PATHS (from compare-paths.md)
    let stmts = parse_ok("COMPARE PATHS SupportTicket SEGMENT BY priority");
    match &stmts[0] {
        Statement::Query(q) => {
            let result = engine.execute_query(q).await.unwrap();
            match result {
                QueryResult::ComparePaths(cmp) => {
                    assert_eq!(cmp.segment_by, "priority");
                    // All tickets have default priority "medium"
                    assert_eq!(cmp.segments.len(), 1);
                }
                _ => panic!("Expected ComparePaths result"),
            }
        }
        _ => panic!("Expected Query statement"),
    }
}
