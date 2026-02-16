/// End-to-end query tests: simple → complex.
///
/// Exercises the full pipeline: SMQL string → parser → AST → engine execution → results.
/// Tests are ordered from simplest to most complex to verify all query types work as expected.
use std::collections::BTreeMap;
use std::sync::Arc;

use smql_ast::expression::{Expression, ExpressionKind};
use smql_ast::query::{
    AggregateQuery, FindQuery, FunnelQuery, GetQuery, MeasureClause, PathsQuery, Query,
    TrailQuery,
};
use smql_ast::types::AggregateFunction;
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

fn lit(v: Value) -> Expression {
    Expression::new(ExpressionKind::Literal(v))
}

fn actor_map(id: &str) -> Value {
    let mut m = BTreeMap::new();
    m.insert("id".to_string(), Value::Text(id.to_string()));
    Value::Map(m)
}

fn make_engine() -> Engine {
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

/// Spawn a ticket programmatically and return its ID (since SPAWN needs UUIDs which are verbose in SMQL strings).
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
    };
    let r = engine.spawn(&cmd).await.expect("spawn ticket");
    assert_eq!(r.instance.state, "open");
    r.instance.id.as_str()
}

/// Transition with data (for setting assignee, resolution_note, etc.)
async fn transition_with_data(
    engine: &Engine,
    machine: &str,
    id: &str,
    to: &str,
    actor: &str,
    data: Vec<(&str, Value)>,
) {
    let mut cmd = smql_ast::command::TransitionCommand::new(machine.to_string(), id.to_string(), to.to_string());
    cmd.as_actor = Some(actor.to_string());
    cmd.with_data = data
        .into_iter()
        .map(|(k, v)| (k.to_string(), lit(v)))
        .collect();
    engine.transition(&cmd).await.expect(&format!("transition {} to {}", id, to));
}

/// Transition as actor (no data).
async fn transition_as(engine: &Engine, machine: &str, id: &str, to: &str, actor: &str) {
    let mut cmd = smql_ast::command::TransitionCommand::new(machine.to_string(), id.to_string(), to.to_string());
    cmd.as_actor = Some(actor.to_string());
    engine.transition(&cmd).await.expect(&format!("transition {} to {}", id, to));
}

// ===========================================================================
// LEVEL 1: Simplest queries — single instance operations
// ===========================================================================

/// 1.1 GET a single instance by ID.
#[tokio::test]
async fn level1_get_instance() {
    let engine = make_engine();
    let id = spawn_ticket(&engine).await;

    let q = Query::Get(GetQuery {
        machine: "SupportTicket".into(),
        instance_id: id.clone(),
    });
    let result = engine.execute_query(&q).await.unwrap();
    match result {
        QueryResult::Instance(inst) => {
            assert_eq!(inst.id.as_str(), id);
            assert_eq!(inst.state, "open");
            assert_eq!(inst.machine, "SupportTicket");
            assert!(inst.data.contains_key("customer_id"));
            assert_eq!(inst.data.get("subject"), Some(&Value::Text("Test issue".into())));
            assert_eq!(
                inst.data.get("priority"),
                Some(&Value::Text("medium".into())),
                "DEFAULT(medium) should apply"
            );
        }
        _ => panic!("expected Instance"),
    }
}

/// 1.2 GET non-existent instance returns error.
#[tokio::test]
async fn level1_get_nonexistent() {
    let engine = make_engine();

    let q = Query::Get(GetQuery {
        machine: "SupportTicket".into(),
        instance_id: "nonexistent_id_12345".into(),
    });
    let result = engine.execute_query(&q).await;
    assert!(result.is_err(), "nonexistent instance should error");
}

/// 1.3 TRAIL of a freshly spawned instance (only spawn entry).
#[tokio::test]
async fn level1_trail_spawn_only() {
    let engine = make_engine();
    let id = spawn_ticket(&engine).await;

    let q = Query::Trail(TrailQuery {
        machine: Some("SupportTicket".into()),
        instance_id: id,
        filter: None,
    });
    let result = engine.execute_query(&q).await.unwrap();
    match result {
        QueryResult::Trail(entries) => {
            assert_eq!(entries.len(), 1, "spawn creates one trail entry");
            assert_eq!(entries[0].from_state, "", "spawn has empty from_state");
            assert_eq!(entries[0].to_state, "open");
            assert_eq!(entries[0].sequence, 0);
        }
        _ => panic!("expected Trail"),
    }
}

// ===========================================================================
// LEVEL 2: Simple FIND queries — filter by state
// ===========================================================================

/// 2.1 FIND all instances (no filter).
#[tokio::test]
async fn level2_find_all() {
    let engine = make_engine();
    spawn_ticket(&engine).await;
    spawn_ticket(&engine).await;
    spawn_ticket(&engine).await;

    let q = Query::Find(FindQuery {
        machine: "SupportTicket".into(),
        filter: None,
        sort: vec![],
        limit: None,
        offset: None,
        after: None,
    });
    let result = engine.execute_query(&q).await.unwrap();
    match result {
        QueryResult::Instances(instances) => {
            assert_eq!(instances.len(), 3);
        }
        _ => panic!("expected Instances"),
    }
}

/// 2.2 FIND by STATE IS.
#[tokio::test]
async fn level2_find_by_state() {
    let engine = make_engine();
    let id1 = spawn_ticket(&engine).await;
    let _id2 = spawn_ticket(&engine).await;
    let _id3 = spawn_ticket(&engine).await;

    // Move id1 to triaged
    transition_with_data(
        &engine, "SupportTicket", &id1, "triaged", "agent_1",
        vec![("assignee", actor_map("agent_1"))],
    ).await;

    // Find open tickets (should be 2)
    let q = Query::Find(FindQuery {
        machine: "SupportTicket".into(),
        filter: Some(Expression::new(ExpressionKind::StateIs("open".into()))),
        sort: vec![],
        limit: None,
        offset: None,
        after: None,
    });
    let result = engine.execute_query(&q).await.unwrap();
    match result {
        QueryResult::Instances(instances) => {
            assert_eq!(instances.len(), 2);
            for inst in &instances {
                assert_eq!(inst.state, "open");
            }
        }
        _ => panic!("expected Instances"),
    }

    // Find triaged tickets (should be 1)
    let q = Query::Find(FindQuery {
        machine: "SupportTicket".into(),
        filter: Some(Expression::new(ExpressionKind::StateIs("triaged".into()))),
        sort: vec![],
        limit: None,
        offset: None,
        after: None,
    });
    let result = engine.execute_query(&q).await.unwrap();
    match result {
        QueryResult::Instances(instances) => {
            assert_eq!(instances.len(), 1);
            assert_eq!(instances[0].state, "triaged");
        }
        _ => panic!("expected Instances"),
    }
}

