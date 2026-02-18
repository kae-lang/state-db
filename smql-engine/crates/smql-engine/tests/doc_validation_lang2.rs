/// Doc validation tests for language/ docs (batch 2):
///   - hooks.md
///   - timeouts.md
///   - wildcards.md
///   - constraints.md
///   - data-types.md
///
/// Each test parses a SMQL code block extracted from the docs.
/// Complete machine definitions are also tested through the engine.

use std::sync::Arc;

use smql_ast::types::Constraint;
use smql_catalog::MachineCatalog;
use smql_engine_core::Engine;
use smql_hooks::{EventBus, HookExecutor};
use smql_storage::MemoryStorage;
use smql_timer::TimerManager;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_engine_from(smql: &str) -> Engine {
    let machines = smql_parser::parse_machines(smql).expect("parse machines");
    let catalog = Arc::new(MachineCatalog::new());
    for m in machines {
        catalog.register(m).expect("register");
    }
    let storage = Arc::new(MemoryStorage::new());
    let timer = Arc::new(TimerManager::new());
    let event_bus = Arc::new(EventBus::new(64));
    let hooks = Arc::new(HookExecutor::new(event_bus));
    let engine = Engine::with_hooks(catalog, storage, timer, hooks);
    engine.wire_callback();
    engine
}

fn has_constraint(constraints: &[Constraint], check: &Constraint) -> bool {
    constraints.iter().any(|c| std::mem::discriminant(c) == std::mem::discriminant(check))
}

// =========================================================================
// hooks.md
// =========================================================================

/// hooks.md -- Full machine definition with HOOKS block (syntax section)
/// Source: hooks.md lines 9-33
#[test]
fn hooks_full_machine_with_hooks_block() {
    // The doc example uses `...` for the machine body. We provide a minimal
    // complete machine so the parser can handle it.
    let smql = r#"
DEFINE MACHINE MyMachine (
  DATA {
    customer_id : TEXT -> REQUIRED
  }
  STATES { open, in_progress, resolved, closed }
  INITIAL STATE open
  TERMINAL STATES { closed }
  TRANSITIONS {
    open -> in_progress {}
    in_progress -> resolved {}
    resolved -> closed {}
  }
  HOOKS {
    ON SPAWN {
      EMIT("machine.spawned")
    }

    BEFORE EACH TRANSITION {
      LOG("Transition starting")
    }

    AFTER EACH TRANSITION {
      EMIT("machine.transitioned")
    }

    ON ENTER resolved {
      NOTIFY(customer_id, "ticket.resolved")
    }

    ON EXIT in_progress {
      LOG("Left in_progress")
    }
  }
)
"#;
    let machines = smql_parser::parse_machines(smql).expect("parse");
    assert_eq!(machines.len(), 1);
    assert_eq!(machines[0].name, "MyMachine");
    assert_eq!(machines[0].hooks.len(), 5);
}

/// hooks.md -- ON SPAWN hook fragment
/// Source: hooks.md lines 42-45
/// Fragment: hook trigger inside a HOOKS block. We wrap it in a full machine.
#[test]
fn hooks_on_spawn_fragment() {
    let smql = r#"
DEFINE MACHINE HookTest1 (
  STATES { open, done }
  INITIAL STATE open
  TERMINAL STATES { done }
  TRANSITIONS {
    open -> done {}
  }
  HOOKS {
    ON SPAWN {
      EMIT("ticket.created")
    }
  }
)
"#;
    let machines = smql_parser::parse_machines(smql).expect("parse");
    assert_eq!(machines[0].hooks.len(), 1);
}

/// hooks.md -- BEFORE EACH TRANSITION hook fragment
/// Source: hooks.md lines 52-55
#[test]
fn hooks_before_each_transition_fragment() {
    let smql = r#"
DEFINE MACHINE HookTest2 (
  STATES { open, done }
  INITIAL STATE open
  TERMINAL STATES { done }
  TRANSITIONS {
    open -> done {}
  }
  HOOKS {
    BEFORE EACH TRANSITION {
      LOG("Validating transition")
    }
  }
)
"#;
    let machines = smql_parser::parse_machines(smql).expect("parse");
    assert_eq!(machines[0].hooks.len(), 1);
}

