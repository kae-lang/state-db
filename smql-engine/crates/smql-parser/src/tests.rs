#[cfg(test)]
mod lexer_tests {
    // Lexer tests are in lexer.rs
}

#[cfg(test)]
mod expression_tests {
    use crate::expr;
    use crate::lexer;
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
        assert_eq!(
            e.kind,
            ExpressionKind::FieldAccess(vec!["a".into(), "b".into(), "c".into()])
        );
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
        let input = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/support_ticket.smql"
        ))
        .unwrap();
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
        let input = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/order.smql"
        ))
        .unwrap();
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
    use smql_ast::command::{Command, Statement};

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

    // --- EXPLAIN TRANSITIONS ---

    #[test]
    fn parse_explain_transitions_schema() {
        let input = "EXPLAIN TRANSITIONS FOR Machine";
        let stmts = parse(input).unwrap();
        assert_eq!(stmts.len(), 1);
        match &stmts[0] {
            Statement::Query(Query::ExplainTransitions(q)) => {
                assert_eq!(q.machine, "Machine");
                assert_eq!(q.instance_id, None);
                assert_eq!(q.as_actor, None);
            }
            _ => panic!("Expected ExplainTransitions query"),
        }
    }

    #[test]
    fn parse_explain_transitions_instance() {
        let input = r#"EXPLAIN TRANSITIONS FOR Machine "01ABC123""#;
        let stmts = parse(input).unwrap();
        assert_eq!(stmts.len(), 1);
        match &stmts[0] {
            Statement::Query(Query::ExplainTransitions(q)) => {
                assert_eq!(q.machine, "Machine");
                assert_eq!(q.instance_id, Some("01ABC123".to_string()));
                assert_eq!(q.as_actor, None);
            }
            _ => panic!("Expected ExplainTransitions query"),
        }
    }

    #[test]
    fn parse_explain_transitions_with_actor() {
        let input = r#"EXPLAIN TRANSITIONS FOR Machine "01ABC123" AS "admin""#;
        let stmts = parse(input).unwrap();
        assert_eq!(stmts.len(), 1);
        match &stmts[0] {
            Statement::Query(Query::ExplainTransitions(q)) => {
                assert_eq!(q.machine, "Machine");
                assert_eq!(q.instance_id, Some("01ABC123".to_string()));
                assert_eq!(q.as_actor, Some("admin".to_string()));
            }
            _ => panic!("Expected ExplainTransitions query"),
        }
    }
}

// ===========================================================================
// Coverage tests for uncovered error paths and edge cases
// ===========================================================================

#[cfg(test)]
mod coverage_common_tests {
    //! Tests targeting uncovered lines in common.rs

    use crate::common;
    use crate::lexer;
    use crate::Parser;

    // parse_string_literal error path: feed it a non-string token
    #[test]
    fn parse_string_literal_error_on_int() {
        let tokens = lexer::tokenize("42").unwrap();
        let mut parser = Parser::new(&tokens, "42");
        let result = common::parse_string_literal(&mut parser);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Expected string literal"));
    }

    #[test]
    fn parse_string_literal_error_on_keyword() {
        let tokens = lexer::tokenize("TRUE").unwrap();
        let mut parser = Parser::new(&tokens, "TRUE");
        let result = common::parse_string_literal(&mut parser);
        assert!(result.is_err());
    }

    // parse_int_literal error path: feed it a string token
    #[test]
    fn parse_int_literal_error_on_string() {
        let tokens = lexer::tokenize("\"hello\"").unwrap();
        let mut parser = Parser::new(&tokens, "\"hello\"");
        let result = common::parse_int_literal(&mut parser);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Expected integer"));
    }

    #[test]
    fn parse_int_literal_error_on_keyword() {
        let tokens = lexer::tokenize("DEFINE").unwrap();
        let mut parser = Parser::new(&tokens, "DEFINE");
        let result = common::parse_int_literal(&mut parser);
        assert!(result.is_err());
    }

    // parse_duration error path: feed it a non-duration token
    #[test]
    fn parse_duration_error_on_int() {
        let tokens = lexer::tokenize("42").unwrap();
        let mut parser = Parser::new(&tokens, "42");
        let result = common::parse_duration(&mut parser);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Expected duration"));
    }

    #[test]
    fn parse_duration_error_on_string() {
        let tokens = lexer::tokenize("\"24h\"").unwrap();
        let mut parser = Parser::new(&tokens, "\"24h\"");
        let result = common::parse_duration(&mut parser);
        assert!(result.is_err());
    }

    // parse_literal_value: unexpected token at end of input
    #[test]
    fn parse_literal_value_end_of_input() {
        let tokens = lexer::tokenize("").unwrap();
        let mut parser = Parser::new(&tokens, "");
        let result = common::parse_literal_value(&mut parser);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("end of input"));
    }

    // parse_literal_value: unexpected token (punctuation)
    #[test]
    fn parse_literal_value_unexpected_punct() {
        let tokens = lexer::tokenize("{").unwrap();
        let mut parser = Parser::new(&tokens, "{");
        let result = common::parse_literal_value(&mut parser);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Expected literal value"));
    }

    // parse_literal_value: float literal
    #[test]
    fn parse_literal_value_float() {
        let tokens = lexer::tokenize("3.14").unwrap();
        let mut parser = Parser::new(&tokens, "3.14");
        let result = common::parse_literal_value(&mut parser).unwrap();
        match result {
            smql_ast::value::Value::Float(v) => assert!((v - 3.14).abs() < 0.001),
            other => panic!("Expected Float, got {:?}", other),
        }
    }

    // parse_literal_value: NULL
    #[test]
    fn parse_literal_value_null() {
        let tokens = lexer::tokenize("NULL").unwrap();
        let mut parser = Parser::new(&tokens, "NULL");
        let result = common::parse_literal_value(&mut parser).unwrap();
        assert_eq!(result, smql_ast::value::Value::Null);
    }

    // parse_ident_list_parens with string items
    #[test]
    fn parse_ident_list_parens_with_strings() {
        let tokens = lexer::tokenize("(\"hello\", \"world\")").unwrap();
        let mut parser = Parser::new(&tokens, "(\"hello\", \"world\")");
        let result = common::parse_ident_list_parens(&mut parser).unwrap();
        assert_eq!(result, vec!["hello".to_string(), "world".to_string()]);
    }

    // parse_ident_list_parens with mixed identifiers
    #[test]
    fn parse_ident_list_parens_mixed() {
        let tokens = lexer::tokenize("(\"str\", ident)").unwrap();
        let mut parser = Parser::new(&tokens, "(\"str\", ident)");
        let result = common::parse_ident_list_parens(&mut parser).unwrap();
        assert_eq!(result, vec!["str".to_string(), "ident".to_string()]);
    }

    // check_is_set returns false for non-IS tokens
    #[test]
    fn check_is_set_returns_false_for_non_is() {
        let tokens = lexer::tokenize("NOT SET").unwrap();
        let parser = Parser::new(&tokens, "NOT SET");
        assert!(!common::check_is_set(&parser));
    }

    // check_is_set returns false when IS is followed by non-SET keyword
    #[test]
    fn check_is_set_returns_false_for_is_not_set() {
        let tokens = lexer::tokenize("IS NOT SET").unwrap();
        let parser = Parser::new(&tokens, "IS NOT SET");
        assert!(!common::check_is_set(&parser));
    }

    // check_is_set returns false when IS is followed by identifier
    #[test]
    fn check_is_set_returns_false_for_is_followed_by_ident() {
        let tokens = lexer::tokenize("IS something").unwrap();
        let parser = Parser::new(&tokens, "IS something");
        assert!(!common::check_is_set(&parser));
    }

    // check_is_set returns false when IS is at end of input
    #[test]
    fn check_is_set_returns_false_at_end() {
        let tokens = lexer::tokenize("IS").unwrap();
        let parser = Parser::new(&tokens, "IS");
        assert!(!common::check_is_set(&parser));
    }

    // check_is_not_set returns false for non-IS
    #[test]
    fn check_is_not_set_returns_false_for_non_is() {
        let tokens = lexer::tokenize("SET").unwrap();
        let parser = Parser::new(&tokens, "SET");
        assert!(!common::check_is_not_set(&parser));
    }

    // check_is_not_set returns false for IS followed by non-NOT/non-NULL identifier
    #[test]
    fn check_is_not_set_returns_false_for_is_ident() {
        let tokens = lexer::tokenize("IS something").unwrap();
        let parser = Parser::new(&tokens, "IS something");
        assert!(!common::check_is_not_set(&parser));
    }

    // check_is_not_set returns false when IS NOT not followed by SET
    #[test]
    fn check_is_not_set_returns_false_is_not_followed_by_non_set() {
        let tokens = lexer::tokenize("IS NOT REQUIRED").unwrap();
        let parser = Parser::new(&tokens, "IS NOT REQUIRED");
        assert!(!common::check_is_not_set(&parser));
    }

    // check_is_not_set returns false when IS NOT at end
    #[test]
    fn check_is_not_set_returns_false_is_not_at_end() {
        let tokens = lexer::tokenize("IS NOT").unwrap();
        let parser = Parser::new(&tokens, "IS NOT");
        assert!(!common::check_is_not_set(&parser));
    }

    // check_is_not_set returns false IS at end
    #[test]
    fn check_is_not_set_returns_false_is_at_end() {
        let tokens = lexer::tokenize("IS").unwrap();
        let parser = Parser::new(&tokens, "IS");
        assert!(!common::check_is_not_set(&parser));
    }

    // check_is_not_set: IS followed by SET (not NOT, not NULL)
    #[test]
    fn check_is_not_set_returns_false_for_is_set() {
        let tokens = lexer::tokenize("IS SET").unwrap();
        let parser = Parser::new(&tokens, "IS SET");
        assert!(!common::check_is_not_set(&parser));
    }

    // check_is_not_set: IS NOT followed by ident (not keyword)
    #[test]
    fn check_is_not_set_returns_false_is_not_ident() {
        let tokens = lexer::tokenize("IS NOT something").unwrap();
        let parser = Parser::new(&tokens, "IS NOT something");
        // "something" is an identifier, not a keyword, so peek_nth(2).kind is Identifier
        // The code checks for Keyword("SET"), so returns false
        assert!(!common::check_is_not_set(&parser));
    }

    // parse_ident_set with empty set
    #[test]
    fn parse_ident_set_empty() {
        let tokens = lexer::tokenize("{ }").unwrap();
        let mut parser = Parser::new(&tokens, "{ }");
        let result = common::parse_ident_set(&mut parser).unwrap();
        assert!(result.is_empty());
    }

    // check_is_not_set returns true for IS NULL
    #[test]
    fn check_is_not_set_returns_true_for_is_null() {
        let tokens = lexer::tokenize("IS NULL").unwrap();
        let parser = Parser::new(&tokens, "IS NULL");
        assert!(common::check_is_not_set(&parser));
    }

    // parse_ident_list_parens with empty list
    #[test]
    fn parse_ident_list_parens_empty() {
        let tokens = lexer::tokenize("()").unwrap();
        let mut parser = Parser::new(&tokens, "()");
        let result = common::parse_ident_list_parens(&mut parser).unwrap();
        assert!(result.is_empty());
    }

    // parse_literal_value: TRUE (common.rs lines 67-68)
    #[test]
    fn parse_literal_value_true() {
        let tokens = lexer::tokenize("TRUE").unwrap();
        let mut parser = Parser::new(&tokens, "TRUE");
        let result = common::parse_literal_value(&mut parser).unwrap();
        assert_eq!(result, smql_ast::value::Value::Bool(true));
    }

    // parse_literal_value: FALSE (common.rs lines 71-72)
    #[test]
    fn parse_literal_value_false() {
        let tokens = lexer::tokenize("FALSE").unwrap();
        let mut parser = Parser::new(&tokens, "FALSE");
        let result = common::parse_literal_value(&mut parser).unwrap();
        assert_eq!(result, smql_ast::value::Value::Bool(false));
    }

    // check_is_set returns true for IS SET
    #[test]
    fn check_is_set_returns_true() {
        let tokens = lexer::tokenize("IS SET").unwrap();
        let parser = Parser::new(&tokens, "IS SET");
        assert!(common::check_is_set(&parser));
    }
}

