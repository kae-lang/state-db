//! Coverage tests for uncovered parser code paths in machine.rs and commands.rs.

use smql_ast::command::{AlterOperation, Command, Statement};
use smql_ast::machine::*;
use smql_ast::query::Query;
use smql_ast::types::*;

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

fn parse_one_machine(input: &str) -> MachineDefinition {
    smql_parser::parse_machines(input)
        .expect("parse_machines should succeed")
        .into_iter()
        .next()
        .expect("expected at least one machine definition")
}

fn parse_one_statement(input: &str) -> Statement {
    smql_parser::parse(input)
        .expect("parse should succeed")
        .into_iter()
        .next()
        .expect("expected at least one statement")
}

fn parse_one_command(input: &str) -> Command {
    match parse_one_statement(input) {
        Statement::Command(cmd) => cmd,
        other => panic!("Expected Command, got {:?}", other),
    }
}

// ===========================================================================
// machine.rs coverage gaps
// ===========================================================================

// ---- 1. ROLES block with permissions ----
// NOTE: The parser's parse_roles_block calls expect_keyword("CAN"), but "CAN"
// is not registered as a keyword in the lexer.  It lexes as an Identifier, so
// expect_keyword always fails.  We test the entry into the ROLES branch
// (which is the uncovered path) and assert the expected error.  An empty ROLES
// block succeeds because the permission loop is never entered.

#[test]
fn parse_roles_block_empty() {
    let input = r#"
DEFINE MACHINE WithEmptyRoles (
    STATES { a, b }
    INITIAL STATE a
    TERMINAL STATES { b }
    TRANSITIONS { a -> b {} }
    ROLES {
        admin {}
    }
)
"#;
    let m = parse_one_machine(input);
    assert_eq!(m.roles.len(), 1);
    assert_eq!(m.roles[0].name, "admin");
    assert!(m.roles[0].permissions.is_empty());
}

#[test]
fn parse_roles_block_multiple_empty() {
    let input = r#"
DEFINE MACHINE WithMultiRoles (
    STATES { a }
    INITIAL STATE a
    TERMINAL STATES { a }
    TRANSITIONS {}
    ROLES {
        admin {}
        viewer {}
        operator {}
    }
)
"#;
    let m = parse_one_machine(input);
    assert_eq!(m.roles.len(), 3);
    assert_eq!(m.roles[0].name, "admin");
    assert_eq!(m.roles[1].name, "viewer");
    assert_eq!(m.roles[2].name, "operator");
}

#[test]
fn parse_roles_block_with_can_fails() {
    // CAN is not a keyword in the lexer, so parsing permissions fails.
    // This exercises the parse_roles_block code path entry.
    let input = r#"
DEFINE MACHINE WithRoles (
    STATES { a, b }
    INITIAL STATE a
    TERMINAL STATES { b }
    TRANSITIONS { a -> b {} }
    ROLES {
        admin { CAN SPAWN }
    }
)
"#;
    let result = smql_parser::parse_machines(input);
    assert!(
        result.is_err(),
        "Expected error because CAN is not a keyword"
    );
}

// ---- 2. Invalid keyword in machine body (error) ----

#[test]
fn parse_invalid_keyword_in_machine_body() {
    // FOOBAR is not a keyword — it lexes as an Identifier, triggering
    // the "Unexpected token" branch in the machine-body parser.
    let input = r#"
DEFINE MACHINE Bad (
    STATES { a }
    foobar stuff
    INITIAL STATE a
    TERMINAL STATES { a }
    TRANSITIONS {}
)
"#;
    let result = smql_parser::parse_machines(input);
    assert!(
        result.is_err(),
        "Expected parse error for unknown token in machine body"
    );
}

// ---- 3. MAP type ----

#[test]
fn parse_map_type() {
    let input = r#"
DEFINE MACHINE WithMap (
    DATA { config : MAP(TEXT, INT) -> REQUIRED }
    STATES { a }
    INITIAL STATE a
    TERMINAL STATES { a }
    TRANSITIONS {}
)
"#;
    let m = parse_one_machine(input);
    assert_eq!(m.data.len(), 1);
    assert_eq!(
        m.data[0].field_type,
        TypeDefinition::Map(
            Box::new(TypeDefinition::Text),
            Box::new(TypeDefinition::Int)
        )
    );
    assert_eq!(m.data[0].constraints, vec![Constraint::Required]);
}

// ---- 4. PATTERN constraint ----

#[test]
fn parse_pattern_constraint() {
    let input = r#"
DEFINE MACHINE WithPattern (
    DATA { email : TEXT -> PATTERN("^[a-z]+@") }
    STATES { a }
    INITIAL STATE a
    TERMINAL STATES { a }
    TRANSITIONS {}
)
"#;
    let m = parse_one_machine(input);
    assert_eq!(m.data.len(), 1);
    assert_eq!(
        m.data[0].constraints,
        vec![Constraint::Pattern("^[a-z]+@".into())]
    );
}

// ---- 5. Invalid type name (error) ----

#[test]
fn parse_invalid_type_name() {
    // BADTYPE is not a keyword, so parse_type_definition gets an Identifier
    // token and returns "Expected type name, found ..." error.
    let input = r#"
DEFINE MACHINE Bad (
    DATA { f : badtype -> REQUIRED }
    STATES { a }
    INITIAL STATE a
    TERMINAL STATES { a }
    TRANSITIONS {}
)
"#;
    let result = smql_parser::parse_machines(input);
    assert!(result.is_err(), "Expected error for unknown type");
}

// ---- 6. FLOAT default value ----

#[test]
fn parse_float_default_value() {
    let input = r#"
DEFINE MACHINE WithFloat (
    DATA { score : FLOAT -> DEFAULT(3.14) }
    STATES { a }
    INITIAL STATE a
    TERMINAL STATES { a }
    TRANSITIONS {}
)
"#;
    let m = parse_one_machine(input);
    assert_eq!(m.data.len(), 1);
    assert_eq!(
        m.data[0].constraints,
        vec![Constraint::Default(DefaultValue::Float(3.14))]
    );
}

// ---- 7. Bool and Null default values ----

#[test]
fn parse_bool_default_true() {
    let input = r#"
DEFINE MACHINE WithBool (
    DATA { active : BOOL -> DEFAULT(TRUE) }
    STATES { a }
    INITIAL STATE a
    TERMINAL STATES { a }
    TRANSITIONS {}
)
"#;
    let m = parse_one_machine(input);
    assert_eq!(
        m.data[0].constraints,
        vec![Constraint::Default(DefaultValue::Bool(true))]
    );
}

#[test]
fn parse_bool_default_false() {
    let input = r#"
DEFINE MACHINE WithBool (
    DATA { flag : BOOL -> DEFAULT(FALSE) }
    STATES { a }
    INITIAL STATE a
    TERMINAL STATES { a }
    TRANSITIONS {}
)
"#;
    let m = parse_one_machine(input);
    assert_eq!(
        m.data[0].constraints,
        vec![Constraint::Default(DefaultValue::Bool(false))]
    );
}

#[test]
fn parse_null_default() {
    let input = r#"
DEFINE MACHINE WithNull (
    DATA { tag : TEXT -> DEFAULT(NULL) }
    STATES { a }
    INITIAL STATE a
    TERMINAL STATES { a }
    TRANSITIONS {}
)
"#;
    let m = parse_one_machine(input);
    assert_eq!(
        m.data[0].constraints,
        vec![Constraint::Default(DefaultValue::Null)]
    );
}