/// 2.3 FIND with LIMIT.
#[tokio::test]
async fn level2_find_with_limit() {
    let engine = make_engine();
    for _ in 0..5 {
        spawn_ticket(&engine).await;
    }

    let q = Query::Find(FindQuery {
        machine: "SupportTicket".into(),
        filter: None,
        sort: vec![],
        limit: Some(3),
        offset: None,
        after: None,
    });
    let result = engine.execute_query(&q).await.unwrap();
    match result {
        QueryResult::Instances(instances) => {
            assert_eq!(instances.len(), 3, "LIMIT 3 should return 3");
        }
        _ => panic!("expected Instances"),
    }
}

/// 2.4 FIND with LIMIT + OFFSET.
#[tokio::test]
async fn level2_find_with_limit_offset() {
    let engine = make_engine();
    for _ in 0..5 {
        spawn_ticket(&engine).await;
    }

    let q = Query::Find(FindQuery {
        machine: "SupportTicket".into(),
        filter: None,
        sort: vec![],
        limit: Some(2),
        offset: Some(3),
        after: None,
    });
    let result = engine.execute_query(&q).await.unwrap();
    match result {
        QueryResult::Instances(instances) => {
            assert_eq!(instances.len(), 2, "OFFSET 3 LIMIT 2 from 5 should return 2");
        }
        _ => panic!("expected Instances"),
    }
}

// ===========================================================================
// LEVEL 3: TRAIL with transitions
// ===========================================================================

/// 3.1 TRAIL shows all transitions.
#[tokio::test]
async fn level3_trail_with_transitions() {
    let engine = make_engine();
    let id = spawn_ticket(&engine).await;

    // open → triaged → in_progress
    transition_with_data(
        &engine, "SupportTicket", &id, "triaged", "agent_1",
        vec![("assignee", actor_map("agent_1"))],
    ).await;
    transition_as(&engine, "SupportTicket", &id, "in_progress", "agent_1").await;

    let q = Query::Trail(TrailQuery {
        machine: Some("SupportTicket".into()),
        instance_id: id,
        filter: None,
    });
    let result = engine.execute_query(&q).await.unwrap();
    match result {
        QueryResult::Trail(entries) => {
            assert_eq!(entries.len(), 3, "spawn + 2 transitions = 3 entries");
            // Verify order
            assert_eq!(entries[0].from_state, "");
            assert_eq!(entries[0].to_state, "open");
            assert_eq!(entries[1].from_state, "open");
            assert_eq!(entries[1].to_state, "triaged");
            assert_eq!(entries[2].from_state, "triaged");
            assert_eq!(entries[2].to_state, "in_progress");
            // Verify sequential sequence numbers
            assert_eq!(entries[0].sequence, 0);
            assert_eq!(entries[1].sequence, 1);
            assert_eq!(entries[2].sequence, 2);
        }
        _ => panic!("expected Trail"),
    }
}

/// 3.2 TRAIL records actors.
#[tokio::test]
async fn level3_trail_records_actor() {
    let engine = make_engine();
    let id = spawn_ticket(&engine).await;

    transition_with_data(
        &engine, "SupportTicket", &id, "triaged", "agent_1",
        vec![("assignee", actor_map("agent_1"))],
    ).await;

    let q = Query::Trail(TrailQuery {
        machine: Some("SupportTicket".into()),
        instance_id: id,
        filter: None,
    });
    let result = engine.execute_query(&q).await.unwrap();
    match result {
        QueryResult::Trail(entries) => {
            assert_eq!(entries[1].actor.as_deref(), Some("agent_1"));
        }
        _ => panic!("expected Trail"),
    }
}

/// 3.3 TRAIL records memo.
#[tokio::test]
async fn level3_trail_records_memo() {
    let engine = make_engine();
    let id = spawn_ticket(&engine).await;

    let mut cmd = smql_ast::command::TransitionCommand::new("SupportTicket".to_string(), id.clone(), "triaged".to_string());
    cmd.as_actor = Some("agent_1".to_string());
    cmd.memo = Some("Urgent escalation".to_string());
    cmd.with_data = vec![("assignee".to_string(), lit(actor_map("agent_1")))].into_iter().collect();
    engine.transition(&cmd).await.unwrap();

    let q = Query::Trail(TrailQuery {
        machine: Some("SupportTicket".into()),
        instance_id: id,
        filter: None,
    });
    let result = engine.execute_query(&q).await.unwrap();
    match result {
        QueryResult::Trail(entries) => {
            assert_eq!(entries[1].memo.as_deref(), Some("Urgent escalation"));
        }
        _ => panic!("expected Trail"),
    }
}

// ===========================================================================
// LEVEL 4: AGGREGATE queries — counts and grouping
// ===========================================================================

/// 4.1 Simple COUNT() across all instances.
#[tokio::test]
async fn level4_aggregate_count_all() {
    let engine = make_engine();
    spawn_ticket(&engine).await;
    spawn_ticket(&engine).await;
    spawn_ticket(&engine).await;

    let q = Query::Aggregate(AggregateQuery {
        machine: "SupportTicket".into(),
        measures: vec![MeasureClause {
            function: AggregateFunction::Count,
            field: None,
            alias: None,
        }],
        filter: None,
        group_by: vec![],
    });
    let result = engine.execute_query(&q).await.unwrap();
    match result {
        QueryResult::Aggregate(rows) => {
            assert_eq!(rows.len(), 1, "no GROUP BY = single row");
            assert_eq!(
                rows[0].measures.get("COUNT"),
                Some(&Value::Int(3)),
            );
        }
        _ => panic!("expected Aggregate"),
    }
}

