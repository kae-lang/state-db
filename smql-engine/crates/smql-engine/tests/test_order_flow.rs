/// Phase 14.2 — E-Commerce Order end-to-end integration test.
///
/// Tests machine composition with Order → LineItem → Shipment:
/// - Parse multi-machine definitions
/// - Spawn parent Order with child LineItems
/// - Transition children independently
/// - Guards that reference children (ALL(items, STATE IS confirmed))
/// - CASCADE cancellation
/// - FIND / TRAIL / AGGREGATE queries across composed machines
use std::sync::Arc;

use smql_ast::command::{SpawnCommand, TransitionCommand};
use smql_ast::expression::{Expression, ExpressionKind};
use smql_ast::query::{AggregateQuery, FindQuery, GetQuery, MeasureClause, Query, TrailQuery};
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
        as_actor: None,
        idempotency_key: None,
        tags: Vec::new(),
        ttl: None,
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
        as_actor: None,
        idempotency_key: None,
        tags: Vec::new(),
        ttl: None,
    }
}

fn transition_cmd(machine: &str, instance_id: &str, to_state: &str) -> TransitionCommand {
    TransitionCommand::new(
        machine.to_string(),
        instance_id.to_string(),
        to_state.to_string(),
    )
}

fn transition_with(
    machine: &str,
    instance_id: &str,
    to_state: &str,
    data: Vec<(&str, Value)>,
) -> TransitionCommand {
    let mut cmd = TransitionCommand::new(
        machine.to_string(),
        instance_id.to_string(),
        to_state.to_string(),
    );
    cmd.with_data = data
        .into_iter()
        .map(|(k, v)| (k.to_string(), lit(v)))
        .collect();
    cmd
}

fn cascade_transition(machine: &str, instance_id: &str, to_state: &str) -> TransitionCommand {
    let mut cmd = TransitionCommand::new(
        machine.to_string(),
        instance_id.to_string(),
        to_state.to_string(),
    );
    cmd.cascade = true;
    cmd
}

/// Simplified Order/LineItem/Shipment definitions compatible with current evaluator.
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
        in_transit -> delivered {}
        in_transit -> lost {}
    }
)
"#;

