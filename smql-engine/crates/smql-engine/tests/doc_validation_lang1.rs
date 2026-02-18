/// Doc validation tests for language docs (Phase 1):
///   1. define-machine.md
///   2. states-and-transitions.md
///   3. guards.md
///   4. mutations.md
///   5. actions.md
///
/// Each test parses (and where possible, executes) SMQL examples from the docs.
/// Fragment examples (partial blocks) are noted but tested only as part of
/// complete machine definitions.

use std::sync::Arc;

use smql_catalog::MachineCatalog;
use smql_engine_core::Engine;
use smql_hooks::{EventBus, HookExecutor};
use smql_storage::MemoryStorage;
use smql_timer::TimerManager;

// ---------------------------------------------------------------------------
// Helper: build engine from SMQL input
// ---------------------------------------------------------------------------
fn make_engine(smql: &str) -> Engine {
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

// ===========================================================================
// 1. define-machine.md
// ===========================================================================

/// Doc example: define-machine.md — Syntax overview (skeleton)
/// This is the high-level syntax template. It contains placeholder `...` so
/// it cannot be parsed as-is. We skip it and note it as a fragment.
#[test]
fn define_machine_syntax_overview_is_fragment() {
    // Fragment — contains `...` placeholders, not parseable
    // DEFINE MACHINE MachineName ( DATA { ... } STATES { ... } ... )
    // Noted as fragment — no failure.
}

/// Doc example: define-machine.md — DATA block (fragment)
/// DATA { customer_id : UUID -> REQUIRED ... } — standalone DATA block is
/// not a valid top-level statement.
#[test]
fn define_machine_data_block_is_fragment() {
    // Fragment: standalone DATA { ... } is not a top-level statement.
    // Noted.
}

/// Doc example: define-machine.md — STATES block (fragment)
#[test]
fn define_machine_states_block_is_fragment() {
    // Fragment: standalone STATES { ... } is not a top-level statement.
    // Noted.
}

/// Doc example: define-machine.md — CHILDREN block (fragment)
#[test]
fn define_machine_children_block_is_fragment() {
    // Fragment: standalone CHILDREN { ... } is not a top-level statement.
    // Noted.
}

/// Doc example: define-machine.md — PARENT declaration (fragment with `...`)
/// ```sql
/// DEFINE MACHINE LineItem (
///   PARENT : Order
///   ...
/// )
/// ```
/// Contains `...` placeholder — not parseable as-is.
#[test]
fn define_machine_parent_declaration_is_fragment() {
    // Fragment due to `...` placeholder.
    // Noted.
}

/// Doc example: define-machine.md — Complete SupportTicket definition
/// Tests both parsing and engine registration.
#[test]
fn define_machine_complete_support_ticket_parse() {
    let smql = r#"
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

  STATES { open, triaged, in_progress, waiting_on_customer, resolved, closed, reopened }
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

    in_progress -> resolved {
      GUARD  : resolution_note IS SET
      GUARD  : ACTOR == assignee OR ACTOR.role == "admin"
      TIMEOUT: 7d -> closed
      ACTION : NOTIFY(customer_id, "ticket.resolved")
    }

    resolved -> closed {
      GUARD : elapsed_since(resolved) >= 7d OR ACTOR.role == "admin"
    }
  }
)
"#;
    let result = smql_parser::parse_machines(smql);
    match &result {
        Ok(machines) => {
            assert_eq!(machines.len(), 1);
            assert_eq!(machines[0].name, "SupportTicket");
        }
        Err(e) => panic!("Failed to parse define-machine.md complete example: {}", e),
    }
}

/// Doc example: define-machine.md — Complete SupportTicket through engine
#[tokio::test]
async fn define_machine_complete_support_ticket_engine() {
    let smql = r#"
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

  STATES { open, triaged, in_progress, waiting_on_customer, resolved, closed, reopened }
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

    in_progress -> resolved {
      GUARD  : resolution_note IS SET
      GUARD  : ACTOR == assignee OR ACTOR.role == "admin"
      TIMEOUT: 7d -> closed
      ACTION : NOTIFY(customer_id, "ticket.resolved")
    }

    resolved -> closed {
      GUARD : elapsed_since(resolved) >= 7d OR ACTOR.role == "admin"
    }
  }
)
"#;
    let _engine = make_engine(smql);
    // Engine successfully registered the machine — no panic.
}

// ===========================================================================
// 2. states-and-transitions.md
// ===========================================================================