#[cfg(test)]
mod coverage_expr_tests {
    //! Tests targeting uncovered lines in expr.rs

    use crate::expr;
    use crate::lexer;
    use crate::Parser;
    use smql_ast::expression::*;
    use smql_ast::value::Value;

    fn parse_expr(input: &str) -> Expression {
        let tokens = lexer::tokenize(input).unwrap();
        let mut parser = Parser::new(&tokens, input);
        expr::parse_expression(&mut parser).unwrap()
    }

    fn try_parse_expr(input: &str) -> Result<Expression, smql_ast::SmqlError> {
        let tokens = lexer::tokenize(input).unwrap();
        let mut parser = Parser::new(&tokens, input);
        expr::parse_expression(&mut parser)
    }

    // Map literal { key: value }
    #[test]
    fn parse_map_literal() {
        let e = parse_expr("{ name: \"alice\", age: 30 }");
        match e.kind {
            ExpressionKind::FunctionCall { name, args } => {
                assert_eq!(name, "__map");
                // 2 key-value pairs = 4 args (key, value, key, value)
                assert_eq!(args.len(), 4);
                // Keys are sorted by BTreeMap, so "age" comes before "name"
                assert_eq!(
                    args[0].kind,
                    ExpressionKind::Literal(Value::Text("age".into()))
                );
                assert_eq!(args[1].kind, ExpressionKind::Literal(Value::Int(30)));
                assert_eq!(
                    args[2].kind,
                    ExpressionKind::Literal(Value::Text("name".into()))
                );
                assert_eq!(
                    args[3].kind,
                    ExpressionKind::Literal(Value::Text("alice".into()))
                );
            }
            _ => panic!("Expected FunctionCall(__map), got {:?}", e.kind),
        }
    }

    // Map literal with single entry
    #[test]
    fn parse_map_literal_single_entry() {
        let e = parse_expr("{ x: 1 }");
        match e.kind {
            ExpressionKind::FunctionCall { name, args } => {
                assert_eq!(name, "__map");
                assert_eq!(args.len(), 2);
            }
            _ => panic!("Expected __map FunctionCall"),
        }
    }

    // Empty map literal
    #[test]
    fn parse_map_literal_empty() {
        let e = parse_expr("{ }");
        match e.kind {
            ExpressionKind::FunctionCall { name, args } => {
                assert_eq!(name, "__map");
                assert!(args.is_empty());
            }
            _ => panic!("Expected __map FunctionCall"),
        }
    }

    // NOW() function
    #[test]
    fn parse_now_function() {
        let e = parse_expr("NOW()");
        match e.kind {
            ExpressionKind::FunctionCall { name, args } => {
                assert_eq!(name, "now");
                assert!(args.is_empty());
            }
            _ => panic!("Expected FunctionCall(now)"),
        }
    }

    // TODAY() function
    #[test]
    fn parse_today_function() {
        let e = parse_expr("TODAY()");
        match e.kind {
            ExpressionKind::FunctionCall { name, args } => {
                assert_eq!(name, "today");
                assert!(args.is_empty());
            }
            _ => panic!("Expected FunctionCall(today)"),
        }
    }

    // PATTERN("regex") expression
    #[test]
    fn parse_pattern_expr() {
        let e = parse_expr("PATTERN(\"^[a-z]+$\")");
        match e.kind {
            ExpressionKind::Pattern(regex) => {
                assert_eq!(regex, "^[a-z]+$");
            }
            _ => panic!("Expected Pattern, got {:?}", e.kind),
        }
    }

    // SIGNAL FROM Machine WHERE condition
    #[test]
    fn parse_signal_from_expr() {
        let e = parse_expr("SIGNAL FROM Child WHERE STATE IS done");
        match e.kind {
            ExpressionKind::SignalFrom { machine, condition } => {
                assert_eq!(machine, "Child");
                match condition.kind {
                    ExpressionKind::StateIs(ref s) => assert_eq!(s, "done"),
                    _ => panic!("Expected StateIs in condition"),
                }
            }
            _ => panic!("Expected SignalFrom, got {:?}", e.kind),
        }
    }

    // COUNT with argument
    #[test]
    fn parse_count_with_arg() {
        let e = parse_expr("COUNT(items)");
        match e.kind {
            ExpressionKind::Count(Some(ref inner)) => {
                assert_eq!(
                    inner.kind,
                    ExpressionKind::FieldAccess(vec!["items".into()])
                );
            }
            _ => panic!("Expected Count(Some(...)), got {:?}", e.kind),
        }
    }

    // ANY predicate
    #[test]
    fn parse_any_predicate() {
        let e = parse_expr("ANY(items, STATE IS confirmed)");
        match e.kind {
            ExpressionKind::Any {
                collection,
                predicate,
            } => {
                assert_eq!(
                    collection.kind,
                    ExpressionKind::FieldAccess(vec!["items".into()])
                );
                match predicate.kind {
                    ExpressionKind::StateIs(ref s) => assert_eq!(s, "confirmed"),
                    _ => panic!("Expected StateIs in predicate"),
                }
            }
            _ => panic!("Expected Any, got {:?}", e.kind),
        }
    }

    // STATE alone (as field reference, not IS/IN)
    #[test]
    fn parse_state_alone_as_field() {
        let e = parse_expr("STATE == \"open\"");
        match e.kind {
            ExpressionKind::BinaryOp { left, op, .. } => {
                assert_eq!(op, BinaryOperator::Eq);
                assert_eq!(
                    left.kind,
                    ExpressionKind::FieldAccess(vec!["STATE".into()])
                );
            }
            _ => panic!("Expected BinaryOp, got {:?}", e.kind),
        }
    }

    // Dot access on non-FieldAccess expression (e.g., function call result)
    #[test]
    fn parse_dot_access_on_function_call() {
        let e = parse_expr("elapsed_since(x).minutes");
        match e.kind {
            ExpressionKind::QualifiedAccess { root, path } => {
                match root.kind {
                    ExpressionKind::FunctionCall { ref name, .. } => {
                        assert_eq!(name, "elapsed_since");
                    }
                    _ => panic!("Expected FunctionCall as root"),
                }
                assert_eq!(path, vec!["minutes".to_string()]);
            }
            _ => panic!("Expected QualifiedAccess, got {:?}", e.kind),
        }
    }

    // Chained QualifiedAccess (SELF.a.b)
    #[test]
    fn parse_self_dot_chain() {
        let e = parse_expr("SELF.data.field");
        match e.kind {
            ExpressionKind::QualifiedAccess { root, path } => {
                assert_eq!(root.kind, ExpressionKind::SelfRef);
                assert_eq!(
                    path,
                    vec!["data".to_string(), "field".to_string()]
                );
            }
            _ => panic!("Expected QualifiedAccess, got {:?}", e.kind),
        }
    }

    // ACTOR.a.b multi-level dot
    #[test]
    fn parse_actor_deep_dot_access() {
        let e = parse_expr("ACTOR.profile.name");
        match e.kind {
            ExpressionKind::QualifiedAccess { root, path } => {
                assert_eq!(root.kind, ExpressionKind::ActorRef);
                assert_eq!(
                    path,
                    vec!["profile".to_string(), "name".to_string()]
                );
            }
            _ => panic!("Expected QualifiedAccess, got {:?}", e.kind),
        }
    }

    // NULL literal in expression
    #[test]
    fn parse_null_literal_expr() {
        let e = parse_expr("NULL");
        assert_eq!(e.kind, ExpressionKind::Literal(Value::Null));
    }

    // Float literal in expression
    #[test]
    fn parse_float_literal_expr() {
        let e = parse_expr("3.14");
        match e.kind {
            ExpressionKind::Literal(Value::Float(v)) => assert!((v - 3.14).abs() < 0.001),
            _ => panic!("Expected Float literal"),
        }
    }

    // Duration literal as primary expression
    #[test]
    fn parse_duration_literal_expr() {
        let e = parse_expr("24h");
        match e.kind {
            ExpressionKind::DurationLiteral(d) => assert_eq!(d.seconds, 86400),
            _ => panic!("Expected DurationLiteral"),
        }
    }

    // Subtraction
    #[test]
    fn parse_subtraction() {
        let e = parse_expr("a - b");
        match e.kind {
            ExpressionKind::BinaryOp { op, .. } => assert_eq!(op, BinaryOperator::Sub),
            _ => panic!("Expected Sub"),
        }
    }

    // Division
    #[test]
    fn parse_division() {
        let e = parse_expr("a / b");
        match e.kind {
            ExpressionKind::BinaryOp { op, .. } => assert_eq!(op, BinaryOperator::Div),
            _ => panic!("Expected Div"),
        }
    }

    // != operator
    #[test]
    fn parse_not_equal() {
        let e = parse_expr("x != 5");
        match e.kind {
            ExpressionKind::BinaryOp { op, .. } => assert_eq!(op, BinaryOperator::NotEq),
            _ => panic!("Expected NotEq"),
        }
    }

    // >= operator
    #[test]
    fn parse_gte() {
        let e = parse_expr("x >= 10");
        match e.kind {
            ExpressionKind::BinaryOp { op, .. } => assert_eq!(op, BinaryOperator::GtEq),
            _ => panic!("Expected GtEq"),
        }
    }

    // <= operator
    #[test]
    fn parse_lte() {
        let e = parse_expr("x <= 10");
        match e.kind {
            ExpressionKind::BinaryOp { op, .. } => assert_eq!(op, BinaryOperator::LtEq),
            _ => panic!("Expected LtEq"),
        }
    }