fn make_engine() -> Engine {
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

/// Spawn an Order and return its ID.
async fn spawn_order(engine: &Engine) -> String {
    let cmd = spawn_cmd(
        "Order",
        vec![
            ("customer", Value::Text("customer_001".into())),
            ("total", Value::Int(9999)),
        ],
    );
    let result = engine.spawn(&cmd).await.expect("spawn Order");
    assert_eq!(result.instance.state, "draft");
    result.instance.id.to_string()
}

/// Spawn a LineItem child and return its ID.
async fn spawn_line_item(
    engine: &Engine,
    order_id: &str,
    product: &str,
    qty: i64,
    price: i64,
) -> String {
    let cmd = spawn_child_cmd(
        "LineItem",
        vec![
            ("product", Value::Text(product.into())),
            ("quantity", Value::Int(qty)),
            ("price", Value::Int(price)),
        ],
        order_id,
        "Order",
    );
    let result = engine.spawn(&cmd).await.expect("spawn LineItem");
    assert_eq!(result.instance.state, "pending");
    result.instance.id.to_string()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Spawn Order in draft state with child LineItems.
#[tokio::test]
async fn spawn_order_with_line_items() {
    let engine = make_engine();
    let order_id = spawn_order(&engine).await;
    let item1 = spawn_line_item(&engine, &order_id, "Widget A", 2, 2500).await;
    let item2 = spawn_line_item(&engine, &order_id, "Widget B", 1, 4999).await;

    // Verify order
    let q = Query::Get(GetQuery {
        machine: "Order".into(),
        instance_id: order_id.clone(),
        as_actor: None,
    });
    match engine.execute_query(&q).await.unwrap() {
        QueryResult::Instance(inst) => {
            assert_eq!(inst.state, "draft");
            assert_eq!(inst.data.get("total"), Some(&Value::Int(9999)));
        }
        _ => panic!("expected Instance"),
    }

    // Verify children exist with parent link
    let q = Query::Get(GetQuery {
        machine: "LineItem".into(),
        instance_id: item1,
        as_actor: None,
    });
    match engine.execute_query(&q).await.unwrap() {
        QueryResult::Instance(inst) => {
            assert_eq!(inst.state, "pending");
            assert_eq!(inst.parent_machine.as_deref(), Some("Order"));
        }
        _ => panic!("expected Instance"),
    }

    let q = Query::Get(GetQuery {
        machine: "LineItem".into(),
        instance_id: item2,
        as_actor: None,
    });
    match engine.execute_query(&q).await.unwrap() {
        QueryResult::Instance(inst) => assert_eq!(inst.state, "pending"),
        _ => panic!("expected Instance"),
    }
}

/// Order draft → placed requires total > 0.
#[tokio::test]
async fn order_draft_to_placed() {
    let engine = make_engine();
    let order_id = spawn_order(&engine).await;
    let _item = spawn_line_item(&engine, &order_id, "Widget", 1, 5000).await;

    let cmd = transition_cmd("Order", &order_id, "placed");
    let r = engine.transition(&cmd).await.unwrap();
    assert_eq!(r.to_state, "placed");
}

/// Order with zero total cannot be placed.
#[tokio::test]
async fn order_zero_total_rejected() {
    let engine = make_engine();
    let cmd = spawn_cmd(
        "Order",
        vec![
            ("customer", Value::Text("cust".into())),
            ("total", Value::Int(0)),
        ],
    );
    let result = engine.spawn(&cmd).await.unwrap();
    let order_id = result.instance.id.as_str();

    let cmd = transition_cmd("Order", &order_id, "placed");
    let result = engine.transition(&cmd).await;
    assert!(result.is_err(), "total=0 should fail guard total > 0");
}

/// LineItem transitions: pending → confirmed.
#[tokio::test]
async fn line_item_lifecycle() {
    let engine = make_engine();
    let order_id = spawn_order(&engine).await;
    let item_id = spawn_line_item(&engine, &order_id, "Widget", 3, 1500).await;

    let cmd = transition_cmd("LineItem", &item_id, "confirmed");
    let r = engine.transition(&cmd).await.unwrap();
    assert_eq!(r.to_state, "confirmed");
}

/// LineItem pending → backordered → confirmed.
#[tokio::test]
async fn line_item_backordered_flow() {
    let engine = make_engine();
    let order_id = spawn_order(&engine).await;
    let item_id = spawn_line_item(&engine, &order_id, "Rare Widget", 1, 9999).await;

    let cmd = transition_cmd("LineItem", &item_id, "backordered");
    let r = engine.transition(&cmd).await.unwrap();
    assert_eq!(r.to_state, "backordered");

    let cmd = transition_cmd("LineItem", &item_id, "confirmed");
    let r = engine.transition(&cmd).await.unwrap();
    assert_eq!(r.to_state, "confirmed");
}

/// CASCADE cancellation: cancel order cascades to children.
#[tokio::test]
async fn cascade_cancel_order() {
    let engine = make_engine();
    let order_id = spawn_order(&engine).await;
    let item1 = spawn_line_item(&engine, &order_id, "Widget A", 2, 2500).await;
    let item2 = spawn_line_item(&engine, &order_id, "Widget B", 1, 4999).await;

    let cmd = cascade_transition("Order", &order_id, "cancelled");
    let r = engine.transition(&cmd).await.unwrap();
    assert_eq!(r.to_state, "cancelled");

    // Children should be in terminal states (cancelled or confirmed)
    for item_id in [&item1, &item2] {
        let q = Query::Get(GetQuery {
            machine: "LineItem".into(),
            instance_id: item_id.to_string(),
        as_actor: None,
    });
        match engine.execute_query(&q).await.unwrap() {
            QueryResult::Instance(inst) => {
                assert!(
                    inst.state == "cancelled" || inst.state == "confirmed",
                    "child should be in terminal state, got {}",
                    inst.state
                );
            }
            _ => panic!("expected Instance"),
        }
    }
}

/// Order paid → fulfilled requires ALL(items, STATE IS confirmed).
#[tokio::test]
async fn paid_to_fulfilled_requires_all_items_confirmed() {
    let engine = make_engine();
    let order_id = spawn_order(&engine).await;
    let item1 = spawn_line_item(&engine, &order_id, "Widget A", 2, 2500).await;
    let item2 = spawn_line_item(&engine, &order_id, "Widget B", 1, 4999).await;

    // draft → placed → paid
    engine
        .transition(&transition_cmd("Order", &order_id, "placed"))
        .await
        .unwrap();
    engine
        .transition(&transition_cmd("Order", &order_id, "paid"))
        .await
        .unwrap();

    // Try fulfilled with only one item confirmed — should fail
    engine
        .transition(&transition_cmd("LineItem", &item1, "confirmed"))
        .await
        .unwrap();
    let result = engine
        .transition(&transition_cmd("Order", &order_id, "fulfilled"))
        .await;
    assert!(result.is_err(), "not all items confirmed yet");

    // Confirm second item
    engine
        .transition(&transition_cmd("LineItem", &item2, "confirmed"))
        .await
        .unwrap();

    // Now fulfilled should succeed
    let r = engine
        .transition(&transition_cmd("Order", &order_id, "fulfilled"))
        .await
        .unwrap();
    assert_eq!(r.to_state, "fulfilled");
}

/// Guard blocks when signal is missing (placed → paid requires external signal).
#[tokio::test]
async fn order_placed_to_paid() {
    let engine = make_engine();
    let order_id = spawn_order(&engine).await;
    let _item = spawn_line_item(&engine, &order_id, "Widget", 1, 5000).await;

    engine
        .transition(&transition_cmd("Order", &order_id, "placed"))
        .await
        .unwrap();

    // placed → paid has no guard in our simplified def, so it should work
    let r = engine
        .transition(&transition_cmd("Order", &order_id, "paid"))
        .await
        .unwrap();
    assert_eq!(r.to_state, "paid");
}

/// TRAIL for Order shows spawn and transitions.
#[tokio::test]
async fn order_trail() {
    let engine = make_engine();
    let order_id = spawn_order(&engine).await;
    let _item = spawn_line_item(&engine, &order_id, "Widget", 1, 5000).await;

    engine
        .transition(&transition_cmd("Order", &order_id, "placed"))
        .await
        .unwrap();

    let q = Query::Trail(TrailQuery {
        machine: Some("Order".into()),
        instance_id: order_id,
        filter: None,
    });
    match engine.execute_query(&q).await.unwrap() {
        QueryResult::Trail(entries) => {
            assert!(entries.len() >= 2, "trail: spawn + transition");
        }
        _ => panic!("expected Trail"),
    }
}

/// FIND LineItems by state.
#[tokio::test]
async fn find_line_items_by_state() {
    let engine = make_engine();
    let order_id = spawn_order(&engine).await;
    let item1 = spawn_line_item(&engine, &order_id, "Widget A", 2, 2500).await;
    let _item2 = spawn_line_item(&engine, &order_id, "Widget B", 1, 4999).await;

    engine
        .transition(&transition_cmd("LineItem", &item1, "confirmed"))
        .await
        .unwrap();

    // Find pending
    let q = Query::Find(FindQuery {
        machine: "LineItem".into(),
        select: None,
        filter: Some(Expression::new(ExpressionKind::StateIs("pending".into()))),
        sort: vec![],
        limit: None,
        offset: None,
        after: None,
        as_actor: None,
    });
    match engine.execute_query(&q).await.unwrap() {
        QueryResult::Instances(instances) => {
            assert_eq!(instances.len(), 1, "one item should still be pending");
        }
        _ => panic!("expected Instances"),
    }

    // Find confirmed
    let q = Query::Find(FindQuery {
        machine: "LineItem".into(),
        select: None,
        filter: Some(Expression::new(ExpressionKind::StateIs("confirmed".into()))),
        sort: vec![],
        limit: None,
        offset: None,
        after: None,
        as_actor: None,
    });
    match engine.execute_query(&q).await.unwrap() {
        QueryResult::Instances(instances) => {
            assert_eq!(instances.len(), 1, "one item should be confirmed");
        }
        _ => panic!("expected Instances"),
    }
}

/// AGGREGATE count of LineItems by state.
#[tokio::test]
async fn aggregate_line_items() {
    let engine = make_engine();
    let order_id = spawn_order(&engine).await;
    let item1 = spawn_line_item(&engine, &order_id, "Widget A", 2, 2500).await;
    let _item2 = spawn_line_item(&engine, &order_id, "Widget B", 1, 4999).await;
    let _item3 = spawn_line_item(&engine, &order_id, "Widget C", 5, 999).await;

    engine
        .transition(&transition_cmd("LineItem", &item1, "confirmed"))
        .await
        .unwrap();

    let q = Query::Aggregate(AggregateQuery {
        machine: "LineItem".into(),
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
            assert_eq!(rows.len(), 2, "2 groups: pending(2), confirmed(1)");
        }
        _ => panic!("expected Aggregate"),
    }
}

/// Multiple orders with different states.
#[tokio::test]
async fn multiple_orders_lifecycle() {
    let engine = make_engine();

    // Order 1: draft → placed
    let order1 = spawn_order(&engine).await;
    let _item1 = spawn_line_item(&engine, &order1, "Laptop", 1, 99900).await;
    engine
        .transition(&transition_cmd("Order", &order1, "placed"))
        .await
        .unwrap();

    // Order 2: stays in draft
    let _order2 = spawn_order(&engine).await;

    // Order 3: draft → placed → cancelled
    let order3 = spawn_order(&engine).await;
    let _item3 = spawn_line_item(&engine, &order3, "Keyboard", 1, 7500).await;
    engine
        .transition(&transition_cmd("Order", &order3, "placed"))
        .await
        .unwrap();
    engine
        .transition(&cascade_transition("Order", &order3, "cancelled"))
        .await
        .unwrap();

    // FIND placed orders
    let q = Query::Find(FindQuery {
        machine: "Order".into(),
        select: None,
        filter: Some(Expression::new(ExpressionKind::StateIs("placed".into()))),
        sort: vec![],
        limit: None,
        offset: None,
        after: None,
        as_actor: None,
    });
    match engine.execute_query(&q).await.unwrap() {
        QueryResult::Instances(instances) => {
            assert_eq!(instances.len(), 1, "only order1 should be placed");
        }
        _ => panic!("expected Instances"),
    }

    // FIND cancelled orders
    let q = Query::Find(FindQuery {
        machine: "Order".into(),
        select: None,
        filter: Some(Expression::new(ExpressionKind::StateIs("cancelled".into()))),
        sort: vec![],
        limit: None,
        offset: None,
        after: None,
        as_actor: None,
    });
    match engine.execute_query(&q).await.unwrap() {
        QueryResult::Instances(instances) => {
            assert_eq!(instances.len(), 1, "only order3 should be cancelled");
        }
        _ => panic!("expected Instances"),
    }
}

/// Spawn Shipment as child of Order and transition it.
#[tokio::test]
async fn spawn_shipment_child() {
    let engine = make_engine();
    let order_id = spawn_order(&engine).await;

    let cmd = spawn_child_cmd("Shipment", vec![], &order_id, "Order");
    let result = engine.spawn(&cmd).await.unwrap();
    assert_eq!(result.instance.state, "created");
    assert_eq!(result.instance.parent_machine.as_deref(), Some("Order"));
    let ship_id = result.instance.id.as_str();

    // created → dispatched (requires tracking and carrier IS SET)
    let cmd = transition_with(
        "Shipment",
        &ship_id,
        "dispatched",
        vec![
            ("tracking", Value::Text("TRACK123456".into())),
            ("carrier", Value::Text("fedex".into())),
        ],
    );
    let r = engine.transition(&cmd).await.unwrap();
    assert_eq!(r.to_state, "dispatched");

    // dispatched → in_transit
    let r = engine
        .transition(&transition_cmd("Shipment", &ship_id, "in_transit"))
        .await
        .unwrap();
    assert_eq!(r.to_state, "in_transit");

    // in_transit → delivered
    let r = engine
        .transition(&transition_cmd("Shipment", &ship_id, "delivered"))
        .await
        .unwrap();
    assert_eq!(r.to_state, "delivered");
}

/// Shipment guard: dispatched requires tracking and carrier IS SET.
#[tokio::test]
async fn shipment_guard_rejects_without_tracking() {
    let engine = make_engine();
    let order_id = spawn_order(&engine).await;

    let cmd = spawn_child_cmd("Shipment", vec![], &order_id, "Order");
    let result = engine.spawn(&cmd).await.unwrap();
    let ship_id = result.instance.id.as_str();

    let cmd = transition_cmd("Shipment", &ship_id, "dispatched");
    let result = engine.transition(&cmd).await;
    assert!(result.is_err(), "dispatched requires tracking IS SET");
}

/// LineItem cancellation via ANY → cancelled (except confirmed).
#[tokio::test]
async fn line_item_wildcard_cancel() {
    let engine = make_engine();
    let order_id = spawn_order(&engine).await;
    let item_id = spawn_line_item(&engine, &order_id, "Widget", 1, 5000).await;

    let cmd = transition_cmd("LineItem", &item_id, "cancelled");
    let r = engine.transition(&cmd).await.unwrap();
    assert_eq!(r.to_state, "cancelled");
}

/// Cannot cancel a confirmed LineItem (EXCEPT FROM { confirmed }).
#[tokio::test]
async fn confirmed_item_cannot_be_cancelled() {
    let engine = make_engine();
    let order_id = spawn_order(&engine).await;
    let item_id = spawn_line_item(&engine, &order_id, "Widget", 1, 5000).await;

    engine
        .transition(&transition_cmd("LineItem", &item_id, "confirmed"))
        .await
        .unwrap();

    let cmd = transition_cmd("LineItem", &item_id, "cancelled");
    let result = engine.transition(&cmd).await;
    assert!(result.is_err(), "confirmed items should not be cancellable");
}

/// Full order flow: draft → placed → paid → fulfilled (all items confirmed).
#[tokio::test]
async fn full_order_fulfillment_flow() {
    let engine = make_engine();
    let order_id = spawn_order(&engine).await;
    let item1 = spawn_line_item(&engine, &order_id, "Widget A", 2, 2500).await;
    let item2 = spawn_line_item(&engine, &order_id, "Widget B", 1, 4999).await;

    // draft → placed
    engine
        .transition(&transition_cmd("Order", &order_id, "placed"))
        .await
        .unwrap();
    // placed → paid
    engine
        .transition(&transition_cmd("Order", &order_id, "paid"))
        .await
        .unwrap();

    // Confirm all items
    engine
        .transition(&transition_cmd("LineItem", &item1, "confirmed"))
        .await
        .unwrap();
    engine
        .transition(&transition_cmd("LineItem", &item2, "confirmed"))
        .await
        .unwrap();

    // paid → fulfilled
    let r = engine
        .transition(&transition_cmd("Order", &order_id, "fulfilled"))
        .await
        .unwrap();
    assert_eq!(r.to_state, "fulfilled");

    // fulfilled → shipped → delivered
    engine
        .transition(&transition_cmd("Order", &order_id, "shipped"))
        .await
        .unwrap();
    let r = engine
        .transition(&transition_cmd("Order", &order_id, "delivered"))
        .await
        .unwrap();
    assert_eq!(r.to_state, "delivered");

    // Verify trail has full history
    let q = Query::Trail(TrailQuery {
        machine: Some("Order".into()),
        instance_id: order_id,
        filter: None,
    });
    match engine.execute_query(&q).await.unwrap() {
        QueryResult::Trail(entries) => {
            // spawn + draft→placed + placed→paid + paid→fulfilled + fulfilled→shipped + shipped→delivered = 6
            assert!(
                entries.len() >= 6,
                "trail should have at least 6 entries, got {}",
                entries.len()
            );
        }
        _ => panic!("expected Trail"),
    }
}

// ---------------------------------------------------------------------------
// SIGNAL PARENT test
// ---------------------------------------------------------------------------

/// Machines where Shipment's dispatched → in_transit fires SIGNAL PARENT TO shipped.
const SIGNAL_PARENT_SMQL: &str = r#"
DEFINE MACHINE SigOrder (
    DATA {
        customer : TEXT -> REQUIRED
        total    : INT  -> REQUIRED
    }
    STATES { draft, placed, paid, fulfilled, shipped, delivered, cancelled }
    INITIAL STATE draft
    TERMINAL STATES { delivered, cancelled }

    CHILDREN {
        shipment : OPTIONAL(SigShipment)
    }

    TRANSITIONS {
        draft -> placed {
            GUARD : total > 0
        }
        placed -> paid {}
        paid -> fulfilled {}
        fulfilled -> shipped {}
        shipped -> delivered {}
        ANY -> cancelled {
            EXCEPT FROM { shipped, delivered }
        }
    }
)

DEFINE MACHINE SigShipment (
    PARENT : SigOrder
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
        dispatched -> in_transit {
            SIGNAL PARENT TO shipped
        }
        in_transit -> delivered {}
        in_transit -> lost {}
    }
)
"#;

/// Verify SIGNAL PARENT: transitioning Shipment dispatched → in_transit
/// signals the parent Order to transition to "shipped".
#[tokio::test]
async fn signal_parent_from_shipment_delivery() {
    let machines =
        smql_parser::parse_machines(SIGNAL_PARENT_SMQL).expect("parse signal parent defs");
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

    // Spawn order, transition to fulfilled
    let order = engine
        .spawn(&spawn_cmd(
            "SigOrder",
            vec![
                ("customer", Value::Text("cust_001".into())),
                ("total", Value::Int(5000)),
            ],
        ))
        .await
        .unwrap();
    let order_id = order.instance.id.as_str();

    engine
        .transition(&transition_cmd("SigOrder", &order_id, "placed"))
        .await
        .unwrap();
    engine
        .transition(&transition_cmd("SigOrder", &order_id, "paid"))
        .await
        .unwrap();
    engine
        .transition(&transition_cmd("SigOrder", &order_id, "fulfilled"))
        .await
        .unwrap();

    // Spawn shipment as child of order
    let ship = engine
        .spawn(&spawn_child_cmd(
            "SigShipment",
            vec![],
            &order_id,
            "SigOrder",
        ))
        .await
        .unwrap();
    let ship_id = ship.instance.id.as_str();

    // Transition shipment: created → dispatched (needs tracking + carrier)
    let cmd = transition_with(
        "SigShipment",
        &ship_id,
        "dispatched",
        vec![
            ("tracking", Value::Text("TRACK999".into())),
            ("carrier", Value::Text("ups".into())),
        ],
    );
    engine.transition(&cmd).await.unwrap();

    // Transition shipment: dispatched → in_transit (fires SIGNAL PARENT TO shipped)
    engine
        .transition(&transition_cmd("SigShipment", &ship_id, "in_transit"))
        .await
        .unwrap();

    // Assert: order should now be in "shipped" state
    let q = Query::Get(GetQuery {
        machine: "SigOrder".into(),
        instance_id: order_id.to_string(),
        as_actor: None,
    });
    match engine.execute_query(&q).await.unwrap() {
        QueryResult::Instance(inst) => {
            assert_eq!(
                inst.state, "shipped",
                "SIGNAL PARENT should have moved order to shipped"
            );
        }
        _ => panic!("expected Instance"),
    }
}