/// 4.2 COUNT() GROUP BY state.
#[tokio::test]
async fn level4_aggregate_count_by_state() {
    let engine = make_engine();
    let id1 = spawn_ticket(&engine).await;
    let id2 = spawn_ticket(&engine).await;
    let _id3 = spawn_ticket(&engine).await; // stays open

    // Move id1 and id2 to triaged
    transition_with_data(
        &engine, "SupportTicket", &id1, "triaged", "agent_1",
        vec![("assignee", actor_map("agent_1"))],
    ).await;
    transition_with_data(
        &engine, "SupportTicket", &id2, "triaged", "agent_2",
        vec![("assignee", actor_map("agent_2"))],
    ).await;

    // Move id1 further to in_progress
    transition_as(&engine, "SupportTicket", &id1, "in_progress", "agent_1").await;

    let q = Query::Aggregate(AggregateQuery {
        machine: "SupportTicket".into(),
        measures: vec![MeasureClause {
            function: AggregateFunction::Count,
            field: None,
            alias: None,
        }],
        filter: None,
        group_by: vec![smql_ast::query::GroupByClause::State],
    });
    let result = engine.execute_query(&q).await.unwrap();
    match result {
        QueryResult::Aggregate(rows) => {
            assert_eq!(rows.len(), 3, "3 distinct states: open(1), triaged(1), in_progress(1)");

            // Build a map of state → count for easier assertions
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

            assert_eq!(state_counts.get("open"), Some(&1));
            assert_eq!(state_counts.get("triaged"), Some(&1));
            assert_eq!(state_counts.get("in_progress"), Some(&1));
        }
        _ => panic!("expected Aggregate"),
    }
}

/// 4.3 GROUP BY data field (priority).
#[tokio::test]
async fn level4_aggregate_group_by_field() {
    let engine = make_engine();

    // All 3 tickets get default priority = "medium"
    spawn_ticket(&engine).await;
    spawn_ticket(&engine).await;
    spawn_ticket(&engine).await;

    let q = Query::Aggregate(AggregateQuery {
        machine: "SupportTicket".into(),
        measures: vec![MeasureClause {
            function: AggregateFunction::Count,
            field: None,
            alias: None,
        }],
        filter: None,
        group_by: vec![smql_ast::query::GroupByClause::Field("priority".to_string())],
    });
    let result = engine.execute_query(&q).await.unwrap();
    match result {
        QueryResult::Aggregate(rows) => {
            assert_eq!(rows.len(), 1, "all tickets have same priority");
            assert_eq!(
                rows[0].group_key.get("priority"),
                Some(&Value::Text("medium".into()))
            );
            assert_eq!(rows[0].measures.get("COUNT"), Some(&Value::Int(3)));
        }
        _ => panic!("expected Aggregate"),
    }
}

// ===========================================================================
// LEVEL 5: PATHS query — state sequence analysis
// ===========================================================================

/// 5.1 PATHS with a single path.
#[tokio::test]
async fn level5_paths_single() {
    let engine = make_engine();
    let id = spawn_ticket(&engine).await;

    transition_with_data(
        &engine, "SupportTicket", &id, "triaged", "agent_1",
        vec![("assignee", actor_map("agent_1"))],
    ).await;
    transition_as(&engine, "SupportTicket", &id, "in_progress", "agent_1").await;

    let q = Query::Paths(PathsQuery {
        machine: "SupportTicket".into(),
        filter: None,
        limit: None,
    });
    let result = engine.execute_query(&q).await.unwrap();
    match result {
        QueryResult::Paths(paths) => {
            assert_eq!(paths.len(), 1);
            assert_eq!(paths[0].path, vec!["", "open", "triaged", "in_progress"]);
            assert_eq!(paths[0].count, 1);
        }
        _ => panic!("expected Paths"),
    }
}

/// 5.2 PATHS with multiple distinct paths.
#[tokio::test]
async fn level5_paths_multiple() {
    let engine = make_engine();
    let id1 = spawn_ticket(&engine).await;
    let id2 = spawn_ticket(&engine).await;
    let _id3 = spawn_ticket(&engine).await;

    // Ticket 1: open → triaged → in_progress
    transition_with_data(
        &engine, "SupportTicket", &id1, "triaged", "agent_1",
        vec![("assignee", actor_map("agent_1"))],
    ).await;
    transition_as(&engine, "SupportTicket", &id1, "in_progress", "agent_1").await;

    // Ticket 2: open → triaged → in_progress → resolved
    transition_with_data(
        &engine, "SupportTicket", &id2, "triaged", "agent_2",
        vec![("assignee", actor_map("agent_2"))],
    ).await;
    transition_as(&engine, "SupportTicket", &id2, "in_progress", "agent_2").await;
    transition_with_data(
        &engine, "SupportTicket", &id2, "resolved", "agent_2",
        vec![("resolution_note", Value::Text("Fixed".into()))],
    ).await;

    // Ticket 3: stays open (different path from above)

    let q = Query::Paths(PathsQuery {
        machine: "SupportTicket".into(),
        filter: None,
        limit: None,
    });
    let result = engine.execute_query(&q).await.unwrap();
    match result {
        QueryResult::Paths(paths) => {
            assert_eq!(paths.len(), 3, "3 distinct paths");
        }
        _ => panic!("expected Paths"),
    }
}