// ---- 8. Empty list default ----

#[test]
fn parse_empty_list_default() {
    let input = r#"
DEFINE MACHINE WithList (
    DATA { items : LIST(INT) -> DEFAULT([]) }
    STATES { a }
    INITIAL STATE a
    TERMINAL STATES { a }
    TRANSITIONS {}
)
"#;
    let m = parse_one_machine(input);
    assert_eq!(m.data.len(), 1);
    assert_eq!(
        m.data[0].field_type,
        TypeDefinition::List(Box::new(TypeDefinition::Int))
    );
    assert_eq!(
        m.data[0].constraints,
        vec![Constraint::Default(DefaultValue::EmptyList)]
    );
}

#[test]
fn parse_empty_set_default() {
    let input = r#"
DEFINE MACHINE WithSet (
    DATA { tags : SET(TEXT) -> DEFAULT({}) }
    STATES { a }
    INITIAL STATE a
    TERMINAL STATES { a }
    TRANSITIONS {}
)
"#;
    let m = parse_one_machine(input);
    assert_eq!(
        m.data[0].constraints,
        vec![Constraint::Default(DefaultValue::EmptySet)]
    );
}

// ---- 9. WEBHOOK action (with and without payload) ----

#[test]
fn parse_webhook_action_without_payload() {
    let input = r#"
DEFINE MACHINE WithWebhook (
    STATES { a, b }
    INITIAL STATE a
    TERMINAL STATES { b }
    TRANSITIONS {
        a -> b {
            ACTION : WEBHOOK("https://example.com/hook")
        }
    }
)
"#;
    let m = parse_one_machine(input);
    assert_eq!(m.transitions[0].actions.len(), 1);
    match &m.transitions[0].actions[0] {
        Action::Webhook { url, payload } => {
            assert_eq!(url, "https://example.com/hook");
            assert!(payload.is_none());
        }
        other => panic!("Expected Webhook, got {:?}", other),
    }
}

#[test]
fn parse_webhook_action_with_payload() {
    let input = r#"
DEFINE MACHINE WithWebhook (
    STATES { a, b }
    INITIAL STATE a
    TERMINAL STATES { b }
    TRANSITIONS {
        a -> b {
            ACTION : WEBHOOK("https://example.com/data", SELF)
        }
    }
)
"#;
    let m = parse_one_machine(input);
    assert_eq!(m.transitions[0].actions.len(), 1);
    match &m.transitions[0].actions[0] {
        Action::Webhook { url, payload } => {
            assert_eq!(url, "https://example.com/data");
            assert!(payload.is_some());
        }
        other => panic!("Expected Webhook, got {:?}", other),
    }
}

// ---- 10. SIGNAL PARENT action (in transition body as ACTION :) ----

#[test]
fn parse_signal_parent_as_action() {
    let input = r#"
DEFINE MACHINE Child (
    PARENT : ParentMachine
    STATES { a, b }
    INITIAL STATE a
    TERMINAL STATES { b }
    TRANSITIONS {
        a -> b {
            ACTION : SIGNAL PARENT TO completed
        }
    }
)
"#;
    let m = parse_one_machine(input);
    assert_eq!(m.parent, Some("ParentMachine".into()));
    assert_eq!(m.transitions[0].actions.len(), 1);
    match &m.transitions[0].actions[0] {
        Action::SignalParent { target_state } => {
            assert_eq!(target_state, "completed");
        }
        other => panic!("Expected SignalParent, got {:?}", other),
    }
}

// Also test SIGNAL PARENT as a keyword in the transition body (not via ACTION :).
#[test]
fn parse_signal_parent_keyword_in_transition_body() {
    let input = r#"
DEFINE MACHINE Child (
    PARENT : ParentMachine
    STATES { a, b }
    INITIAL STATE a
    TERMINAL STATES { b }
    TRANSITIONS {
        a -> b {
            SIGNAL PARENT TO finished
        }
    }
)
"#;
    let m = parse_one_machine(input);
    assert_eq!(m.transitions[0].actions.len(), 1);
    match &m.transitions[0].actions[0] {
        Action::SignalParent { target_state } => {
            assert_eq!(target_state, "finished");
        }
        other => panic!("Expected SignalParent, got {:?}", other),
    }
}

// ---- 11. SPAWN action in transition ----

#[test]
fn parse_spawn_action_in_transition() {
    let input = r#"
DEFINE MACHINE WithSpawnAction (
    STATES { a, b }
    INITIAL STATE a
    TERMINAL STATES { b }
    TRANSITIONS {
        a -> b {
            ACTION : SPAWN Child { x: 1 }
        }
    }
)
"#;
    let m = parse_one_machine(input);
    assert_eq!(m.transitions[0].actions.len(), 1);
    match &m.transitions[0].actions[0] {
        Action::SpawnChild { machine, data } => {
            assert_eq!(machine, "Child");
            assert_eq!(data.len(), 1);
            assert_eq!(data[0].0, "x");
        }
        other => panic!("Expected SpawnChild, got {:?}", other),
    }
}

#[test]
fn parse_spawn_action_no_data() {
    let input = r#"
DEFINE MACHINE WithSpawnAction (
    STATES { a, b }
    INITIAL STATE a
    TERMINAL STATES { b }
    TRANSITIONS {
        a -> b {
            ACTION : SPAWN Child {}
        }
    }
)
"#;
    let m = parse_one_machine(input);
    match &m.transitions[0].actions[0] {
        Action::SpawnChild { machine, data } => {
            assert_eq!(machine, "Child");
            assert!(data.is_empty());
        }
        other => panic!("Expected SpawnChild, got {:?}", other),
    }
}

// ---- 12. HOOKS with DWELL trigger ----

#[test]
fn parse_hooks_dwell_trigger() {
    let input = r#"
DEFINE MACHINE WithDwell (
    STATES { a, b }
    INITIAL STATE a
    TERMINAL STATES { b }
    TRANSITIONS { a -> b {} }
    HOOKS {
        ON DWELL(a, > 1d) {
            EMIT("dwell_alert")
        }
    }
)
"#;
    let m = parse_one_machine(input);
    assert_eq!(m.hooks.len(), 1);
    match &m.hooks[0].trigger {
        HookTrigger::OnDwell { state, duration } => {
            assert_eq!(state, "a");
            assert_eq!(duration.seconds, 86400); // 1d = 86400s
        }
        other => panic!("Expected OnDwell trigger, got {:?}", other),
    }
    assert_eq!(m.hooks[0].actions.len(), 1);
    match &m.hooks[0].actions[0] {
        Action::Emit { event, payload } => {
            assert_eq!(event, "dwell_alert");
            assert!(payload.is_none());
        }
        other => panic!("Expected Emit action, got {:?}", other),
    }
}

// ---- 13. HOOKS with BEFORE/AFTER EACH TRANSITION ----