/// Doc example: states-and-transitions.md — STATES block (fragment)
#[test]
fn states_transitions_states_block_fragment() {
    // Fragment: standalone STATES { ... } block.
    // Noted.
}

/// Doc example: states-and-transitions.md — INITIAL STATE (fragment)
#[test]
fn states_transitions_initial_state_fragment() {
    // Fragment: standalone INITIAL STATE open.
    // Noted.
}

/// Doc example: states-and-transitions.md — TERMINAL STATES single (fragment)
#[test]
fn states_transitions_terminal_states_single_fragment() {
    // Fragment: TERMINAL STATES { closed }
    // Noted.
}

/// Doc example: states-and-transitions.md — TERMINAL STATES multiple (fragment)
#[test]
fn states_transitions_terminal_states_multiple_fragment() {
    // Fragment: TERMINAL STATES { delivered, cancelled, returned }
    // Noted.
}

/// Doc example: states-and-transitions.md — basic transition syntax (fragment)
/// ```sql
/// TRANSITIONS {
///   source -> target {
///     GUARD   : <expression>
///     MUTATE  : <field> = <expression>
///     ACTION  : <action>
///     TIMEOUT : <duration> -> <state>
///   }
/// }
/// ```
/// Contains placeholders `<expression>`, `<action>`, etc.
#[test]
fn states_transitions_basic_syntax_fragment() {
    // Fragment — contains placeholders.
    // Noted.
}

/// Doc example: states-and-transitions.md — empty transition body
/// `pending -> running {}`
/// Testable inside a machine definition.
#[test]
fn states_transitions_empty_transition_body() {
    let smql = r#"
DEFINE MACHINE TestMachine (
  DATA {}
  STATES { pending, running }
  INITIAL STATE pending
  TERMINAL STATES { running }
  TRANSITIONS {
    pending -> running {}
  }
)
"#;
    let result = smql_parser::parse_machines(smql);
    match &result {
        Ok(machines) => {
            assert_eq!(machines.len(), 1);
            assert_eq!(machines[0].name, "TestMachine");
        }
        Err(e) => panic!("Failed to parse empty transition body: {}", e),
    }
}

/// Doc example: states-and-transitions.md — multiple guards
/// Wrap in a full machine so it can be parsed.
#[test]
fn states_transitions_multiple_guards() {
    let smql = r#"
DEFINE MACHINE TestMachine (
  DATA {
    resolution_note : TEXT -> OPTIONAL
    assignee : REF(Agent) -> OPTIONAL
  }
  STATES { in_progress, resolved }
  INITIAL STATE in_progress
  TERMINAL STATES { resolved }
  TRANSITIONS {
    in_progress -> resolved {
      GUARD : resolution_note IS SET
      GUARD : ACTOR == assignee OR ACTOR.role == "admin"
    }
  }
)
"#;
    let result = smql_parser::parse_machines(smql);
    match &result {
        Ok(machines) => {
            assert_eq!(machines.len(), 1);
        }
        Err(e) => panic!("Failed to parse multiple guards: {}", e),
    }
}

/// Doc example: states-and-transitions.md — comment in transition
/// `payment_failed -> placed { -- retry payment }`
#[test]
fn states_transitions_comment_in_transition() {
    let smql = r#"
DEFINE MACHINE TestMachine (
  DATA {}
  STATES { payment_failed, placed }
  INITIAL STATE payment_failed
  TERMINAL STATES { placed }
  TRANSITIONS {
    payment_failed -> placed {
      -- retry payment
    }
  }
)
"#;
    let result = smql_parser::parse_machines(smql);
    match &result {
        Ok(machines) => {
            assert_eq!(machines.len(), 1);
        }
        Err(e) => panic!("Failed to parse comment in transition: {}", e),
    }
}

// ===========================================================================
// 3. guards.md
// ===========================================================================

/// Doc example: guards.md — guard syntax (fragment)
/// ```sql
/// source -> target {
///   GUARD : <expression>
/// }
/// ```
/// Contains `<expression>` placeholder.
#[test]
fn guards_syntax_fragment() {
    // Fragment with placeholder.
    // Noted.
}