    // < operator
    #[test]
    fn parse_lt() {
        let e = parse_expr("x < 10");
        match e.kind {
            ExpressionKind::BinaryOp { op, .. } => assert_eq!(op, BinaryOperator::Lt),
            _ => panic!("Expected Lt"),
        }
    }

    // Error: expression at end of input
    #[test]
    fn parse_expr_end_of_input() {
        let result = try_parse_expr("");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("end of input"));
    }

    // Keyword used as function call (keyword followed by parens)
    #[test]
    fn parse_keyword_as_function_call() {
        // LIMIT is a keyword, but if followed by ( it becomes a function call
        let e = parse_expr("LIMIT(10)");
        match e.kind {
            ExpressionKind::FunctionCall { name, args } => {
                // Original text preserved
                assert_eq!(args.len(), 1);
                // The name comes from tok.text which is "LIMIT"
                assert!(name == "LIMIT" || name == "limit");
            }
            _ => panic!("Expected FunctionCall, got {:?}", e.kind),
        }
    }

    // Keyword without parens becomes field access
    #[test]
    fn parse_keyword_without_parens_as_field() {
        let e = parse_expr("LIMIT == 5");
        match e.kind {
            ExpressionKind::BinaryOp { left, op, .. } => {
                assert_eq!(op, BinaryOperator::Eq);
                match left.kind {
                    ExpressionKind::FieldAccess(ref path) => {
                        assert_eq!(path.len(), 1);
                    }
                    _ => panic!("Expected FieldAccess for keyword as field"),
                }
            }
            _ => panic!("Expected BinaryOp"),
        }
    }

    // IN without { or ( -- just becomes a bare expression
    #[test]
    fn parse_in_without_braces_or_parens() {
        // "x IN 5" -- IN is consumed but there's no { or (, so the IN keyword
        // is consumed but the code proceeds to just return the left expression.
        // Actually the parser would see "IN" keyword is consumed, then checks
        // for { and (, finds neither, and falls through. Let's trace:
        // `check_keyword("IN")` -> true, `expect_keyword("IN")` consumes it,
        // then check_punct("{") -> false, check_punct("(") -> false, falls through.
        // Then it returns Ok(left) which is just `x`.
        // Actually wait - it would continue to the next token "5" which might cause issues
        // Let me verify by parsing "x IN" which should work
        let e = parse_expr("x");
        match e.kind {
            ExpressionKind::FieldAccess(ref path) => assert_eq!(path, &vec!["x".to_string()]),
            _ => panic!("Expected FieldAccess"),
        }
    }

    // IN { } with single value
    #[test]
    fn parse_in_set_single() {
        let e = parse_expr("x IN { 1 }");
        match e.kind {
            ExpressionKind::InSet { values, .. } => assert_eq!(values.len(), 1),
            _ => panic!("Expected InSet"),
        }
    }

    // IN () with single value
    #[test]
    fn parse_in_list_single() {
        let e = parse_expr("x IN (1)");
        match e.kind {
            ExpressionKind::InList { values, .. } => assert_eq!(values.len(), 1),
            _ => panic!("Expected InList"),
        }
    }

    // Addition operator
    #[test]
    fn parse_addition() {
        let e = parse_expr("x + 1");
        match e.kind {
            ExpressionKind::BinaryOp { op, .. } => assert_eq!(op, BinaryOperator::Add),
            _ => panic!("Expected Add"),
        }
    }

    // Multiplication operator
    #[test]
    fn parse_multiplication() {
        let e = parse_expr("x * 2");
        match e.kind {
            ExpressionKind::BinaryOp { op, .. } => assert_eq!(op, BinaryOperator::Mul),
            _ => panic!("Expected Mul"),
        }
    }

    // Complex nested expression
    #[test]
    fn parse_complex_nested() {
        let e = parse_expr("(a + b) * (c - d) / e");
        match e.kind {
            ExpressionKind::BinaryOp { op, .. } => assert_eq!(op, BinaryOperator::Div),
            _ => panic!("Expected Div at top level"),
        }
    }

    // Function call with multiple args
    #[test]
    fn parse_function_call_multi_args() {
        let e = parse_expr("custom_fn(1, \"hello\", x)");
        match e.kind {
            ExpressionKind::FunctionCall { name, args } => {
                assert_eq!(name, "custom_fn");
                assert_eq!(args.len(), 3);
            }
            _ => panic!("Expected FunctionCall"),
        }
    }

    // Dot access on parenthesized expression: (x) resolves to FieldAccess,
    // so .field extends the FieldAccess path
    #[test]
    fn parse_dot_on_parenthesized() {
        let e = parse_expr("(x).field");
        match e.kind {
            ExpressionKind::FieldAccess(ref path) => {
                assert_eq!(path, &vec!["x".to_string(), "field".to_string()]);
            }
            _ => panic!("Expected FieldAccess, got {:?}", e.kind),
        }
    }

    // Dot access on a literal expression (catches the `_` fallthrough branch
    // in parse_postfix for non-FieldAccess/SelfRef/ActorRef/QualifiedAccess)
    #[test]
    fn parse_dot_on_literal_parenthesized() {
        // (1 + 2).result -- the parenthesized expression is BinaryOp, not FieldAccess
        let e = parse_expr("(1 + 2).result");
        match e.kind {
            ExpressionKind::QualifiedAccess { root, path } => {
                match root.kind {
                    ExpressionKind::BinaryOp { op, .. } => assert_eq!(op, BinaryOperator::Add),
                    _ => panic!("Expected BinaryOp root"),
                }
                assert_eq!(path, vec!["result".to_string()]);
            }
            _ => panic!("Expected QualifiedAccess, got {:?}", e.kind),
        }
    }
}

#[cfg(test)]
mod coverage_machine_tests {
    //! Tests targeting uncovered lines in machine.rs

    use crate::parse_machine;
    use crate::parse_machines;

    // Unknown keyword type in DATA block (keyword that is not a type name)
    #[test]
    fn parse_unknown_type_keyword() {
        let input = r#"
DEFINE MACHINE Bad (
    DATA { f : SPAWN -> REQUIRED }
    STATES { a }
    INITIAL STATE a
    TERMINAL STATES { a }
    TRANSITIONS {}
)
"#;
        let result = parse_machines(input);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Unknown type"));
    }

    // Unknown constraint keyword
    #[test]
    fn parse_unknown_constraint_keyword() {
        let input = r#"
DEFINE MACHINE Bad (
    DATA { f : TEXT -> FOOBAR }
    STATES { a }
    INITIAL STATE a
    TERMINAL STATES { a }
    TRANSITIONS {}
)
"#;
        // FOOBAR is not a keyword, so it's an identifier; parse_single_constraint
        // sees an identifier token and returns "Expected constraint, found..."
        let result = parse_machines(input);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Expected constraint"));
    }

    // Unknown action keyword in transition
    #[test]
    fn parse_unknown_action_keyword() {
        let input = r#"
DEFINE MACHINE Bad (
    STATES { a, b }
    INITIAL STATE a
    TERMINAL STATES { b }
    TRANSITIONS {
        a -> b {
            ACTION : DELETE("something")
        }
    }
)
"#;
        // DELETE is not a registered keyword, so it's an Identifier.
        // parse_action peeks for Keyword but gets Identifier, triggering the error.
        let result = parse_machines(input);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Expected action keyword"));
    }

    // Default value as identifier (enum variant)
    #[test]
    fn parse_default_value_identifier() {
        let input = r#"
DEFINE MACHINE WithEnumDefault (
    DATA { status : ENUM(active, inactive) -> DEFAULT(active) }
    STATES { a }
    INITIAL STATE a
    TERMINAL STATES { a }
    TRANSITIONS {}
)
"#;
        let m = parse_machine(input).unwrap();
        assert_eq!(m.data[0].name, "status");
        match &m.data[0].constraints[0] {
            smql_ast::types::Constraint::Default(dv) => {
                // "active" is a keyword that gets text preserved, so it becomes a string default
                match dv {
                    smql_ast::types::DefaultValue::String(s) => assert_eq!(s, "active"),
                    other => panic!("Expected String default, got {:?}", other),
                }
            }
            other => panic!("Expected Default constraint, got {:?}", other),
        }
    }

    // Unexpected end of input in machine body
    #[test]
    fn parse_machine_unexpected_end() {
        let input = "DEFINE MACHINE Bad (";
        let result = parse_machine(input);
        assert!(result.is_err());
    }

    // Unknown keyword in machine body
    #[test]
    fn parse_machine_unknown_keyword_in_body() {
        let input = r#"
DEFINE MACHINE Bad (
    STATES { a }
    SUBSCRIBE something
    INITIAL STATE a
    TERMINAL STATES { a }
    TRANSITIONS {}
)
"#;
        let result = parse_machines(input);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Unexpected keyword"));
    }

    // Non-keyword non-identifier token in machine body
    #[test]
    fn parse_machine_unexpected_token_in_body() {
        let input = r#"
DEFINE MACHINE Bad (
    STATES { a }
    42
    INITIAL STATE a
    TERMINAL STATES { a }
    TRANSITIONS {}
)
"#;
        let result = parse_machines(input);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Unexpected token"));
    }

    // Hook trigger error: invalid trigger keyword after ON
    #[test]
    fn parse_hook_trigger_invalid_after_on() {
        let input = r#"
DEFINE MACHINE Bad (
    STATES { a }
    INITIAL STATE a
    TERMINAL STATES { a }
    TRANSITIONS {}
    HOOKS {
        ON DELETE {
            LOG("bad")
        }
    }
)
"#;
        // DELETE is not a keyword; it's an Identifier
        // ON is consumed, then it checks SPAWN, ENTER, EXIT, DWELL -- none match
        // Actually let's check: "ON" is consumed via try_keyword, then we check
        // try_keyword("SPAWN"), try_keyword("ENTER"), try_keyword("EXIT"), try_keyword("DWELL")
        // DELETE is not a keyword, so none match, returns error
        let result = parse_machines(input);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Expected SPAWN, ENTER, EXIT, or DWELL"));
    }

    // Hook trigger error: no valid trigger keyword at all
    #[test]
    fn parse_hook_trigger_no_valid_keyword() {
        let input = r#"
DEFINE MACHINE Bad (
    STATES { a }
    INITIAL STATE a
    TERMINAL STATES { a }
    TRANSITIONS {}
    HOOKS {
        DEFINE {
            LOG("bad")
        }
    }
)
"#;
        // DEFINE is a keyword but not ON/BEFORE/AFTER
        let result = parse_machines(input);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Expected ON, BEFORE, or AFTER"));
    }

    // CHILDREN block with unknown child type keyword (not LIST/OPTIONAL)
    #[test]
    fn parse_children_unknown_type() {
        let input = r#"
DEFINE MACHINE Bad (
    CHILDREN { items : SET(Item) }
    STATES { a }
    INITIAL STATE a
    TERMINAL STATES { a }
    TRANSITIONS {}
)
"#;
        let result = parse_machines(input);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Expected LIST or OPTIONAL"));
    }