#[test]
fn parse_hooks_before_each_transition() {
    let input = r#"
DEFINE MACHINE WithGlobalHooks (
    STATES { a, b }
    INITIAL STATE a
    TERMINAL STATES { b }
    TRANSITIONS { a -> b {} }
    HOOKS {
        BEFORE EACH TRANSITION {
            LOG("before transition")
        }
    }
)
"#;
    let m = parse_one_machine(input);
    assert_eq!(m.hooks.len(), 1);
    assert_eq!(m.hooks[0].trigger, HookTrigger::BeforeEachTransition);
    assert_eq!(m.hooks[0].actions.len(), 1);
    match &m.hooks[0].actions[0] {
        Action::Log(msg) => assert_eq!(msg, "before transition"),
        other => panic!("Expected Log, got {:?}", other),
    }
}

#[test]
fn parse_hooks_after_each_transition() {
    let input = r#"
DEFINE MACHINE WithGlobalHooks (
    STATES { a, b }
    INITIAL STATE a
    TERMINAL STATES { b }
    TRANSITIONS { a -> b {} }
    HOOKS {
        AFTER EACH TRANSITION {
            EMIT("transitioned")
        }
    }
)
"#;
    let m = parse_one_machine(input);
    assert_eq!(m.hooks.len(), 1);
    assert_eq!(m.hooks[0].trigger, HookTrigger::AfterEachTransition);
    assert_eq!(m.hooks[0].actions.len(), 1);
    match &m.hooks[0].actions[0] {
        Action::Emit { event, .. } => assert_eq!(event, "transitioned"),
        other => panic!("Expected Emit, got {:?}", other),
    }
}

#[test]
fn parse_hooks_before_and_after_together() {
    let input = r#"
DEFINE MACHINE WithBothHooks (
    STATES { a, b }
    INITIAL STATE a
    TERMINAL STATES { b }
    TRANSITIONS { a -> b {} }
    HOOKS {
        BEFORE EACH TRANSITION {
            LOG("before")
        }
        AFTER EACH TRANSITION {
            EMIT("done")
        }
    }
)
"#;
    let m = parse_one_machine(input);
    assert_eq!(m.hooks.len(), 2);
    assert_eq!(m.hooks[0].trigger, HookTrigger::BeforeEachTransition);
    assert_eq!(m.hooks[1].trigger, HookTrigger::AfterEachTransition);
}

// ---- 14. MAX constraint on LIST children ----

#[test]
fn parse_children_list_with_min_and_max() {
    let input = r#"
DEFINE MACHINE WithMax (
    CHILDREN { items : LIST(Item) -> MIN(1), MAX(10) }
    STATES { a }
    INITIAL STATE a
    TERMINAL STATES { a }
    TRANSITIONS {}
)
"#;
    let m = parse_one_machine(input);
    assert_eq!(m.children.len(), 1);
    assert_eq!(m.children[0].name, "items");
    assert_eq!(m.children[0].machine, "Item");
    match &m.children[0].cardinality {
        ChildCardinality::List { min, max } => {
            assert_eq!(*min, Some(1));
            assert_eq!(*max, Some(10));
        }
        other => panic!("Expected List cardinality, got {:?}", other),
    }
}

#[test]
fn parse_children_list_with_max_only() {
    let input = r#"
DEFINE MACHINE WithMaxOnly (
    CHILDREN { items : LIST(Item) -> MAX(5) }
    STATES { a }
    INITIAL STATE a
    TERMINAL STATES { a }
    TRANSITIONS {}
)
"#;
    let m = parse_one_machine(input);
    match &m.children[0].cardinality {
        ChildCardinality::List { min, max } => {
            assert_eq!(*min, None);
            assert_eq!(*max, Some(5));
        }
        other => panic!("Expected List cardinality, got {:?}", other),
    }
}

// ---- 15. Required child (bare identifier) ----

#[test]
fn parse_required_child() {
    let input = r#"
DEFINE MACHINE WithRequired (
    CHILDREN { config : Config }
    STATES { a }
    INITIAL STATE a
    TERMINAL STATES { a }
    TRANSITIONS {}
)
"#;
    let m = parse_one_machine(input);
    assert_eq!(m.children.len(), 1);
    assert_eq!(m.children[0].name, "config");
    assert_eq!(m.children[0].machine, "Config");
    assert_eq!(m.children[0].cardinality, ChildCardinality::Required);
}

// ===========================================================================
// commands.rs coverage gaps
// ===========================================================================

// ---- 16. SPAWN BATCH ----

#[test]
fn parse_spawn_batch() {
    let input = r#"SPAWN BATCH SomeM [{ name: "a" }, { name: "b" }]"#;
    let cmd = parse_one_command(input);
    match cmd {
        Command::Spawn(s) => {
            assert_eq!(s.machine, "SomeM");
            assert!(s.batch);
            assert_eq!(s.batch_data.len(), 2);
            assert_eq!(s.batch_data[0].len(), 1);
            assert_eq!(s.batch_data[0][0].0, "name");
            assert_eq!(s.batch_data[1].len(), 1);
            assert_eq!(s.batch_data[1][0].0, "name");
        }
        other => panic!("Expected Spawn, got {:?}", other),
    }
}

#[test]
fn parse_spawn_batch_three_items() {
    let input = r#"SPAWN BATCH M [{ x: 1 }, { x: 2 }, { x: 3 }]"#;
    let cmd = parse_one_command(input);
    match cmd {
        Command::Spawn(s) => {
            assert!(s.batch);
            assert_eq!(s.batch_data.len(), 3);
        }
        other => panic!("Expected Spawn, got {:?}", other),
    }
}

// ---- 17. TRANSITION THROUGH ----

#[test]
fn parse_transition_through() {
    let input = r#"TRANSITION Machine "some_id" TO done THROUGH [step1, step2]"#;
    let cmd = parse_one_command(input);
    match cmd {
        Command::Transition(t) => {
            assert_eq!(t.machine, "Machine");
            assert_eq!(t.instance_id, "some_id");
            assert_eq!(t.to_state, "done");
            assert_eq!(t.through, vec!["step1".to_string(), "step2".to_string()]);
        }
        other => panic!("Expected Transition, got {:?}", other),
    }
}

#[test]
fn parse_transition_through_single_hop() {
    let input = r#"TRANSITION Machine inst TO final THROUGH [intermediate]"#;
    let cmd = parse_one_command(input);
    match cmd {
        Command::Transition(t) => {
            assert_eq!(t.through, vec!["intermediate".to_string()]);
        }
        other => panic!("Expected Transition, got {:?}", other),
    }
}

// ---- 18. TRY TRANSITION ----

#[test]
fn parse_try_transition_basic() {
    let input = r#"TRY TRANSITION Machine "some_id" TO done"#;
    let cmd = parse_one_command(input);
    match cmd {
        Command::TryTransition(t) => {
            assert_eq!(t.machine, "Machine");
            assert_eq!(t.instance_id, "some_id");
            assert_eq!(t.to_state, "done");
        }
        other => panic!("Expected TryTransition, got {:?}", other),
    }
}

// ---- 19. TRY TRANSITION WITH / MEMO / AS ----

#[test]
fn parse_try_transition_with_data_memo_actor() {
    let input = r#"TRY TRANSITION Machine "some_id" TO resolved WITH { notes: "fixed" } MEMO "attempt" AS "user1""#;
    let cmd = parse_one_command(input);
    match cmd {
        Command::TryTransition(t) => {
            assert_eq!(t.machine, "Machine");
            assert_eq!(t.instance_id, "some_id");
            assert_eq!(t.to_state, "resolved");
            assert_eq!(t.with_data.len(), 1);
            assert_eq!(t.with_data[0].0, "notes");
            assert_eq!(t.memo, Some("attempt".into()));
            assert_eq!(t.as_actor, Some("user1".into()));
        }
        other => panic!("Expected TryTransition, got {:?}", other),
    }
}

