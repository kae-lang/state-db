#[cfg(test)]
mod lexer_tests {
    // Lexer tests are in lexer.rs
}

#[cfg(test)]
mod expression_tests {
    use crate::lexer;
    use crate::expr;
    use crate::Parser;
    use smql_ast::expression::*;
    use smql_ast::value::Value;

    fn parse_expr(input: &str) -> Expression {
        let tokens = lexer::tokenize(input).unwrap();
        let mut parser = Parser::new(&tokens, input);
        expr::parse_expression(&mut parser).unwrap()
    }

    #[test]
    fn parse_simple_literal() {
        let e = parse_expr("42");
        assert_eq!(e.kind, ExpressionKind::Literal(Value::Int(42)));
    }

    #[test]
    fn parse_string_literal() {
        let e = parse_expr("\"hello\"");
        assert_eq!(e.kind, ExpressionKind::Literal(Value::Text("hello".into())));
    }

    #[test]
    fn parse_bool_literals() {
        let t = parse_expr("TRUE");
        assert_eq!(t.kind, ExpressionKind::Literal(Value::Bool(true)));
        let f = parse_expr("FALSE");
        assert_eq!(f.kind, ExpressionKind::Literal(Value::Bool(false)));
    }

    #[test]
    fn parse_field_access() {
        let e = parse_expr("priority");
        assert_eq!(e.kind, ExpressionKind::FieldAccess(vec!["priority".into()]));
    }

    #[test]
    fn parse_dotted_field_access() {
        let e = parse_expr("a.b.c");
        // This should be FieldAccess with ["a", "b", "c"] since `a` is an identifier
        // parsed as FieldAccess(["a"]), then dots extend it
        assert_eq!(e.kind, ExpressionKind::FieldAccess(vec!["a".into(), "b".into(), "c".into()]));
    }

    #[test]
    fn parse_comparison() {
        let e = parse_expr("x > 10");
        match e.kind {
            ExpressionKind::BinaryOp { op, .. } => assert_eq!(op, BinaryOperator::Gt),
            _ => panic!("Expected BinaryOp"),
        }
    }

    #[test]
    fn parse_equality() {
        let e = parse_expr("x == 5");
        match e.kind {
            ExpressionKind::BinaryOp { op, .. } => assert_eq!(op, BinaryOperator::Eq),
            _ => panic!("Expected BinaryOp"),
        }
    }

    #[test]
    fn parse_and_or() {
        let e = parse_expr("a == 1 AND b == 2");
        match e.kind {
            ExpressionKind::BinaryOp { op, .. } => assert_eq!(op, BinaryOperator::And),
            _ => panic!("Expected AND"),
        }
    }

    #[test]
    fn parse_or_lower_precedence_than_and() {
        // a OR b AND c should parse as a OR (b AND c)
        let e = parse_expr("a OR b AND c");
        match &e.kind {
            ExpressionKind::BinaryOp { op, right, .. } => {
                assert_eq!(*op, BinaryOperator::Or);
                match &right.kind {
                    ExpressionKind::BinaryOp { op: inner_op, .. } => {
                        assert_eq!(*inner_op, BinaryOperator::And);
                    }
                    _ => panic!("Expected AND on right"),
                }
            }
            _ => panic!("Expected OR"),
        }
    }

    #[test]
    fn parse_is_set() {
        let e = parse_expr("assignee IS SET");
        match e.kind {
            ExpressionKind::IsSet(_) => {}
            _ => panic!("Expected IsSet"),
        }
    }

    #[test]
    fn parse_is_not_set() {
        let e = parse_expr("assignee IS NOT SET");
        match e.kind {
            ExpressionKind::IsNotSet(_) => {}
            _ => panic!("Expected IsNotSet"),
        }
    }

    #[test]
    fn parse_is_null() {
        let e = parse_expr("assignee IS NULL");
        match e.kind {
            ExpressionKind::IsNotSet(_) => {}
            _ => panic!("Expected IsNotSet for IS NULL"),
        }
    }

    #[test]
    fn parse_in_set() {
        let e = parse_expr("x IN { 1, 2, 3 }");
        match e.kind {
            ExpressionKind::InSet { values, .. } => assert_eq!(values.len(), 3),
            _ => panic!("Expected InSet"),
        }
    }