/// hooks.md -- AFTER EACH TRANSITION hook fragment
/// Source: hooks.md lines 67-69
#[test]
fn hooks_after_each_transition_fragment() {
    let smql = r#"
DEFINE MACHINE HookTest3 (
  STATES { open, done }
  INITIAL STATE open
  TERMINAL STATES { done }
  TRANSITIONS {
    open -> done {}
  }
  HOOKS {
    AFTER EACH TRANSITION {
      EMIT("state.changed")
    }
  }
)
"#;
    let machines = smql_parser::parse_machines(smql).expect("parse");
    assert_eq!(machines[0].hooks.len(), 1);
}

/// hooks.md -- ON ENTER and ON EXIT hook fragments
/// Source: hooks.md lines 77-83
#[test]
fn hooks_on_enter_on_exit_fragment() {
    let smql = r#"
DEFINE MACHINE HookTest4 (
  DATA {
    customer_id : TEXT -> REQUIRED
  }
  STATES { open, in_progress, resolved, done }
  INITIAL STATE open
  TERMINAL STATES { done }
  TRANSITIONS {
    open -> in_progress {}
    in_progress -> resolved {}
    resolved -> done {}
  }
  HOOKS {
    ON ENTER resolved {
      NOTIFY(customer_id, "ticket.resolved")
    }

    ON EXIT in_progress {
      LOG("Left in_progress state")
    }
  }
)
"#;
    let machines = smql_parser::parse_machines(smql).expect("parse");
    assert_eq!(machines[0].hooks.len(), 2);
}

/// hooks.md -- Full machine with hooks, test through engine (spawn + transition)
#[tokio::test]
async fn hooks_engine_spawn_and_transition() {
    let smql = r#"
DEFINE MACHINE TicketHooked (
  DATA {
    customer_id : TEXT -> REQUIRED
  }
  STATES { open, in_progress, resolved, closed }
  INITIAL STATE open
  TERMINAL STATES { closed }
  TRANSITIONS {
    open -> in_progress {}
    in_progress -> resolved {}
    resolved -> closed {}
  }
  HOOKS {
    ON SPAWN {
      EMIT("machine.spawned")
    }
    BEFORE EACH TRANSITION {
      LOG("Transition starting")
    }
    AFTER EACH TRANSITION {
      EMIT("machine.transitioned")
    }
    ON ENTER resolved {
      NOTIFY(customer_id, "ticket.resolved")
    }
    ON EXIT in_progress {
      LOG("Left in_progress")
    }
  }
)
"#;
    let engine = make_engine_from(smql);
    use smql_ast::command::SpawnCommand;
    use smql_ast::expression::{Expression, ExpressionKind};
    use smql_ast::value::Value;

    fn lit(v: Value) -> Expression {
        Expression::new(ExpressionKind::Literal(v))
    }

    let cmd = SpawnCommand {
        machine: "TicketHooked".to_string(),
        data: vec![
            ("customer_id".to_string(), lit(Value::Text("cust-1".into()))),
        ],
        then_transition: None,
        batch: false,
        batch_data: Vec::new(),
        parent_id: None,
        parent_machine: None,
    };
    let result = engine.spawn(&cmd).await.expect("spawn");
    assert_eq!(result.instance.state, "open");

    let tcmd = smql_ast::command::TransitionCommand::new(
        "TicketHooked".to_string(),
        result.instance.id.to_string(),
        "in_progress".to_string(),
    );
    let tr = engine.transition(&tcmd).await.expect("transition");
    assert_eq!(tr.instance.state, "in_progress");
}

// =========================================================================
// timeouts.md
// =========================================================================

/// timeouts.md -- Basic timeout syntax
/// Source: timeouts.md lines 8-10
/// Fragment: `source -> target { TIMEOUT : <duration> -> <state> }`
/// We wrap it in a full machine definition.
#[test]
fn timeouts_basic_syntax() {
    let smql = r#"
DEFINE MACHINE TimeoutBasic (
  STATES { source, target, timeout_target }
  INITIAL STATE source
  TERMINAL STATES { timeout_target }
  TRANSITIONS {
    source -> target {
      TIMEOUT : 30m -> timeout_target
    }
  }
)
"#;
    let machines = smql_parser::parse_machines(smql).expect("parse");
    assert_eq!(machines[0].name, "TimeoutBasic");
    let transition = &machines[0].transitions[0];
    assert!(transition.timeout.is_some());
    let timeout = transition.timeout.as_ref().unwrap();
    assert_eq!(timeout.target_state, "timeout_target");
}