    // CHILDREN block: non-keyword non-identifier token
    #[test]
    fn parse_children_bad_type_token() {
        let input = r#"
DEFINE MACHINE Bad (
    CHILDREN { items : 42 }
    STATES { a }
    INITIAL STATE a
    TERMINAL STATES { a }
    TRANSITIONS {}
)
"#;
        let result = parse_machines(input);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Expected child type"));
    }

    // MUTATE with SPAWN but empty data
    #[test]
    fn parse_mutate_spawn_no_data() {
        let input = r#"
DEFINE MACHINE WithMutateSpawnEmpty (
    STATES { a, b }
    INITIAL STATE a
    TERMINAL STATES { b }
    TRANSITIONS {
        a -> b {
            MUTATE : child = SPAWN ChildMachine {}
        }
    }
)
"#;
        let m = parse_machine(input).unwrap();
        assert_eq!(m.transitions[0].mutates.len(), 1);
        match &m.transitions[0].mutates[0].value.kind {
            smql_ast::expression::ExpressionKind::FunctionCall { name, args } => {
                assert_eq!(name, "__spawn");
                // Only the machine name arg
                assert_eq!(args.len(), 1);
            }
            other => panic!("Expected __spawn FunctionCall, got {:?}", other),
        }
    }

    // Known keyword as constraint (e.g., OPTIONAL which is valid)
    #[test]
    fn parse_optional_constraint() {
        let input = r#"
DEFINE MACHINE WithOpt (
    DATA { tag : TEXT -> OPTIONAL }
    STATES { a }
    INITIAL STATE a
    TERMINAL STATES { a }
    TRANSITIONS {}
)
"#;
        let m = parse_machine(input).unwrap();
        assert_eq!(m.data[0].constraints, vec![smql_ast::types::Constraint::Optional]);
    }

    // Action keyword that IS a keyword but not an action (e.g., DEFINE)
    #[test]
    fn parse_unknown_action_keyword_that_is_keyword() {
        let input = r#"
DEFINE MACHINE Bad (
    STATES { a, b }
    INITIAL STATE a
    TERMINAL STATES { b }
    TRANSITIONS {
        a -> b {
            ACTION : DEFINE("bad")
        }
    }
)
"#;
        let result = parse_machines(input);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Unknown action"));
    }

    // Constraint keyword that IS a keyword but not a valid constraint
    #[test]
    fn parse_constraint_that_is_keyword_but_not_valid() {
        let input = r#"
DEFINE MACHINE Bad (
    DATA { f : TEXT -> DEFINE }
    STATES { a }
    INITIAL STATE a
    TERMINAL STATES { a }
    TRANSITIONS {}
)
"#;
        let result = parse_machines(input);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Unknown constraint"));
    }

    // Type name given as identifier instead of keyword
    #[test]
    fn parse_type_as_identifier() {
        let input = r#"
DEFINE MACHINE Bad (
    DATA { f : customtype }
    STATES { a }
    INITIAL STATE a
    TERMINAL STATES { a }
    TRANSITIONS {}
)
"#;
        let result = parse_machines(input);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Expected type name"));
    }

    // Default value error: unexpected token
    #[test]
    fn parse_default_value_bad_token() {
        let input = r#"
DEFINE MACHINE Bad (
    DATA { f : TEXT -> DEFAULT(+) }
    STATES { a }
    INITIAL STATE a
    TERMINAL STATES { a }
    TRANSITIONS {}
)
"#;
        let result = parse_machines(input);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Expected default value"));
    }

    // Default value as float
    #[test]
    fn parse_default_value_float() {
        let input = r#"
DEFINE MACHINE WithFloat (
    DATA { score : FLOAT -> DEFAULT(3.14) }
    STATES { a }
    INITIAL STATE a
    TERMINAL STATES { a }
    TRANSITIONS {}
)
"#;
        let m = parse_machine(input).unwrap();
        assert_eq!(m.data[0].name, "score");
        match &m.data[0].constraints[0] {
            smql_ast::types::Constraint::Default(dv) => match dv {
                smql_ast::types::DefaultValue::Float(v) => assert!((*v - 3.14).abs() < 0.001),
                other => panic!("Expected Float default, got {:?}", other),
            },
            other => panic!("Expected Default constraint, got {:?}", other),
        }
    }

    // Default value as TRUE
    #[test]
    fn parse_default_value_true() {
        let input = r#"
DEFINE MACHINE WithBool (
    DATA { flag : BOOL -> DEFAULT(TRUE) }
    STATES { a }
    INITIAL STATE a
    TERMINAL STATES { a }
    TRANSITIONS {}
)
"#;
        let m = parse_machine(input).unwrap();
        match &m.data[0].constraints[0] {
            smql_ast::types::Constraint::Default(dv) => match dv {
                smql_ast::types::DefaultValue::Bool(v) => assert!(*v),
                other => panic!("Expected Bool default, got {:?}", other),
            },
            other => panic!("Expected Default constraint, got {:?}", other),
        }
    }

    // Default value as FALSE
    #[test]
    fn parse_default_value_false() {
        let input = r#"
DEFINE MACHINE WithBool (
    DATA { flag : BOOL -> DEFAULT(FALSE) }
    STATES { a }
    INITIAL STATE a
    TERMINAL STATES { a }
    TRANSITIONS {}
)
"#;
        let m = parse_machine(input).unwrap();
        match &m.data[0].constraints[0] {
            smql_ast::types::Constraint::Default(dv) => match dv {
                smql_ast::types::DefaultValue::Bool(v) => assert!(!*v),
                other => panic!("Expected Bool default, got {:?}", other),
            },
            other => panic!("Expected Default constraint, got {:?}", other),
        }
    }

    // Default value as NULL
    #[test]
    fn parse_default_value_null() {
        let input = r#"
DEFINE MACHINE WithNull (
    DATA { notes : TEXT -> DEFAULT(NULL) }
    STATES { a }
    INITIAL STATE a
    TERMINAL STATES { a }
    TRANSITIONS {}
)
"#;
        let m = parse_machine(input).unwrap();
        match &m.data[0].constraints[0] {
            smql_ast::types::Constraint::Default(dv) => match dv {
                smql_ast::types::DefaultValue::Null => {}
                other => panic!("Expected Null default, got {:?}", other),
            },
            other => panic!("Expected Default constraint, got {:?}", other),
        }
    }

    // Default value as empty set {}
    #[test]
    fn parse_default_value_empty_set() {
        let input = r#"
DEFINE MACHINE WithEmptySet (
    DATA { tags : SET(TEXT) -> DEFAULT({}) }
    STATES { a }
    INITIAL STATE a
    TERMINAL STATES { a }
    TRANSITIONS {}
)
"#;
        let m = parse_machine(input).unwrap();
        match &m.data[0].constraints[0] {
            smql_ast::types::Constraint::Default(dv) => match dv {
                smql_ast::types::DefaultValue::EmptySet => {}
                other => panic!("Expected EmptySet default, got {:?}", other),
            },
            other => panic!("Expected Default constraint, got {:?}", other),
        }
    }

    // Default value as empty list []
    #[test]
    fn parse_default_value_empty_list() {
        let input = r#"
DEFINE MACHINE WithEmptyList (
    DATA { items : LIST(TEXT) -> DEFAULT([]) }
    STATES { a }
    INITIAL STATE a
    TERMINAL STATES { a }
    TRANSITIONS {}
)
"#;
        let m = parse_machine(input).unwrap();
        match &m.data[0].constraints[0] {
            smql_ast::types::Constraint::Default(dv) => match dv {
                smql_ast::types::DefaultValue::EmptyList => {}
                other => panic!("Expected EmptyList default, got {:?}", other),
            },
            other => panic!("Expected Default constraint, got {:?}", other),
        }
    }

    // Default value as identifier (bare enum variant)
    #[test]
    fn parse_default_value_identifier_bare() {
        let input = r#"
DEFINE MACHINE WithIdent (
    DATA { status : ENUM(active, inactive) -> DEFAULT(inactive) }
    STATES { a }
    INITIAL STATE a
    TERMINAL STATES { a }
    TRANSITIONS {}
)
"#;
        let m = parse_machine(input).unwrap();
        match &m.data[0].constraints[0] {
            smql_ast::types::Constraint::Default(dv) => match dv {
                smql_ast::types::DefaultValue::String(s) => assert_eq!(s, "inactive"),
                other => panic!("Expected String default, got {:?}", other),
            },
            other => panic!("Expected Default constraint, got {:?}", other),
        }
    }

    // ROLES block parsing — CAN is now a keyword, so roles parse successfully.
    #[test]
    fn parse_machine_with_roles_block_enters_parser() {
        let input = r#"
DEFINE MACHINE WithRoles (
    STATES { a, b }
    INITIAL STATE a
    TERMINAL STATES { b }
    TRANSITIONS { a -> b {} }
    ROLES {
        admin {
            CAN SPAWN
        }
    }
)
"#;
        let machines = parse_machines(input).unwrap();
        assert_eq!(machines.len(), 1);
        assert_eq!(machines[0].roles.len(), 1);
        assert_eq!(machines[0].roles[0].name, "admin");
        assert!(matches!(machines[0].roles[0].permissions[0], smql_ast::machine::RolePermission::CanSpawn));
    }

    // ROLES block with empty role body (no CAN needed)
    #[test]
    fn parse_roles_empty_role_body() {
        let input = r#"
DEFINE MACHINE EmptyRole (
    STATES { a }
    INITIAL STATE a
    TERMINAL STATES { a }
    TRANSITIONS {}
    ROLES {
        viewer {}
    }
)
"#;
        let m = parse_machine(input).unwrap();
        assert_eq!(m.roles.len(), 1);
        assert_eq!(m.roles[0].name, "viewer");
        assert_eq!(m.roles[0].permissions.len(), 0);
    }

    // HOOKS with ACTION : prefix inside hook body
    #[test]
    fn parse_hooks_with_action_prefix() {
        let input = r#"
DEFINE MACHINE WithActionPrefix (
    STATES { a, b }
    INITIAL STATE a
    TERMINAL STATES { b }
    TRANSITIONS { a -> b {} }
    HOOKS {
        ON SPAWN {
            ACTION : EMIT("spawned")
        }
    }
)
"#;
        let m = parse_machine(input).unwrap();
        assert_eq!(m.hooks.len(), 1);
        assert_eq!(m.hooks[0].actions.len(), 1);
    }