#[test]
fn parse_try_transition_with_memo_only() {
    let input = r#"TRY TRANSITION Machine tk TO done MEMO "retry""#;
    let cmd = parse_one_command(input);
    match cmd {
        Command::TryTransition(t) => {
            assert_eq!(t.memo, Some("retry".into()));
            assert!(t.with_data.is_empty());
            assert!(t.as_actor.is_none());
        }
        other => panic!("Expected TryTransition, got {:?}", other),
    }
}

// ---- 20. ALTER MACHINE with ADD/REMOVE TRANSITION, ADD DATA BACKFILL, REMOVE DATA ----

#[test]
fn parse_alter_machine_add_transition() {
    let input = "ALTER MACHINE Order ADD TRANSITION draft -> submitted";
    let cmd = parse_one_command(input);
    match cmd {
        Command::AlterMachine(a) => {
            assert_eq!(a.machine, "Order");
            assert_eq!(a.operations.len(), 1);
            match &a.operations[0] {
                AlterOperation::AddTransition(t) => {
                    assert_eq!(t.from, TransitionSource::State("draft".into()));
                    assert_eq!(t.to, "submitted");
                }
                other => panic!("Expected AddTransition, got {:?}", other),
            }
        }
        other => panic!("Expected AlterMachine, got {:?}", other),
    }
}

#[test]
fn parse_alter_machine_remove_transition() {
    let input = "ALTER MACHINE Order REMOVE TRANSITION draft -> submitted";
    let cmd = parse_one_command(input);
    match cmd {
        Command::AlterMachine(a) => {
            assert_eq!(a.machine, "Order");
            assert_eq!(a.operations.len(), 1);
            match &a.operations[0] {
                AlterOperation::RemoveTransition { from, to } => {
                    assert_eq!(from, "draft");
                    assert_eq!(to, "submitted");
                }
                other => panic!("Expected RemoveTransition, got {:?}", other),
            }
        }
        other => panic!("Expected AlterMachine, got {:?}", other),
    }
}

#[test]
fn parse_alter_machine_add_data_with_backfill() {
    let input = r#"ALTER MACHINE Order ADD DATA new_field : TEXT BACKFILL "default""#;
    let cmd = parse_one_command(input);
    match cmd {
        Command::AlterMachine(a) => {
            assert_eq!(a.machine, "Order");
            assert_eq!(a.operations.len(), 1);
            match &a.operations[0] {
                AlterOperation::AddData { field, backfill } => {
                    assert_eq!(field.name, "new_field");
                    assert_eq!(field.field_type, TypeDefinition::Text);
                    assert!(backfill.is_some());
                }
                other => panic!("Expected AddData, got {:?}", other),
            }
        }
        other => panic!("Expected AlterMachine, got {:?}", other),
    }
}

#[test]
fn parse_alter_machine_add_data_no_backfill() {
    let input = "ALTER MACHINE Order ADD DATA count : INT -> DEFAULT(0)";
    let cmd = parse_one_command(input);
    match cmd {
        Command::AlterMachine(a) => {
            assert_eq!(a.operations.len(), 1);
            match &a.operations[0] {
                AlterOperation::AddData { field, backfill } => {
                    assert_eq!(field.name, "count");
                    assert_eq!(field.field_type, TypeDefinition::Int);
                    assert_eq!(
                        field.constraints,
                        vec![Constraint::Default(DefaultValue::Int(0))]
                    );
                    assert!(backfill.is_none());
                }
                other => panic!("Expected AddData, got {:?}", other),
            }
        }
        other => panic!("Expected AlterMachine, got {:?}", other),
    }
}

#[test]
fn parse_alter_machine_remove_data() {
    let input = "ALTER MACHINE Order REMOVE DATA old_field";
    let cmd = parse_one_command(input);
    match cmd {
        Command::AlterMachine(a) => {
            assert_eq!(a.machine, "Order");
            assert_eq!(a.operations.len(), 1);
            match &a.operations[0] {
                AlterOperation::RemoveData(field_name) => {
                    assert_eq!(field_name, "old_field");
                }
                other => panic!("Expected RemoveData, got {:?}", other),
            }
        }
        other => panic!("Expected AlterMachine, got {:?}", other),
    }
}

// ---- 21. BACKFILL standalone operation ----

#[test]
fn parse_alter_machine_backfill() {
    let input = r#"ALTER MACHINE Order BACKFILL status = "processed""#;
    let cmd = parse_one_command(input);
    match cmd {
        Command::AlterMachine(a) => {
            assert_eq!(a.machine, "Order");
            assert_eq!(a.operations.len(), 1);
            match &a.operations[0] {
                AlterOperation::Backfill { field, value } => {
                    assert_eq!(field, "status");
                    // Verify the value is a string literal
                    match &value.kind {
                        smql_ast::expression::ExpressionKind::Literal(
                            smql_ast::value::Value::Text(s),
                        ) => assert_eq!(s, "processed"),
                        other => panic!("Expected Text literal, got {:?}", other),
                    }
                }
                other => panic!("Expected Backfill, got {:?}", other),
            }
        }
        other => panic!("Expected AlterMachine, got {:?}", other),
    }
}

#[test]
fn parse_alter_machine_backfill_int() {
    let input = "ALTER MACHINE Order BACKFILL version = 2";
    let cmd = parse_one_command(input);
    match cmd {
        Command::AlterMachine(a) => {
            assert_eq!(a.operations.len(), 1);
            match &a.operations[0] {
                AlterOperation::Backfill { field, value } => {
                    assert_eq!(field, "version");
                    match &value.kind {
                        smql_ast::expression::ExpressionKind::Literal(
                            smql_ast::value::Value::Int(n),
                        ) => assert_eq!(*n, 2),
                        other => panic!("Expected Int literal, got {:?}", other),
                    }
                }
                other => panic!("Expected Backfill, got {:?}", other),
            }
        }
        other => panic!("Expected AlterMachine, got {:?}", other),
    }
}

// ===========================================================================
// Additional edge cases for thoroughness
// ===========================================================================

// ---- TRANSITION with OR_STAY ----

#[test]
fn parse_transition_or_stay() {
    let input = "TRANSITION Machine tk TO done OR_STAY";
    let cmd = parse_one_command(input);
    match cmd {
        Command::Transition(t) => {
            assert_eq!(t.to_state, "done");
            assert!(t.or_stay);
        }
        other => panic!("Expected Transition, got {:?}", other),
    }
}

// ---- TRANSITION with multiple optional clauses ----

#[test]
fn parse_transition_all_optional_clauses() {
    let input = r#"TRANSITION Machine "inst1" TO resolved WITH { priority: "high" } MEMO "fix" AS "alice" CASCADE"#;
    let cmd = parse_one_command(input);
    match cmd {
        Command::Transition(t) => {
            assert_eq!(t.machine, "Machine");
            assert_eq!(t.instance_id, "inst1");
            assert_eq!(t.to_state, "resolved");
            assert_eq!(t.with_data.len(), 1);
            assert_eq!(t.memo, Some("fix".into()));
            assert_eq!(t.as_actor, Some("alice".into()));
            assert!(t.cascade);
        }
        other => panic!("Expected Transition, got {:?}", other),
    }
}