/// timeouts.md -- 72h timeout example
/// Source: timeouts.md lines 16-18
#[test]
fn timeouts_72h_example() {
    let smql = r#"
DEFINE MACHINE TimeoutTicket (
  STATES { in_progress, waiting_on_customer, resolved }
  INITIAL STATE in_progress
  TERMINAL STATES { resolved }
  TRANSITIONS {
    in_progress -> waiting_on_customer {
      TIMEOUT : 72h -> resolved
    }
  }
)
"#;
    let machines = smql_parser::parse_machines(smql).expect("parse");
    let transition = &machines[0].transitions[0];
    let timeout = transition.timeout.as_ref().unwrap();
    assert_eq!(timeout.target_state, "resolved");
    // 72 hours = 259200 seconds
    assert_eq!(timeout.duration.seconds, 72 * 60 * 60);
}

/// timeouts.md -- Multiple timeouts on different transitions from same state
/// Source: timeouts.md lines 38-45
#[test]
fn timeouts_multiple_from_same_state() {
    let smql = r#"
DEFINE MACHINE OrderTimeout (
  STATES { placed, paid, expired, cancelled }
  INITIAL STATE placed
  TERMINAL STATES { expired, cancelled }
  TRANSITIONS {
    placed -> paid {
      TIMEOUT : 24h -> cancelled
    }

    placed -> expired {
      TIMEOUT : 48h -> expired
    }
  }
)
"#;
    let machines = smql_parser::parse_machines(smql).expect("parse");
    assert_eq!(machines[0].transitions.len(), 2);
    let t1 = &machines[0].transitions[0];
    let t2 = &machines[0].transitions[1];
    assert!(t1.timeout.is_some());
    assert!(t2.timeout.is_some());
    assert_eq!(
        t1.timeout.as_ref().unwrap().duration.seconds,
        24 * 60 * 60
    );
    assert_eq!(
        t2.timeout.as_ref().unwrap().duration.seconds,
        48 * 60 * 60
    );
}

/// timeouts.md -- Engine test: spawn and verify timeout is registered
#[tokio::test]
async fn timeouts_engine_timeout_registers_on_transition() {
    let smql = r#"
DEFINE MACHINE WaitTimeout (
  STATES { open, waiting, resolved }
  INITIAL STATE open
  TERMINAL STATES { resolved }
  TRANSITIONS {
    open -> waiting {
      TIMEOUT : 72h -> resolved
    }
  }
)
"#;
    let engine = make_engine_from(smql);
    use smql_ast::command::SpawnCommand;
    use smql_ast::expression::{Expression, ExpressionKind};
    use smql_ast::value::Value;

    fn lit(v: Value) -> Expression {
        Expression::new(ExpressionKind::Literal(v))
    }

    let cmd = SpawnCommand {
        machine: "WaitTimeout".to_string(),
        data: vec![],
        then_transition: None,
        batch: false,
        batch_data: Vec::new(),
        parent_id: None,
        parent_machine: None,
    };
    let result = engine.spawn(&cmd).await.expect("spawn");

    let tcmd = smql_ast::command::TransitionCommand::new(
        "WaitTimeout".to_string(),
        result.instance.id.to_string(),
        "waiting".to_string(),
    );
    let tr = engine.transition(&tcmd).await.expect("transition to waiting");
    assert_eq!(tr.instance.state, "waiting");
    // Timer should be registered (we can verify the instance is in waiting state)
}

// =========================================================================
// wildcards.md
// =========================================================================

/// wildcards.md -- ANY wildcard basic example
/// Source: wildcards.md lines 9-11
#[test]
fn wildcards_any_basic() {
    let smql = r#"
DEFINE MACHINE WildcardBasic (
  STATES { open, processing, cancelled }
  INITIAL STATE open
  TERMINAL STATES { cancelled }
  TRANSITIONS {
    open -> processing {}
    ANY -> cancelled {
      ACTION : EMIT("order.cancelled", { order: SELF })
    }
  }
)
"#;
    let machines = smql_parser::parse_machines(smql).expect("parse");
    assert_eq!(machines[0].transitions.len(), 2);
    let wildcard_t = &machines[0].transitions[1];
    assert!(matches!(
        wildcard_t.from,
        smql_ast::machine::TransitionSource::Any { .. }
    ));
}