    #[test]
    fn parse_in_list_parens() {
        let e = parse_expr("role IN (\"admin\", \"supervisor\")");
        match e.kind {
            ExpressionKind::InList { values, .. } => assert_eq!(values.len(), 2),
            _ => panic!("Expected InList"),
        }
    }

    #[test]
    fn parse_self_ref() {
        let e = parse_expr("SELF");
        assert_eq!(e.kind, ExpressionKind::SelfRef);
    }

    #[test]
    fn parse_actor_ref() {
        let e = parse_expr("ACTOR");
        assert_eq!(e.kind, ExpressionKind::ActorRef);
    }

    #[test]
    fn parse_actor_dot_access() {
        let e = parse_expr("ACTOR.role");
        match e.kind {
            ExpressionKind::QualifiedAccess { root, path } => {
                assert_eq!(root.kind, ExpressionKind::ActorRef);
                assert_eq!(path, vec!["role".to_string()]);
            }
            _ => panic!("Expected QualifiedAccess, got {:?}", e.kind),
        }
    }

    #[test]
    fn parse_state_is() {
        let e = parse_expr("STATE IS open");
        assert_eq!(e.kind, ExpressionKind::StateIs("open".into()));
    }

    #[test]
    fn parse_state_in() {
        let e = parse_expr("STATE IN { open, triaged }");
        match e.kind {
            ExpressionKind::StateIn(states) => {
                assert_eq!(states, vec!["open", "triaged"]);
            }
            _ => panic!("Expected StateIn"),
        }
    }

    #[test]
    fn parse_function_call() {
        let e = parse_expr("elapsed()");
        match e.kind {
            ExpressionKind::FunctionCall { name, args } => {
                assert_eq!(name, "elapsed");
                assert!(args.is_empty());
            }
            _ => panic!("Expected FunctionCall"),
        }
    }

    #[test]
    fn parse_function_with_args() {
        let e = parse_expr("elapsed_since(resolved)");
        match e.kind {
            ExpressionKind::FunctionCall { name, args } => {
                assert_eq!(name, "elapsed_since");
                assert_eq!(args.len(), 1);
            }
            _ => panic!("Expected FunctionCall"),
        }
    }

    #[test]
    fn parse_duration_comparison() {
        let e = parse_expr("elapsed() > 24h");
        match e.kind {
            ExpressionKind::BinaryOp { op, .. } => assert_eq!(op, BinaryOperator::Gt),
            _ => panic!("Expected BinaryOp"),
        }
    }

    #[test]
    fn parse_complex_guard() {
        // ACTOR == assignee OR ACTOR.role == "admin"
        let e = parse_expr("ACTOR == assignee OR ACTOR.role == \"admin\"");
        match e.kind {
            ExpressionKind::BinaryOp { op, .. } => assert_eq!(op, BinaryOperator::Or),
            _ => panic!("Expected OR"),
        }
    }

    #[test]
    fn parse_all_predicate() {
        let e = parse_expr("ALL(items, STATE IS confirmed)");
        match e.kind {
            ExpressionKind::All { .. } => {}
            _ => panic!("Expected All"),
        }
    }

    #[test]
    fn parse_count() {
        let e = parse_expr("COUNT()");
        match e.kind {
            ExpressionKind::Count(None) => {}
            _ => panic!("Expected Count(None)"),
        }
    }

    #[test]
    fn parse_not() {
        let e = parse_expr("NOT x");
        match e.kind {
            ExpressionKind::UnaryOp { op, .. } => assert_eq!(op, UnaryOperator::Not),
            _ => panic!("Expected UnaryOp NOT"),
        }
    }

    #[test]
    fn parse_parenthesized() {
        let e = parse_expr("(x + 1) * 2");
        match e.kind {
            ExpressionKind::BinaryOp { op, .. } => assert_eq!(op, BinaryOperator::Mul),
            _ => panic!("Expected Mul"),
        }
    }
}

#[cfg(test)]
mod machine_tests {
    use crate::parse_machine;
    use crate::parse_machines;

    #[test]
    fn parse_simple_machine() {
        let input = r#"
DEFINE MACHINE Simple (
    STATES { a, b, c }
    INITIAL STATE a
    TERMINAL STATES { c }
    TRANSITIONS {
        a -> b {}
        b -> c {}
    }
)
"#;
        let m = parse_machine(input).unwrap();
        assert_eq!(m.name, "Simple");
        assert_eq!(m.states.len(), 3);
        assert_eq!(m.initial_state, "a");
        assert_eq!(m.terminal_states, vec!["c"]);
        assert_eq!(m.transitions.len(), 2);
    }