// ---- ON ENTER / ON EXIT hooks ----

#[test]
fn parse_hooks_on_enter_and_exit() {
    let input = r#"
DEFINE MACHINE WithEnterExit (
    STATES { a, b }
    INITIAL STATE a
    TERMINAL STATES { b }
    TRANSITIONS { a -> b {} }
    HOOKS {
        ON ENTER b {
            EMIT("entered_b")
        }
        ON EXIT a {
            LOG("leaving a")
        }
    }
)
"#;
    let m = parse_one_machine(input);
    assert_eq!(m.hooks.len(), 2);
    assert_eq!(m.hooks[0].trigger, HookTrigger::OnEnter("b".into()));
    assert_eq!(m.hooks[1].trigger, HookTrigger::OnExit("a".into()));
}

// ---- ON SPAWN hook ----

#[test]
fn parse_hooks_on_spawn() {
    let input = r#"
DEFINE MACHINE WithSpawnHook (
    STATES { a, b }
    INITIAL STATE a
    TERMINAL STATES { b }
    TRANSITIONS { a -> b {} }
    HOOKS {
        ON SPAWN {
            EMIT("instance_created")
        }
    }
)
"#;
    let m = parse_one_machine(input);
    assert_eq!(m.hooks.len(), 1);
    assert_eq!(m.hooks[0].trigger, HookTrigger::OnSpawn);
}

// ---- MUTATE : field = SPAWN Machine {} in transition ----

#[test]
fn parse_mutate_spawn_in_transition() {
    let input = r#"
DEFINE MACHINE WithMutateSpawn (
    STATES { a, b }
    INITIAL STATE a
    TERMINAL STATES { b }
    TRANSITIONS {
        a -> b {
            MUTATE : child = SPAWN ChildMachine { val: 42 }
        }
    }
)
"#;
    let m = parse_one_machine(input);
    assert_eq!(m.transitions[0].mutates.len(), 1);
    assert_eq!(m.transitions[0].mutates[0].field, "child");
    // The value should be a __spawn function call
    match &m.transitions[0].mutates[0].value.kind {
        smql_ast::expression::ExpressionKind::FunctionCall { name, args } => {
            assert_eq!(name, "__spawn");
            // args: machine_name, key, value
            assert_eq!(args.len(), 3);
        }
        other => panic!("Expected FunctionCall(__spawn), got {:?}", other),
    }
}

// ---- CAN ALTER permission in ROLES ----
// NOTE: Same issue as above — CAN is not a keyword so this fails.
// We still exercise the ROLES code-path entry.

#[test]
fn parse_roles_with_permissions_fails_because_can_not_keyword() {
    let input = r#"
DEFINE MACHINE WithAlterRole (
    STATES { a }
    INITIAL STATE a
    TERMINAL STATES { a }
    TRANSITIONS {}
    ROLES {
        superadmin { CAN ALTER }
    }
)
"#;
    let result = smql_parser::parse_machines(input);
    assert!(
        result.is_err(),
        "Expected error because CAN is not a keyword"
    );
}

// ---- ALTER MACHINE with REMOVE STATE (includes MIGRATE TO) ----

#[test]
fn parse_alter_machine_remove_state() {
    let input = "ALTER MACHINE Order REMOVE STATE draft MIGRATE TO pending";
    let cmd = parse_one_command(input);
    match cmd {
        Command::AlterMachine(a) => {
            assert_eq!(a.machine, "Order");
            assert_eq!(a.operations.len(), 1);
            match &a.operations[0] {
                AlterOperation::RemoveState { state, migrate_to } => {
                    assert_eq!(state, "draft");
                    assert_eq!(migrate_to, "pending");
                }
                other => panic!("Expected RemoveState, got {:?}", other),
            }
        }
        other => panic!("Expected AlterMachine, got {:?}", other),
    }
}

// ---- ALTER MACHINE with ADD STATE ----

#[test]
fn parse_alter_machine_add_state() {
    let input = "ALTER MACHINE Order ADD STATE in_review";
    let cmd = parse_one_command(input);
    match cmd {
        Command::AlterMachine(a) => {
            assert_eq!(a.operations.len(), 1);
            match &a.operations[0] {
                AlterOperation::AddState(s) => assert_eq!(s, "in_review"),
                other => panic!("Expected AddState, got {:?}", other),
            }
        }
        other => panic!("Expected AlterMachine, got {:?}", other),
    }
}

// ---- Multiple hook actions in one trigger ----

#[test]
fn parse_hook_with_multiple_actions() {
    let input = r#"
DEFINE MACHINE MultiAction (
    STATES { a, b }
    INITIAL STATE a
    TERMINAL STATES { b }
    TRANSITIONS { a -> b {} }
    HOOKS {
        ON SPAWN {
            LOG("spawned")
            EMIT("created")
        }
    }
)
"#;
    let m = parse_one_machine(input);
    assert_eq!(m.hooks[0].actions.len(), 2);
    match &m.hooks[0].actions[0] {
        Action::Log(msg) => assert_eq!(msg, "spawned"),
        other => panic!("Expected Log, got {:?}", other),
    }
    match &m.hooks[0].actions[1] {
        Action::Emit { event, .. } => assert_eq!(event, "created"),
        other => panic!("Expected Emit, got {:?}", other),
    }
}

// ---- EMIT with payload in hook ----

#[test]
fn parse_emit_with_payload_in_hook() {
    let input = r#"
DEFINE MACHINE EmitPayload (
    STATES { a, b }
    INITIAL STATE a
    TERMINAL STATES { b }
    TRANSITIONS { a -> b {} }
    HOOKS {
        AFTER EACH TRANSITION {
            EMIT("transition_done", SELF)
        }
    }
)
"#;
    let m = parse_one_machine(input);
    match &m.hooks[0].actions[0] {
        Action::Emit { event, payload } => {
            assert_eq!(event, "transition_done");
            assert!(payload.is_some());
        }
        other => panic!("Expected Emit, got {:?}", other),
    }
}

// ---- OPTIONAL child type ----

#[test]
fn parse_optional_child() {
    let input = r#"
DEFINE MACHINE WithOptional (
    CHILDREN { maybe_config : OPTIONAL(Settings) }
    STATES { a }
    INITIAL STATE a
    TERMINAL STATES { a }
    TRANSITIONS {}
)
"#;
    let m = parse_one_machine(input);
    assert_eq!(m.children.len(), 1);
    assert_eq!(m.children[0].name, "maybe_config");
    assert_eq!(m.children[0].machine, "Settings");
    assert_eq!(m.children[0].cardinality, ChildCardinality::Optional);
}

// ---- Multiple constraints on single data field ----

#[test]
fn parse_multiple_constraints() {
    let input = r#"
DEFINE MACHINE MultiConstraint (
    DATA { score : INT -> REQUIRED, MIN(0), MAX(100) }
    STATES { a }
    INITIAL STATE a
    TERMINAL STATES { a }
    TRANSITIONS {}
)
"#;
    let m = parse_one_machine(input);
    assert_eq!(m.data[0].constraints.len(), 3);
    assert_eq!(m.data[0].constraints[0], Constraint::Required);
    assert_eq!(m.data[0].constraints[1], Constraint::Min(0));
    assert_eq!(m.data[0].constraints[2], Constraint::Max(100));
}