/// 5.3 PATHS with LIMIT.
#[tokio::test]
async fn level5_paths_with_limit() {
    let engine = make_engine();
    // Create 3 tickets that all stay open (same path)
    spawn_ticket(&engine).await;
    spawn_ticket(&engine).await;
    spawn_ticket(&engine).await;

    // Create 1 ticket with a different path
    let id = spawn_ticket(&engine).await;
    transition_with_data(
        &engine, "SupportTicket", &id, "triaged", "agent_1",
        vec![("assignee", actor_map("agent_1"))],
    ).await;

    let q = Query::Paths(PathsQuery {
        machine: "SupportTicket".into(),
        filter: None,
        limit: Some(1),
    });
    let result = engine.execute_query(&q).await.unwrap();
    match result {
        QueryResult::Paths(paths) => {
            assert_eq!(paths.len(), 1, "LIMIT 1 returns 1 path");
            // Most common path (3 instances stayed open) should come first
            assert_eq!(paths[0].count, 3);
        }
        _ => panic!("expected Paths"),
    }
}

// ===========================================================================
// LEVEL 6: FUNNEL query — conversion analysis
// ===========================================================================

/// 6.1 Simple funnel through the ticket lifecycle.
#[tokio::test]
async fn level6_funnel_basic() {
    let engine = make_engine();
    let id1 = spawn_ticket(&engine).await;
    let id2 = spawn_ticket(&engine).await;
    let _id3 = spawn_ticket(&engine).await;

    // All 3 go through open (via spawn)
    // 2 reach triaged
    transition_with_data(
        &engine, "SupportTicket", &id1, "triaged", "agent_1",
        vec![("assignee", actor_map("agent_1"))],
    ).await;
    transition_with_data(
        &engine, "SupportTicket", &id2, "triaged", "agent_2",
        vec![("assignee", actor_map("agent_2"))],
    ).await;

    // 1 reaches in_progress
    transition_as(&engine, "SupportTicket", &id1, "in_progress", "agent_1").await;

    // 1 reaches resolved
    transition_with_data(
        &engine, "SupportTicket", &id1, "resolved", "agent_1",
        vec![("resolution_note", Value::Text("Fixed it".into()))],
    ).await;

    let q = Query::Funnel(FunnelQuery {
        machine: "SupportTicket".into(),
        states: vec![
            "open".into(),
            "triaged".into(),
            "in_progress".into(),
            "resolved".into(),
        ],
        filter: None,
    });
    let result = engine.execute_query(&q).await.unwrap();
    match result {
        QueryResult::Funnel(funnel) => {
            assert_eq!(funnel.stages.len(), 4);

            // open: all 3 visited (100%)
            assert_eq!(funnel.stages[0].state, "open");
            assert_eq!(funnel.stages[0].count, 3);
            assert!((funnel.stages[0].conversion_rate - 1.0).abs() < 0.01);

            // triaged: 2 visited (66.7%)
            assert_eq!(funnel.stages[1].state, "triaged");
            assert_eq!(funnel.stages[1].count, 2);
            assert!((funnel.stages[1].conversion_rate - 2.0 / 3.0).abs() < 0.01);

            // in_progress: 1 visited (33.3%)
            assert_eq!(funnel.stages[2].state, "in_progress");
            assert_eq!(funnel.stages[2].count, 1);
            assert!((funnel.stages[2].conversion_rate - 1.0 / 3.0).abs() < 0.01);

            // resolved: 1 visited (33.3%)
            assert_eq!(funnel.stages[3].state, "resolved");
            assert_eq!(funnel.stages[3].count, 1);
        }
        _ => panic!("expected Funnel"),
    }
}

// ===========================================================================
// LEVEL 7: Multi-machine composition queries
// ===========================================================================

const ORDER_SMQL: &str = r#"
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
        draft -> placed { GUARD : total > 0 }
        placed -> paid {}
        paid -> fulfilled { GUARD : ALL(items, STATE IS confirmed) }
        fulfilled -> shipped {}
        shipped -> delivered {}
        delivered -> returned {}
        ANY -> cancelled { EXCEPT FROM { shipped, delivered, returned } }
    }
)

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
        pending -> confirmed { GUARD : quantity > 0 }
        pending -> backordered {}
        backordered -> confirmed {}
        ANY -> cancelled { EXCEPT FROM { confirmed } }
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

fn spawn_order_cmd(customer: &str, total: i64) -> smql_ast::command::SpawnCommand {
    smql_ast::command::SpawnCommand {
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
    }
}

fn spawn_item_cmd(order_id: &str, product: &str, qty: i64, price: i64) -> smql_ast::command::SpawnCommand {
    smql_ast::command::SpawnCommand {
        machine: "LineItem".to_string(),
        data: vec![
            ("product", Value::Text(product.into())),
            ("quantity", Value::Int(qty)),
            ("price", Value::Int(price)),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), lit(v)))
        .collect(),
        then_transition: None,
        batch: false,
        batch_data: Vec::new(),
        parent_id: Some(order_id.to_string()),
        parent_machine: Some("Order".to_string()),
    }
}

fn trans(machine: &str, id: &str, to: &str) -> smql_ast::command::TransitionCommand {
    smql_ast::command::TransitionCommand::new(machine.to_string(), id.to_string(), to.to_string())
}

/// 7.1 Query parent and child instances across machines.
#[tokio::test]
async fn level7_cross_machine_get() {
    let engine = make_order_engine();

    let order = engine.spawn(&spawn_order_cmd("Alice", 5000)).await.unwrap();
    let order_id = order.instance.id.as_str();

    let item = engine.spawn(&spawn_item_cmd(&order_id, "Widget", 2, 2500)).await.unwrap();
    let item_id = item.instance.id.as_str();

    // Query order
    let q = Query::Get(GetQuery { machine: "Order".into(), instance_id: order_id.clone() });
    match engine.execute_query(&q).await.unwrap() {
        QueryResult::Instance(inst) => {
            assert_eq!(inst.state, "draft");
            assert_eq!(inst.data.get("customer"), Some(&Value::Text("Alice".into())));
        }
        _ => panic!("expected Instance"),
    }

    // Query child item
    let q = Query::Get(GetQuery { machine: "LineItem".into(), instance_id: item_id.clone() });
    match engine.execute_query(&q).await.unwrap() {
        QueryResult::Instance(inst) => {
            assert_eq!(inst.state, "pending");
            assert_eq!(inst.parent_id.as_ref().map(|id| id.as_str()), Some(order_id.clone()));
            assert_eq!(inst.parent_machine.as_deref(), Some("Order"));
        }
        _ => panic!("expected Instance"),
    }
}