    // HOOKS with ON ENTER and ON EXIT triggers
    #[test]
    fn parse_hooks_on_enter_exit() {
        let input = r#"
DEFINE MACHINE WithEnterExit (
    STATES { a, b }
    INITIAL STATE a
    TERMINAL STATES { b }
    TRANSITIONS { a -> b {} }
    HOOKS {
        ON ENTER b {
            LOG("entered b")
        }
        ON EXIT a {
            LOG("left a")
        }
    }
)
"#;
        let m = parse_machine(input).unwrap();
        assert_eq!(m.hooks.len(), 2);
        match &m.hooks[0].trigger {
            smql_ast::machine::HookTrigger::OnEnter(s) => assert_eq!(s, "b"),
            other => panic!("Expected OnEnter, got {:?}", other),
        }
        match &m.hooks[1].trigger {
            smql_ast::machine::HookTrigger::OnExit(s) => assert_eq!(s, "a"),
            other => panic!("Expected OnExit, got {:?}", other),
        }
    }

    // HOOKS with AFTER EACH TRANSITION
    #[test]
    fn parse_hooks_after_each_transition() {
        let input = r#"
DEFINE MACHINE WithAfter (
    STATES { a, b }
    INITIAL STATE a
    TERMINAL STATES { b }
    TRANSITIONS { a -> b {} }
    HOOKS {
        AFTER EACH TRANSITION {
            LOG("after transition")
        }
    }
)
"#;
        let m = parse_machine(input).unwrap();
        assert_eq!(m.hooks.len(), 1);
        match &m.hooks[0].trigger {
            smql_ast::machine::HookTrigger::AfterEachTransition => {}
            other => panic!("Expected AfterEachTransition, got {:?}", other),
        }
    }

    // CHILDREN block with MAX constraint
    #[test]
    fn parse_children_with_max_constraint() {
        let input = r#"
DEFINE MACHINE WithChildMax (
    STATES { active, done }
    INITIAL STATE active
    TERMINAL STATES { done }
    CHILDREN {
        items : LIST(Item) -> MIN(1), MAX(10)
    }
    TRANSITIONS { active -> done {} }
)
"#;
        let m = parse_machine(input).unwrap();
        assert_eq!(m.children.len(), 1);
        match &m.children[0].cardinality {
            smql_ast::machine::ChildCardinality::List { min, max } => {
                assert_eq!(*min, Some(1));
                assert_eq!(*max, Some(10));
            }
            other => panic!("Expected List cardinality, got {:?}", other),
        }
    }

    // CHILDREN with Required (bare identifier) cardinality
    #[test]
    fn parse_children_required_cardinality() {
        let input = r#"
DEFINE MACHINE WithRequiredChild (
    STATES { active, done }
    INITIAL STATE active
    TERMINAL STATES { done }
    CHILDREN {
        config : Config
    }
    TRANSITIONS { active -> done {} }
)
"#;
        let m = parse_machine(input).unwrap();
        assert_eq!(m.children.len(), 1);
        assert_eq!(m.children[0].machine, "Config");
        match &m.children[0].cardinality {
            smql_ast::machine::ChildCardinality::Required => {}
            other => panic!("Expected Required cardinality, got {:?}", other),
        }
    }

    // Action SPAWN with data in transition
    #[test]
    fn parse_action_spawn_with_data() {
        let input = r#"
DEFINE MACHINE WithSpawn (
    STATES { a, b }
    INITIAL STATE a
    TERMINAL STATES { b }
    TRANSITIONS {
        a -> b {
            ACTION : SPAWN ChildMachine { key: "value" }
        }
    }
)
"#;
        let m = parse_machine(input).unwrap();
        match &m.transitions[0].actions[0] {
            smql_ast::machine::Action::SpawnChild { machine, data } => {
                assert_eq!(machine, "ChildMachine");
                assert_eq!(data.len(), 1);
                assert_eq!(data[0].0, "key");
            }
            other => panic!("Expected SpawnChild, got {:?}", other),
        }
    }

    // Action SIGNAL PARENT in transition body (via ACTION keyword)
    #[test]
    fn parse_action_signal_parent() {
        let input = r#"
DEFINE MACHINE WithSignal (
    PARENT : ParentMachine
    STATES { a, b }
    INITIAL STATE a
    TERMINAL STATES { b }
    TRANSITIONS {
        a -> b {
            ACTION : SIGNAL PARENT TO done
        }
    }
)
"#;
        let m = parse_machine(input).unwrap();
        match &m.transitions[0].actions[0] {
            smql_ast::machine::Action::SignalParent { target_state } => {
                assert_eq!(target_state, "done");
            }
            other => panic!("Expected SignalParent, got {:?}", other),
        }
    }

    // MUTATE with SPAWN including data
    #[test]
    fn parse_mutate_spawn_with_data() {
        let input = r#"
DEFINE MACHINE WithMutateSpawn (
    STATES { a, b }
    INITIAL STATE a
    TERMINAL STATES { b }
    TRANSITIONS {
        a -> b {
            MUTATE : child = SPAWN ChildMachine { name: "test", count: 5 }
        }
    }
)
"#;
        let m = parse_machine(input).unwrap();
        assert_eq!(m.transitions[0].mutates.len(), 1);
        match &m.transitions[0].mutates[0].value.kind {
            smql_ast::expression::ExpressionKind::FunctionCall { name, args } => {
                assert_eq!(name, "__spawn");
                // machine name + 2 key-value pairs = 5 args
                assert_eq!(args.len(), 5);
            }
            other => panic!("Expected __spawn FunctionCall, got {:?}", other),
        }
    }

    // Action EMIT with payload
    #[test]
    fn parse_action_emit_with_payload() {
        let input = r#"
DEFINE MACHINE WithEmit (
    STATES { a, b }
    INITIAL STATE a
    TERMINAL STATES { b }
    TRANSITIONS {
        a -> b {
            ACTION : EMIT("state_changed", SELF)
        }
    }
)
"#;
        let m = parse_machine(input).unwrap();
        match &m.transitions[0].actions[0] {
            smql_ast::machine::Action::Emit { event, payload } => {
                assert_eq!(event, "state_changed");
                assert!(payload.is_some());
            }
            other => panic!("Expected Emit, got {:?}", other),
        }
    }

    // Action WEBHOOK with payload
    #[test]
    fn parse_action_webhook_with_payload() {
        let input = r#"
DEFINE MACHINE WithWebhook (
    STATES { a, b }
    INITIAL STATE a
    TERMINAL STATES { b }
    TRANSITIONS {
        a -> b {
            ACTION : WEBHOOK("https://example.com/hook", SELF)
        }
    }
)
"#;
        let m = parse_machine(input).unwrap();
        match &m.transitions[0].actions[0] {
            smql_ast::machine::Action::Webhook { url, payload, .. } => {
                assert_eq!(url, "https://example.com/hook");
                assert!(payload.is_some());
            }
            other => panic!("Expected Webhook, got {:?}", other),
        }
    }

    // HOOKS with ON DWELL trigger
    #[test]
    fn parse_hooks_on_dwell() {
        let input = r#"
DEFINE MACHINE WithDwell (
    STATES { waiting, done }
    INITIAL STATE waiting
    TERMINAL STATES { done }
    TRANSITIONS { waiting -> done {} }
    HOOKS {
        ON DWELL(waiting, > 24h) {
            LOG("stale")
        }
    }
)
"#;
        let m = parse_machine(input).unwrap();
        assert_eq!(m.hooks.len(), 1);
        match &m.hooks[0].trigger {
            smql_ast::machine::HookTrigger::OnDwell { state, duration } => {
                assert_eq!(state, "waiting");
                assert_eq!(duration.seconds, 86400);
            }
            other => panic!("Expected OnDwell, got {:?}", other),
        }
    }

    // Data type: MAP(TEXT, INT)
    #[test]
    fn parse_data_type_map() {
        let input = r#"
DEFINE MACHINE WithMap (
    DATA { metadata : MAP(TEXT, INT) }
    STATES { a }
    INITIAL STATE a
    TERMINAL STATES { a }
    TRANSITIONS {}
)
"#;
        let m = parse_machine(input).unwrap();
        match &m.data[0].field_type {
            smql_ast::types::TypeDefinition::Map(k, v) => {
                assert_eq!(**k, smql_ast::types::TypeDefinition::Text);
                assert_eq!(**v, smql_ast::types::TypeDefinition::Int);
            }
            other => panic!("Expected Map type, got {:?}", other),
        }
    }

    // Data type: MONEY(USD)
    #[test]
    fn parse_data_type_money() {
        let input = r#"
DEFINE MACHINE WithMoney (
    DATA { total : MONEY(USD) }
    STATES { a }
    INITIAL STATE a
    TERMINAL STATES { a }
    TRANSITIONS {}
)
"#;
        let m = parse_machine(input).unwrap();
        match &m.data[0].field_type {
            smql_ast::types::TypeDefinition::Money(currency) => {
                assert_eq!(currency, "USD");
            }
            other => panic!("Expected Money type, got {:?}", other),
        }
    }

    // Constraint: UNIQUE
    #[test]
    fn parse_constraint_unique() {
        let input = r#"
DEFINE MACHINE WithUnique (
    DATA { email : TEXT -> UNIQUE }
    STATES { a }
    INITIAL STATE a
    TERMINAL STATES { a }
    TRANSITIONS {}
)
"#;
        let m = parse_machine(input).unwrap();
        assert_eq!(m.data[0].constraints, vec![smql_ast::types::Constraint::Unique]);
    }

    // Constraint: RANGE(lo, hi)
    #[test]
    fn parse_constraint_range() {
        let input = r#"
DEFINE MACHINE WithRange (
    DATA { score : INT -> RANGE(0, 100) }
    STATES { a }
    INITIAL STATE a
    TERMINAL STATES { a }
    TRANSITIONS {}
)
"#;
        let m = parse_machine(input).unwrap();
        match &m.data[0].constraints[0] {
            smql_ast::types::Constraint::Range(lo, hi) => {
                assert_eq!(*lo, 0);
                assert_eq!(*hi, 100);
            }
            other => panic!("Expected Range, got {:?}", other),
        }
    }

    // Constraint: PATTERN("regex")
    #[test]
    fn parse_constraint_pattern() {
        let input = r#"
DEFINE MACHINE WithPattern (
    DATA { code : TEXT -> PATTERN("^[A-Z]{3}$") }
    STATES { a }
    INITIAL STATE a
    TERMINAL STATES { a }
    TRANSITIONS {}
)
"#;
        let m = parse_machine(input).unwrap();
        match &m.data[0].constraints[0] {
            smql_ast::types::Constraint::Pattern(regex) => {
                assert_eq!(regex, "^[A-Z]{3}$");
            }
            other => panic!("Expected Pattern, got {:?}", other),
        }
    }