/// Doc example: guards.md — multiple guards ANDed
/// ```sql
/// open -> triaged {
///   GUARD : assignee IS SET
///   GUARD : priority != "low"
/// }
/// ```
#[test]
fn guards_multiple_guards_anded() {
    let smql = r#"
DEFINE MACHINE TestMachine (
  DATA {
    assignee : REF(Agent) -> OPTIONAL
    priority : ENUM(low, medium, high) -> DEFAULT(medium)
  }
  STATES { open, triaged }
  INITIAL STATE open
  TERMINAL STATES { triaged }
  TRANSITIONS {
    open -> triaged {
      GUARD : assignee IS SET
      GUARD : priority != "low"
    }
  }
)
"#;
    let result = smql_parser::parse_machines(smql);
    match &result {
        Ok(machines) => assert_eq!(machines.len(), 1),
        Err(e) => panic!("Failed to parse guards AND example: {}", e),
    }
}

/// Doc example: guards.md — referencing data fields (fragments)
/// ```sql
/// GUARD : priority == "critical"
/// GUARD : item_count > 0
/// GUARD : assignee IS SET
/// ```
/// These are individual guard lines, not top-level statements. Wrap in machine.
#[test]
fn guards_referencing_data_fields() {
    let smql = r#"
DEFINE MACHINE TestMachine (
  DATA {
    priority : ENUM(low, medium, high, critical) -> DEFAULT(medium)
    item_count : INT -> DEFAULT(0)
    assignee : REF(Agent) -> OPTIONAL
  }
  STATES { a, b, c, d }
  INITIAL STATE a
  TERMINAL STATES { d }
  TRANSITIONS {
    a -> b { GUARD : priority == "critical" }
    b -> c { GUARD : item_count > 0 }
    c -> d { GUARD : assignee IS SET }
  }
)
"#;
    let result = smql_parser::parse_machines(smql);
    match &result {
        Ok(machines) => assert_eq!(machines.len(), 1),
        Err(e) => panic!("Failed to parse guards referencing data: {}", e),
    }
}

/// Doc example: guards.md — ACTOR keyword examples
/// ```sql
/// GUARD : ACTOR == assignee
/// GUARD : ACTOR.role == "admin"
/// GUARD : ACTOR.id == customer_id
/// GUARD : ACTOR.role IN ("admin", "supervisor")
/// ```
#[test]
fn guards_actor_keyword() {
    let smql = r#"
DEFINE MACHINE TestMachine (
  DATA {
    assignee : REF(Agent) -> OPTIONAL
    customer_id : UUID -> OPTIONAL
  }
  STATES { a, b, c, d, e }
  INITIAL STATE a
  TERMINAL STATES { e }
  TRANSITIONS {
    a -> b { GUARD : ACTOR == assignee }
    b -> c { GUARD : ACTOR.role == "admin" }
    c -> d { GUARD : ACTOR.id == customer_id }
    d -> e { GUARD : ACTOR.role IN ("admin", "supervisor") }
  }
)
"#;
    let result = smql_parser::parse_machines(smql);
    match &result {
        Ok(machines) => assert_eq!(machines.len(), 1),
        Err(e) => panic!("Failed to parse ACTOR guards: {}", e),
    }
}

/// Doc example: guards.md — SELF keyword in ACTION
/// ```sql
/// ACTION : EMIT("order.placed", { order: SELF })
/// ```
#[test]
fn guards_self_keyword_in_action() {
    let smql = r#"
DEFINE MACHINE TestMachine (
  DATA {}
  STATES { a, b }
  INITIAL STATE a
  TERMINAL STATES { b }
  TRANSITIONS {
    a -> b {
      ACTION : EMIT("order.placed", { order: SELF })
    }
  }
)
"#;
    let result = smql_parser::parse_machines(smql);
    match &result {
        Ok(machines) => assert_eq!(machines.len(), 1),
        Err(e) => panic!("Failed to parse SELF keyword in action: {}", e),
    }
}

/// Doc example: guards.md — built-in functions in guards
/// ```sql
/// elapsed_since(resolved) < 30d   (from table)
/// len(tags) > 0                    (from table)
/// contains(tags, "urgent")         (from table)
/// now() - 7d (used as created_at < now() - 7d) (from table)
/// ```
/// Test `elapsed_since` and `len` in guard context.
#[test]
fn guards_builtin_functions() {
    let smql = r#"
DEFINE MACHINE TestMachine (
  DATA {
    tags : SET(TEXT) -> DEFAULT({})
  }
  STATES { a, b, c }
  INITIAL STATE a
  TERMINAL STATES { c }
  TRANSITIONS {
    a -> b {
      GUARD : len(tags) > 0
    }
    b -> c {
      GUARD : elapsed_since(a) >= 7d
    }
  }
)
"#;
    let result = smql_parser::parse_machines(smql);
    match &result {
        Ok(machines) => assert_eq!(machines.len(), 1),
        Err(e) => panic!("Failed to parse built-in functions in guards: {}", e),
    }
}