/// 7.2 FIND across child machine.
#[tokio::test]
async fn level7_find_children() {
    let engine = make_order_engine();

    let o1 = engine.spawn(&spawn_order_cmd("Alice", 5000)).await.unwrap().instance.id.as_str();
    let o2 = engine.spawn(&spawn_order_cmd("Bob", 8000)).await.unwrap().instance.id.as_str();

    // Alice's items
    engine.spawn(&spawn_item_cmd(&o1, "Widget A", 1, 2500)).await.unwrap();
    engine.spawn(&spawn_item_cmd(&o1, "Widget B", 2, 2500)).await.unwrap();

    // Bob's items
    let item_c = engine.spawn(&spawn_item_cmd(&o2, "Widget C", 3, 8000)).await.unwrap().instance.id.as_str();
    engine.transition(&trans("LineItem", &item_c, "confirmed")).await.unwrap();

    // Find all pending line items
    let q = Query::Find(FindQuery {
        machine: "LineItem".into(),
        filter: Some(Expression::new(ExpressionKind::StateIs("pending".into()))),
        sort: vec![],
        limit: None,
        offset: None,
        after: None,
    });
    match engine.execute_query(&q).await.unwrap() {
        QueryResult::Instances(instances) => {
            assert_eq!(instances.len(), 2, "2 of 3 items still pending");
        }
        _ => panic!("expected Instances"),
    }

    // Find confirmed items
    let q = Query::Find(FindQuery {
        machine: "LineItem".into(),
        filter: Some(Expression::new(ExpressionKind::StateIs("confirmed".into()))),
        sort: vec![],
        limit: None,
        offset: None,
        after: None,
    });
    match engine.execute_query(&q).await.unwrap() {
        QueryResult::Instances(instances) => {
            assert_eq!(instances.len(), 1, "1 item confirmed");
        }
        _ => panic!("expected Instances"),
    }
}

/// 7.3 AGGREGATE across orders and line items.
#[tokio::test]
async fn level7_aggregate_across_machines() {
    let engine = make_order_engine();

    let o1 = engine.spawn(&spawn_order_cmd("Alice", 5000)).await.unwrap().instance.id.as_str();
    let o2 = engine.spawn(&spawn_order_cmd("Bob", 8000)).await.unwrap().instance.id.as_str();

    engine.spawn(&spawn_item_cmd(&o1, "A", 1, 2500)).await.unwrap();
    engine.spawn(&spawn_item_cmd(&o1, "B", 2, 2500)).await.unwrap();
    engine.spawn(&spawn_item_cmd(&o2, "C", 3, 8000)).await.unwrap();

    // Place order 1
    engine.transition(&trans("Order", &o1, "placed")).await.unwrap();

    // Aggregate orders by state
    let q = Query::Aggregate(AggregateQuery {
        machine: "Order".into(),
        measures: vec![MeasureClause {
            function: AggregateFunction::Count,
            field: None,
            alias: None,
        }],
        filter: None,
        group_by: vec![smql_ast::query::GroupByClause::State],
    });
    match engine.execute_query(&q).await.unwrap() {
        QueryResult::Aggregate(rows) => {
            assert_eq!(rows.len(), 2, "draft(1) and placed(1)");
        }
        _ => panic!("expected Aggregate"),
    }

    // SUM(total) across all orders
    let q = Query::Aggregate(AggregateQuery {
        machine: "Order".into(),
        measures: vec![MeasureClause {
            function: AggregateFunction::Sum,
            field: Some("total".into()),
            alias: Some("total_revenue".into()),
        }],
        filter: None,
        group_by: vec![],
    });
    match engine.execute_query(&q).await.unwrap() {
        QueryResult::Aggregate(rows) => {
            assert_eq!(rows.len(), 1);
            assert_eq!(
                rows[0].measures.get("total_revenue"),
                Some(&Value::Int(13000)),
                "5000 + 8000 = 13000"
            );
        }
        _ => panic!("expected Aggregate"),
    }

    // AVG(price) across line items
    let q = Query::Aggregate(AggregateQuery {
        machine: "LineItem".into(),
        measures: vec![MeasureClause {
            function: AggregateFunction::Avg,
            field: Some("price".into()),
            alias: Some("avg_price".into()),
        }],
        filter: None,
        group_by: vec![],
    });
    match engine.execute_query(&q).await.unwrap() {
        QueryResult::Aggregate(rows) => {
            assert_eq!(rows.len(), 1);
            match rows[0].measures.get("avg_price") {
                Some(Value::Float(avg)) => {
                    // (2500 + 2500 + 8000) / 3 ≈ 4333.33
                    assert!((avg - 4333.33).abs() < 1.0, "avg should be ~4333.33, got {}", avg);
                }
                other => panic!("expected Float, got {:?}", other),
            }
        }
        _ => panic!("expected Aggregate"),
    }
}

// ===========================================================================
// LEVEL 8: Complex workflows with guards, then query results
// ===========================================================================