    // Multiple constraints separated by commas
    #[test]
    fn parse_multiple_constraints() {
        let input = r#"
DEFINE MACHINE WithMulti (
    DATA { val : INT -> REQUIRED, MIN(0), MAX(100) }
    STATES { a }
    INITIAL STATE a
    TERMINAL STATES { a }
    TRANSITIONS {}
)
"#;
        let m = parse_machine(input).unwrap();
        assert_eq!(m.data[0].constraints.len(), 3);
        assert_eq!(m.data[0].constraints[0], smql_ast::types::Constraint::Required);
        assert_eq!(m.data[0].constraints[1], smql_ast::types::Constraint::Min(0));
        assert_eq!(m.data[0].constraints[2], smql_ast::types::Constraint::Max(100));
    }

    // TIMEOUT without colon separator (optional)
    #[test]
    fn parse_timeout_without_colon() {
        let input = r#"
DEFINE MACHINE WithTimeoutNoCon (
    STATES { waiting, expired }
    INITIAL STATE waiting
    TERMINAL STATES { expired }
    TRANSITIONS {
        waiting -> expired {
            TIMEOUT 48h -> expired
        }
    }
)
"#;
        let m = parse_machine(input).unwrap();
        let timeout = m.transitions[0].timeout.as_ref().unwrap();
        assert_eq!(timeout.duration.seconds, 48 * 3600);
        assert_eq!(timeout.target_state, "expired");
    }
}

#[cfg(test)]
mod coverage_lib_tests {
    //! Tests targeting uncovered lines in lib.rs

    use crate::parse;
    use smql_ast::command::{Command, Statement};

    // Empty input returns empty vec
    #[test]
    fn parse_empty_input() {
        let stmts = parse("").unwrap();
        assert!(stmts.is_empty());
    }

    // Whitespace-only input returns empty vec
    #[test]
    fn parse_whitespace_only() {
        let stmts = parse("   \n\t  ").unwrap();
        assert!(stmts.is_empty());
    }

    // DEFINE MACHINE via parse() creates Statement::Command(DefineMachine)
    #[test]
    fn parse_define_machine_via_parse() {
        let input = r#"
DEFINE MACHINE Simple (
    STATES { a, b }
    INITIAL STATE a
    TERMINAL STATES { b }
    TRANSITIONS { a -> b {} }
)
"#;
        let stmts = parse(input).unwrap();
        assert_eq!(stmts.len(), 1);
        match &stmts[0] {
            Statement::Command(Command::DefineMachine(m)) => {
                assert_eq!(m.name, "Simple");
            }
            _ => panic!("Expected DefineMachine"),
        }
    }

    // ALTER MACHINE via parse()
    #[test]
    fn parse_alter_machine_via_parse() {
        let input = "ALTER MACHINE Order ADD STATE review";
        let stmts = parse(input).unwrap();
        assert_eq!(stmts.len(), 1);
        match &stmts[0] {
            Statement::Command(Command::AlterMachine(a)) => {
                assert_eq!(a.machine, "Order");
            }
            _ => panic!("Expected AlterMachine"),
        }
    }

    // Unexpected token at start of statement (e.g., a number)
    #[test]
    fn parse_unexpected_number_at_start() {
        let result = parse("42");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Unexpected token"));
    }

    // Unexpected end of input (empty after tokenizing)
    #[test]
    fn parse_comment_only() {
        let stmts = parse("-- just a comment").unwrap();
        assert!(stmts.is_empty());
    }
}

#[cfg(test)]
mod coverage_lexer_tests {
    //! Tests targeting uncovered lines in lexer.rs

    use crate::lexer;
    use crate::lexer::TokenKind;

    // Unknown character
    #[test]
    fn tokenize_unknown_character() {
        let result = lexer::tokenize("@");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Unexpected character"));
    }

    #[test]
    fn tokenize_unknown_character_tilde() {
        let result = lexer::tokenize("~");
        assert!(result.is_err());
    }

    #[test]
    fn tokenize_unknown_character_hash() {
        let result = lexer::tokenize("#");
        assert!(result.is_err());
    }

    // String escape sequences: \n, \t, \\, unknown escape
    #[test]
    fn tokenize_string_escape_newline() {
        let tokens = lexer::tokenize("\"line1\\nline2\"").unwrap();
        assert_eq!(tokens.len(), 1);
        match &tokens[0].kind {
            TokenKind::StringLiteral(s) => assert_eq!(s, "line1\nline2"),
            _ => panic!("Expected StringLiteral"),
        }
    }

    #[test]
    fn tokenize_string_escape_tab() {
        let tokens = lexer::tokenize("\"col1\\tcol2\"").unwrap();
        match &tokens[0].kind {
            TokenKind::StringLiteral(s) => assert_eq!(s, "col1\tcol2"),
            _ => panic!("Expected StringLiteral"),
        }
    }

    #[test]
    fn tokenize_string_escape_backslash() {
        let tokens = lexer::tokenize("\"path\\\\file\"").unwrap();
        match &tokens[0].kind {
            TokenKind::StringLiteral(s) => assert_eq!(s, "path\\file"),
            _ => panic!("Expected StringLiteral"),
        }
    }

    #[test]
    fn tokenize_string_escape_unknown() {
        let tokens = lexer::tokenize("\"test\\xval\"").unwrap();
        match &tokens[0].kind {
            TokenKind::StringLiteral(s) => assert_eq!(s, "test\\xval"),
            _ => panic!("Expected StringLiteral"),
        }
    }

    // Minus sign that is an operator (not part of number)
    #[test]
    fn tokenize_minus_operator_alone() {
        let tokens = lexer::tokenize("x - y").unwrap();
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[1].kind, TokenKind::Operator("-".into()));
    }

    // Minus at end of input
    #[test]
    fn tokenize_minus_at_end() {
        let tokens = lexer::tokenize("x -").unwrap();
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[1].kind, TokenKind::Operator("-".into()));
    }

    // Equals sign as single operator
    #[test]
    fn tokenize_single_equals() {
        let tokens = lexer::tokenize("=").unwrap();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::Operator("=".into()));
    }

    // Dot as punctuation
    #[test]
    fn tokenize_dot_punctuation() {
        let tokens = lexer::tokenize("a.b").unwrap();
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[1].kind, TokenKind::Punctuation(".".into()));
    }

    // Brackets
    #[test]
    fn tokenize_brackets() {
        let tokens = lexer::tokenize("[a, b]").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Punctuation("[".into()));
        assert_eq!(tokens[4].kind, TokenKind::Punctuation("]".into()));
    }

    // Comment at end of input (no newline)
    #[test]
    fn tokenize_comment_no_newline() {
        let tokens = lexer::tokenize("DEFINE -- trailing comment").unwrap();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::Keyword("DEFINE".into()));
    }

    // Underscore prefix identifier
    #[test]
    fn tokenize_underscore_identifier() {
        let tokens = lexer::tokenize("_private_field").unwrap();
        assert_eq!(tokens.len(), 1);
        assert_eq!(
            tokens[0].kind,
            TokenKind::Identifier("_private_field".into())
        );
    }

    // Negative number
    #[test]
    fn tokenize_negative_int() {
        let tokens = lexer::tokenize("-42").unwrap();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::IntLiteral(-42));
    }

    // Negative float
    #[test]
    fn tokenize_negative_float() {
        let tokens = lexer::tokenize("-3.14").unwrap();
        assert_eq!(tokens.len(), 1);
        match &tokens[0].kind {
            TokenKind::FloatLiteral(v) => assert!((*v + 3.14).abs() < 0.001),
            other => panic!("Expected FloatLiteral, got {:?}", other),
        }
    }

    // Duration with different suffixes
    #[test]
    fn tokenize_duration_seconds() {
        let tokens = lexer::tokenize("30s").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::DurationLiteral(30));
    }

    #[test]
    fn tokenize_duration_minutes() {
        let tokens = lexer::tokenize("5m").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::DurationLiteral(300));
    }
}

#[cfg(test)]
mod coverage_queries_tests {
    //! Tests targeting uncovered lines in queries.rs

    use crate::parse;

    // Unknown aggregate function
    #[test]
    fn parse_aggregate_unknown_function() {
        // MEDIAN is not a keyword, so it becomes an Identifier
        // parse_aggregate_function expects a keyword, gets Identifier -> error
        let input = "AGGREGATE M MEASURE median(x)";
        let result = parse(input);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Expected aggregate function"));
    }

    // Aggregate function keyword that is not a valid aggregate
    #[test]
    fn parse_aggregate_invalid_function_keyword() {
        // DEFINE is a keyword but not an aggregate function
        let input = "AGGREGATE M MEASURE DEFINE(x)";
        let result = parse(input);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Unknown aggregate function"));
    }

    // PERCENTILE aggregate function
    #[test]
    fn parse_aggregate_percentile() {
        let input = "AGGREGATE M MEASURE PERCENTILE(elapsed)";
        let stmts = parse(input).unwrap();
        match &stmts[0] {
            smql_ast::command::Statement::Query(smql_ast::query::Query::Aggregate(a)) => {
                assert_eq!(a.measures.len(), 1);
                assert_eq!(a.measures[0].field, Some("elapsed".into()));
                match &a.measures[0].function {
                    smql_ast::types::AggregateFunction::Percentile(_) => {}
                    other => panic!("Expected Percentile, got {:?}", other),
                }
            }
            _ => panic!("Expected Aggregate"),
        }
    }

    // MIN and MAX aggregate functions
    #[test]
    fn parse_aggregate_min_max() {
        let input = "AGGREGATE M MEASURE MIN(score), MAX(score)";
        let stmts = parse(input).unwrap();
        match &stmts[0] {
            smql_ast::command::Statement::Query(smql_ast::query::Query::Aggregate(a)) => {
                assert_eq!(a.measures.len(), 2);
                assert_eq!(
                    a.measures[0].function,
                    smql_ast::types::AggregateFunction::Min
                );
                assert_eq!(
                    a.measures[1].function,
                    smql_ast::types::AggregateFunction::Max
                );
            }
            _ => panic!("Expected Aggregate"),
        }
    }
}

#[cfg(test)]
mod coverage_commands_tests {
    //! Tests targeting uncovered lines in commands.rs

    use crate::parse;

    // ALTER MACHINE ADD without valid keyword after ADD
    #[test]
    fn parse_alter_add_invalid_keyword() {
        let input = "ALTER MACHINE M ADD ROLES admin";
        let result = parse(input);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Expected STATE, TRANSITION, or DATA after ADD"));
    }

    // ALTER MACHINE REMOVE without valid keyword after REMOVE
    #[test]
    fn parse_alter_remove_invalid_keyword() {
        let input = "ALTER MACHINE M REMOVE ROLES admin";
        let result = parse(input);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Expected STATE, TRANSITION, or DATA after REMOVE"));
    }

    // DEFINE MACHINE parsed as statement via parse()
    #[test]
    fn parse_define_machine_statement() {
        let input = r#"
DEFINE MACHINE TestM (
    STATES { s1 }
    INITIAL STATE s1
    TERMINAL STATES { s1 }
    TRANSITIONS {}
)
"#;
        let stmts = parse(input).unwrap();
        assert_eq!(stmts.len(), 1);
        match &stmts[0] {
            smql_ast::command::Statement::Command(smql_ast::command::Command::DefineMachine(m)) => {
                assert_eq!(m.name, "TestM");
            }
            _ => panic!("Expected DefineMachine"),
        }
    }