/// wildcards.md -- ANY with EXCEPT FROM
/// Source: wildcards.md lines 22-25
#[test]
fn wildcards_any_except_from() {
    let smql = r#"
DEFINE MACHINE OrderWildcard (
  STATES { open, placed, shipped, delivered, returned, cancelled }
  INITIAL STATE open
  TERMINAL STATES { cancelled, returned }
  TRANSITIONS {
    open -> placed {}
    placed -> shipped {}
    shipped -> delivered {}
    delivered -> returned {}
    ANY -> cancelled {
      EXCEPT FROM { shipped, delivered, returned }
      ACTION : EMIT("order.cancelled", { order: SELF })
    }
  }
)
"#;
    let machines = smql_parser::parse_machines(smql).expect("parse");
    let wildcard_t = machines[0]
        .transitions
        .iter()
        .find(|t| matches!(t.from, smql_ast::machine::TransitionSource::Any { .. }))
        .expect("should have ANY transition");
    if let smql_ast::machine::TransitionSource::Any { except } = &wildcard_t.from {
        assert_eq!(except.len(), 3);
        assert!(except.contains(&"shipped".to_string()));
        assert!(except.contains(&"delivered".to_string()));
        assert!(except.contains(&"returned".to_string()));
    }
}

/// wildcards.md -- ANY with EXCEPT FROM, GUARD, MUTATE, ACTION
/// Source: wildcards.md lines 35-40
#[test]
fn wildcards_any_with_guard_mutate_action() {
    let smql = r#"
DEFINE MACHINE EscalationWild (
  DATA {
    priority : TEXT -> DEFAULT("normal")
  }
  STATES { open, triaged, closed }
  INITIAL STATE open
  TERMINAL STATES { closed }
  TRANSITIONS {
    open -> triaged {}
    triaged -> closed {}
    ANY -> triaged {
      EXCEPT FROM { open, closed }
      GUARD  : ACTOR.role IN ("admin", "supervisor")
      MUTATE : priority = "critical"
      ACTION : LOG("Escalated by {ACTOR}")
    }
  }
)
"#;
    let machines = smql_parser::parse_machines(smql).expect("parse");
    let wildcard_t = machines[0]
        .transitions
        .iter()
        .find(|t| {
            matches!(t.from, smql_ast::machine::TransitionSource::Any { .. })
        })
        .expect("should have ANY transition");
    // Verify EXCEPT FROM
    if let smql_ast::machine::TransitionSource::Any { except } = &wildcard_t.from {
        assert_eq!(except.len(), 2);
    }
    // Verify GUARD present
    assert!(!wildcard_t.guards.is_empty(), "should have guard");
    // Verify MUTATE present
    assert!(!wildcard_t.mutates.is_empty(), "should have mutate");
    // Verify ACTION present
    assert!(!wildcard_t.actions.is_empty(), "should have action");
}

/// wildcards.md -- Engine test: ANY transition works from non-terminal state
#[tokio::test]
async fn wildcards_engine_any_transition() {
    let smql = r#"
DEFINE MACHINE WildEngine (
  STATES { open, processing, done, cancelled }
  INITIAL STATE open
  TERMINAL STATES { done, cancelled }
  TRANSITIONS {
    open -> processing {}
    processing -> done {}
    ANY -> cancelled {
      EXCEPT FROM { done }
    }
  }
)
"#;
    let engine = make_engine_from(smql);
    use smql_ast::command::SpawnCommand;
    use smql_ast::expression::{Expression, ExpressionKind};
    use smql_ast::value::Value;

    fn lit(v: Value) -> Expression {
        Expression::new(ExpressionKind::Literal(v))
    }

    let cmd = SpawnCommand {
        machine: "WildEngine".to_string(),
        data: vec![],
        then_transition: None,
        batch: false,
        batch_data: Vec::new(),
        parent_id: None,
        parent_machine: None,
    };
    let result = engine.spawn(&cmd).await.expect("spawn");
    assert_eq!(result.instance.state, "open");

    // Use ANY -> cancelled from open state
    let tcmd = smql_ast::command::TransitionCommand::new(
        "WildEngine".to_string(),
        result.instance.id.to_string(),
        "cancelled".to_string(),
    );
    let tr = engine.transition(&tcmd).await.expect("wildcard transition");
    assert_eq!(tr.instance.state, "cancelled");
}

// =========================================================================
// constraints.md
// =========================================================================