/// 8.1 Full order lifecycle → query at each stage.
#[tokio::test]
async fn level8_full_order_lifecycle_queries() {
    let engine = make_order_engine();

    // Create order with 2 items
    let order = engine.spawn(&spawn_order_cmd("Charlie", 15000)).await.unwrap();
    let oid = order.instance.id.as_str();
    let i1 = engine.spawn(&spawn_item_cmd(&oid, "Laptop", 1, 10000)).await.unwrap().instance.id.as_str();
    let i2 = engine.spawn(&spawn_item_cmd(&oid, "Mouse", 2, 2500)).await.unwrap().instance.id.as_str();

    // Phase 1: draft → placed
    engine.transition(&trans("Order", &oid, "placed")).await.unwrap();

    // Query: order should be "placed"
    let q = Query::Get(GetQuery { machine: "Order".into(), instance_id: oid.clone() });
    match engine.execute_query(&q).await.unwrap() {
        QueryResult::Instance(inst) => assert_eq!(inst.state, "placed"),
        _ => panic!("expected Instance"),
    }

    // Phase 2: placed → paid
    engine.transition(&trans("Order", &oid, "paid")).await.unwrap();

    // Phase 3: Try to fulfil without confirming items — should fail
    let result = engine.transition(&trans("Order", &oid, "fulfilled")).await;
    assert!(result.is_err(), "ALL(items, STATE IS confirmed) guard should block");

    // Confirm items
    engine.transition(&trans("LineItem", &i1, "confirmed")).await.unwrap();
    engine.transition(&trans("LineItem", &i2, "confirmed")).await.unwrap();

    // Phase 4: Now fulfil should work
    engine.transition(&trans("Order", &oid, "fulfilled")).await.unwrap();

    // Phase 5: ship → deliver
    engine.transition(&trans("Order", &oid, "shipped")).await.unwrap();
    engine.transition(&trans("Order", &oid, "delivered")).await.unwrap();

    // Verify final state
    let q = Query::Get(GetQuery { machine: "Order".into(), instance_id: oid.clone() });
    match engine.execute_query(&q).await.unwrap() {
        QueryResult::Instance(inst) => assert_eq!(inst.state, "delivered"),
        _ => panic!("expected Instance"),
    }

    // Trail should have 6+ entries: spawn + draft→placed + placed→paid + paid→fulfilled + fulfilled→shipped + shipped→delivered
    let q = Query::Trail(TrailQuery {
        machine: Some("Order".into()),
        instance_id: oid.clone(),
        filter: None,
    });
    match engine.execute_query(&q).await.unwrap() {
        QueryResult::Trail(entries) => {
            assert!(entries.len() >= 6, "full lifecycle trail, got {}", entries.len());
            // Check first and last
            assert_eq!(entries[0].to_state, "draft");
            assert_eq!(entries.last().unwrap().to_state, "delivered");
        }
        _ => panic!("expected Trail"),
    }

    // PATHS should show the full delivery path
    let q = Query::Paths(PathsQuery {
        machine: "Order".into(),
        filter: None,
        limit: None,
    });
    match engine.execute_query(&q).await.unwrap() {
        QueryResult::Paths(paths) => {
            assert_eq!(paths.len(), 1, "single order = single path");
            let p = &paths[0].path;
            assert!(p.contains(&"delivered".to_string()), "path should end at delivered");
        }
        _ => panic!("expected Paths"),
    }
}

/// 8.2 CASCADE cancel and verify children states via queries.
#[tokio::test]
async fn level8_cascade_cancel_query() {
    let engine = make_order_engine();

    let order = engine.spawn(&spawn_order_cmd("Dave", 7500)).await.unwrap();
    let oid = order.instance.id.as_str();
    let i1 = engine.spawn(&spawn_item_cmd(&oid, "Keyboard", 1, 5000)).await.unwrap().instance.id.as_str();
    let i2 = engine.spawn(&spawn_item_cmd(&oid, "Cable", 3, 833)).await.unwrap().instance.id.as_str();

    // Cancel with CASCADE
    let mut cmd = smql_ast::command::TransitionCommand::new("Order".to_string(), oid.clone(), "cancelled".to_string());
    cmd.cascade = true;
    engine.transition(&cmd).await.unwrap();

    // Verify order is cancelled
    let q = Query::Get(GetQuery { machine: "Order".into(), instance_id: oid.clone() });
    match engine.execute_query(&q).await.unwrap() {
        QueryResult::Instance(inst) => assert_eq!(inst.state, "cancelled"),
        _ => panic!("expected Instance"),
    }

    // Verify children are in terminal states
    for item_id in [&i1, &i2] {
        let q = Query::Get(GetQuery { machine: "LineItem".into(), instance_id: item_id.to_string() });
        match engine.execute_query(&q).await.unwrap() {
            QueryResult::Instance(inst) => {
                assert!(
                    inst.state == "cancelled" || inst.state == "confirmed",
                    "child should be terminal, got: {}", inst.state
                );
            }
            _ => panic!("expected Instance"),
        }
    }

    // AGGREGATE should show all cancelled
    let q = Query::Aggregate(AggregateQuery {
        machine: "Order".into(),
        measures: vec![MeasureClause {
            function: AggregateFunction::Count,
            field: None,
            alias: None,
        }],
        filter: None,
        group_by: vec![smql_ast::query::GroupByClause::State],
    });
    match engine.execute_query(&q).await.unwrap() {
        QueryResult::Aggregate(rows) => {
            assert_eq!(rows.len(), 1);
            assert_eq!(
                rows[0].group_key.get("state"),
                Some(&Value::Text("cancelled".into()))
            );
        }
        _ => panic!("expected Aggregate"),
    }
}

// ===========================================================================
// LEVEL 9: Complex aggregate — SUM, AVG, MIN, MAX, PERCENTILE
// ===========================================================================