    #[test]
    fn parse_machine_with_data() {
        let input = r#"
DEFINE MACHINE WithData (
    DATA {
        name : TEXT -> REQUIRED
        count : INT -> DEFAULT(0)
        priority : ENUM(low, medium, high) -> DEFAULT(medium)
    }
    STATES { open, closed }
    INITIAL STATE open
    TERMINAL STATES { closed }
    TRANSITIONS {
        open -> closed {}
    }
)
"#;
        let m = parse_machine(input).unwrap();
        assert_eq!(m.data.len(), 3);
        assert_eq!(m.data[0].name, "name");
        assert_eq!(m.data[1].name, "count");
        assert_eq!(m.data[2].name, "priority");
    }

    #[test]
    fn parse_machine_with_guards() {
        let input = r#"
DEFINE MACHINE Guarded (
    STATES { open, closed }
    INITIAL STATE open
    TERMINAL STATES { closed }
    TRANSITIONS {
        open -> closed {
            GUARD : assignee IS SET
            GUARD : ACTOR.role == "admin"
        }
    }
)
"#;
        let m = parse_machine(input).unwrap();
        assert_eq!(m.transitions[0].guards.len(), 2);
    }

    #[test]
    fn parse_machine_with_actions() {
        let input = r#"
DEFINE MACHINE WithActions (
    STATES { a, b }
    INITIAL STATE a
    TERMINAL STATES { b }
    TRANSITIONS {
        a -> b {
            ACTION : NOTIFY(customer, "event.happened")
            ACTION : LOG("Transitioned")
        }
    }
)
"#;
        let m = parse_machine(input).unwrap();
        assert_eq!(m.transitions[0].actions.len(), 2);
    }

    #[test]
    fn parse_machine_with_timeout() {
        let input = r#"
DEFINE MACHINE WithTimeout (
    STATES { waiting, done }
    INITIAL STATE waiting
    TERMINAL STATES { done }
    TRANSITIONS {
        waiting -> done {
            TIMEOUT : 72h -> done
        }
    }
)
"#;
        let m = parse_machine(input).unwrap();
        let timeout = m.transitions[0].timeout.as_ref().unwrap();
        assert_eq!(timeout.duration.seconds, 72 * 3600);
        assert_eq!(timeout.target_state, "done");
    }

    #[test]
    fn parse_machine_with_wildcard() {
        let input = r#"
DEFINE MACHINE WithWildcard (
    STATES { a, b, c, d }
    INITIAL STATE a
    TERMINAL STATES { d }
    TRANSITIONS {
        a -> b {}
        b -> c {}
        c -> d {}
        ANY -> d {
            EXCEPT FROM { d }
        }
    }
)
"#;
        let m = parse_machine(input).unwrap();
        assert_eq!(m.transitions.len(), 4);
        let wildcard = &m.transitions[3];
        match &wildcard.from {
            smql_ast::machine::TransitionSource::Any { except } => {
                assert_eq!(except, &vec!["d".to_string()]);
            }
            _ => panic!("Expected Any"),
        }
    }

    #[test]
    fn parse_machine_with_children() {
        let input = r#"
DEFINE MACHINE Parent (
    STATES { active, done }
    INITIAL STATE active
    TERMINAL STATES { done }
    CHILDREN {
        items : LIST(Item) -> MIN(1)
        config : OPTIONAL(Config)
    }
    TRANSITIONS {
        active -> done {}
    }
)
"#;
        let m = parse_machine(input).unwrap();
        assert_eq!(m.children.len(), 2);
        assert_eq!(m.children[0].name, "items");
        assert_eq!(m.children[0].machine, "Item");
        assert_eq!(m.children[1].name, "config");
        assert_eq!(m.children[1].machine, "Config");
    }

    #[test]
    fn parse_machine_with_parent() {
        let input = r#"
DEFINE MACHINE Child (
    PARENT : Parent
    STATES { pending, done }
    INITIAL STATE pending
    TERMINAL STATES { done }
    TRANSITIONS {
        pending -> done {}
    }
)
"#;
        let m = parse_machine(input).unwrap();
        assert_eq!(m.parent, Some("Parent".to_string()));
    }