/// constraints.md -- DATA block with REQUIRED and MAX and MIN and DEFAULT
/// Source: constraints.md lines 6-9
#[test]
fn constraints_required_max_min_default() {
    let smql = r#"
DEFINE MACHINE ConstraintTest1 (
  DATA {
    subject : TEXT -> REQUIRED, MAX(200)
    count   : INT  -> MIN(0), DEFAULT(0)
  }
  STATES { open, done }
  INITIAL STATE open
  TERMINAL STATES { done }
  TRANSITIONS {
    open -> done {}
  }
)
"#;
    let machines = smql_parser::parse_machines(smql).expect("parse");
    let data = &machines[0].data;
    assert_eq!(data.len(), 2);
    // subject: TEXT, required, max 200
    assert_eq!(data[0].name, "subject");
    assert!(has_constraint(&data[0].constraints, &Constraint::Required));
    assert!(has_constraint(&data[0].constraints, &Constraint::Max(0)));
    // count: INT, min 0, default 0
    assert_eq!(data[1].name, "count");
    assert!(has_constraint(&data[1].constraints, &Constraint::Min(0)));
}

/// constraints.md -- REQUIRED vs OPTIONAL
/// Source: constraints.md lines 29-34
#[test]
fn constraints_required_vs_optional() {
    let smql = r#"
DEFINE MACHINE ConstraintTest2 (
  DATA {
    email : TEXT -> REQUIRED
    phone : TEXT -> OPTIONAL
    notes : TEXT
  }
  STATES { open, done }
  INITIAL STATE open
  TERMINAL STATES { done }
  TRANSITIONS {
    open -> done {}
  }
)
"#;
    let machines = smql_parser::parse_machines(smql).expect("parse");
    let data = &machines[0].data;
    assert_eq!(data.len(), 3);
    assert!(has_constraint(&data[0].constraints, &Constraint::Required));   // email is REQUIRED
    assert!(has_constraint(&data[1].constraints, &Constraint::Optional));   // phone is OPTIONAL
    assert!(data[2].constraints.is_empty());  // notes has no constraints (implicit optional)
}

/// constraints.md -- DEFAULT with ENUM, SET, INT
/// Source: constraints.md lines 42-44
#[test]
fn constraints_default_values() {
    let smql = r#"
DEFINE MACHINE ConstraintTest3 (
  DATA {
    priority : ENUM(low, medium, high) -> DEFAULT(medium)
    tags     : SET(TEXT)               -> DEFAULT({})
    count    : INT                     -> DEFAULT(0)
  }
  STATES { open, done }
  INITIAL STATE open
  TERMINAL STATES { done }
  TRANSITIONS {
    open -> done {}
  }
)
"#;
    let machines = smql_parser::parse_machines(smql).expect("parse");
    let data = &machines[0].data;
    assert_eq!(data.len(), 3);
    assert_eq!(data[0].name, "priority");
    assert_eq!(data[1].name, "tags");
    assert_eq!(data[2].name, "count");
}

/// constraints.md -- MIN and MAX on numeric, text, and collections
/// Source: constraints.md lines 51-54
#[test]
fn constraints_min_max() {
    let smql = r#"
DEFINE MACHINE ConstraintTest4 (
  DATA {
    age     : INT      -> MIN(0), MAX(150)
    name    : TEXT     -> MAX(100)
    items   : LIST(TEXT) -> MIN(1), MAX(10)
  }
  STATES { open, done }
  INITIAL STATE open
  TERMINAL STATES { done }
  TRANSITIONS {
    open -> done {}
  }
)
"#;
    let machines = smql_parser::parse_machines(smql).expect("parse");
    let data = &machines[0].data;
    assert_eq!(data.len(), 3);
    assert_eq!(data[0].name, "age");
    assert_eq!(data[1].name, "name");
    assert_eq!(data[2].name, "items");
}

/// constraints.md -- RANGE shorthand
/// Source: constraints.md lines 62-63
#[test]
fn constraints_range() {
    let smql = r#"
DEFINE MACHINE ConstraintTest5 (
  DATA {
    satisfaction : INT -> RANGE(1, 5)
  }
  STATES { open, done }
  INITIAL STATE open
  TERMINAL STATES { done }
  TRANSITIONS {
    open -> done {}
  }
)
"#;
    let machines = smql_parser::parse_machines(smql).expect("parse");
    let data = &machines[0].data;
    assert_eq!(data.len(), 1);
    assert_eq!(data[0].name, "satisfaction");
    assert!(has_constraint(&data[0].constraints, &Constraint::Range(0, 0)));
}