/// Doc example: guards.md — contains() function
#[test]
fn guards_contains_function() {
    let smql = r#"
DEFINE MACHINE TestMachine (
  DATA {
    tags : SET(TEXT) -> DEFAULT({})
  }
  STATES { a, b }
  INITIAL STATE a
  TERMINAL STATES { b }
  TRANSITIONS {
    a -> b {
      GUARD : contains(tags, "urgent")
    }
  }
)
"#;
    let result = smql_parser::parse_machines(smql);
    match &result {
        Ok(machines) => assert_eq!(machines.len(), 1),
        Err(e) => panic!("Failed to parse contains() guard: {}", e),
    }
}

/// Doc example: guards.md — child predicates ALL/ANY
/// ```sql
/// GUARD : ALL(items, STATE IS confirmed)
/// GUARD : ANY(stages, STATE IS failed)
/// ```
#[test]
fn guards_child_predicates_all_any() {
    let smql = r#"
DEFINE MACHINE TestMachine (
  DATA {}
  CHILDREN {
    items  : LIST(Item) -> MIN(1)
    stages : LIST(Stage) -> MIN(1)
  }
  STATES { a, b, c }
  INITIAL STATE a
  TERMINAL STATES { c }
  TRANSITIONS {
    a -> b { GUARD : ALL(items, STATE IS confirmed) }
    b -> c { GUARD : ANY(stages, STATE IS failed) }
  }
)
"#;
    let result = smql_parser::parse_machines(smql);
    match &result {
        Ok(machines) => assert_eq!(machines.len(), 1),
        Err(e) => panic!("Failed to parse ALL/ANY child predicates: {}", e),
    }
}

// ===========================================================================
// 4. mutations.md
// ===========================================================================

/// Doc example: mutations.md — MUTATE syntax (fragment)
/// ```sql
/// source -> target {
///   MUTATE : field_name = expression
/// }
/// ```
/// Contains placeholders.
#[test]
fn mutations_syntax_fragment() {
    // Fragment with placeholders.
    // Noted.
}

/// Doc example: mutations.md — Set a field
/// ```sql
/// ANY -> triaged {
///   MUTATE : priority = critical
/// }
/// ```
/// Uses ANY wildcard source.
#[test]
fn mutations_set_a_field() {
    let smql = r#"
DEFINE MACHINE TestMachine (
  DATA {
    priority : ENUM(low, medium, high, critical) -> DEFAULT(medium)
  }
  STATES { open, triaged }
  INITIAL STATE open
  TERMINAL STATES { triaged }
  TRANSITIONS {
    ANY -> triaged {
      MUTATE : priority = critical
    }
  }
)
"#;
    let result = smql_parser::parse_machines(smql);
    match &result {
        Ok(machines) => assert_eq!(machines.len(), 1),
        Err(e) => panic!("Failed to parse MUTATE set field: {}", e),
    }
}

/// Doc example: mutations.md — TRANSITION WITH external data
/// ```sql
/// TRANSITION SupportTicket "01J5..." TO resolved
///   WITH { resolution_note: "Fixed the issue" }
/// ```
#[test]
fn mutations_transition_with_data() {
    let smql = r#"TRANSITION SupportTicket "01J5ABCDEFGHIJKLMNOPQRSTUV" TO resolved WITH { resolution_note: "Fixed the issue" }"#;
    let result = smql_parser::parse(smql);
    match &result {
        Ok(stmts) => assert_eq!(stmts.len(), 1),
        Err(e) => panic!("Failed to parse TRANSITION WITH: {}", e),
    }
}

/// Doc example: mutations.md — SPAWN in MUTATE
/// ```sql
/// paid -> fulfilled {
///   MUTATE : shipment = SPAWN Shipment { order: SELF }
/// }
/// ```
#[test]
fn mutations_spawn_in_mutate() {
    let smql = r#"
DEFINE MACHINE TestMachine (
  DATA {
    shipment : REF(Shipment) -> OPTIONAL
  }
  STATES { paid, fulfilled }
  INITIAL STATE paid
  TERMINAL STATES { fulfilled }
  TRANSITIONS {
    paid -> fulfilled {
      MUTATE : shipment = SPAWN Shipment { order: SELF }
    }
  }
)
"#;
    let result = smql_parser::parse_machines(smql);
    match &result {
        Ok(machines) => assert_eq!(machines.len(), 1),
        Err(e) => panic!("Failed to parse SPAWN in MUTATE: {}", e),
    }
}