    #[test]
    fn parse_machine_with_mutate() {
        let input = r#"
DEFINE MACHINE WithMutate (
    STATES { a, b }
    INITIAL STATE a
    TERMINAL STATES { b }
    TRANSITIONS {
        a -> b {
            MUTATE : priority = critical
        }
    }
)
"#;
        let m = parse_machine(input).unwrap();
        assert_eq!(m.transitions[0].mutates.len(), 1);
        assert_eq!(m.transitions[0].mutates[0].field, "priority");
    }

    #[test]
    fn parse_support_ticket_smql() {
        let input = std::fs::read_to_string(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/support_ticket.smql")
        ).unwrap();
        let m = parse_machine(&input).unwrap();
        assert_eq!(m.name, "SupportTicket");
        assert_eq!(m.states.len(), 7);
        assert_eq!(m.data.len(), 8);
        assert_eq!(m.initial_state, "open");
        assert_eq!(m.terminal_states, vec!["closed"]);
        assert!(m.transitions.len() >= 8);
    }

    #[test]
    fn parse_order_smql() {
        let input = std::fs::read_to_string(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/order.smql")
        ).unwrap();
        let machines = parse_machines(&input).unwrap();
        assert_eq!(machines.len(), 3);
        assert_eq!(machines[0].name, "Order");
        assert_eq!(machines[1].name, "LineItem");
        assert_eq!(machines[2].name, "Shipment");
    }
}

#[cfg(test)]
mod command_tests {
    use crate::parse;
    use smql_ast::command::{Statement, Command};

    #[test]
    fn parse_spawn_command() {
        let input = r#"SPAWN SupportTicket { subject: "Test", description: "Desc" }"#;
        let stmts = parse(input).unwrap();
        assert_eq!(stmts.len(), 1);
        match &stmts[0] {
            Statement::Command(Command::Spawn(s)) => {
                assert_eq!(s.machine, "SupportTicket");
                assert_eq!(s.data.len(), 2);
            }
            _ => panic!("Expected Spawn"),
        }
    }

    #[test]
    fn parse_spawn_then_transition() {
        let input = r#"SPAWN Machine { x: 1 } THEN TRANSITION TO active"#;
        let stmts = parse(input).unwrap();
        match &stmts[0] {
            Statement::Command(Command::Spawn(s)) => {
                assert_eq!(s.then_transition, Some("active".into()));
            }
            _ => panic!("Expected Spawn"),
        }
    }

    #[test]
    fn parse_transition_command() {
        let input = r#"TRANSITION SupportTicket tk_123 TO resolved WITH { resolution_note: "Fixed" } MEMO "Resolved the issue""#;
        let stmts = parse(input).unwrap();
        match &stmts[0] {
            Statement::Command(Command::Transition(t)) => {
                assert_eq!(t.machine, "SupportTicket");
                assert_eq!(t.instance_id, "tk_123");
                assert_eq!(t.to_state, "resolved");
                assert_eq!(t.with_data.len(), 1);
                assert_eq!(t.memo, Some("Resolved the issue".into()));
            }
            _ => panic!("Expected Transition"),
        }
    }

    #[test]
    fn parse_try_transition() {
        let input = r#"TRY TRANSITION SupportTicket tk_123 TO resolved"#;
        let stmts = parse(input).unwrap();
        match &stmts[0] {
            Statement::Command(Command::TryTransition(t)) => {
                assert_eq!(t.machine, "SupportTicket");
                assert_eq!(t.instance_id, "tk_123");
                assert_eq!(t.to_state, "resolved");
            }
            _ => panic!("Expected TryTransition"),
        }
    }

    #[test]
    fn parse_transition_string_id() {
        let input = r#"TRANSITION Machine "01ABC" TO resolved"#;
        let stmts = parse(input).unwrap();
        match &stmts[0] {
            Statement::Command(Command::Transition(t)) => {
                assert_eq!(t.machine, "Machine");
                assert_eq!(t.instance_id, "01ABC");
                assert_eq!(t.to_state, "resolved");
            }
            _ => panic!("Expected Transition"),
        }
    }

    #[test]
    fn parse_transition_with_as_actor() {
        let input = r#"TRANSITION Machine tk TO resolved AS "alice""#;
        let stmts = parse(input).unwrap();
        match &stmts[0] {
            Statement::Command(Command::Transition(t)) => {
                assert_eq!(t.machine, "Machine");
                assert_eq!(t.instance_id, "tk");
                assert_eq!(t.to_state, "resolved");
                assert_eq!(t.as_actor, Some("alice".to_string()));
            }
            _ => panic!("Expected Transition"),
        }
    }