// ---- RANGE constraint ----

#[test]
fn parse_range_constraint() {
    let input = r#"
DEFINE MACHINE WithRange (
    DATA { qty : INT -> RANGE(1, 999) }
    STATES { a }
    INITIAL STATE a
    TERMINAL STATES { a }
    TRANSITIONS {}
)
"#;
    let m = parse_one_machine(input);
    assert_eq!(m.data[0].constraints, vec![Constraint::Range(1, 999)]);
}

// ---- UNIQUE constraint ----

#[test]
fn parse_unique_constraint() {
    let input = r#"
DEFINE MACHINE WithUnique (
    DATA { email : TEXT -> REQUIRED, UNIQUE }
    STATES { a }
    INITIAL STATE a
    TERMINAL STATES { a }
    TRANSITIONS {}
)
"#;
    let m = parse_one_machine(input);
    assert_eq!(m.data[0].constraints.len(), 2);
    assert_eq!(m.data[0].constraints[0], Constraint::Required);
    assert_eq!(m.data[0].constraints[1], Constraint::Unique);
}

// ---- SET type parsing ----

#[test]
fn parse_set_type() {
    let input = r#"
DEFINE MACHINE WithSet (
    DATA { labels : SET(TEXT) -> OPTIONAL }
    STATES { a }
    INITIAL STATE a
    TERMINAL STATES { a }
    TRANSITIONS {}
)
"#;
    let m = parse_one_machine(input);
    assert_eq!(
        m.data[0].field_type,
        TypeDefinition::Set(Box::new(TypeDefinition::Text))
    );
}

// ---- MONEY type parsing ----

#[test]
fn parse_money_type() {
    let input = r#"
DEFINE MACHINE WithMoney (
    DATA { total : MONEY(USD) -> REQUIRED }
    STATES { a }
    INITIAL STATE a
    TERMINAL STATES { a }
    TRANSITIONS {}
)
"#;
    let m = parse_one_machine(input);
    assert_eq!(m.data[0].field_type, TypeDefinition::Money("USD".into()));
}

// ---- REF type parsing ----

#[test]
fn parse_ref_type() {
    let input = r#"
DEFINE MACHINE WithRef (
    DATA { owner : REF(Agent) -> OPTIONAL }
    STATES { a }
    INITIAL STATE a
    TERMINAL STATES { a }
    TRANSITIONS {}
)
"#;
    let m = parse_one_machine(input);
    assert_eq!(m.data[0].field_type, TypeDefinition::Ref("Agent".into()));
}

// ---- NOTIFY action in transition ----

#[test]
fn parse_notify_action() {
    let input = r#"
DEFINE MACHINE WithNotify (
    STATES { a, b }
    INITIAL STATE a
    TERMINAL STATES { b }
    TRANSITIONS {
        a -> b {
            ACTION : NOTIFY(assignee, "task.complete")
        }
    }
)
"#;
    let m = parse_one_machine(input);
    match &m.transitions[0].actions[0] {
        Action::Notify { target, event } => {
            assert_eq!(event, "task.complete");
            // target is an expression (field access to 'assignee')
            match &target.kind {
                smql_ast::expression::ExpressionKind::FieldAccess(path) => {
                    assert_eq!(path, &vec!["assignee".to_string()]);
                }
                other => panic!("Expected FieldAccess, got {:?}", other),
            }
        }
        other => panic!("Expected Notify, got {:?}", other),
    }
}

// ---- TRANSITION ALL (batch) ----

#[test]
fn parse_transition_all_batch() {
    let input = r#"TRANSITION ALL Order WHERE STATE IS draft TO cancelled"#;
    let cmd = parse_one_command(input);
    match cmd {
        Command::BatchTransition(b) => {
            assert_eq!(b.machine, "Order");
            assert_eq!(b.to_state, "cancelled");
        }
        other => panic!("Expected BatchTransition, got {:?}", other),
    }
}

// ---- String default value ----

#[test]
fn parse_string_default_value() {
    let input = r#"
DEFINE MACHINE WithStringDefault (
    DATA { status : TEXT -> DEFAULT("pending") }
    STATES { a }
    INITIAL STATE a
    TERMINAL STATES { a }
    TRANSITIONS {}
)
"#;
    let m = parse_one_machine(input);
    assert_eq!(
        m.data[0].constraints,
        vec![Constraint::Default(DefaultValue::String("pending".into()))]
    );
}

// ---- Int default value ----

#[test]
fn parse_int_default_value() {
    let input = r#"
DEFINE MACHINE WithIntDefault (
    DATA { count : INT -> DEFAULT(0) }
    STATES { a }
    INITIAL STATE a
    TERMINAL STATES { a }
    TRANSITIONS {}
)
"#;
    let m = parse_one_machine(input);
    assert_eq!(
        m.data[0].constraints,
        vec![Constraint::Default(DefaultValue::Int(0))]
    );
}

// ---- TIMEOUT in transition ----

#[test]
fn parse_timeout_in_transition() {
    let input = r#"
DEFINE MACHINE WithTimeout (
    STATES { waiting, expired, done }
    INITIAL STATE waiting
    TERMINAL STATES { done, expired }
    TRANSITIONS {
        waiting -> done {}
        waiting -> expired {
            TIMEOUT 72h -> expired
        }
    }
)
"#;
    let m = parse_one_machine(input);
    let timeout = m.transitions[1].timeout.as_ref().unwrap();
    assert_eq!(timeout.duration.seconds, 72 * 3600);
    assert_eq!(timeout.target_state, "expired");
}

// ---- ANY EXCEPT FROM in transition ----

#[test]
fn parse_any_except_from() {
    let input = r#"
DEFINE MACHINE WithAnyExcept (
    STATES { a, b, c }
    INITIAL STATE a
    TERMINAL STATES { c }
    TRANSITIONS {
        a -> b {}
        b -> c {}
        ANY -> c {
            EXCEPT FROM { c }
        }
    }
)
"#;
    let m = parse_one_machine(input);
    match &m.transitions[2].from {
        TransitionSource::Any { except } => {
            assert_eq!(except, &vec!["c".to_string()]);
        }
        other => panic!("Expected Any, got {:?}", other),
    }
}

// ---- HOOK with ACTION : prefix inside body ----

#[test]
fn parse_hook_with_action_prefix() {
    let input = r#"
DEFINE MACHINE WithActionPrefix (
    STATES { a, b }
    INITIAL STATE a
    TERMINAL STATES { b }
    TRANSITIONS { a -> b {} }
    HOOKS {
        ON SPAWN {
            ACTION : LOG("with prefix")
        }
    }
)
"#;
    let m = parse_one_machine(input);
    assert_eq!(m.hooks[0].actions.len(), 1);
    match &m.hooks[0].actions[0] {
        Action::Log(msg) => assert_eq!(msg, "with prefix"),
        other => panic!("Expected Log, got {:?}", other),
    }
}

// ===========================================================================
// Additional coverage tests
// ===========================================================================

// ---- Multiple statements in single parse call ----