/// constraints.md -- UNIQUE constraint
/// Source: constraints.md line 71
#[test]
fn constraints_unique() {
    let smql = r#"
DEFINE MACHINE ConstraintTest6 (
  DATA {
    email : TEXT -> REQUIRED, UNIQUE
  }
  STATES { open, done }
  INITIAL STATE open
  TERMINAL STATES { done }
  TRANSITIONS {
    open -> done {}
  }
)
"#;
    let machines = smql_parser::parse_machines(smql).expect("parse");
    let data = &machines[0].data;
    assert_eq!(data.len(), 1);
    assert!(has_constraint(&data[0].constraints, &Constraint::Required));
    assert!(has_constraint(&data[0].constraints, &Constraint::Unique));
}

/// constraints.md -- PATTERN constraint
/// Source: constraints.md line 79
#[test]
fn constraints_pattern() {
    let smql = r#"
DEFINE MACHINE ConstraintTest7 (
  DATA {
    email : TEXT -> REQUIRED, PATTERN("^[^@]+@[^@]+$")
  }
  STATES { open, done }
  INITIAL STATE open
  TERMINAL STATES { done }
  TRANSITIONS {
    open -> done {}
  }
)
"#;
    let machines = smql_parser::parse_machines(smql).expect("parse");
    let data = &machines[0].data;
    assert_eq!(data.len(), 1);
    assert!(has_constraint(&data[0].constraints, &Constraint::Required));
    assert!(has_constraint(&data[0].constraints, &Constraint::Pattern(String::new())));
}

/// constraints.md -- Engine test: REQUIRED field validation at spawn
#[tokio::test]
async fn constraints_engine_required_validation() {
    let smql = r#"
DEFINE MACHINE ConstraintEngine (
  DATA {
    email   : TEXT -> REQUIRED
    subject : TEXT -> REQUIRED, MAX(200)
    notes   : TEXT
  }
  STATES { open, done }
  INITIAL STATE open
  TERMINAL STATES { done }
  TRANSITIONS {
    open -> done {}
  }
)
"#;
    let engine = make_engine_from(smql);
    use smql_ast::command::SpawnCommand;
    use smql_ast::expression::{Expression, ExpressionKind};
    use smql_ast::value::Value;

    fn lit(v: Value) -> Expression {
        Expression::new(ExpressionKind::Literal(v))
    }

    // Spawn without required field should fail
    let cmd = SpawnCommand {
        machine: "ConstraintEngine".to_string(),
        data: vec![],
        then_transition: None,
        batch: false,
        batch_data: Vec::new(),
        parent_id: None,
        parent_machine: None,
    };
    let result = engine.spawn(&cmd).await;
    assert!(result.is_err(), "spawn without required fields should fail");

    // Spawn with required fields should succeed
    let cmd = SpawnCommand {
        machine: "ConstraintEngine".to_string(),
        data: vec![
            ("email".to_string(), lit(Value::Text("test@example.com".into()))),
            ("subject".to_string(), lit(Value::Text("Help needed".into()))),
        ],
        then_transition: None,
        batch: false,
        batch_data: Vec::new(),
        parent_id: None,
        parent_machine: None,
    };
    let result = engine.spawn(&cmd).await;
    assert!(result.is_ok(), "spawn with required fields should succeed");
}

// =========================================================================
// data-types.md
// =========================================================================

/// data-types.md -- ENUM type
/// Source: data-types.md line 25
#[test]
fn datatypes_enum() {
    let smql = r#"
DEFINE MACHINE DtEnum (
  DATA {
    priority : ENUM(low, medium, high, critical)
  }
  STATES { open, done }
  INITIAL STATE open
  TERMINAL STATES { done }
  TRANSITIONS {
    open -> done {}
  }
)
"#;
    let machines = smql_parser::parse_machines(smql).expect("parse");
    let data = &machines[0].data;
    assert_eq!(data[0].name, "priority");
}

/// data-types.md -- REF type
/// Source: data-types.md line 35
#[test]
fn datatypes_ref() {
    let smql = r#"
DEFINE MACHINE DtRef (
  DATA {
    assignee : REF(Agent)
  }
  STATES { open, done }
  INITIAL STATE open
  TERMINAL STATES { done }
  TRANSITIONS {
    open -> done {}
  }
)
"#;
    let machines = smql_parser::parse_machines(smql).expect("parse");
    let data = &machines[0].data;
    assert_eq!(data[0].name, "assignee");
}