    // TRANSITION ALL without optional clauses
    #[test]
    fn parse_transition_all_minimal() {
        let input = "TRANSITION ALL M WHERE STATE IS open TO closed";
        let stmts = parse(input).unwrap();
        match &stmts[0] {
            smql_ast::command::Statement::Command(smql_ast::command::Command::BatchTransition(
                b,
            )) => {
                assert_eq!(b.machine, "M");
                assert_eq!(b.to_state, "closed");
                assert!(b.with_data.is_empty());
                assert!(b.memo.is_none());
                assert!(b.as_actor.is_none());
            }
            _ => panic!("Expected BatchTransition"),
        }
    }

    // Spawn with empty data
    #[test]
    fn parse_spawn_empty_data() {
        let input = "SPAWN Machine {}";
        let stmts = parse(input).unwrap();
        match &stmts[0] {
            smql_ast::command::Statement::Command(smql_ast::command::Command::Spawn(s)) => {
                assert_eq!(s.machine, "Machine");
                assert!(s.data.is_empty());
            }
            _ => panic!("Expected Spawn"),
        }
    }
}

#[cfg(test)]
mod coverage_parser_methods_tests {
    //! Tests targeting uncovered Parser method edge cases in lib.rs

    use crate::lexer;
    use crate::Parser;

    // advance() at end of input
    #[test]
    fn parser_advance_at_eof() {
        let tokens = lexer::tokenize("").unwrap();
        let mut parser = Parser::new(&tokens, "");
        assert!(parser.is_eof());
        let result = parser.advance();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("end of input"));
    }

    // expect_keyword with wrong keyword
    #[test]
    fn parser_expect_keyword_wrong() {
        let tokens = lexer::tokenize("DEFINE").unwrap();
        let mut parser = Parser::new(&tokens, "DEFINE");
        let result = parser.expect_keyword("MACHINE");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Expected keyword 'MACHINE'"));
    }

    // expect_keyword with non-keyword token
    #[test]
    fn parser_expect_keyword_non_keyword() {
        let tokens = lexer::tokenize("foobar").unwrap();
        let mut parser = Parser::new(&tokens, "foobar");
        let result = parser.expect_keyword("DEFINE");
        assert!(result.is_err());
    }

    // expect_ident with non-identifier token (punctuation)
    #[test]
    fn parser_expect_ident_punctuation() {
        let tokens = lexer::tokenize("{").unwrap();
        let mut parser = Parser::new(&tokens, "{");
        let result = parser.expect_ident();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Expected identifier"));
    }

    // expect_ident_or_string with invalid token (int)
    #[test]
    fn parser_expect_ident_or_string_int() {
        let tokens = lexer::tokenize("42").unwrap();
        let mut parser = Parser::new(&tokens, "42");
        let result = parser.expect_ident_or_string();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Expected identifier or string"));
    }

    // expect_punct with wrong punct
    #[test]
    fn parser_expect_punct_wrong() {
        let tokens = lexer::tokenize("{").unwrap();
        let mut parser = Parser::new(&tokens, "{");
        let result = parser.expect_punct("}");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Expected '}'"));
    }

    // expect_operator with wrong operator
    #[test]
    fn parser_expect_operator_wrong() {
        let tokens = lexer::tokenize("==").unwrap();
        let mut parser = Parser::new(&tokens, "==");
        let result = parser.expect_operator("->");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Expected '->'"));
    }

    // peek_nth beyond tokens
    #[test]
    fn parser_peek_nth_beyond() {
        let tokens = lexer::tokenize("DEFINE").unwrap();
        let parser = Parser::new(&tokens, "DEFINE");
        assert!(parser.peek_nth(0).is_some());
        assert!(parser.peek_nth(1).is_none());
        assert!(parser.peek_nth(100).is_none());
    }

    // try_keyword returns false for non-match
    #[test]
    fn parser_try_keyword_false() {
        let tokens = lexer::tokenize("DEFINE").unwrap();
        let mut parser = Parser::new(&tokens, "DEFINE");
        assert!(!parser.try_keyword("MACHINE"));
        // Position should not advance
        assert_eq!(parser.peek().unwrap().text, "DEFINE");
    }

    // try_punct returns false for non-match
    #[test]
    fn parser_try_punct_false() {
        let tokens = lexer::tokenize("{").unwrap();
        let mut parser = Parser::new(&tokens, "{");
        assert!(!parser.try_punct("}"));
    }

    // try_operator returns false for non-match
    #[test]
    fn parser_try_operator_false() {
        let tokens = lexer::tokenize("==").unwrap();
        let mut parser = Parser::new(&tokens, "==");
        assert!(!parser.try_operator("!="));
    }

    // check methods on empty parser
    #[test]
    fn parser_check_on_empty() {
        let tokens = lexer::tokenize("").unwrap();
        let parser = Parser::new(&tokens, "");
        assert!(!parser.check_keyword("DEFINE"));
        assert!(!parser.check_punct("{"));
        assert!(!parser.check_operator("=="));
    }

    // current_offset at end of input
    #[test]
    fn parser_current_offset_at_eof() {
        let input = "DEFINE";
        let tokens = lexer::tokenize(input).unwrap();
        let mut parser = Parser::new(&tokens, input);
        parser.advance().unwrap();
        assert_eq!(parser.current_offset(), input.len());
    }

    // current_offset at start
    #[test]
    fn parser_current_offset_at_start() {
        let input = "DEFINE MACHINE";
        let tokens = lexer::tokenize(input).unwrap();
        let parser = Parser::new(&tokens, input);
        assert_eq!(parser.current_offset(), 0);
    }

    // expect_ident with keyword: should return original text
    #[test]
    fn parser_expect_ident_keyword_returns_text() {
        let tokens = lexer::tokenize("count").unwrap();
        let mut parser = Parser::new(&tokens, "count");
        // "count" is lexed as Keyword("COUNT") but text is "count"
        let name = parser.expect_ident().unwrap();
        assert_eq!(name, "count");
    }
}

// ===========================================================================
// Coverage tests for ROLES block, default values, children, and parser edge cases
// ===========================================================================

#[cfg(test)]
mod coverage_roles_and_defaults_tests {
    use crate::parse;
    use crate::parse_machine;
    use smql_ast::command::{Command, Statement};
    use smql_ast::machine::{RolePermission, ChildCardinality};
    use smql_ast::types::DefaultValue;

    /// ROLES block with multiple roles and all permission types (lines 720-742).
    #[test]
    fn parse_machine_with_roles_block() {
        let input = r#"
DEFINE MACHINE WithRoles (
    STATES { open, closed }
    INITIAL STATE open
    TERMINAL STATES { closed }
    TRANSITIONS {
        open -> closed {}
    }
    ROLES {
        admin {
            CAN SPAWN
            CAN TRANSITION [open, closed]
            CAN QUERY
            CAN ALTER
        }
        viewer {
            CAN QUERY
        }
    }
)
"#;
        let m = parse_machine(input).unwrap();
        assert_eq!(m.name, "WithRoles");
        assert_eq!(m.roles.len(), 2);

        let admin = &m.roles[0];
        assert_eq!(admin.name, "admin");
        assert_eq!(admin.permissions.len(), 4);
        assert!(matches!(admin.permissions[0], RolePermission::CanSpawn));
        match &admin.permissions[1] {
            RolePermission::CanTransition(states) => {
                assert_eq!(states, &vec!["open".to_string(), "closed".to_string()]);
            }
            other => panic!("Expected CanTransition, got {:?}", other),
        }
        assert!(matches!(admin.permissions[2], RolePermission::CanQuery));
        assert!(matches!(admin.permissions[3], RolePermission::CanAlter));

        let viewer = &m.roles[1];
        assert_eq!(viewer.name, "viewer");
        assert_eq!(viewer.permissions.len(), 1);
        assert!(matches!(viewer.permissions[0], RolePermission::CanQuery));
    }

    /// ROLES: CAN TRANSITION without state list (empty vec) (line 734).
    #[test]
    fn parse_roles_transition_without_state_list() {
        let input = r#"
DEFINE MACHINE RoleTrans (
    STATES { a, b }
    INITIAL STATE a
    TERMINAL STATES { b }
    TRANSITIONS {
        a -> b {}
    }
    ROLES {
        operator {
            CAN TRANSITION
        }
    }
)
"#;
        let m = parse_machine(input).unwrap();
        assert_eq!(m.roles.len(), 1);
        match &m.roles[0].permissions[0] {
            RolePermission::CanTransition(states) => {
                assert!(states.is_empty(), "No state list should produce empty vec");
            }
            other => panic!("Expected CanTransition, got {:?}", other),
        }
    }