#[test]
fn parse_multiple_statements() {
    let input = r#"
SPAWN Machine { x: 1 }
GET Machine "abc123"
TRANSITION Machine tk TO done
FIND Machine WHERE STATE IS open LIMIT 10
"#;
    let stmts = smql_parser::parse(input).unwrap();
    assert_eq!(stmts.len(), 4);
    assert!(matches!(&stmts[0], Statement::Command(Command::Spawn(_))));
    assert!(matches!(&stmts[1], Statement::Query(Query::Get(_))));
    assert!(matches!(
        &stmts[2],
        Statement::Command(Command::Transition(_))
    ));
    assert!(matches!(&stmts[3], Statement::Query(Query::Find(_))));
}

// ---- Error: unexpected token at start of statement ----

#[test]
fn parse_error_unexpected_token_at_start() {
    let input = "FOOBAR something";
    let result = smql_parser::parse(input);
    assert!(
        result.is_err(),
        "Expected parse error for unknown statement keyword"
    );
}

#[test]
fn parse_error_incomplete_machine() {
    let input = "DEFINE MACHINE Broken (";
    let result = smql_parser::parse_machine(input);
    assert!(
        result.is_err(),
        "Expected parse error for incomplete machine"
    );
}

// ---- BATCH TRANSITION with WITH / MEMO / AS clauses ----

#[test]
fn parse_batch_transition_with_clauses() {
    let input = r#"TRANSITION ALL Order WHERE STATE IS draft TO cancelled WITH { reason: "bulk close" } MEMO "cleanup" AS admin"#;
    let cmd = parse_one_command(input);
    match cmd {
        Command::BatchTransition(b) => {
            assert_eq!(b.machine, "Order");
            assert_eq!(b.to_state, "cancelled");
            assert_eq!(b.with_data.len(), 1);
            assert_eq!(b.with_data[0].0, "reason");
            assert_eq!(b.memo, Some("cleanup".into()));
            assert_eq!(b.as_actor, Some("admin".into()));
        }
        other => panic!("Expected BatchTransition, got {:?}", other),
    }
}

// ---- FIND with explicit SORT ASC ----

#[test]
fn parse_find_sort_asc_explicit() {
    let input = "FIND Ticket WHERE priority > 1 SORT BY created_at ASC LIMIT 5";
    let stmts = smql_parser::parse(input).unwrap();
    match &stmts[0] {
        Statement::Query(Query::Find(f)) => {
            assert_eq!(f.machine, "Ticket");
            assert!(f.filter.is_some());
            assert_eq!(f.sort.len(), 1);
            assert_eq!(f.sort[0].field, "created_at");
            assert_eq!(f.sort[0].direction, smql_ast::types::SortDirection::Asc);
            assert_eq!(f.limit, Some(5));
        }
        other => panic!("Expected Find, got {:?}", other),
    }
}

// ---- AGGREGATE with WHERE filter ----

#[test]
fn parse_aggregate_with_where_filter() {
    let input = r#"AGGREGATE Ticket MEASURE COUNT() WHERE priority == "high" GROUP BY STATE"#;
    let stmts = smql_parser::parse(input).unwrap();
    match &stmts[0] {
        Statement::Query(Query::Aggregate(a)) => {
            assert_eq!(a.machine, "Ticket");
            assert_eq!(a.measures.len(), 1);
            assert!(a.filter.is_some());
            assert_eq!(a.group_by.len(), 1);
        }
        other => panic!("Expected Aggregate, got {:?}", other),
    }
}

// ---- AGGREGATE measure with alias (AS) ----

#[test]
fn parse_aggregate_measure_with_alias() {
    let input = "AGGREGATE M MEASURE COUNT() AS total, SUM(amount) AS sum_amount GROUP BY region";
    let stmts = smql_parser::parse(input).unwrap();
    match &stmts[0] {
        Statement::Query(Query::Aggregate(a)) => {
            assert_eq!(a.measures.len(), 2);
            assert_eq!(a.measures[0].alias, Some("total".into()));
            assert_eq!(a.measures[1].alias, Some("sum_amount".into()));
            assert_eq!(a.measures[1].field, Some("amount".into()));
            assert_eq!(a.group_by.len(), 1);
        }
        other => panic!("Expected Aggregate, got {:?}", other),
    }
}

// ---- FUNNEL with WHERE filter ----

#[test]
fn parse_funnel_with_filter() {
    let input = r#"FUNNEL Order THROUGH [draft, placed, paid] WHERE priority == "high""#;
    let stmts = smql_parser::parse(input).unwrap();
    match &stmts[0] {
        Statement::Query(Query::Funnel(f)) => {
            assert_eq!(f.machine, "Order");
            assert_eq!(f.states, vec!["draft", "placed", "paid"]);
            assert!(f.filter.is_some());
        }
        other => panic!("Expected Funnel, got {:?}", other),
    }
}

// ---- PATHS with WHERE filter and LIMIT ----

#[test]
fn parse_paths_with_filter_and_limit() {
    let input = "PATHS FROM Ticket WHERE STATE IS open LIMIT 20";
    let stmts = smql_parser::parse(input).unwrap();
    match &stmts[0] {
        Statement::Query(Query::Paths(p)) => {
            assert_eq!(p.machine, "Ticket");
            assert!(p.filter.is_some());
            assert_eq!(p.limit, Some(20));
        }
        other => panic!("Expected Paths, got {:?}", other),
    }
}

// ---- COMPARE PATHS with WHERE filter ----

#[test]
fn parse_compare_paths_with_filter() {
    let input = r#"COMPARE PATHS Ticket SEGMENT BY region WHERE priority == "high""#;
    let stmts = smql_parser::parse(input).unwrap();
    match &stmts[0] {
        Statement::Query(Query::ComparePaths(c)) => {
            assert_eq!(c.machine, "Ticket");
            assert_eq!(c.segment_by, "region");
            assert!(c.filter.is_some());
        }
        other => panic!("Expected ComparePaths, got {:?}", other),
    }
}

// ---- TRAIL with WHERE (triggers TrailFilter branch) ----

#[test]
fn parse_trail_with_where() {
    let input = "TRAIL OF tk_123 WHERE";
    let stmts = smql_parser::parse(input).unwrap();
    match &stmts[0] {
        Statement::Query(Query::Trail(t)) => {
            assert_eq!(t.instance_id, "tk_123");
            assert!(t.filter.is_some());
        }
        other => panic!("Expected Trail, got {:?}", other),
    }
}

// ---- parse_machines with multiple machines ----

#[test]
fn parse_multiple_machines() {
    let input = r#"
DEFINE MACHINE Order (
    STATES { draft, placed }
    INITIAL STATE draft
    TERMINAL STATES { placed }
    TRANSITIONS { draft -> placed {} }
)
DEFINE MACHINE LineItem (
    STATES { active, removed }
    INITIAL STATE active
    TERMINAL STATES { removed }
    TRANSITIONS { active -> removed {} }
    PARENT : Order
)
"#;
    let machines = smql_parser::parse_machines(input).unwrap();
    assert_eq!(machines.len(), 2);
    assert_eq!(machines[0].name, "Order");
    assert_eq!(machines[1].name, "LineItem");
    assert_eq!(machines[1].parent, Some("Order".into()));
}

// ---- Additional data types: DURATION, DATE, DATETIME, UUID, BOOL, JSON, BLOB ----