/// Doc example: mutations.md — second TRANSITION WITH example
/// ```sql
/// TRANSITION SupportTicket "01J5..." TO resolved
///   WITH { resolution_note: "Customer confirmed fix" }
/// ```
#[test]
fn mutations_transition_with_data_second() {
    let smql = r#"TRANSITION SupportTicket "01J5ABCDEFGHIJKLMNOPQRSTUV" TO resolved WITH { resolution_note: "Customer confirmed fix" }"#;
    let result = smql_parser::parse(smql);
    match &result {
        Ok(stmts) => assert_eq!(stmts.len(), 1),
        Err(e) => panic!("Failed to parse second TRANSITION WITH: {}", e),
    }
}

// ===========================================================================
// 5. actions.md
// ===========================================================================

/// Doc example: actions.md — ACTION syntax (fragment)
/// ```sql
/// source -> target {
///   ACTION : <action_call>
/// }
/// ```
/// Contains `<action_call>` placeholder.
#[test]
fn actions_syntax_fragment() {
    // Fragment with placeholder.
    // Noted.
}

/// Doc example: actions.md — LOG action
/// ```sql
/// ACTION : LOG("Ticket escalated by {ACTOR}")
/// ```
#[test]
fn actions_log() {
    let smql = r#"
DEFINE MACHINE TestMachine (
  DATA {}
  STATES { a, b }
  INITIAL STATE a
  TERMINAL STATES { b }
  TRANSITIONS {
    a -> b {
      ACTION : LOG("Ticket escalated by {ACTOR}")
    }
  }
)
"#;
    let result = smql_parser::parse_machines(smql);
    match &result {
        Ok(machines) => assert_eq!(machines.len(), 1),
        Err(e) => panic!("Failed to parse LOG action: {}", e),
    }
}

/// Doc example: actions.md — NOTIFY actions (3 variants)
/// ```sql
/// ACTION : NOTIFY(assignee, "ticket.assigned")
/// ACTION : NOTIFY(customer_id, "ticket.resolved")
/// ACTION : NOTIFY(PARENT.customer, "item.backordered")
/// ```
#[test]
fn actions_notify_variants() {
    let smql = r#"
DEFINE MACHINE TestMachine (
  DATA {
    assignee : REF(Agent) -> OPTIONAL
    customer_id : UUID -> OPTIONAL
  }
  STATES { a, b, c, d }
  INITIAL STATE a
  TERMINAL STATES { d }
  TRANSITIONS {
    a -> b { ACTION : NOTIFY(assignee, "ticket.assigned") }
    b -> c { ACTION : NOTIFY(customer_id, "ticket.resolved") }
    c -> d { ACTION : NOTIFY(PARENT.customer, "item.backordered") }
  }
)
"#;
    let result = smql_parser::parse_machines(smql);
    match &result {
        Ok(machines) => assert_eq!(machines.len(), 1),
        Err(e) => panic!("Failed to parse NOTIFY action variants: {}", e),
    }
}

/// Doc example: actions.md — EMIT actions
/// ```sql
/// ACTION : EMIT("order.placed")
/// ACTION : EMIT("order.placed", { order: SELF })
/// ```
#[test]
fn actions_emit_variants() {
    let smql = r#"
DEFINE MACHINE TestMachine (
  DATA {}
  STATES { a, b, c }
  INITIAL STATE a
  TERMINAL STATES { c }
  TRANSITIONS {
    a -> b { ACTION : EMIT("order.placed") }
    b -> c { ACTION : EMIT("order.placed", { order: SELF }) }
  }
)
"#;
    let result = smql_parser::parse_machines(smql);
    match &result {
        Ok(machines) => assert_eq!(machines.len(), 1),
        Err(e) => panic!("Failed to parse EMIT action variants: {}", e),
    }
}