    #[test]
    fn parse_try_transition_string_id() {
        let input = r#"TRY TRANSITION Machine "01XYZ" TO done"#;
        let stmts = parse(input).unwrap();
        match &stmts[0] {
            Statement::Command(Command::TryTransition(t)) => {
                assert_eq!(t.machine, "Machine");
                assert_eq!(t.instance_id, "01XYZ");
                assert_eq!(t.to_state, "done");
            }
            _ => panic!("Expected TryTransition"),
        }
    }

    #[test]
    fn parse_transition_cascade() {
        let input = "TRANSITION Machine tk TO cancelled CASCADE";
        let stmts = parse(input).unwrap();
        match &stmts[0] {
            Statement::Command(Command::Transition(t)) => {
                assert_eq!(t.machine, "Machine");
                assert_eq!(t.instance_id, "tk");
                assert_eq!(t.to_state, "cancelled");
                assert!(t.cascade);
            }
            _ => panic!("Expected Transition"),
        }
    }
}

#[cfg(test)]
mod query_tests {
    use crate::parse;
    use smql_ast::command::Statement;
    use smql_ast::query::Query;

    #[test]
    fn parse_get_query() {
        let input = "GET SupportTicket tk_123";
        let stmts = parse(input).unwrap();
        match &stmts[0] {
            Statement::Query(Query::Get(g)) => {
                assert_eq!(g.machine, "SupportTicket");
                assert_eq!(g.instance_id, "tk_123");
            }
            _ => panic!("Expected Get"),
        }
    }

    #[test]
    fn parse_find_query() {
        let input = "FIND SupportTicket WHERE STATE IS open SORT BY priority DESC LIMIT 10";
        let stmts = parse(input).unwrap();
        match &stmts[0] {
            Statement::Query(Query::Find(f)) => {
                assert_eq!(f.machine, "SupportTicket");
                assert!(f.filter.is_some());
                assert_eq!(f.sort.len(), 1);
                assert_eq!(f.limit, Some(10));
            }
            _ => panic!("Expected Find"),
        }
    }

    #[test]
    fn parse_find_no_filter() {
        let input = "FIND SupportTicket LIMIT 50";
        let stmts = parse(input).unwrap();
        match &stmts[0] {
            Statement::Query(Query::Find(f)) => {
                assert!(f.filter.is_none());
                assert_eq!(f.limit, Some(50));
            }
            _ => panic!("Expected Find"),
        }
    }

    #[test]
    fn parse_trail_query() {
        let input = "TRAIL OF tk_123";
        let stmts = parse(input).unwrap();
        match &stmts[0] {
            Statement::Query(Query::Trail(t)) => {
                assert_eq!(t.instance_id, "tk_123");
            }
            _ => panic!("Expected Trail"),
        }
    }

    #[test]
    fn parse_aggregate_query() {
        let input = "AGGREGATE SupportTicket MEASURE COUNT() GROUP BY STATE";
        let stmts = parse(input).unwrap();
        match &stmts[0] {
            Statement::Query(Query::Aggregate(a)) => {
                assert_eq!(a.machine, "SupportTicket");
                assert_eq!(a.measures.len(), 1);
                assert_eq!(a.group_by.len(), 1);
            }
            _ => panic!("Expected Aggregate"),
        }
    }

    #[test]
    fn parse_paths_query() {
        let input = "PATHS FROM SupportTicket LIMIT 5";
        let stmts = parse(input).unwrap();
        match &stmts[0] {
            Statement::Query(Query::Paths(p)) => {
                assert_eq!(p.machine, "SupportTicket");
                assert_eq!(p.limit, Some(5));
            }
            _ => panic!("Expected Paths"),
        }
    }

    #[test]
    fn parse_funnel_query() {
        let input = "FUNNEL Order THROUGH [draft, placed, paid, fulfilled, delivered]";
        let stmts = parse(input).unwrap();
        match &stmts[0] {
            Statement::Query(Query::Funnel(f)) => {
                assert_eq!(f.machine, "Order");
                assert_eq!(f.states.len(), 5);
            }
            _ => panic!("Expected Funnel"),
        }
    }