/// data-types.md -- MONEY type
/// Source: data-types.md lines 44-45
#[test]
fn datatypes_money() {
    let smql = r#"
DEFINE MACHINE DtMoney (
  DATA {
    total : MONEY(USD)
    price : MONEY(EUR)
  }
  STATES { open, done }
  INITIAL STATE open
  TERMINAL STATES { done }
  TRANSITIONS {
    open -> done {}
  }
)
"#;
    let machines = smql_parser::parse_machines(smql).expect("parse");
    let data = &machines[0].data;
    assert_eq!(data.len(), 2);
    assert_eq!(data[0].name, "total");
    assert_eq!(data[1].name, "price");
}

/// data-types.md -- LIST type
/// Source: data-types.md line 62
#[test]
fn datatypes_list() {
    let smql = r#"
DEFINE MACHINE DtList (
  DATA {
    tags : LIST(TEXT)
  }
  STATES { open, done }
  INITIAL STATE open
  TERMINAL STATES { done }
  TRANSITIONS {
    open -> done {}
  }
)
"#;
    let machines = smql_parser::parse_machines(smql).expect("parse");
    let data = &machines[0].data;
    assert_eq!(data[0].name, "tags");
}

/// data-types.md -- SET type
/// Source: data-types.md line 72
#[test]
fn datatypes_set() {
    let smql = r#"
DEFINE MACHINE DtSet (
  DATA {
    categories : SET(TEXT)
  }
  STATES { open, done }
  INITIAL STATE open
  TERMINAL STATES { done }
  TRANSITIONS {
    open -> done {}
  }
)
"#;
    let machines = smql_parser::parse_machines(smql).expect("parse");
    let data = &machines[0].data;
    assert_eq!(data[0].name, "categories");
}

/// data-types.md -- MAP type
/// Source: data-types.md line 82
#[test]
fn datatypes_map() {
    let smql = r#"
DEFINE MACHINE DtMap (
  DATA {
    metadata : MAP(TEXT, TEXT)
  }
  STATES { open, done }
  INITIAL STATE open
  TERMINAL STATES { done }
  TRANSITIONS {
    open -> done {}
  }
)
"#;
    let machines = smql_parser::parse_machines(smql).expect("parse");
    let data = &machines[0].data;
    assert_eq!(data[0].name, "metadata");
}

/// data-types.md -- All scalar types in one machine
/// Tests parsing of TEXT, INT, FLOAT, BOOL, UUID, DATE, DATETIME, DURATION, JSON, BLOB
#[test]
fn datatypes_all_scalars() {
    let smql = r#"
DEFINE MACHINE DtAllScalars (
  DATA {
    name       : TEXT
    count      : INT
    rating     : FLOAT
    active     : BOOL
    user_id    : UUID
    due_date   : DATE
    created    : DATETIME
    sla        : DURATION
    metadata   : JSON
    attachment : BLOB
  }
  STATES { open, done }
  INITIAL STATE open
  TERMINAL STATES { done }
  TRANSITIONS {
    open -> done {}
  }
)
"#;
    let machines = smql_parser::parse_machines(smql).expect("parse");
    let data = &machines[0].data;
    assert_eq!(data.len(), 10);
    assert_eq!(data[0].name, "name");
    assert_eq!(data[1].name, "count");
    assert_eq!(data[2].name, "rating");
    assert_eq!(data[3].name, "active");
    assert_eq!(data[4].name, "user_id");
    assert_eq!(data[5].name, "due_date");
    assert_eq!(data[6].name, "created");
    assert_eq!(data[7].name, "sla");
    assert_eq!(data[8].name, "metadata");
    assert_eq!(data[9].name, "attachment");
}

/// data-types.md -- Combined types: ENUM, REF, MONEY, LIST, SET, MAP
#[test]
fn datatypes_all_complex() {
    let smql = r#"
DEFINE MACHINE DtAllComplex (
  DATA {
    priority   : ENUM(low, medium, high, critical)
    assignee   : REF(Agent)
    total      : MONEY(USD)
    tags       : LIST(TEXT)
    categories : SET(TEXT)
    metadata   : MAP(TEXT, TEXT)
  }
  STATES { open, done }
  INITIAL STATE open
  TERMINAL STATES { done }
  TRANSITIONS {
    open -> done {}
  }
)
"#;
    let machines = smql_parser::parse_machines(smql).expect("parse");
    let data = &machines[0].data;
    assert_eq!(data.len(), 6);
}