#[test]
fn parse_all_data_types() {
    let input = r#"
DEFINE MACHINE AllTypes (
    DATA {
        txt: TEXT
        num: INT
        flt: FLOAT
        flag: BOOL
        uid: UUID
        dt: DATE
        dttm: DATETIME
        dur: DURATION
        blb: BLOB
        jsn: JSON
        enm: ENUM(a, b, c)
    }
    STATES { s }
    INITIAL STATE s
    TERMINAL STATES { s }
    TRANSITIONS {}
)
"#;
    let m = parse_one_machine(input);
    assert_eq!(m.data.len(), 11);
    assert_eq!(m.data[0].field_type, TypeDefinition::Text);
    assert_eq!(m.data[1].field_type, TypeDefinition::Int);
    assert_eq!(m.data[2].field_type, TypeDefinition::Float);
    assert_eq!(m.data[3].field_type, TypeDefinition::Bool);
    assert_eq!(m.data[4].field_type, TypeDefinition::Uuid);
    assert_eq!(m.data[5].field_type, TypeDefinition::Date);
    assert_eq!(m.data[6].field_type, TypeDefinition::DateTime);
    assert_eq!(m.data[7].field_type, TypeDefinition::Duration);
    assert_eq!(m.data[8].field_type, TypeDefinition::Blob);
    assert_eq!(m.data[9].field_type, TypeDefinition::Json);
    assert_eq!(
        m.data[10].field_type,
        TypeDefinition::Enum(vec!["a".into(), "b".into(), "c".into()])
    );
}

// ---- LIST child with MIN only ----

#[test]
fn parse_children_list_with_min_only() {
    let input = r#"
DEFINE MACHINE WithMinOnly (
    CHILDREN { items : LIST(Item) -> MIN(2) }
    STATES { a }
    INITIAL STATE a
    TERMINAL STATES { a }
    TRANSITIONS {}
)
"#;
    let m = parse_one_machine(input);
    match &m.children[0].cardinality {
        ChildCardinality::List { min, max } => {
            assert_eq!(*min, Some(2));
            assert_eq!(*max, None);
        }
        other => panic!("Expected List cardinality, got {:?}", other),
    }
}

// ---- SPAWN THEN TRANSITION via parse() ----

#[test]
fn parse_spawn_then_transition_toplevel() {
    let input = r#"SPAWN Machine { x: 1 } THEN TRANSITION TO active"#;
    let stmts = smql_parser::parse(input).unwrap();
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Statement::Command(Command::Spawn(s)) => {
            assert_eq!(s.machine, "Machine");
            assert_eq!(s.then_transition, Some("active".into()));
            assert_eq!(s.data.len(), 1);
        }
        other => panic!("Expected Spawn, got {:?}", other),
    }
}

// ---- EMIT with payload in transition ACTION ----

#[test]
fn parse_emit_with_payload_in_action() {
    let input = r#"
DEFINE MACHINE WithEmitPayload (
    STATES { a, b }
    INITIAL STATE a
    TERMINAL STATES { b }
    TRANSITIONS {
        a -> b {
            ACTION : EMIT("event_name", SELF)
        }
    }
)
"#;
    let m = parse_one_machine(input);
    match &m.transitions[0].actions[0] {
        Action::Emit { event, payload } => {
            assert_eq!(event, "event_name");
            assert!(payload.is_some());
        }
        other => panic!("Expected Emit, got {:?}", other),
    }
}

// ---- AGGREGATE with GROUP BY multiple fields ----

#[test]
fn parse_aggregate_group_by_multiple_fields() {
    let input = "AGGREGATE M MEASURE COUNT() GROUP BY STATE, region, priority";
    let stmts = smql_parser::parse(input).unwrap();
    match &stmts[0] {
        Statement::Query(Query::Aggregate(a)) => {
            assert_eq!(a.group_by.len(), 3);
            match &a.group_by[0] {
                smql_ast::query::GroupByClause::State => {}
                other => panic!("Expected State, got {:?}", other),
            }
            match &a.group_by[1] {
                smql_ast::query::GroupByClause::Field(f) => assert_eq!(f, "region"),
                other => panic!("Expected Field, got {:?}", other),
            }
            match &a.group_by[2] {
                smql_ast::query::GroupByClause::Field(f) => assert_eq!(f, "priority"),
                other => panic!("Expected Field, got {:?}", other),
            }
        }
        other => panic!("Expected Aggregate, got {:?}", other),
    }
}

// ---- FIND with multiple SORT fields ----

#[test]
fn parse_find_multiple_sort_fields() {
    let input = "FIND Ticket SORT BY priority DESC, created_at ASC";
    let stmts = smql_parser::parse(input).unwrap();
    match &stmts[0] {
        Statement::Query(Query::Find(f)) => {
            assert_eq!(f.sort.len(), 2);
            assert_eq!(f.sort[0].field, "priority");
            assert_eq!(f.sort[0].direction, smql_ast::types::SortDirection::Desc);
            assert_eq!(f.sort[1].field, "created_at");
            assert_eq!(f.sort[1].direction, smql_ast::types::SortDirection::Asc);
        }
        other => panic!("Expected Find, got {:?}", other),
    }
}

// ---- ALTER MACHINE with multiple operations ----

#[test]
fn parse_alter_machine_multiple_operations() {
    let input =
        "ALTER MACHINE Ticket ADD STATE review ADD TRANSITION open -> review REMOVE DATA old_field";
    let cmd = parse_one_command(input);
    match cmd {
        Command::AlterMachine(a) => {
            assert_eq!(a.machine, "Ticket");
            assert_eq!(a.operations.len(), 3);
            assert!(matches!(&a.operations[0], AlterOperation::AddState(s) if s == "review"));
            assert!(matches!(&a.operations[1], AlterOperation::AddTransition(_)));
            assert!(matches!(&a.operations[2], AlterOperation::RemoveData(f) if f == "old_field"));
        }
        other => panic!("Expected AlterMachine, got {:?}", other),
    }
}

// ---- Nested LIST type ----

#[test]
fn parse_nested_list_type() {
    let input = r#"
DEFINE MACHINE Nested (
    DATA { matrix : LIST(LIST(INT)) }
    STATES { a }
    INITIAL STATE a
    TERMINAL STATES { a }
    TRANSITIONS {}
)
"#;
    let m = parse_one_machine(input);
    assert_eq!(
        m.data[0].field_type,
        TypeDefinition::List(Box::new(TypeDefinition::List(Box::new(
            TypeDefinition::Int
        ))))
    );
}

// ---- Transition with all optional clauses combined ----

#[test]
fn parse_transition_with_through_or_stay_cascade() {
    let input = r#"TRANSITION Machine inst TO done THROUGH [step1] OR_STAY CASCADE"#;
    let cmd = parse_one_command(input);
    match cmd {
        Command::Transition(t) => {
            assert_eq!(t.to_state, "done");
            assert_eq!(t.through, vec!["step1".to_string()]);
            assert!(t.or_stay);
            assert!(t.cascade);
        }
        other => panic!("Expected Transition, got {:?}", other),
    }
}

// ---- Old syntax without machine name should fail ----

#[test]
fn parse_transition_without_machine_name_fails() {
    // Old syntax: TRANSITION "id" TO state (no machine name) should now fail
    // because the parser expects: TRANSITION Machine "id" TO state
    // When given `TRANSITION "id" TO state`, the parser reads the string as ident which fails.
    let input = r#"TRANSITION "01ABC" TO done"#;
    let result = smql_parser::parse(input);
    assert!(
        result.is_err(),
        "Old syntax without machine name should fail"
    );
}