    #[test]
    fn parse_compare_paths_query() {
        let input = "COMPARE PATHS SupportTicket SEGMENT BY priority";
        let stmts = parse(input).unwrap();
        match &stmts[0] {
            Statement::Query(Query::ComparePaths(c)) => {
                assert_eq!(c.machine, "SupportTicket");
                assert_eq!(c.segment_by, "priority");
            }
            _ => panic!("Expected ComparePaths"),
        }
    }

    #[test]
    fn parse_get_string_id() {
        let input = r#"GET Machine "01ABC""#;
        let stmts = parse(input).unwrap();
        match &stmts[0] {
            Statement::Query(Query::Get(g)) => {
                assert_eq!(g.machine, "Machine");
                assert_eq!(g.instance_id, "01ABC");
            }
            _ => panic!("Expected Get"),
        }
    }

    #[test]
    fn parse_trail_string_id() {
        let input = r#"TRAIL OF "01ABC""#;
        let stmts = parse(input).unwrap();
        match &stmts[0] {
            Statement::Query(Query::Trail(t)) => {
                assert_eq!(t.instance_id, "01ABC");
            }
            _ => panic!("Expected Trail"),
        }
    }

    #[test]
    fn parse_find_with_sort_desc_and_offset() {
        let input = "FIND M WHERE x > 1 SORT BY x DESC LIMIT 10 OFFSET 5";
        let stmts = parse(input).unwrap();
        match &stmts[0] {
            Statement::Query(Query::Find(f)) => {
                assert_eq!(f.machine, "M");
                assert!(f.filter.is_some());
                assert_eq!(f.sort.len(), 1);
                assert_eq!(f.sort[0].field, "x");
                assert_eq!(f.sort[0].direction, smql_ast::types::SortDirection::Desc);
                assert_eq!(f.limit, Some(10));
                assert_eq!(f.offset, Some(5));
            }
            _ => panic!("Expected Find"),
        }
    }

    #[test]
    fn parse_aggregate_multiple_measures() {
        let input = "AGGREGATE M MEASURE COUNT(), SUM(x), AVG(y)";
        let stmts = parse(input).unwrap();
        match &stmts[0] {
            Statement::Query(Query::Aggregate(a)) => {
                assert_eq!(a.machine, "M");
                assert_eq!(a.measures.len(), 3);
            }
            _ => panic!("Expected Aggregate"),
        }
    }

    // --- Cursor-based pagination (AFTER) ---

    #[test]
    fn parse_find_after_alone() {
        let input = r#"FIND Order AFTER "01HWZK4G5C8T3RNMK1VNSH7HYM""#;
        let stmts = parse(input).unwrap();
        match &stmts[0] {
            Statement::Query(Query::Find(f)) => {
                assert_eq!(f.machine, "Order");
                assert_eq!(f.after, Some("01HWZK4G5C8T3RNMK1VNSH7HYM".to_string()));
                assert_eq!(f.limit, None);
                assert_eq!(f.offset, None);
            }
            _ => panic!("Expected Find"),
        }
    }

    #[test]
    fn parse_find_after_with_limit() {
        let input = r#"FIND Order LIMIT 20 AFTER "01HWZK4G5C8T3RNMK1VNSH7HYM""#;
        let stmts = parse(input).unwrap();
        match &stmts[0] {
            Statement::Query(Query::Find(f)) => {
                assert_eq!(f.machine, "Order");
                assert_eq!(f.limit, Some(20));
                assert_eq!(f.after, Some("01HWZK4G5C8T3RNMK1VNSH7HYM".to_string()));
            }
            _ => panic!("Expected Find"),
        }
    }

    #[test]
    fn parse_find_full_syntax_with_after() {
        let input = r#"FIND Order WHERE state == "open" SORT BY priority DESC LIMIT 10 OFFSET 0 AFTER "01ABC""#;
        let stmts = parse(input).unwrap();
        match &stmts[0] {
            Statement::Query(Query::Find(f)) => {
                assert_eq!(f.machine, "Order");
                assert!(f.filter.is_some());
                assert_eq!(f.sort.len(), 1);
                assert_eq!(f.limit, Some(10));
                assert_eq!(f.offset, Some(0));
                assert_eq!(f.after, Some("01ABC".to_string()));
            }
            _ => panic!("Expected Find"),
        }
    }
}
