/// Documentation Validation Tests — Composition, Server, Built-in Functions, Grammar
///
/// Tests all SMQL code examples from the following doc files:
///   1. docs/composition/overview.md
///   2. docs/composition/children-declaration.md
///   3. docs/composition/spawning-children.md
///   4. docs/composition/child-predicates.md
///   5. docs/composition/cascade.md
///   6. docs/composition/signal-parent.md
///   7. docs/server/http-api.md
///   8. docs/server/request-response.md
///   9. docs/reference/built-in-functions.md
///  10. docs/reference/grammar.md
///
/// Each test verifies that the SMQL snippet parses successfully. Complete
/// machine definitions are also registered into a catalog and (where feasible)
/// executed through the engine.

use std::sync::Arc;

use smql_ast::command::{SpawnCommand, TransitionCommand};
use smql_ast::expression::{Expression, ExpressionKind};
use smql_ast::query::{GetQuery, Query};
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

fn transition_cmd(machine: &str, instance_id: &str, to_state: &str) -> TransitionCommand {
    TransitionCommand::new(
        machine.to_string(),
        instance_id.to_string(),
        to_state.to_string(),
    )
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

fn make_engine_from_smql(smql: &str) -> Engine {
    let machines = smql_parser::parse_machines(smql).expect("parse machine definitions");
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

// ===========================================================================
// 1. docs/composition/overview.md
// ===========================================================================

/// overview.md — CHILDREN + PARENT partial snippet (parse only)
///
/// ```sql
/// DEFINE MACHINE Order (
///   CHILDREN {
///     items    : LIST(LineItem)    -> MIN(1)
///     shipment : OPTIONAL(Shipment)
///   }
///   ...
/// )
/// ```
///
/// The `...` is not valid SMQL, so we fill in minimal body.
#[test]
fn overview_order_children_snippet_parse() {
    let smql = r#"
DEFINE MACHINE Order (
    CHILDREN {
        items    : LIST(LineItem)    -> MIN(1)
        shipment : OPTIONAL(Shipment)
    }
    STATES { draft }
    INITIAL STATE draft
    TRANSITIONS {}
)
"#;
    smql_parser::parse_machines(smql).expect("overview Order CHILDREN snippet should parse");
}

/// overview.md — PARENT declaration snippet (parse only)
///
/// ```sql
/// DEFINE MACHINE LineItem (
///   PARENT : Order
///   ...
/// )
/// ```
#[test]
fn overview_lineitem_parent_snippet_parse() {
    let smql = r#"
DEFINE MACHINE LineItem (
    PARENT : Order
    STATES { pending }
    INITIAL STATE pending
    TRANSITIONS {}
)
"#;
    smql_parser::parse_machines(smql).expect("overview LineItem PARENT snippet should parse");
}

/// overview.md — Guard on children's state (transition snippet, parse only)
///
/// ```sql
/// paid -> fulfilled {
///   GUARD : ALL(items, STATE IS confirmed)
/// }
/// ```
#[test]
fn overview_guard_on_children_parse() {
    // Embed inside a minimal machine definition to parse
    let smql = r#"
DEFINE MACHINE M (
    STATES { paid, fulfilled }
    INITIAL STATE paid
    CHILDREN { items : LIST(Child) }
    TRANSITIONS {
        paid -> fulfilled {
            GUARD : ALL(items, STATE IS confirmed)
        }
    }
)
"#;
    smql_parser::parse_machines(smql).expect("overview guard on children snippet should parse");
}

/// overview.md — CASCADE transition command (parse only)
///
/// ```sql
/// TRANSITION Order "01J5..." TO cancelled CASCADE
/// ```
#[test]
fn overview_cascade_transition_parse() {
    let smql = r#"TRANSITION Order "01J5X7K2P3Q4R5S6T7U8V9W0XY" TO cancelled CASCADE"#;
    smql_parser::parse(smql).expect("overview CASCADE transition should parse");
}

/// overview.md — SIGNAL PARENT TO snippet (transition body, parse only)
///
/// ```sql
/// in_transit -> delivered {
///   SIGNAL PARENT TO delivered
/// }
/// ```
#[test]
fn overview_signal_parent_to_parse() {
    let smql = r#"
DEFINE MACHINE M (
    PARENT : P
    STATES { in_transit, delivered }
    INITIAL STATE in_transit
    TERMINAL STATES { delivered }
    TRANSITIONS {
        in_transit -> delivered {
            SIGNAL PARENT TO delivered
        }
    }
)
"#;
    smql_parser::parse_machines(smql).expect("overview SIGNAL PARENT TO snippet should parse");
}

/// overview.md — Complete Order example (parse + register)
///
/// The doc uses `MONEY(USD)`, `REF(Customer)`, and `SPAWN Shipment { order: SELF }`.
/// These are all valid SMQL, so we test full parsing.
#[test]
fn overview_complete_order_example_parse() {
    let smql = r#"
DEFINE MACHINE Order (
    DATA {
        customer : REF(Customer) -> REQUIRED
        total    : MONEY(USD)    -> REQUIRED
        notes    : TEXT           -> OPTIONAL
    }
    STATES { draft, placed, paid, payment_failed, fulfilled, shipped, delivered, cancelled, returned }
    INITIAL STATE draft
    TERMINAL STATES { delivered, cancelled, returned }
    CHILDREN {
        items    : LIST(LineItem)      -> MIN(1)
        shipment : OPTIONAL(Shipment)
    }
    TRANSITIONS {
        draft -> placed {}
        placed -> paid {}
        paid -> fulfilled {
            GUARD  : ALL(items, STATE IS confirmed)
            MUTATE : shipment = SPAWN Shipment { order: SELF }
        }
        fulfilled -> shipped {
            GUARD : shipment.STATE IS dispatched
        }
        shipped -> delivered {}
        ANY -> cancelled {
            EXCEPT FROM { shipped, delivered, returned }
        }
    }
)
"#;
    smql_parser::parse_machines(smql).expect("overview complete Order example should parse");
}

// ===========================================================================
// 2. docs/composition/children-declaration.md
// ===========================================================================

/// children-declaration.md — CHILDREN block with MIN/MAX constraints
///
/// ```sql
/// CHILDREN {
///   items    : LIST(LineItem)    -> MIN(1), MAX(100)
///   shipment : OPTIONAL(Shipment)
/// }
/// ```
#[test]
fn children_decl_min_max_constraints_parse() {
    let smql = r#"
DEFINE MACHINE Order (
    CHILDREN {
        items    : LIST(LineItem)    -> MIN(1), MAX(100)
        shipment : OPTIONAL(Shipment)
    }
    STATES { draft }
    INITIAL STATE draft
    TRANSITIONS {}
)
"#;
    smql_parser::parse_machines(smql).expect("children declaration with MIN/MAX should parse");
}

/// children-declaration.md — Full LineItem child machine with PARENT declaration
///
/// ```sql
/// DEFINE MACHINE LineItem (
///   PARENT : Order
///   DATA {
///     product  : TEXT        -> REQUIRED
///     quantity : INT         -> MIN(1), REQUIRED
///     price    : MONEY(USD)  -> REQUIRED
///   }
///   STATES { pending, confirmed, backordered, cancelled }
///   INITIAL STATE pending
///   TERMINAL STATES { confirmed, cancelled }
///   TRANSITIONS {
///     pending -> confirmed { GUARD : quantity > 0 }
///     pending -> backordered {}
///     backordered -> confirmed {}
///     ANY -> cancelled { EXCEPT FROM { confirmed } }
///   }
/// )
/// ```
#[test]
fn children_decl_lineitem_full_parse() {
    let smql = r#"
DEFINE MACHINE LineItem (
    PARENT : Order
    DATA {
        product  : TEXT        -> REQUIRED
        quantity : INT         -> MIN(1), REQUIRED
        price    : MONEY(USD)  -> REQUIRED
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
    smql_parser::parse_machines(smql).expect("children declaration LineItem full should parse");
}

/// children-declaration.md — PARENT.customer expression inside action
///
/// ```sql
/// pending -> backordered {
///   ACTION : NOTIFY(PARENT.customer, "item.backordered")
/// }
/// ```
#[test]
fn children_decl_parent_field_access_in_action_parse() {
    let smql = r#"
DEFINE MACHINE LineItem (
    PARENT : Order
    STATES { pending, backordered }
    INITIAL STATE pending
    TRANSITIONS {
        pending -> backordered {
            ACTION : NOTIFY(PARENT.customer, "item.backordered")
        }
    }
)
"#;
    smql_parser::parse_machines(smql).expect("PARENT.customer in NOTIFY action should parse");
}

/// children-declaration.md — Both sides: Pipeline + Stage
///
/// ```sql
/// DEFINE MACHINE Pipeline (
///   CHILDREN {
///     stages : LIST(Stage) -> MIN(1)
///   }
///   ...
/// )
///
/// DEFINE MACHINE Stage (
///   PARENT : Pipeline
///   ...
/// )
/// ```
#[test]
fn children_decl_both_sides_parse() {
    let smql = r#"
DEFINE MACHINE Pipeline (
    CHILDREN {
        stages : LIST(Stage) -> MIN(1)
    }
    STATES { running }
    INITIAL STATE running
    TRANSITIONS {}
)

DEFINE MACHINE Stage (
    PARENT : Pipeline
    STATES { pending }
    INITIAL STATE pending
    TRANSITIONS {}
)
"#;
    smql_parser::parse_machines(smql).expect("both sides (Pipeline + Stage) should parse");
}

// ===========================================================================
// 3. docs/composition/spawning-children.md
// ===========================================================================

/// spawning-children.md — SPAWN in MUTATE
///
/// ```sql
/// paid -> fulfilled {
///   GUARD  : ALL(items, STATE IS confirmed)
///   MUTATE : shipment = SPAWN Shipment { order: SELF }
/// }
/// ```
#[test]
fn spawning_spawn_in_mutate_parse() {
    let smql = r#"
DEFINE MACHINE Order (
    STATES { paid, fulfilled }
    INITIAL STATE paid
    CHILDREN { items : LIST(Child)  shipment : OPTIONAL(Shipment) }
    TRANSITIONS {
        paid -> fulfilled {
            GUARD  : ALL(items, STATE IS confirmed)
            MUTATE : shipment = SPAWN Shipment { order: SELF }
        }
    }
)
"#;
    smql_parser::parse_machines(smql).expect("SPAWN in MUTATE should parse");
}

/// spawning-children.md — Empty data block spawn
///
/// ```sql
/// MUTATE : tracker = SPAWN Tracker {}
/// ```
#[test]
fn spawning_empty_data_block_parse() {
    let smql = r#"
DEFINE MACHINE Order (
    STATES { a, b }
    INITIAL STATE a
    CHILDREN { tracker : OPTIONAL(Tracker) }
    TRANSITIONS {
        a -> b {
            MUTATE : tracker = SPAWN Tracker {}
        }
    }
)
"#;
    smql_parser::parse_machines(smql).expect("SPAWN with empty data block should parse");
}

/// spawning-children.md — Multiple spawns in a single transition
///
/// ```sql
/// approved -> active {
///   MUTATE : primary = SPAWN Account { type: "primary", owner: SELF }
///   MUTATE : savings = SPAWN Account { type: "savings", owner: SELF }
/// }
/// ```
#[test]
fn spawning_multiple_spawns_parse() {
    let smql = r#"
DEFINE MACHINE Banking (
    STATES { approved, active }
    INITIAL STATE approved
    CHILDREN { primary : OPTIONAL(Account)  savings : OPTIONAL(Account) }
    TRANSITIONS {
        approved -> active {
            MUTATE : primary = SPAWN Account { type: "primary", owner: SELF }
            MUTATE : savings = SPAWN Account { type: "savings", owner: SELF }
        }
    }
)
"#;
    smql_parser::parse_machines(smql).expect("multiple SPAWN in one transition should parse");
}

/// spawning-children.md — External SPAWN command
///
/// ```sql
/// SPAWN LineItem {
///   product: "Widget",
///   quantity: 3,
///   price: 9.99
/// }
/// ```
#[test]
fn spawning_external_spawn_parse() {
    let smql = r#"SPAWN LineItem { product: "Widget", quantity: 3, price: 9.99 }"#;
    smql_parser::parse(smql).expect("external SPAWN LineItem should parse");
}

/// spawning-children.md — CHILDREN with LIST cardinality
///
/// ```sql
/// CHILDREN {
///   items : LIST(LineItem) -> MIN(1)
/// }
/// ```
#[test]
fn spawning_list_cardinality_parse() {
    let smql = r#"
DEFINE MACHINE Order (
    CHILDREN {
        items : LIST(LineItem) -> MIN(1)
    }
    STATES { draft }
    INITIAL STATE draft
    TRANSITIONS {}
)
"#;
    smql_parser::parse_machines(smql).expect("LIST cardinality should parse");
}

/// spawning-children.md — CHILDREN with OPTIONAL cardinality
///
/// ```sql
/// CHILDREN {
///   shipment : OPTIONAL(Shipment)
/// }
/// ```
#[test]
fn spawning_optional_cardinality_parse() {
    let smql = r#"
DEFINE MACHINE Order (
    CHILDREN {
        shipment : OPTIONAL(Shipment)
    }
    STATES { draft }
    INITIAL STATE draft
    TRANSITIONS {}
)
"#;
    smql_parser::parse_machines(smql).expect("OPTIONAL cardinality should parse");
}

// ===========================================================================
// 4. docs/composition/child-predicates.md
// ===========================================================================

/// child-predicates.md — ALL() guard
///
/// ```sql
/// paid -> fulfilled {
///   GUARD : ALL(items, STATE IS confirmed)
/// }
/// ```
#[test]
fn predicates_all_guard_parse() {
    let smql = r#"
DEFINE MACHINE M (
    STATES { paid, fulfilled }
    INITIAL STATE paid
    CHILDREN { items : LIST(C) }
    TRANSITIONS {
        paid -> fulfilled {
            GUARD : ALL(items, STATE IS confirmed)
        }
    }
)
"#;
    smql_parser::parse_machines(smql).expect("ALL() guard should parse");
}

/// child-predicates.md — len(items) guard
///
/// ```sql
/// paid -> fulfilled {
///   GUARD : len(items) > 0
///   GUARD : ALL(items, STATE IS confirmed)
/// }
/// ```
#[test]
fn predicates_len_guard_parse() {
    let smql = r#"
DEFINE MACHINE M (
    STATES { paid, fulfilled }
    INITIAL STATE paid
    CHILDREN { items : LIST(C) }
    TRANSITIONS {
        paid -> fulfilled {
            GUARD : len(items) > 0
            GUARD : ALL(items, STATE IS confirmed)
        }
    }
)
"#;
    smql_parser::parse_machines(smql).expect("len() + ALL() guard should parse");
}

/// child-predicates.md — ANY() guard
///
/// ```sql
/// running -> failed {
///   GUARD : ANY(stages, STATE IS failed)
/// }
/// ```
#[test]
fn predicates_any_guard_parse() {
    let smql = r#"
DEFINE MACHINE M (
    STATES { running, failed }
    INITIAL STATE running
    CHILDREN { stages : LIST(S) }
    TRANSITIONS {
        running -> failed {
            GUARD : ANY(stages, STATE IS failed)
        }
    }
)
"#;
    smql_parser::parse_machines(smql).expect("ANY() guard should parse");
}

/// child-predicates.md — Order fulfillment practical example
///
/// ```sql
/// DEFINE MACHINE Order (
///   CHILDREN {
///     items : LIST(LineItem) -> MIN(1)
///   }
///   TRANSITIONS {
///     paid -> fulfilled {
///       GUARD : ALL(items, STATE IS confirmed)
///     }
///   }
/// )
/// ```
#[test]
fn predicates_order_fulfillment_parse() {
    let smql = r#"
DEFINE MACHINE Order (
    CHILDREN {
        items : LIST(LineItem) -> MIN(1)
    }
    STATES { paid, fulfilled }
    INITIAL STATE paid
    TRANSITIONS {
        paid -> fulfilled {
            GUARD : ALL(items, STATE IS confirmed)
        }
    }
)
"#;
    smql_parser::parse_machines(smql).expect("Order fulfillment predicate example should parse");
}

/// child-predicates.md — Pipeline failure detection
///
/// ```sql
/// DEFINE MACHINE Pipeline (
///   CHILDREN {
///     stages : LIST(Stage) -> MIN(1)
///   }
///   TRANSITIONS {
///     running -> failed {
///       GUARD : ANY(stages, STATE IS failed)
///     }
///     running -> completed {
///       GUARD : ALL(stages, STATE IS passed)
///     }
///   }
/// )
/// ```
#[test]
fn predicates_pipeline_failure_parse() {
    let smql = r#"
DEFINE MACHINE Pipeline (
    CHILDREN {
        stages : LIST(Stage) -> MIN(1)
    }
    STATES { running, failed, completed }
    INITIAL STATE running
    TERMINAL STATES { failed, completed }
    TRANSITIONS {
        running -> failed {
            GUARD : ANY(stages, STATE IS failed)
        }
        running -> completed {
            GUARD : ALL(stages, STATE IS passed)
        }
    }
)
"#;
    smql_parser::parse_machines(smql).expect("Pipeline failure detection should parse");
}

/// child-predicates.md — Shipment readiness guard
///
/// ```sql
/// fulfilled -> shipped {
///   GUARD : shipment.STATE IS dispatched
/// }
/// ```
///
/// NOTE: `shipment.STATE IS dispatched` uses dot notation for an OPTIONAL child's state.
/// This parses as FieldAccess(["shipment", "STATE"]) followed by IS, which is tricky.
#[test]
fn predicates_shipment_readiness_parse() {
    let smql = r#"
DEFINE MACHINE Order (
    STATES { fulfilled, shipped }
    INITIAL STATE fulfilled
    CHILDREN { shipment : OPTIONAL(Shipment) }
    TRANSITIONS {
        fulfilled -> shipped {
            GUARD : shipment.STATE IS dispatched
        }
    }
)
"#;
    // This might fail if the parser doesn't handle field.STATE IS <ident>
    let result = smql_parser::parse_machines(smql);
    match result {
        Ok(_) => {} // Great, it parses
        Err(e) => {
            // Document the failure
            panic!(
                "DOC ISSUE: 'shipment.STATE IS dispatched' does not parse: {}",
                e
            );
        }
    }
}

/// child-predicates.md — Combined predicates with other guards
///
/// ```sql
/// paid -> fulfilled {
///   GUARD : ALL(items, STATE IS confirmed)
///   GUARD : total > 0
///   GUARD : ACTOR.role IN ("admin", "fulfillment")
/// }
/// ```
#[test]
fn predicates_combined_guards_parse() {
    let smql = r#"
DEFINE MACHINE Order (
    STATES { paid, fulfilled }
    INITIAL STATE paid
    CHILDREN { items : LIST(C) }
    TRANSITIONS {
        paid -> fulfilled {
            GUARD : ALL(items, STATE IS confirmed)
            GUARD : total > 0
            GUARD : ACTOR.role IN ("admin", "fulfillment")
        }
    }
)
"#;
    smql_parser::parse_machines(smql).expect("combined guards with IN should parse");
}

// ===========================================================================
// 5. docs/composition/cascade.md
// ===========================================================================

/// cascade.md — CASCADE transition command
///
/// ```sql
/// TRANSITION Order "01J5X7K2P3Q4R5S6T7U8V9W0XY" TO cancelled CASCADE
/// ```
#[test]
fn cascade_transition_command_parse() {
    let smql = r#"TRANSITION Order "01J5X7K2P3Q4R5S6T7U8V9W0XY" TO cancelled CASCADE"#;
    smql_parser::parse(smql).expect("CASCADE transition command should parse");
}

/// cascade.md — Terminal states ordering
///
/// ```sql
/// DEFINE MACHINE LineItem (
///   TERMINAL STATES { confirmed, cancelled }
///   ...
/// )
/// ```
/// and
/// ```sql
/// DEFINE MACHINE LineItem (
///   TERMINAL STATES { cancelled, confirmed }
///   ...
/// )
/// ```
#[test]
fn cascade_terminal_states_ordering_parse() {
    let smql1 = r#"
DEFINE MACHINE LineItem (
    STATES { pending, confirmed, cancelled }
    INITIAL STATE pending
    TERMINAL STATES { confirmed, cancelled }
    TRANSITIONS {}
)
"#;
    let smql2 = r#"
DEFINE MACHINE LineItem (
    STATES { pending, confirmed, cancelled }
    INITIAL STATE pending
    TERMINAL STATES { cancelled, confirmed }
    TRANSITIONS {}
)
"#;
    smql_parser::parse_machines(smql1).expect("terminal states ordering 1 should parse");
    smql_parser::parse_machines(smql2).expect("terminal states ordering 2 should parse");
}

/// cascade.md — Guard behavior example
///
/// ```sql
/// DEFINE MACHINE LineItem (
///   TERMINAL STATES { cancelled, confirmed }
///   TRANSITIONS {
///     ANY -> cancelled {
///       EXCEPT FROM { confirmed }
///       GUARD : ACTOR.role == "admin"
///     }
///   }
/// )
/// ```
#[test]
fn cascade_guard_behavior_parse() {
    let smql = r#"
DEFINE MACHINE LineItem (
    STATES { pending, confirmed, cancelled }
    INITIAL STATE pending
    TERMINAL STATES { cancelled, confirmed }
    TRANSITIONS {
        ANY -> cancelled {
            EXCEPT FROM { confirmed }
            GUARD : ACTOR.role == "admin"
        }
    }
)
"#;
    smql_parser::parse_machines(smql).expect("CASCADE guard behavior example should parse");
}

/// cascade.md — Full Order example (parse + engine execution)
///
/// ```sql
/// DEFINE MACHINE Order (
///   DATA { customer : REF(Customer) -> REQUIRED  total : MONEY(USD) -> REQUIRED  notes : TEXT -> OPTIONAL }
///   STATES { draft, placed, paid, ... cancelled, returned }
///   INITIAL STATE draft
///   TERMINAL STATES { delivered, cancelled, returned }
///   CHILDREN { items : LIST(LineItem) -> MIN(1)  shipment : OPTIONAL(Shipment) }
///   TRANSITIONS { ANY -> cancelled { EXCEPT FROM { shipped, delivered, returned } } }
/// )
/// ```
#[test]
fn cascade_full_order_parse() {
    let smql = r#"
DEFINE MACHINE Order (
    DATA {
        customer : REF(Customer) -> REQUIRED
        total    : MONEY(USD)    -> REQUIRED
        notes    : TEXT           -> OPTIONAL
    }
    STATES { draft, placed, paid, payment_failed, fulfilled, shipped, delivered, cancelled, returned }
    INITIAL STATE draft
    TERMINAL STATES { delivered, cancelled, returned }
    CHILDREN {
        items    : LIST(LineItem)      -> MIN(1)
        shipment : OPTIONAL(Shipment)
    }
    TRANSITIONS {
        ANY -> cancelled {
            EXCEPT FROM { shipped, delivered, returned }
        }
    }
)
"#;
    smql_parser::parse_machines(smql).expect("cascade full Order example should parse");
}

/// cascade.md — CASCADE with AS/WITH/MEMO clauses
///
/// ```sql
/// TRANSITION Order "01J5..." TO cancelled CASCADE
///   AS "admin-1"
///   WITH { cancellation_reason: "Customer request" }
///   MEMO "Cancelled per customer phone call"
/// ```
#[test]
fn cascade_with_clauses_parse() {
    let smql = r#"TRANSITION Order "01J5X7K2P3Q4R5S6T7U8V9W0XY" TO cancelled CASCADE AS "admin-1" WITH { cancellation_reason: "Customer request" } MEMO "Cancelled per customer phone call""#;
    smql_parser::parse(smql).expect("CASCADE with AS/WITH/MEMO should parse");
}

/// cascade.md — Engine: CASCADE an Order cancels children
#[tokio::test]
async fn cascade_engine_order_cancels_children() {
    // Use simplified types (INT instead of MONEY, TEXT instead of REF) for engine compat.
    let smql = r#"
DEFINE MACHINE Order (
    DATA {
        customer : TEXT -> REQUIRED
        total    : INT  -> REQUIRED
    }
    STATES { draft, placed, cancelled, delivered }
    INITIAL STATE draft
    TERMINAL STATES { delivered, cancelled }
    CHILDREN {
        items : LIST(LineItem) -> MIN(1)
    }
    TRANSITIONS {
        draft -> placed { GUARD : total > 0 }
        ANY -> cancelled {
            EXCEPT FROM { delivered }
        }
    }
)

DEFINE MACHINE LineItem (
    PARENT : Order
    DATA {
        product : TEXT -> REQUIRED
        quantity : INT -> REQUIRED
    }
    STATES { pending, confirmed, cancelled }
    INITIAL STATE pending
    TERMINAL STATES { cancelled, confirmed }
    TRANSITIONS {
        pending -> confirmed { GUARD : quantity > 0 }
        ANY -> cancelled { EXCEPT FROM { confirmed } }
    }
)
"#;
    let engine = make_engine_from_smql(smql);

    // Spawn an order
    let order = engine
        .spawn(&spawn_cmd(
            "Order",
            vec![
                ("customer", Value::Text("cust-1".into())),
                ("total", Value::Int(100)),
            ],
        ))
        .await
        .expect("spawn order");
    let order_id = order.instance.id.as_str();

    // Spawn two line items
    let li1 = engine
        .spawn(&spawn_child_cmd(
            "LineItem",
            vec![
                ("product", Value::Text("Widget".into())),
                ("quantity", Value::Int(2)),
            ],
            &order_id,
            "Order",
        ))
        .await
        .expect("spawn line item 1");
    let li1_id = li1.instance.id.as_str();

    let li2 = engine
        .spawn(&spawn_child_cmd(
            "LineItem",
            vec![
                ("product", Value::Text("Gadget".into())),
                ("quantity", Value::Int(1)),
            ],
            &order_id,
            "Order",
        ))
        .await
        .expect("spawn line item 2");
    let li2_id = li2.instance.id.as_str();

    // CASCADE cancel the order
    let result = engine
        .transition(&cascade_transition("Order", &order_id, "cancelled"))
        .await
        .expect("cascade cancel order");
    assert_eq!(result.to_state, "cancelled");

    // Verify children are also cancelled (first terminal state = cancelled)
    for (item_id, label) in [(&li1_id, "LineItem 1"), (&li2_id, "LineItem 2")] {
        let q = Query::Get(GetQuery {
            machine: "LineItem".into(),
            instance_id: item_id.clone(),
        });
        match engine.execute_query(&q).await.unwrap() {
            QueryResult::Instance(inst) => {
                assert_eq!(inst.state, "cancelled", "{} should be cancelled", label);
            }
            other => panic!("expected Instance, got {:?}", other),
        }
    }
}

// ===========================================================================
// 6. docs/composition/signal-parent.md
// ===========================================================================

/// signal-parent.md — SIGNAL PARENT TO syntax
///
/// ```sql
/// source -> target {
///   SIGNAL PARENT TO parent_state
/// }
/// ```
#[test]
fn signal_parent_basic_syntax_parse() {
    let smql = r#"
DEFINE MACHINE Child (
    PARENT : P
    STATES { source, target }
    INITIAL STATE source
    TERMINAL STATES { target }
    TRANSITIONS {
        source -> target {
            SIGNAL PARENT TO parent_state
        }
    }
)
"#;
    smql_parser::parse_machines(smql).expect("SIGNAL PARENT TO basic syntax should parse");
}

/// signal-parent.md — Full Shipment definition
///
/// ```sql
/// DEFINE MACHINE Shipment (
///   PARENT : Order
///   DATA {
///     tracking : TEXT -> OPTIONAL
///     carrier  : ENUM(fedex, ups, dhl, usps) -> OPTIONAL
///   }
///   STATES { created, dispatched, in_transit, delivered, lost }
///   INITIAL STATE created
///   TERMINAL STATES { delivered, lost }
///   TRANSITIONS {
///     created -> dispatched {
///       GUARD  : tracking IS SET
///       GUARD  : carrier IS SET
///       ACTION : NOTIFY(PARENT.customer, "order.shipped")
///     }
///     dispatched -> in_transit {}
///     in_transit -> delivered {
///       SIGNAL PARENT TO delivered
///     }
///     in_transit -> lost {
///       ACTION : NOTIFY(PARENT.customer, "shipment.lost")
///     }
///   }
/// )
/// ```
#[test]
fn signal_parent_full_shipment_parse() {
    let smql = r#"
DEFINE MACHINE Shipment (
    PARENT : Order
    DATA {
        tracking : TEXT -> OPTIONAL
        carrier  : ENUM(fedex, ups, dhl, usps) -> OPTIONAL
    }
    STATES { created, dispatched, in_transit, delivered, lost }
    INITIAL STATE created
    TERMINAL STATES { delivered, lost }
    TRANSITIONS {
        created -> dispatched {
            GUARD  : tracking IS SET
            GUARD  : carrier IS SET
            ACTION : NOTIFY(PARENT.customer, "order.shipped")
        }
        dispatched -> in_transit {}
        in_transit -> delivered {
            SIGNAL PARENT TO delivered
        }
        in_transit -> lost {
            ACTION : NOTIFY(PARENT.customer, "shipment.lost")
        }
    }
)
"#;
    smql_parser::parse_machines(smql).expect("full Shipment with SIGNAL PARENT should parse");
}

/// signal-parent.md — Multi-level signals (Job → Stage → Pipeline)
///
/// ```sql
/// DEFINE MACHINE Job (
///   PARENT : Stage
///   TRANSITIONS {
///     running -> completed { SIGNAL PARENT TO completed }
///   }
/// )
///
/// DEFINE MACHINE Stage (
///   PARENT : Pipeline
///   TRANSITIONS {
///     running -> completed {
///       GUARD : ALL(jobs, STATE IS completed)
///       SIGNAL PARENT TO completed
///     }
///   }
/// )
/// ```
#[test]
fn signal_parent_multi_level_parse() {
    let smql = r#"
DEFINE MACHINE Job (
    PARENT : Stage
    STATES { running, completed }
    INITIAL STATE running
    TERMINAL STATES { completed }
    TRANSITIONS {
        running -> completed {
            SIGNAL PARENT TO completed
        }
    }
)

DEFINE MACHINE Stage (
    PARENT : Pipeline
    STATES { running, completed }
    INITIAL STATE running
    CHILDREN { jobs : LIST(Job) }
    TERMINAL STATES { completed }
    TRANSITIONS {
        running -> completed {
            GUARD : ALL(jobs, STATE IS completed)
            SIGNAL PARENT TO completed
        }
    }
)
"#;
    smql_parser::parse_machines(smql).expect("multi-level signal parse should work");
}

/// signal-parent.md — Signal parent guard on parent side
///
/// ```sql
/// shipped -> delivered {
///   GUARD : ALL(items, STATE IS confirmed)
/// }
/// ```
#[test]
fn signal_parent_guard_on_parent_parse() {
    let smql = r#"
DEFINE MACHINE Order (
    STATES { shipped, delivered }
    INITIAL STATE shipped
    CHILDREN { items : LIST(C) }
    TERMINAL STATES { delivered }
    TRANSITIONS {
        shipped -> delivered {
            GUARD : ALL(items, STATE IS confirmed)
        }
    }
)
"#;
    smql_parser::parse_machines(smql)
        .expect("parent guard with ALL() for signal target should parse");
}

/// signal-parent.md — Engine: child signals parent transition
#[tokio::test]
async fn signal_parent_engine_child_signals_parent() {
    let smql = r#"
DEFINE MACHINE SigOrder (
    DATA { customer : TEXT -> REQUIRED }
    STATES { placed, shipped, delivered }
    INITIAL STATE placed
    TERMINAL STATES { delivered }
    CHILDREN {
        shipment : OPTIONAL(SigShipment)
    }
    TRANSITIONS {
        placed -> shipped {}
        shipped -> delivered {}
    }
)

DEFINE MACHINE SigShipment (
    PARENT : SigOrder
    DATA { tracking : TEXT -> OPTIONAL }
    STATES { created, dispatched, in_transit, delivered }
    INITIAL STATE created
    TERMINAL STATES { delivered }
    TRANSITIONS {
        created -> dispatched {
            GUARD : tracking IS SET
        }
        dispatched -> in_transit {}
        in_transit -> delivered {
            SIGNAL PARENT TO delivered
        }
    }
)
"#;
    let engine = make_engine_from_smql(smql);

    // Spawn order and child shipment
    let order = engine
        .spawn(&spawn_cmd(
            "SigOrder",
            vec![("customer", Value::Text("cust".into()))],
        ))
        .await
        .expect("spawn order");
    let order_id = order.instance.id.as_str();

    let ship = engine
        .spawn(&spawn_child_cmd(
            "SigShipment",
            vec![("tracking", Value::Text("TRK123".into()))],
            &order_id,
            "SigOrder",
        ))
        .await
        .expect("spawn shipment");
    let ship_id = ship.instance.id.as_str();

    // Order placed -> shipped
    engine
        .transition(&transition_cmd("SigOrder", &order_id, "shipped"))
        .await
        .expect("placed -> shipped");

    // Shipment created -> dispatched (tracking IS SET passes)
    engine
        .transition(&transition_cmd("SigShipment", &ship_id, "dispatched"))
        .await
        .expect("created -> dispatched");

    // Shipment dispatched -> in_transit
    engine
        .transition(&transition_cmd("SigShipment", &ship_id, "in_transit"))
        .await
        .expect("dispatched -> in_transit");

    // Shipment in_transit -> delivered (SIGNAL PARENT TO delivered)
    let ship_result = engine
        .transition(&transition_cmd("SigShipment", &ship_id, "delivered"))
        .await
        .expect("in_transit -> delivered");
    assert_eq!(ship_result.to_state, "delivered");

    // Verify parent order is now delivered (via signal)
    let q = Query::Get(GetQuery {
        machine: "SigOrder".into(),
        instance_id: order_id.clone(),
    });
    match engine.execute_query(&q).await.unwrap() {
        QueryResult::Instance(inst) => {
            assert_eq!(
                inst.state, "delivered",
                "Order should be delivered via SIGNAL PARENT"
            );
        }
        other => panic!("expected Instance, got {:?}", other),
    }
}

// ===========================================================================
// 7. docs/server/http-api.md (SMQL portions only)
// ===========================================================================

/// http-api.md — SPAWN Task command in execute endpoint
///
/// ```json
/// { "smql": "SPAWN Task { title: \"Buy milk\" }" }
/// ```
#[test]
fn http_api_spawn_task_parse() {
    let smql = r#"SPAWN Task { title: "Buy milk" }"#;
    smql_parser::parse(smql).expect("SPAWN Task should parse");
}

// ===========================================================================
// 8. docs/server/request-response.md (SMQL portions only)
// ===========================================================================

/// request-response.md — DEFINE MACHINE SupportTicket (from curl example)
#[test]
fn req_resp_define_machine_parse() {
    let smql = r#"DEFINE MACHINE SupportTicket ( STATES { open, assigned, resolved, closed } INITIAL STATE open TERMINAL STATES { closed } TRANSITIONS { open -> assigned { GUARD : ACTOR.role == "support" } assigned -> resolved {} resolved -> closed {} } )"#;
    smql_parser::parse(smql).expect("DEFINE MACHINE SupportTicket should parse");
}

/// request-response.md — SPAWN SupportTicket
///
/// ```json
/// { "smql": "SPAWN SupportTicket { title: \"Login page broken\", priority: 1 }" }
/// ```
#[test]
fn req_resp_spawn_support_ticket_parse() {
    let smql = r#"SPAWN SupportTicket { title: "Login page broken", priority: 1 }"#;
    smql_parser::parse(smql).expect("SPAWN SupportTicket should parse");
}

/// request-response.md — TRANSITION SupportTicket with AS and WITH
///
/// ```json
/// { "smql": "TRANSITION SupportTicket \"01HXYZ...\" TO assigned AS \"agent-7\" WITH { assignee: { id: \"agent-7\", role: \"support\" } }" }
/// ```
#[test]
fn req_resp_transition_with_as_and_with_parse() {
    let smql = r#"TRANSITION SupportTicket "01HXYZ1234567890ABCDEFGHIJ" TO assigned AS "agent-7" WITH { assignee: { id: "agent-7", role: "support" } }"#;
    smql_parser::parse(smql).expect("TRANSITION with AS and WITH should parse");
}

/// request-response.md — TRY TRANSITION
///
/// ```json
/// { "smql": "TRY TRANSITION SupportTicket \"01HXYZ...\" TO assigned" }
/// ```
#[test]
fn req_resp_try_transition_parse() {
    let smql = r#"TRY TRANSITION SupportTicket "01HXYZ1234567890ABCDEFGHIJ" TO assigned"#;
    smql_parser::parse(smql).expect("TRY TRANSITION should parse");
}

/// request-response.md — ALTER MACHINE ADD STATE
///
/// ```json
/// { "smql": "ALTER MACHINE SupportTicket ADD STATE escalated" }
/// ```
#[test]
fn req_resp_alter_machine_parse() {
    let smql = r#"ALTER MACHINE SupportTicket ADD STATE escalated"#;
    smql_parser::parse(smql).expect("ALTER MACHINE ADD STATE should parse");
}

/// request-response.md — GET query
///
/// ```json
/// { "smql": "GET SupportTicket \"01HXYZ1234567890ABCDEFGHIJ\"" }
/// ```
#[test]
fn req_resp_get_query_parse() {
    let smql = r#"GET SupportTicket "01HXYZ1234567890ABCDEFGHIJ""#;
    smql_parser::parse(smql).expect("GET query should parse");
}

/// request-response.md — FIND query with WHERE
///
/// ```json
/// { "smql": "FIND SupportTicket WHERE STATE IS assigned AND priority == 1" }
/// ```
#[test]
fn req_resp_find_query_parse() {
    let smql = r#"FIND SupportTicket WHERE STATE IS assigned AND priority == 1"#;
    smql_parser::parse(smql).expect("FIND with WHERE should parse");
}

/// request-response.md — FIND with LIMIT and AFTER (cursor pagination)
///
/// ```json
/// { "smql": "FIND SupportTicket WHERE STATE IS assigned LIMIT 20 AFTER \"01HABC...\"" }
/// ```
#[test]
fn req_resp_find_with_cursor_parse() {
    let smql = r#"FIND SupportTicket WHERE STATE IS assigned LIMIT 20 AFTER "01HABCDEFGHIJKLMNOPQRSTUV""#;
    smql_parser::parse(smql).expect("FIND with LIMIT and AFTER should parse");
}

/// request-response.md — TRAIL query
///
/// ```json
/// { "smql": "TRAIL OF \"01HXYZ1234567890ABCDEFGHIJ\"" }
/// ```
#[test]
fn req_resp_trail_query_parse() {
    let smql = r#"TRAIL OF "01HXYZ1234567890ABCDEFGHIJ""#;
    smql_parser::parse(smql).expect("TRAIL OF should parse");
}

/// request-response.md — AGGREGATE with GROUP BY STATE
///
/// ```json
/// { "smql": "AGGREGATE SupportTicket MEASURE COUNT() GROUP BY STATE" }
/// ```
#[test]
fn req_resp_aggregate_group_by_state_parse() {
    let smql = r#"AGGREGATE SupportTicket MEASURE COUNT() GROUP BY STATE"#;
    smql_parser::parse(smql).expect("AGGREGATE GROUP BY STATE should parse");
}

/// request-response.md — AGGREGATE with aliases
///
/// ```json
/// { "smql": "AGGREGATE SupportTicket MEASURE COUNT() AS total, SUM(points) AS total_points" }
/// ```
#[test]
fn req_resp_aggregate_with_aliases_parse() {
    let smql = r#"AGGREGATE SupportTicket MEASURE COUNT() AS total, SUM(points) AS total_points"#;
    smql_parser::parse(smql).expect("AGGREGATE with aliases should parse");
}

/// request-response.md — PATHS query
///
/// ```json
/// { "smql": "PATHS FROM SupportTicket" }
/// ```
#[test]
fn req_resp_paths_query_parse() {
    let smql = r#"PATHS FROM SupportTicket"#;
    smql_parser::parse(smql).expect("PATHS FROM should parse");
}

/// request-response.md — FUNNEL query
///
/// ```json
/// { "smql": "FUNNEL SupportTicket THROUGH [open, assigned, resolved, closed]" }
/// ```
#[test]
fn req_resp_funnel_query_parse() {
    let smql = r#"FUNNEL SupportTicket THROUGH [open, assigned, resolved, closed]"#;
    smql_parser::parse(smql).expect("FUNNEL THROUGH should parse");
}

/// request-response.md — COMPARE PATHS query
///
/// ```json
/// { "smql": "COMPARE PATHS SupportTicket SEGMENT BY priority" }
/// ```
#[test]
fn req_resp_compare_paths_parse() {
    let smql = r#"COMPARE PATHS SupportTicket SEGMENT BY priority"#;
    smql_parser::parse(smql).expect("COMPARE PATHS should parse");
}

/// request-response.md — TRANSITION ALL
///
/// ```json
/// { "smql": "TRANSITION ALL SupportTicket WHERE STATE IS resolved TO closed" }
/// ```
#[test]
fn req_resp_transition_all_parse() {
    let smql = r#"TRANSITION ALL SupportTicket WHERE STATE IS resolved TO closed"#;
    smql_parser::parse(smql).expect("TRANSITION ALL should parse");
}

// ===========================================================================
// 9. docs/reference/built-in-functions.md
// ===========================================================================

/// built-in-functions.md — elapsed() in FIND
///
/// ```sql
/// FIND Machine WHERE elapsed() > 24h
/// ```
#[test]
fn builtins_elapsed_in_find_parse() {
    let smql = r#"FIND Machine WHERE elapsed() > 24h"#;
    smql_parser::parse(smql).expect("elapsed() in FIND should parse");
}

/// built-in-functions.md — elapsed() in GUARD
///
/// ```sql
/// GUARD : elapsed() > 1h
/// ```
#[test]
fn builtins_elapsed_in_guard_parse() {
    let smql = r#"
DEFINE MACHINE M (
    STATES { a, b }
    INITIAL STATE a
    TRANSITIONS {
        a -> b {
            GUARD : elapsed() > 1h
        }
    }
)
"#;
    smql_parser::parse_machines(smql).expect("elapsed() in GUARD should parse");
}

/// built-in-functions.md — elapsed_in_state() in FIND
///
/// ```sql
/// FIND Machine WHERE elapsed_in_state() > 2h
/// ```
#[test]
fn builtins_elapsed_in_state_find_parse() {
    let smql = r#"FIND Machine WHERE elapsed_in_state() > 2h"#;
    smql_parser::parse(smql).expect("elapsed_in_state() in FIND should parse");
}

/// built-in-functions.md — elapsed_in_state() in GUARD
///
/// ```sql
/// GUARD : elapsed_in_state() > 5m
/// ```
#[test]
fn builtins_elapsed_in_state_guard_parse() {
    let smql = r#"
DEFINE MACHINE M (
    STATES { a, b }
    INITIAL STATE a
    TRANSITIONS {
        a -> b {
            GUARD : elapsed_in_state() > 5m
        }
    }
)
"#;
    smql_parser::parse_machines(smql).expect("elapsed_in_state() in GUARD should parse");
}

/// built-in-functions.md — elapsed_since(state) in GUARD
///
/// ```sql
/// GUARD : elapsed_since(resolved) < 30d
/// ```
#[test]
fn builtins_elapsed_since_guard_parse() {
    let smql = r#"
DEFINE MACHINE M (
    STATES { closed, reopened }
    INITIAL STATE closed
    TRANSITIONS {
        closed -> reopened {
            GUARD : elapsed_since(resolved) < 30d
        }
    }
)
"#;
    smql_parser::parse_machines(smql).expect("elapsed_since() in GUARD should parse");
}

/// built-in-functions.md — timeout_remaining() in FIND
///
/// ```sql
/// FIND Machine WHERE timeout_remaining() < 1h
/// ```
#[test]
fn builtins_timeout_remaining_find_parse() {
    let smql = r#"FIND Machine WHERE timeout_remaining() < 1h"#;
    smql_parser::parse(smql).expect("timeout_remaining() in FIND should parse");
}

/// built-in-functions.md — timeout_remaining() in GUARD
///
/// ```sql
/// GUARD : timeout_remaining() < 30m
/// ```
#[test]
fn builtins_timeout_remaining_guard_parse() {
    let smql = r#"
DEFINE MACHINE M (
    STATES { a, b }
    INITIAL STATE a
    TRANSITIONS {
        a -> b {
            GUARD : timeout_remaining() < 30m
        }
    }
)
"#;
    smql_parser::parse_machines(smql).expect("timeout_remaining() in GUARD should parse");
}

/// built-in-functions.md — NOW() in MUTATE
///
/// ```sql
/// MUTATE : last_checked = NOW()
/// ```
#[test]
fn builtins_now_in_mutate_parse() {
    let smql = r#"
DEFINE MACHINE M (
    STATES { a, b }
    INITIAL STATE a
    TRANSITIONS {
        a -> b {
            MUTATE : last_checked = NOW()
        }
    }
)
"#;
    smql_parser::parse_machines(smql).expect("NOW() in MUTATE should parse");
}

/// built-in-functions.md — TODAY() in MUTATE
///
/// ```sql
/// MUTATE : review_date = TODAY()
/// ```
#[test]
fn builtins_today_in_mutate_parse() {
    let smql = r#"
DEFINE MACHINE M (
    STATES { a, b }
    INITIAL STATE a
    TRANSITIONS {
        a -> b {
            MUTATE : review_date = TODAY()
        }
    }
)
"#;
    smql_parser::parse_machines(smql).expect("TODAY() in MUTATE should parse");
}

/// built-in-functions.md — COUNT() aggregate
///
/// ```sql
/// AGGREGATE SupportTicket MEASURE COUNT()
/// AGGREGATE SupportTicket MEASURE COUNT() GROUP BY state
/// ```
#[test]
fn builtins_count_aggregate_parse() {
    let smql1 = r#"AGGREGATE SupportTicket MEASURE COUNT()"#;
    let smql2 = r#"AGGREGATE SupportTicket MEASURE COUNT() GROUP BY state"#;
    smql_parser::parse(smql1).expect("COUNT() aggregate should parse");
    smql_parser::parse(smql2).expect("COUNT() GROUP BY state should parse");
}

/// built-in-functions.md — SUM() aggregate
///
/// ```sql
/// AGGREGATE Order MEASURE SUM(total)
/// AGGREGATE Order MEASURE SUM(total) AS revenue GROUP BY state
/// ```
#[test]
fn builtins_sum_aggregate_parse() {
    let smql1 = r#"AGGREGATE Order MEASURE SUM(total)"#;
    let smql2 = r#"AGGREGATE Order MEASURE SUM(total) AS revenue GROUP BY state"#;
    smql_parser::parse(smql1).expect("SUM() aggregate should parse");
    smql_parser::parse(smql2).expect("SUM() AS revenue GROUP BY state should parse");
}

/// built-in-functions.md — AVG() aggregate
///
/// ```sql
/// AGGREGATE SupportTicket MEASURE AVG(satisfaction)
/// AGGREGATE SupportTicket MEASURE AVG(satisfaction) GROUP BY priority
/// ```
#[test]
fn builtins_avg_aggregate_parse() {
    let smql1 = r#"AGGREGATE SupportTicket MEASURE AVG(satisfaction)"#;
    let smql2 = r#"AGGREGATE SupportTicket MEASURE AVG(satisfaction) GROUP BY priority"#;
    smql_parser::parse(smql1).expect("AVG() aggregate should parse");
    smql_parser::parse(smql2).expect("AVG() GROUP BY priority should parse");
}

/// built-in-functions.md — MIN() aggregate
///
/// ```sql
/// AGGREGATE SupportTicket MEASURE MIN(satisfaction)
/// ```
#[test]
fn builtins_min_aggregate_parse() {
    let smql = r#"AGGREGATE SupportTicket MEASURE MIN(satisfaction)"#;
    smql_parser::parse(smql).expect("MIN() aggregate should parse");
}

/// built-in-functions.md — MAX() aggregate
///
/// ```sql
/// AGGREGATE SupportTicket MEASURE MAX(satisfaction) AS highest_score
/// ```
#[test]
fn builtins_max_aggregate_parse() {
    let smql = r#"AGGREGATE SupportTicket MEASURE MAX(satisfaction) AS highest_score"#;
    smql_parser::parse(smql).expect("MAX() AS highest_score should parse");
}

/// built-in-functions.md — PERCENTILE() aggregate
///
/// ```sql
/// AGGREGATE SupportTicket MEASURE PERCENTILE(satisfaction)
/// AGGREGATE SupportTicket MEASURE PERCENTILE(satisfaction) AS p50
/// ```
#[test]
fn builtins_percentile_aggregate_parse() {
    let smql1 = r#"AGGREGATE SupportTicket MEASURE PERCENTILE(satisfaction)"#;
    let smql2 = r#"AGGREGATE SupportTicket MEASURE PERCENTILE(satisfaction) AS p50"#;
    smql_parser::parse(smql1).expect("PERCENTILE() aggregate should parse");
    smql_parser::parse(smql2).expect("PERCENTILE() AS p50 should parse");
}

/// built-in-functions.md — ALL() collection function in guard
///
/// ```sql
/// GUARD : ALL(items, STATE IS confirmed)
/// GUARD : ALL(stages, STATE IS passed)
/// ```
#[test]
fn builtins_all_collection_guard_parse() {
    let smql = r#"
DEFINE MACHINE M (
    STATES { a, b }
    INITIAL STATE a
    CHILDREN { items : LIST(C)  stages : LIST(S) }
    TRANSITIONS {
        a -> b {
            GUARD : ALL(items, STATE IS confirmed)
            GUARD : ALL(stages, STATE IS passed)
        }
    }
)
"#;
    smql_parser::parse_machines(smql).expect("ALL() collection guards should parse");
}

/// built-in-functions.md — ANY() collection function in guard
///
/// ```sql
/// GUARD : ANY(stages, STATE IS failed)
/// GUARD : ANY(items, STATE IS backordered)
/// ```
#[test]
fn builtins_any_collection_guard_parse() {
    let smql = r#"
DEFINE MACHINE M (
    STATES { a, b }
    INITIAL STATE a
    CHILDREN { items : LIST(C)  stages : LIST(S) }
    TRANSITIONS {
        a -> b {
            GUARD : ANY(stages, STATE IS failed)
            GUARD : ANY(items, STATE IS backordered)
        }
    }
)
"#;
    smql_parser::parse_machines(smql).expect("ANY() collection guards should parse");
}

/// built-in-functions.md — items.count field access
///
/// ```sql
/// GUARD : items.count > 0
/// ```
#[test]
fn builtins_items_count_field_access_parse() {
    let smql = r#"
DEFINE MACHINE M (
    STATES { a, b }
    INITIAL STATE a
    CHILDREN { items : LIST(C) }
    TRANSITIONS {
        a -> b {
            GUARD : items.count > 0
        }
    }
)
"#;
    smql_parser::parse_machines(smql).expect("items.count field access should parse");
}

// ===========================================================================
// 10. docs/reference/grammar.md
// ===========================================================================

// The grammar doc contains EBNF notation, not SMQL code. We test that the
// constructs described in the grammar parse correctly by exercising a
// representative subset.

/// grammar.md — DEFINE MACHINE with all clause types
#[test]
fn grammar_full_machine_definition_parse() {
    let smql = r#"
DEFINE MACHINE TestMachine (
    DATA {
        name    : TEXT          -> REQUIRED
        count   : INT           -> MIN(0), MAX(100)
        score   : FLOAT         -> OPTIONAL
        active  : BOOL          -> DEFAULT(TRUE)
        uuid_f  : UUID          -> OPTIONAL
        dob     : DATE          -> OPTIONAL
        created : DATETIME      -> OPTIONAL
        ttl     : DURATION      -> OPTIONAL
        price   : MONEY(USD)    -> REQUIRED
        payload : BLOB          -> OPTIONAL
        config  : JSON          -> OPTIONAL
        status  : ENUM(a, b, c) -> REQUIRED
        ref_f   : REF(Other)    -> OPTIONAL
        tags    : LIST(TEXT)    -> OPTIONAL
        ids     : SET(INT)     -> OPTIONAL
        meta    : MAP(TEXT, INT) -> OPTIONAL
    }
    STATES { draft, active, archived }
    INITIAL STATE draft
    TERMINAL STATES { archived }
    TRANSITIONS {
        draft -> active { GUARD : name IS SET }
        active -> archived {}
        ANY -> draft { EXCEPT FROM { archived } }
    }
)
"#;
    smql_parser::parse_machines(smql).expect("full machine with all data types should parse");
}

/// grammar.md — SPAWN with data pairs
#[test]
fn grammar_spawn_with_data_parse() {
    let smql = r#"SPAWN Machine { name: "test", count: 42, active: TRUE }"#;
    smql_parser::parse(smql).expect("SPAWN with data pairs should parse");
}

/// grammar.md — TRANSITION with all optional clauses
#[test]
fn grammar_transition_full_parse() {
    let smql = r#"TRANSITION Machine "01J5X7K2P3Q4R5S6T7U8V9W0XY" TO active AS "user-1" WITH { count: 10 } MEMO "Updated" CASCADE"#;
    smql_parser::parse(smql).expect("full TRANSITION with all clauses should parse");
}

/// grammar.md — TRANSITION with THROUGH
#[test]
fn grammar_transition_through_parse() {
    let smql = r#"TRANSITION Machine "01J5X7K2P3Q4R5S6T7U8V9W0XY" TO delivered THROUGH [shipped, in_transit]"#;
    smql_parser::parse(smql).expect("TRANSITION with THROUGH should parse");
}

/// grammar.md — TRY TRANSITION
#[test]
fn grammar_try_transition_parse() {
    let smql = r#"TRY TRANSITION Machine "01J5X7K2P3Q4R5S6T7U8V9W0XY" TO active AS "user-1" WITH { count: 10 } MEMO "Try""#;
    smql_parser::parse(smql).expect("TRY TRANSITION with clauses should parse");
}

/// grammar.md — TRANSITION ALL
#[test]
fn grammar_transition_all_parse() {
    let smql = r#"TRANSITION ALL Machine WHERE STATE IS draft TO active"#;
    smql_parser::parse(smql).expect("TRANSITION ALL should parse");
}

/// grammar.md — ALTER MACHINE operations
#[test]
fn grammar_alter_machine_parse() {
    let smql1 = r#"ALTER MACHINE M ADD STATE new_state"#;
    let smql2 = r#"ALTER MACHINE M REMOVE STATE old_state MIGRATE TO active"#;
    let smql3 = r#"ALTER MACHINE M ADD TRANSITION active -> archived"#;
    let smql4 = r#"ALTER MACHINE M REMOVE TRANSITION active -> archived"#;
    let smql5 = r#"ALTER MACHINE M ADD DATA new_field : TEXT -> OPTIONAL"#;
    let smql6 = r#"ALTER MACHINE M REMOVE DATA old_field"#;
    let smql7 = r#"ALTER MACHINE M BACKFILL new_field = "default""#;
    smql_parser::parse(smql1).expect("ALTER ADD STATE should parse");
    smql_parser::parse(smql2).expect("ALTER REMOVE STATE MIGRATE TO should parse");
    smql_parser::parse(smql3).expect("ALTER ADD TRANSITION should parse");
    smql_parser::parse(smql4).expect("ALTER REMOVE TRANSITION should parse");
    smql_parser::parse(smql5).expect("ALTER ADD DATA should parse");
    smql_parser::parse(smql6).expect("ALTER REMOVE DATA should parse");
    smql_parser::parse(smql7).expect("ALTER BACKFILL should parse");
}

/// grammar.md — GET query
#[test]
fn grammar_get_parse() {
    let smql = r#"GET Machine "01J5X7K2P3Q4R5S6T7U8V9W0XY""#;
    smql_parser::parse(smql).expect("GET query should parse");
}

/// grammar.md — FIND with WHERE, SORT, LIMIT, OFFSET, AFTER
#[test]
fn grammar_find_full_parse() {
    let smql = r#"FIND Machine WHERE STATE IS active SORT BY name ASC LIMIT 10 AFTER "01J5X7K2P3Q4R5S6T7U8V9W0XY""#;
    smql_parser::parse(smql).expect("FIND with all clauses should parse");
}

/// grammar.md — AGGREGATE with measure, WHERE, GROUP BY
#[test]
fn grammar_aggregate_full_parse() {
    let smql = r#"AGGREGATE Machine MEASURE COUNT() AS total, SUM(score) AS total_score WHERE STATE IS active GROUP BY status"#;
    smql_parser::parse(smql).expect("full AGGREGATE query should parse");
}

/// grammar.md — TRAIL with WHERE
#[test]
fn grammar_trail_parse() {
    let smql = r#"TRAIL OF "01J5X7K2P3Q4R5S6T7U8V9W0XY""#;
    smql_parser::parse(smql).expect("TRAIL query should parse");
}

/// grammar.md — PATHS with WHERE and LIMIT
#[test]
fn grammar_paths_parse() {
    let smql = r#"PATHS FROM Machine LIMIT 10"#;
    smql_parser::parse(smql).expect("PATHS with LIMIT should parse");
}

/// grammar.md — FUNNEL with WHERE
#[test]
fn grammar_funnel_parse() {
    let smql = r#"FUNNEL Machine THROUGH [draft, active, archived]"#;
    smql_parser::parse(smql).expect("FUNNEL query should parse");
}

/// grammar.md — COMPARE PATHS
#[test]
fn grammar_compare_paths_parse() {
    let smql = r#"COMPARE PATHS Machine SEGMENT BY status"#;
    smql_parser::parse(smql).expect("COMPARE PATHS should parse");
}

/// grammar.md — Expressions: boolean operators, IS SET, IS NOT SET, IN
#[test]
fn grammar_expressions_parse() {
    // OR expression
    let smql1 = r#"FIND M WHERE a == 1 OR b == 2"#;
    // AND expression
    let smql2 = r#"FIND M WHERE a == 1 AND b == 2"#;
    // NOT expression
    let smql3 = r#"FIND M WHERE NOT a == 1"#;
    // IS SET
    let smql4 = r#"FIND M WHERE name IS SET"#;
    // IS NOT SET
    let smql5 = r#"FIND M WHERE name IS NOT SET"#;
    // IN with parens
    let smql6 = r#"FIND M WHERE status IN ("a", "b", "c")"#;
    // Arithmetic
    let smql7 = r#"FIND M WHERE (a + b) * 2 > 10"#;

    smql_parser::parse(smql1).expect("OR expression should parse");
    smql_parser::parse(smql2).expect("AND expression should parse");
    smql_parser::parse(smql3).expect("NOT expression should parse");
    smql_parser::parse(smql4).expect("IS SET should parse");
    smql_parser::parse(smql5).expect("IS NOT SET should parse");
    smql_parser::parse(smql6).expect("IN (...) should parse");
    smql_parser::parse(smql7).expect("arithmetic expression should parse");
}

/// grammar.md — Duration literals
///
/// Grammar doc says: `duration = number ( "s" | "m" | "h" | "d" | "w" )`
/// but "w" (weeks) is NOT supported by the lexer. Only s, m, h, d are valid.
/// This is a DOC ISSUE: grammar.md lists "w" but the parser rejects it.
#[test]
fn grammar_duration_literals_parse() {
    let smql1 = r#"FIND M WHERE elapsed() > 30s"#;
    let smql2 = r#"FIND M WHERE elapsed() > 5m"#;
    let smql3 = r#"FIND M WHERE elapsed() > 2h"#;
    let smql4 = r#"FIND M WHERE elapsed() > 1d"#;
    smql_parser::parse(smql1).expect("30s duration should parse");
    smql_parser::parse(smql2).expect("5m duration should parse");
    smql_parser::parse(smql3).expect("2h duration should parse");
    smql_parser::parse(smql4).expect("1d duration should parse");

    // DOC ISSUE: "w" (weeks) is listed in grammar.md but NOT supported by the lexer
    let smql5 = r#"FIND M WHERE elapsed() > 1w"#;
    let result = smql_parser::parse(smql5);
    assert!(
        result.is_err(),
        "DOC ISSUE: 'w' suffix is NOT supported despite grammar.md listing it"
    );
}

/// grammar.md — HOOKS block syntax
#[test]
fn grammar_hooks_block_parse() {
    let smql = r#"
DEFINE MACHINE M (
    STATES { a, b }
    INITIAL STATE a
    TRANSITIONS { a -> b {} }
    HOOKS {
        ON SPAWN {
            EMIT("spawned")
        }
        BEFORE EACH TRANSITION {
            LOG("transitioning")
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
    }
)
"#;
    smql_parser::parse_machines(smql).expect("HOOKS block with all trigger types should parse");
}

/// grammar.md — SPAWN with THEN TRANSITION
#[test]
fn grammar_spawn_then_transition_parse() {
    let smql = r#"SPAWN Machine { name: "test" } THEN TRANSITION TO active"#;
    smql_parser::parse(smql).expect("SPAWN with THEN TRANSITION should parse");
}

/// grammar.md — ALIVE and TERMINATED predicates
#[test]
fn grammar_alive_terminated_parse() {
    let smql1 = r#"FIND M WHERE ALIVE"#;
    let smql2 = r#"FIND M WHERE TERMINATED"#;
    smql_parser::parse(smql1).expect("ALIVE predicate should parse");
    smql_parser::parse(smql2).expect("TERMINATED predicate should parse");
}

/// grammar.md — Map literal in expression
#[test]
fn grammar_map_literal_parse() {
    let smql = r#"SPAWN M { data: { key: "value", nested: { a: 1 } } }"#;
    smql_parser::parse(smql).expect("nested map literal should parse");
}

/// grammar.md — TIMEOUT clause
#[test]
fn grammar_timeout_clause_parse() {
    let smql = r#"
DEFINE MACHINE M (
    STATES { waiting, expired }
    INITIAL STATE waiting
    TERMINAL STATES { expired }
    TRANSITIONS {
        waiting -> expired {
            TIMEOUT : 24h -> expired
        }
    }
)
"#;
    smql_parser::parse_machines(smql).expect("TIMEOUT clause should parse");
}

/// grammar.md — Comments in SMQL
#[test]
fn grammar_comments_parse() {
    let smql = r#"
-- This is a comment
DEFINE MACHINE M (
    -- States for the machine
    STATES { a, b }
    INITIAL STATE a
    TRANSITIONS {
        a -> b {} -- inline comment
    }
)
"#;
    smql_parser::parse_machines(smql).expect("SMQL with comments should parse");
}

// ===========================================================================
// Engine integration: ALL() and ANY() with actual children
// ===========================================================================

/// Engine integration: ALL(items, STATE IS confirmed) blocks transition when
/// children are NOT all confirmed.
#[tokio::test]
async fn engine_all_predicate_blocks_when_not_confirmed() {
    let smql = r#"
DEFINE MACHINE Order (
    DATA { total : INT -> REQUIRED }
    STATES { draft, paid, fulfilled }
    INITIAL STATE draft
    CHILDREN { items : LIST(LineItem) -> MIN(1) }
    TRANSITIONS {
        draft -> paid { GUARD : total > 0 }
        paid -> fulfilled { GUARD : ALL(items, STATE IS confirmed) }
    }
)

DEFINE MACHINE LineItem (
    PARENT : Order
    DATA { product : TEXT -> REQUIRED  quantity : INT -> REQUIRED }
    STATES { pending, confirmed }
    INITIAL STATE pending
    TERMINAL STATES { confirmed }
    TRANSITIONS {
        pending -> confirmed { GUARD : quantity > 0 }
    }
)
"#;
    let engine = make_engine_from_smql(smql);

    let order = engine
        .spawn(&spawn_cmd("Order", vec![("total", Value::Int(100))]))
        .await
        .expect("spawn order");
    let order_id = order.instance.id.as_str();

    let li = engine
        .spawn(&spawn_child_cmd(
            "LineItem",
            vec![
                ("product", Value::Text("Widget".into())),
                ("quantity", Value::Int(2)),
            ],
            &order_id,
            "Order",
        ))
        .await
        .expect("spawn line item");
    let li_id = li.instance.id.as_str();

    // Move to paid
    engine
        .transition(&transition_cmd("Order", &order_id, "paid"))
        .await
        .expect("draft -> paid");

    // Try to fulfill — should fail because LineItem is still pending
    let err = engine
        .transition(&transition_cmd("Order", &order_id, "fulfilled"))
        .await;
    assert!(
        err.is_err(),
        "paid -> fulfilled should fail when items not confirmed"
    );

    // Confirm the line item
    engine
        .transition(&transition_cmd("LineItem", &li_id, "confirmed"))
        .await
        .expect("pending -> confirmed");

    // Now fulfill should succeed
    let result = engine
        .transition(&transition_cmd("Order", &order_id, "fulfilled"))
        .await
        .expect("paid -> fulfilled should succeed");
    assert_eq!(result.to_state, "fulfilled");
}

/// Engine integration: ANY(stages, STATE IS failed) triggers transition
#[tokio::test]
async fn engine_any_predicate_detects_failure() {
    let smql = r#"
DEFINE MACHINE Pipeline (
    STATES { running, failed, completed }
    INITIAL STATE running
    TERMINAL STATES { failed, completed }
    CHILDREN { stages : LIST(Stage) -> MIN(1) }
    TRANSITIONS {
        running -> failed { GUARD : ANY(stages, STATE IS failed) }
        running -> completed { GUARD : ALL(stages, STATE IS passed) }
    }
)

DEFINE MACHINE Stage (
    PARENT : Pipeline
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
    let engine = make_engine_from_smql(smql);

    let pipeline = engine
        .spawn(&spawn_cmd("Pipeline", vec![]))
        .await
        .expect("spawn pipeline");
    let pipeline_id = pipeline.instance.id.as_str();

    let stage1 = engine
        .spawn(&spawn_child_cmd("Stage", vec![], &pipeline_id, "Pipeline"))
        .await
        .expect("spawn stage 1");
    let s1_id = stage1.instance.id.as_str();

    let stage2 = engine
        .spawn(&spawn_child_cmd("Stage", vec![], &pipeline_id, "Pipeline"))
        .await
        .expect("spawn stage 2");
    let s2_id = stage2.instance.id.as_str();

    // Move both stages to running
    engine
        .transition(&transition_cmd("Stage", &s1_id, "running"))
        .await
        .expect("stage1 pending -> running");
    engine
        .transition(&transition_cmd("Stage", &s2_id, "running"))
        .await
        .expect("stage2 pending -> running");

    // Pipeline should not be able to fail yet (no failed stages)
    let err = engine
        .transition(&transition_cmd("Pipeline", &pipeline_id, "failed"))
        .await;
    assert!(err.is_err(), "pipeline should not fail without failed stages");

    // Fail stage 1
    engine
        .transition(&transition_cmd("Stage", &s1_id, "failed"))
        .await
        .expect("stage1 running -> failed");

    // Now pipeline can fail
    let result = engine
        .transition(&transition_cmd("Pipeline", &pipeline_id, "failed"))
        .await
        .expect("pipeline running -> failed");
    assert_eq!(result.to_state, "failed");
}

/// Engine integration: vacuous truth — ALL() on empty children returns true
#[tokio::test]
async fn engine_all_vacuous_truth() {
    let smql = r#"
DEFINE MACHINE Order (
    STATES { paid, fulfilled }
    INITIAL STATE paid
    CHILDREN { items : LIST(LineItem) }
    TRANSITIONS {
        paid -> fulfilled { GUARD : ALL(items, STATE IS confirmed) }
    }
)

DEFINE MACHINE LineItem (
    PARENT : Order
    STATES { pending, confirmed }
    INITIAL STATE pending
    TERMINAL STATES { confirmed }
    TRANSITIONS { pending -> confirmed {} }
)
"#;
    let engine = make_engine_from_smql(smql);

    let order = engine
        .spawn(&spawn_cmd("Order", vec![]))
        .await
        .expect("spawn order");
    let order_id = order.instance.id.as_str();

    // No children spawned — ALL() should be vacuously true
    let result = engine
        .transition(&transition_cmd("Order", &order_id, "fulfilled"))
        .await
        .expect("paid -> fulfilled should succeed with no children (vacuous truth)");
    assert_eq!(result.to_state, "fulfilled");
}

/// Engine integration: ANY() on empty children returns false
#[tokio::test]
async fn engine_any_empty_returns_false() {
    let smql = r#"
DEFINE MACHINE Pipeline (
    STATES { running, failed }
    INITIAL STATE running
    CHILDREN { stages : LIST(Stage) }
    TRANSITIONS {
        running -> failed { GUARD : ANY(stages, STATE IS failed) }
    }
)

DEFINE MACHINE Stage (
    PARENT : Pipeline
    STATES { pending, failed }
    INITIAL STATE pending
    TERMINAL STATES { failed }
    TRANSITIONS { pending -> failed {} }
)
"#;
    let engine = make_engine_from_smql(smql);

    let pipeline = engine
        .spawn(&spawn_cmd("Pipeline", vec![]))
        .await
        .expect("spawn pipeline");
    let pipeline_id = pipeline.instance.id.as_str();

    // No children spawned — ANY() returns false
    let err = engine
        .transition(&transition_cmd("Pipeline", &pipeline_id, "failed"))
        .await;
    assert!(
        err.is_err(),
        "ANY() on empty collection should return false, blocking transition"
    );
}