/// 9.1 Multiple aggregate functions in one query.
#[tokio::test]
async fn level9_multi_aggregate() {
    let engine = make_order_engine();

    // Create several orders with varying totals
    let totals = [1000, 5000, 10000, 20000, 50000];
    for total in &totals {
        engine.spawn(&spawn_order_cmd("Customer", *total)).await.unwrap();
    }

    // SUM, AVG, MIN, MAX in one query
    let q = Query::Aggregate(AggregateQuery {
        machine: "Order".into(),
        measures: vec![
            MeasureClause {
                function: AggregateFunction::Count,
                field: None,
                alias: Some("count".into()),
            },
            MeasureClause {
                function: AggregateFunction::Sum,
                field: Some("total".into()),
                alias: Some("sum_total".into()),
            },
            MeasureClause {
                function: AggregateFunction::Avg,
                field: Some("total".into()),
                alias: Some("avg_total".into()),
            },
            MeasureClause {
                function: AggregateFunction::Min,
                field: Some("total".into()),
                alias: Some("min_total".into()),
            },
            MeasureClause {
                function: AggregateFunction::Max,
                field: Some("total".into()),
                alias: Some("max_total".into()),
            },
        ],
        filter: None,
        group_by: vec![],
    });
    let result = engine.execute_query(&q).await.unwrap();
    match result {
        QueryResult::Aggregate(rows) => {
            assert_eq!(rows.len(), 1);
            let m = &rows[0].measures;

            assert_eq!(m.get("count"), Some(&Value::Int(5)));
            assert_eq!(m.get("sum_total"), Some(&Value::Int(86000)));
            assert_eq!(m.get("min_total"), Some(&Value::Int(1000)));
            assert_eq!(m.get("max_total"), Some(&Value::Int(50000)));

            match m.get("avg_total") {
                Some(Value::Float(avg)) => {
                    assert!((avg - 17200.0).abs() < 1.0, "avg should be 17200, got {}", avg);
                }
                other => panic!("expected Float for avg, got {:?}", other),
            }
        }
        _ => panic!("expected Aggregate"),
    }
}

/// 9.2 PERCENTILE aggregate.
#[tokio::test]
async fn level9_percentile() {
    let engine = make_order_engine();

    for total in [1000, 2000, 3000, 4000, 5000, 6000, 7000, 8000, 9000, 10000] {
        engine.spawn(&spawn_order_cmd("Cust", total)).await.unwrap();
    }

    let q = Query::Aggregate(AggregateQuery {
        machine: "Order".into(),
        measures: vec![MeasureClause {
            function: AggregateFunction::Percentile(50.0),
            field: Some("total".into()),
            alias: Some("p50".into()),
        }],
        filter: None,
        group_by: vec![],
    });
    let result = engine.execute_query(&q).await.unwrap();
    match result {
        QueryResult::Aggregate(rows) => {
            assert_eq!(rows.len(), 1);
            match rows[0].measures.get("p50") {
                Some(Value::Float(p50)) => {
                    // Median of 1000..10000 should be around 5000-6000
                    assert!(*p50 >= 4000.0 && *p50 <= 7000.0, "p50 should be mid-range, got {}", p50);
                }
                other => panic!("expected Float, got {:?}", other),
            }
        }
        _ => panic!("expected Aggregate"),
    }
}

// ===========================================================================
// LEVEL 10: Complex end-to-end scenario — multi-machine funnel & paths
// ===========================================================================

/// 10.1 Multi-order funnel analysis.
#[tokio::test]
async fn level10_order_funnel() {
    let engine = make_order_engine();

    // Create 5 orders, progressively fewer reach each stage
    let mut order_ids = Vec::new();
    for i in 0..5 {
        let o = engine
            .spawn(&spawn_order_cmd(&format!("Customer_{}", i), 5000 + i * 1000))
            .await
            .unwrap();
        let oid = o.instance.id.as_str();
        order_ids.push(oid.clone());

        // Every order gets at least one item
        let item = engine
            .spawn(&spawn_item_cmd(&oid, "Item", 1, 5000))
            .await
            .unwrap();
        let iid = item.instance.id.as_str();

        // Confirm item for future use
        engine.transition(&trans("LineItem", &iid, "confirmed")).await.unwrap();
    }

    // All 5: draft → placed
    for oid in &order_ids {
        engine.transition(&trans("Order", oid, "placed")).await.unwrap();
    }

    // 4 out of 5: placed → paid
    for oid in &order_ids[..4] {
        engine.transition(&trans("Order", oid, "paid")).await.unwrap();
    }

    // 3 out of 5: paid → fulfilled (items already confirmed)
    for oid in &order_ids[..3] {
        engine.transition(&trans("Order", oid, "fulfilled")).await.unwrap();
    }

    // 2 out of 5: fulfilled → shipped
    for oid in &order_ids[..2] {
        engine.transition(&trans("Order", oid, "shipped")).await.unwrap();
    }

    // 1 out of 5: shipped → delivered
    engine.transition(&trans("Order", &order_ids[0], "delivered")).await.unwrap();

    // Funnel analysis
    let q = Query::Funnel(FunnelQuery {
        machine: "Order".into(),
        states: vec![
            "draft".into(),
            "placed".into(),
            "paid".into(),
            "fulfilled".into(),
            "shipped".into(),
            "delivered".into(),
        ],
        filter: None,
    });
    let result = engine.execute_query(&q).await.unwrap();
    match result {
        QueryResult::Funnel(funnel) => {
            assert_eq!(funnel.stages.len(), 6);

            // All visited draft
            assert_eq!(funnel.stages[0].state, "draft");
            assert_eq!(funnel.stages[0].count, 5);

            // All visited placed
            assert_eq!(funnel.stages[1].state, "placed");
            assert_eq!(funnel.stages[1].count, 5);

            // 4 visited paid
            assert_eq!(funnel.stages[2].state, "paid");
            assert_eq!(funnel.stages[2].count, 4);

            // 3 visited fulfilled
            assert_eq!(funnel.stages[3].state, "fulfilled");
            assert_eq!(funnel.stages[3].count, 3);

            // 2 visited shipped
            assert_eq!(funnel.stages[4].state, "shipped");
            assert_eq!(funnel.stages[4].count, 2);

            // 1 visited delivered
            assert_eq!(funnel.stages[5].state, "delivered");
            assert_eq!(funnel.stages[5].count, 1);
        }
        _ => panic!("expected Funnel"),
    }
}