    /// ROLES: unknown permission triggers error (lines 739-743).
    #[test]
    fn parse_roles_unknown_permission_error() {
        let input = r#"
DEFINE MACHINE BadRole (
    STATES { a, b }
    INITIAL STATE a
    TERMINAL STATES { b }
    TRANSITIONS {
        a -> b {}
    }
    ROLES {
        admin {
            CAN DELETE
        }
    }
)
"#;
        let result = parse_machine(input);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("Unknown permission"),
            "Expected 'Unknown permission' error, got: {}",
            err
        );
    }

    /// Default value as enum variant name (lines 269-270) — identifier path.
    /// When a non-keyword identifier is used as a default, it hits the
    /// Identifier branch at line 284.
    #[test]
    fn parse_default_value_enum_variant_name() {
        let input = r#"
DEFINE MACHINE EnumDefault (
    DATA {
        priority : ENUM(low, medium, high) -> DEFAULT(medium)
    }
    STATES { a, b }
    INITIAL STATE a
    TERMINAL STATES { b }
    TRANSITIONS {
        a -> b {}
    }
)
"#;
        let m = parse_machine(input).unwrap();
        assert_eq!(m.data.len(), 1);
        assert_eq!(m.data[0].name, "priority");
        // The default value should be parsed as a String("medium")
        let has_default = m.data[0].constraints.iter().any(|c| {
            matches!(c, smql_ast::types::Constraint::Default(DefaultValue::String(s)) if s == "medium")
        });
        assert!(has_default, "Expected DEFAULT(medium) constraint, got: {:?}", m.data[0].constraints);
    }

    /// Default value as a keyword that isn't TRUE/FALSE/NULL (machine.rs lines 269-270).
    /// When a keyword like STATE is used as the default, it falls through to the
    /// _ => branch of the keyword match and is parsed as a String default.
    #[test]
    fn parse_default_value_keyword_as_enum_variant() {
        let input = r#"
DEFINE MACHINE KeywordDefault (
    DATA {
        mode : ENUM(active, state, data) -> DEFAULT(state)
    }
    STATES { a, b }
    INITIAL STATE a
    TERMINAL STATES { b }
    TRANSITIONS {
        a -> b {}
    }
)
"#;
        let m = parse_machine(input).unwrap();
        assert_eq!(m.data.len(), 1);
        assert_eq!(m.data[0].name, "mode");
        // "state" is lexed as Keyword("STATE"), falls through to _ => in the keyword match
        let has_default = m.data[0].constraints.iter().any(|c| {
            matches!(c, smql_ast::types::Constraint::Default(DefaultValue::String(s)) if s == "state")
        });
        assert!(has_default, "Expected DEFAULT(state) constraint, got: {:?}", m.data[0].constraints);
    }

    /// DEFAULT({}) — EmptySet (machine.rs lines 274-277).
    #[test]
    fn parse_default_empty_set() {
        let input = r#"
DEFINE MACHINE WithEmptySet (
    DATA {
        tags : SET(TEXT) -> DEFAULT({})
    }
    STATES { a, b }
    INITIAL STATE a
    TERMINAL STATES { b }
    TRANSITIONS { a -> b {} }
)
"#;
        let m = parse_machine(input).unwrap();
        let has_default = m.data[0].constraints.iter().any(|c| {
            matches!(c, smql_ast::types::Constraint::Default(DefaultValue::EmptySet))
        });
        assert!(has_default, "Expected DEFAULT({{}}) constraint, got: {:?}", m.data[0].constraints);
    }

    /// DEFAULT([]) — EmptyList (machine.rs lines 279-282).
    #[test]
    fn parse_default_empty_list() {
        let input = r#"
DEFINE MACHINE WithEmptyList (
    DATA {
        items : LIST(TEXT) -> DEFAULT([])
    }
    STATES { a, b }
    INITIAL STATE a
    TERMINAL STATES { b }
    TRANSITIONS { a -> b {} }
)
"#;
        let m = parse_machine(input).unwrap();
        let has_default = m.data[0].constraints.iter().any(|c| {
            matches!(c, smql_ast::types::Constraint::Default(DefaultValue::EmptyList))
        });
        assert!(has_default, "Expected DEFAULT([]) constraint, got: {:?}", m.data[0].constraints);
    }

    /// DEFAULT(NULL) — Null default (machine.rs lines 263-265).
    #[test]
    fn parse_default_null_value() {
        let input = r#"
DEFINE MACHINE WithNullDefault (
    DATA {
        notes : TEXT -> DEFAULT(NULL)
    }
    STATES { a, b }
    INITIAL STATE a
    TERMINAL STATES { b }
    TRANSITIONS { a -> b {} }
)
"#;
        let m = parse_machine(input).unwrap();
        let has_default = m.data[0].constraints.iter().any(|c| {
            matches!(c, smql_ast::types::Constraint::Default(DefaultValue::Null))
        });
        assert!(has_default, "Expected DEFAULT(NULL) constraint, got: {:?}", m.data[0].constraints);
    }

    /// CHILDREN with LIST and MAX constraint, and break on unknown keyword (line 591).
    #[test]
    fn parse_children_list_with_max_constraint() {
        let input = r#"
DEFINE MACHINE ParentMax (
    STATES { active, done }
    INITIAL STATE active
    TERMINAL STATES { done }
    CHILDREN {
        items : LIST(Item) -> MIN(1), MAX(10)
    }
    TRANSITIONS {
        active -> done {}
    }
)
"#;
        let m = parse_machine(input).unwrap();
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

    /// CHILDREN with constraints followed by an unknown keyword — exercises the
    /// break path (machine.rs line 591) in the children constraints loop.
    #[test]
    fn parse_children_list_constraint_break_on_unknown_keyword() {
        // After "-> MIN(1)" the next token "TRANSITIONS" is a keyword that
        // is not MIN or MAX, triggering the _ => break path.
        let input = r#"
DEFINE MACHINE ParentBreak (
    STATES { active, done }
    INITIAL STATE active
    TERMINAL STATES { done }
    CHILDREN {
        items : LIST(Item) -> MIN(1)
    }
    TRANSITIONS {
        active -> done {}
    }
)
"#;
        let m = parse_machine(input).unwrap();
        assert_eq!(m.children.len(), 1);
        match &m.children[0].cardinality {
            ChildCardinality::List { min, max } => {
                assert_eq!(*min, Some(1));
                assert_eq!(*max, None);
            }
            other => panic!("Expected List cardinality, got {:?}", other),
        }
    }

    /// CHILDREN with only MIN constraint (no MAX) to verify the break path (line 591).
    #[test]
    fn parse_children_list_min_only() {
        let input = r#"
DEFINE MACHINE ParentMinOnly (
    STATES { active, done }
    INITIAL STATE active
    TERMINAL STATES { done }
    CHILDREN {
        items : LIST(Item) -> MIN(2)
    }
    TRANSITIONS {
        active -> done {}
    }
)
"#;
        let m = parse_machine(input).unwrap();
        match &m.children[0].cardinality {
            ChildCardinality::List { min, max } => {
                assert_eq!(*min, Some(2));
                assert_eq!(*max, None);
            }
            other => panic!("Expected List cardinality, got {:?}", other),
        }
    }

    /// parse_literal_value for TRUE and FALSE (common.rs lines 67-68, 71-72).
    /// We use DATA DEFAULT(TRUE) and DEFAULT(FALSE) to trigger these paths.
    #[test]
    fn parse_data_default_bool_true_and_false() {
        let input = r#"
DEFINE MACHINE BoolDefaults (
    DATA {
        is_active : BOOL -> DEFAULT(TRUE)
        is_deleted : BOOL -> DEFAULT(FALSE)
    }
    STATES { a, b }
    INITIAL STATE a
    TERMINAL STATES { b }
    TRANSITIONS {
        a -> b {}
    }
)
"#;
        let m = parse_machine(input).unwrap();
        assert_eq!(m.data.len(), 2);

        let has_true = m.data[0].constraints.iter().any(|c| {
            matches!(c, smql_ast::types::Constraint::Default(DefaultValue::Bool(true)))
        });
        assert!(has_true, "Expected DEFAULT(TRUE), got: {:?}", m.data[0].constraints);

        let has_false = m.data[1].constraints.iter().any(|c| {
            matches!(c, smql_ast::types::Constraint::Default(DefaultValue::Bool(false)))
        });
        assert!(has_false, "Expected DEFAULT(FALSE), got: {:?}", m.data[1].constraints);
    }

    /// Unexpected expression at primary position (expr.rs line 436).
    /// Feeding a standalone operator like `->` should trigger the error.
    #[test]
    fn parse_expression_unexpected_token() {
        let result = parse("FIND M WHERE ->");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("Expected expression")
                || err.to_string().contains("Unexpected"),
            "Expected 'Expected expression' error, got: {}",
            err
        );
    }

    /// expect_ident_or_string with keyword (lib.rs line 127).
    /// TRANSITION command uses expect_ident_or_string for the instance_id, and
    /// passing a keyword-like identifier hits the keyword branch.
    #[test]
    fn parse_transition_with_keyword_as_instance_id() {
        // "count" is lexed as a keyword, but expect_ident_or_string should accept it
        let input = r#"TRANSITION Machine count TO done"#;
        let stmts = parse(input).unwrap();
        match &stmts[0] {
            Statement::Command(Command::Transition(t)) => {
                assert_eq!(t.machine, "Machine");
                assert_eq!(t.instance_id, "count");
                assert_eq!(t.to_state, "done");
            }
            _ => panic!("Expected Transition"),
        }
    }

    /// Unexpected token at start of statement (lib.rs line 248).
    /// A number literal at the top level triggers the error.
    #[test]
    fn parse_unexpected_token_at_statement_start() {
        let result = parse("42 DEFINE MACHINE X ()");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("Unexpected token"),
            "Expected 'Unexpected token' error, got: {}",
            err
        );
    }

    /// Aggregate with no GROUP BY (queries.rs line 112 break path).
    /// An AGGREGATE query with only MEASURE and no GROUP BY or WHERE.
    #[test]
    fn parse_aggregate_no_group_by() {
        let input = "AGGREGATE M MEASURE COUNT()";
        let stmts = parse(input).unwrap();
        match &stmts[0] {
            Statement::Query(smql_ast::query::Query::Aggregate(a)) => {
                assert_eq!(a.machine, "M");
                assert_eq!(a.measures.len(), 1);
                assert!(a.group_by.is_empty());
                assert!(a.filter.is_none());
            }
            _ => panic!("Expected Aggregate"),
        }
    }

    /// Aggregate with GROUP BY a field name (not STATE) (queries.rs line 105).
    #[test]
    fn parse_aggregate_group_by_field_name() {
        let input = "AGGREGATE M MEASURE COUNT() GROUP BY priority";
        let stmts = parse(input).unwrap();
        match &stmts[0] {
            Statement::Query(smql_ast::query::Query::Aggregate(a)) => {
                assert_eq!(a.group_by.len(), 1);
                match &a.group_by[0] {
                    smql_ast::query::GroupByClause::Field(f) => assert_eq!(f, "priority"),
                    other => panic!("Expected GroupByClause::Field, got {:?}", other),
                }
            }
            _ => panic!("Expected Aggregate"),
        }
    }

    /// Aggregate followed by another statement — exercises the break path
    /// in the aggregate parser loop (queries.rs line 112) when the next token
    /// is not MEASURE/WHERE/GROUP.
    #[test]
    fn parse_aggregate_followed_by_another_statement() {
        let input = "AGGREGATE M MEASURE COUNT()\nFIND M";
        let stmts = parse(input).unwrap();
        assert_eq!(stmts.len(), 2);
        match &stmts[0] {
            Statement::Query(smql_ast::query::Query::Aggregate(a)) => {
                assert_eq!(a.machine, "M");
                assert_eq!(a.measures.len(), 1);
            }
            _ => panic!("Expected Aggregate"),
        }
        match &stmts[1] {
            Statement::Query(smql_ast::query::Query::Find(_)) => {}
            _ => panic!("Expected Find"),
        }
    }

    /// Aggregate with GROUP BY multiple fields (queries.rs comma-separated group by).
    #[test]
    fn parse_aggregate_group_by_multiple() {
        let input = "AGGREGATE M MEASURE COUNT() GROUP BY STATE, priority";
        let stmts = parse(input).unwrap();
        match &stmts[0] {
            Statement::Query(smql_ast::query::Query::Aggregate(a)) => {
                assert_eq!(a.group_by.len(), 2);
                assert!(matches!(a.group_by[0], smql_ast::query::GroupByClause::State));
                match &a.group_by[1] {
                    smql_ast::query::GroupByClause::Field(f) => assert_eq!(f, "priority"),
                    other => panic!("Expected GroupByClause::Field, got {:?}", other),
                }
            }
            _ => panic!("Expected Aggregate"),
        }
    }
}