/// Doc example: actions.md — WEBHOOK action
/// ```sql
/// ACTION : WEBHOOK("https://api.example.com/hooks", { event: "shipped" })
/// ```
#[test]
fn actions_webhook() {
    let smql = r#"
DEFINE MACHINE TestMachine (
  DATA {}
  STATES { a, b }
  INITIAL STATE a
  TERMINAL STATES { b }
  TRANSITIONS {
    a -> b {
      ACTION : WEBHOOK("https://api.example.com/hooks", { event: "shipped" })
    }
  }
)
"#;
    let result = smql_parser::parse_machines(smql);
    match &result {
        Ok(machines) => assert_eq!(machines.len(), 1),
        Err(e) => panic!("Failed to parse WEBHOOK action: {}", e),
    }
}

/// Doc example: actions.md — multiple actions in one transition
/// ```sql
/// in_progress -> resolved {
///   ACTION : NOTIFY(customer_id, "ticket.resolved")
///   ACTION : EMIT("ticket.resolved", { ticket: SELF })
///   ACTION : LOG("Resolved by {ACTOR}")
/// }
/// ```
#[test]
fn actions_multiple_in_transition() {
    let smql = r#"
DEFINE MACHINE TestMachine (
  DATA {
    customer_id : UUID -> OPTIONAL
  }
  STATES { in_progress, resolved }
  INITIAL STATE in_progress
  TERMINAL STATES { resolved }
  TRANSITIONS {
    in_progress -> resolved {
      ACTION : NOTIFY(customer_id, "ticket.resolved")
      ACTION : EMIT("ticket.resolved", { ticket: SELF })
      ACTION : LOG("Resolved by {ACTOR}")
    }
  }
)
"#;
    let result = smql_parser::parse_machines(smql);
    match &result {
        Ok(machines) => assert_eq!(machines.len(), 1),
        Err(e) => panic!("Failed to parse multiple actions: {}", e),
    }
}

// ===========================================================================
// Cross-doc: Full machine with guards, mutations, actions, timeouts
// ===========================================================================

/// Composite test: machine definition using features from all 5 docs
/// (DATA fields, STATES, TRANSITIONS with GUARD, MUTATE, ACTION, TIMEOUT)
#[test]
fn cross_doc_full_machine() {
    let smql = r#"
DEFINE MACHINE Order (
  DATA {
    customer_id : UUID -> REQUIRED
    total       : INT  -> REQUIRED
    status_note : TEXT -> OPTIONAL
    tags        : SET(TEXT) -> DEFAULT({})
  }
  STATES { placed, paid, shipped, delivered, cancelled }
  INITIAL STATE placed
  TERMINAL STATES { delivered, cancelled }
  TRANSITIONS {
    placed -> paid {
      GUARD  : total > 0
      MUTATE : status_note = "Payment received"
      ACTION : EMIT("order.paid")
    }
    paid -> shipped {
      GUARD   : ACTOR.role == "admin"
      TIMEOUT : 7d -> cancelled
      ACTION  : NOTIFY(customer_id, "order.shipped")
      ACTION  : LOG("Order shipped by {ACTOR}")
    }
    shipped -> delivered {}
    ANY -> cancelled {
      GUARD : ACTOR.role == "admin"
    }
  }
)
"#;
    let result = smql_parser::parse_machines(smql);
    match &result {
        Ok(machines) => {
            assert_eq!(machines.len(), 1);
            assert_eq!(machines[0].name, "Order");
        }
        Err(e) => panic!("Failed to parse cross-doc full machine: {}", e),
    }
}

/// Composite test: full machine through engine
#[tokio::test]
async fn cross_doc_full_machine_engine() {
    let smql = r#"
DEFINE MACHINE Order (
  DATA {
    customer_id : UUID -> REQUIRED
    total       : INT  -> REQUIRED
    status_note : TEXT -> OPTIONAL
  }
  STATES { placed, paid, shipped, delivered, cancelled }
  INITIAL STATE placed
  TERMINAL STATES { delivered, cancelled }
  TRANSITIONS {
    placed -> paid {
      GUARD  : total > 0
      MUTATE : status_note = "Payment received"
      ACTION : EMIT("order.paid")
    }
    paid -> shipped {
      GUARD   : ACTOR.role == "admin"
      TIMEOUT : 7d -> cancelled
      ACTION  : NOTIFY(customer_id, "order.shipped")
    }
    shipped -> delivered {}
    ANY -> cancelled {
      GUARD : ACTOR.role == "admin"
    }
  }
)
"#;
    let _engine = make_engine(smql);
    // Engine successfully registered — no panic.
}