/// 10.2 Comprehensive PATHS analysis across diverging order flows.
#[tokio::test]
async fn level10_order_paths_diverging() {
    let engine = make_order_engine();

    // Order A: draft → placed → cancelled
    let oa = engine.spawn(&spawn_order_cmd("A", 5000)).await.unwrap().instance.id.as_str();
    engine.spawn(&spawn_item_cmd(&oa, "X", 1, 5000)).await.unwrap();
    engine.transition(&trans("Order", &oa, "placed")).await.unwrap();
    engine.transition(&trans("Order", &oa, "cancelled")).await.unwrap();

    // Order B: same path as A (draft → placed → cancelled)
    let ob = engine.spawn(&spawn_order_cmd("B", 3000)).await.unwrap().instance.id.as_str();
    engine.spawn(&spawn_item_cmd(&ob, "Y", 1, 3000)).await.unwrap();
    engine.transition(&trans("Order", &ob, "placed")).await.unwrap();
    engine.transition(&trans("Order", &ob, "cancelled")).await.unwrap();

    // Order C: draft → placed → paid → fulfilled → shipped → delivered
    let oc = engine.spawn(&spawn_order_cmd("C", 10000)).await.unwrap().instance.id.as_str();
    let ic = engine.spawn(&spawn_item_cmd(&oc, "Z", 1, 10000)).await.unwrap().instance.id.as_str();
    engine.transition(&trans("LineItem", &ic, "confirmed")).await.unwrap();
    engine.transition(&trans("Order", &oc, "placed")).await.unwrap();
    engine.transition(&trans("Order", &oc, "paid")).await.unwrap();
    engine.transition(&trans("Order", &oc, "fulfilled")).await.unwrap();
    engine.transition(&trans("Order", &oc, "shipped")).await.unwrap();
    engine.transition(&trans("Order", &oc, "delivered")).await.unwrap();

    let q = Query::Paths(PathsQuery {
        machine: "Order".into(),
        filter: None,
        limit: None,
    });
    let result = engine.execute_query(&q).await.unwrap();
    match result {
        QueryResult::Paths(paths) => {
            assert_eq!(paths.len(), 2, "2 distinct paths");

            // Most common first (cancelled x2)
            assert_eq!(paths[0].count, 2);
            assert!(
                paths[0].path.contains(&"cancelled".to_string()),
                "most common path includes cancelled"
            );

            // Less common (delivered x1)
            assert_eq!(paths[1].count, 1);
            assert!(
                paths[1].path.contains(&"delivered".to_string()),
                "less common path includes delivered"
            );
        }
        _ => panic!("expected Paths"),
    }
}

/// 10.3 Combined: aggregate + funnel + paths on same dataset for consistency.
#[tokio::test]
async fn level10_combined_analysis() {
    let engine = make_order_engine();

    // Create 4 orders with different lifecycles
    let mut ids = Vec::new();
    for i in 0..4 {
        let o = engine
            .spawn(&spawn_order_cmd(&format!("C{}", i), 1000 * (i + 1) as i64))
            .await
            .unwrap();
        let oid = o.instance.id.as_str();
        let item = engine
            .spawn(&spawn_item_cmd(&oid, &format!("P{}", i), 1, 1000 * (i + 1) as i64))
            .await
            .unwrap();
        engine
            .transition(&trans("LineItem", &item.instance.id.as_str(), "confirmed"))
            .await
            .unwrap();
        ids.push(oid);
    }

    // Order 0,1,2,3: all placed
    for id in &ids {
        engine.transition(&trans("Order", id, "placed")).await.unwrap();
    }
    // Order 0,1: paid
    engine.transition(&trans("Order", &ids[0], "paid")).await.unwrap();
    engine.transition(&trans("Order", &ids[1], "paid")).await.unwrap();
    // Order 0: fulfilled
    engine.transition(&trans("Order", &ids[0], "fulfilled")).await.unwrap();

    // 1. AGGREGATE COUNT by state
    let q = Query::Aggregate(AggregateQuery {
        machine: "Order".into(),
        measures: vec![MeasureClause {
            function: AggregateFunction::Count,
            field: None,
            alias: None,
        }],
        filter: None,
        group_by: vec![smql_ast::query::GroupByClause::State],
    });
    match engine.execute_query(&q).await.unwrap() {
        QueryResult::Aggregate(rows) => {
            let state_counts: BTreeMap<String, i64> = rows
                .iter()
                .map(|r| {
                    let state = match r.group_key.get("state") {
                        Some(Value::Text(s)) => s.clone(),
                        _ => "?".into(),
                    };
                    let count = match r.measures.get("COUNT") {
                        Some(Value::Int(n)) => *n,
                        _ => -1,
                    };
                    (state, count)
                })
                .collect();
            assert_eq!(state_counts.get("placed"), Some(&2));
            assert_eq!(state_counts.get("paid"), Some(&1));
            assert_eq!(state_counts.get("fulfilled"), Some(&1));
        }
        _ => panic!("expected Aggregate"),
    }

    // 2. FUNNEL
    let q = Query::Funnel(FunnelQuery {
        machine: "Order".into(),
        states: vec!["draft".into(), "placed".into(), "paid".into(), "fulfilled".into()],
        filter: None,
    });
    match engine.execute_query(&q).await.unwrap() {
        QueryResult::Funnel(funnel) => {
            assert_eq!(funnel.stages[0].count, 4); // all visited draft
            assert_eq!(funnel.stages[1].count, 4); // all visited placed
            assert_eq!(funnel.stages[2].count, 2); // 2 visited paid
            assert_eq!(funnel.stages[3].count, 1); // 1 visited fulfilled
        }
        _ => panic!("expected Funnel"),
    }

    // 3. PATHS
    let q = Query::Paths(PathsQuery {
        machine: "Order".into(),
        filter: None,
        limit: None,
    });
    match engine.execute_query(&q).await.unwrap() {
        QueryResult::Paths(paths) => {
            // 3 distinct paths:
            //   2x draft → placed (orders 2,3)
            //   1x draft → placed → paid (order 1)
            //   1x draft → placed → paid → fulfilled (order 0)
            assert_eq!(paths.len(), 3, "3 distinct paths");
            // Most common should have count 2
            assert_eq!(paths[0].count, 2);
        }
        _ => panic!("expected Paths"),
    }
}