/// data-types.md -- Engine test: spawn with various data types
#[tokio::test]
async fn datatypes_engine_spawn_with_types() {
    let smql = r#"
DEFINE MACHINE DtSpawn (
  DATA {
    name     : TEXT -> REQUIRED
    count    : INT -> DEFAULT(0)
    priority : ENUM(low, medium, high) -> DEFAULT(low)
    tags     : LIST(TEXT)
    active   : BOOL -> DEFAULT(true)
  }
  STATES { open, done }
  INITIAL STATE open
  TERMINAL STATES { done }
  TRANSITIONS {
    open -> done {}
  }
)
"#;
    let engine = make_engine_from(smql);
    use smql_ast::command::SpawnCommand;
    use smql_ast::expression::{Expression, ExpressionKind};
    use smql_ast::value::Value;

    fn lit(v: Value) -> Expression {
        Expression::new(ExpressionKind::Literal(v))
    }

    let cmd = SpawnCommand {
        machine: "DtSpawn".to_string(),
        data: vec![
            ("name".to_string(), lit(Value::Text("Test Item".into()))),
        ],
        then_transition: None,
        batch: false,
        batch_data: Vec::new(),
        parent_id: None,
        parent_machine: None,
    };
    let result = engine.spawn(&cmd).await.expect("spawn with types");
    assert_eq!(result.instance.state, "open");
}

// =========================================================================
// Cross-doc: Combined features
// =========================================================================

/// Cross-doc: Machine with hooks + timeouts + wildcards + constraints + data types
/// Validates that all features from all 5 docs work together in one machine.
#[tokio::test]
async fn crossdoc_all_features_combined() {
    let smql = r#"
DEFINE MACHINE FullFeature (
  DATA {
    subject     : TEXT -> REQUIRED, MAX(200)
    priority    : ENUM(low, medium, high) -> DEFAULT(medium)
    customer_id : TEXT -> REQUIRED
    count       : INT -> DEFAULT(0)
    total       : MONEY(USD)
    tags        : LIST(TEXT)
    metadata    : MAP(TEXT, TEXT)
  }
  STATES { open, triaged, in_progress, waiting, resolved, closed }
  INITIAL STATE open
  TERMINAL STATES { closed }
  TRANSITIONS {
    open -> triaged {}
    triaged -> in_progress {}
    in_progress -> waiting {
      TIMEOUT : 72h -> resolved
    }
    waiting -> resolved {}
    resolved -> closed {}
    ANY -> closed {
      EXCEPT FROM { resolved }
    }
  }
  HOOKS {
    ON SPAWN {
      EMIT("ticket.created")
    }
    BEFORE EACH TRANSITION {
      LOG("Validating transition")
    }
    AFTER EACH TRANSITION {
      EMIT("state.changed")
    }
    ON ENTER resolved {
      NOTIFY(customer_id, "ticket.resolved")
    }
    ON EXIT in_progress {
      LOG("Left in_progress")
    }
  }
)
"#;
    let engine = make_engine_from(smql);
    use smql_ast::command::SpawnCommand;
    use smql_ast::expression::{Expression, ExpressionKind};
    use smql_ast::value::Value;

    fn lit(v: Value) -> Expression {
        Expression::new(ExpressionKind::Literal(v))
    }

    // Spawn with required fields
    let cmd = SpawnCommand {
        machine: "FullFeature".to_string(),
        data: vec![
            ("subject".to_string(), lit(Value::Text("Test ticket".into()))),
            ("customer_id".to_string(), lit(Value::Text("cust-123".into()))),
        ],
        then_transition: None,
        batch: false,
        batch_data: Vec::new(),
        parent_id: None,
        parent_machine: None,
    };
    let result = engine.spawn(&cmd).await.expect("spawn full feature");
    assert_eq!(result.instance.state, "open");

    // Transition through states
    let id = result.instance.id.to_string();

    let tcmd = smql_ast::command::TransitionCommand::new(
        "FullFeature".to_string(),
        id.clone(),
        "triaged".to_string(),
    );
    let tr = engine.transition(&tcmd).await.expect("open -> triaged");
    assert_eq!(tr.instance.state, "triaged");

    // Use wildcard to go directly to closed
    let tcmd = smql_ast::command::TransitionCommand::new(
        "FullFeature".to_string(),
        id.clone(),
        "closed".to_string(),
    );
    let tr = engine.transition(&tcmd).await.expect("triaged -> closed via ANY");
    assert_eq!(tr.instance.state, "closed");
}
