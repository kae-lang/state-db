#[cfg(test)]
mod eval_tests {
    use crate::eval::{eval_expr, eval_guard, ActorInfo, EvalContext};
    use smql_ast::expression::{BinaryOperator, Expression, ExpressionKind, UnaryOperator};
    use smql_ast::value::{SmqlDuration, Value};
    use std::collections::HashMap;

    fn ctx_with_data(data: Vec<(&str, Value)>) -> EvalContext {
        let map: HashMap<String, Value> = data
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        EvalContext::new(map, "open".to_string())
    }

    fn lit(v: Value) -> Expression {
        Expression::new(ExpressionKind::Literal(v))
    }

    fn field(name: &str) -> Expression {
        Expression::new(ExpressionKind::FieldAccess(vec![name.to_string()]))
    }

    fn binop(left: Expression, op: BinaryOperator, right: Expression) -> Expression {
        Expression::new(ExpressionKind::BinaryOp {
            left: Box::new(left),
            op,
            right: Box::new(right),
        })
    }

    #[test]
    fn eval_literal() {
        let ctx = ctx_with_data(vec![]);
        let expr = lit(Value::Int(42));
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Int(42));
    }

    #[test]
    fn eval_field_access() {
        let ctx = ctx_with_data(vec![("priority", Value::Int(5))]);
        let expr = field("priority");
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Int(5));
    }

    #[test]
    fn eval_missing_field() {
        let ctx = ctx_with_data(vec![]);
        let expr = field("missing");
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Null);
    }

    #[test]
    fn eval_comparison_eq() {
        let ctx = ctx_with_data(vec![("x", Value::Int(5))]);
        let expr = binop(field("x"), BinaryOperator::Eq, lit(Value::Int(5)));
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));
    }

    #[test]
    fn eval_comparison_gt() {
        let ctx = ctx_with_data(vec![("x", Value::Int(10))]);
        let expr = binop(field("x"), BinaryOperator::Gt, lit(Value::Int(5)));
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));
    }

    #[test]
    fn eval_comparison_lt() {
        let ctx = ctx_with_data(vec![("x", Value::Int(3))]);
        let expr = binop(field("x"), BinaryOperator::Lt, lit(Value::Int(5)));
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));
    }

    #[test]
    fn eval_and() {
        let ctx = ctx_with_data(vec![("a", Value::Bool(true)), ("b", Value::Bool(false))]);
        let expr = binop(field("a"), BinaryOperator::And, field("b"));
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(false));
    }

    #[test]
    fn eval_or() {
        let ctx = ctx_with_data(vec![("a", Value::Bool(true)), ("b", Value::Bool(false))]);
        let expr = binop(field("a"), BinaryOperator::Or, field("b"));
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));
    }

    #[test]
    fn eval_not() {
        let ctx = ctx_with_data(vec![("x", Value::Bool(true))]);
        let expr = Expression::new(ExpressionKind::UnaryOp {
            op: UnaryOperator::Not,
            operand: Box::new(field("x")),
        });
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(false));
    }

    #[test]
    fn eval_arithmetic_add() {
        let ctx = ctx_with_data(vec![]);
        let expr = binop(lit(Value::Int(3)), BinaryOperator::Add, lit(Value::Int(4)));
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Int(7));
    }

    #[test]
    fn eval_arithmetic_div_by_zero() {
        let ctx = ctx_with_data(vec![]);
        let expr = binop(lit(Value::Int(10)), BinaryOperator::Div, lit(Value::Int(0)));
        assert!(eval_expr(&expr, &ctx).is_err());
    }

    #[test]
    fn eval_int_float_coercion() {
        let ctx = ctx_with_data(vec![]);
        let expr = binop(lit(Value::Int(5)), BinaryOperator::Eq, lit(Value::Float(5.0)));
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));
    }

    #[test]
    fn eval_state_is() {
        let ctx = ctx_with_data(vec![]);
        let expr = Expression::new(ExpressionKind::StateIs("open".to_string()));
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));

        let expr2 = Expression::new(ExpressionKind::StateIs("closed".to_string()));
        assert_eq!(eval_expr(&expr2, &ctx).unwrap(), Value::Bool(false));
    }

    #[test]
    fn eval_state_in() {
        let ctx = ctx_with_data(vec![]);
        let expr = Expression::new(ExpressionKind::StateIn(vec![
            "open".to_string(),
            "pending".to_string(),
        ]));
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));
    }

    #[test]
    fn eval_is_set() {
        let ctx = ctx_with_data(vec![("x", Value::Int(5))]);
        let expr = Expression::new(ExpressionKind::IsSet(Box::new(field("x"))));
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));

        let expr2 = Expression::new(ExpressionKind::IsSet(Box::new(field("missing"))));
        assert_eq!(eval_expr(&expr2, &ctx).unwrap(), Value::Bool(false));
    }

    #[test]
    fn eval_is_not_set() {
        let ctx = ctx_with_data(vec![("x", Value::Null)]);
        let expr = Expression::new(ExpressionKind::IsNotSet(Box::new(field("x"))));
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));
    }

    #[test]
    fn eval_in_set() {
        let ctx = ctx_with_data(vec![("status", Value::Text("active".to_string()))]);
        let expr = Expression::new(ExpressionKind::InSet {
            expr: Box::new(field("status")),
            values: vec![
                lit(Value::Text("active".to_string())),
                lit(Value::Text("pending".to_string())),
            ],
        });
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));
    }

    #[test]
    fn eval_function_now() {
        let ctx = ctx_with_data(vec![]);
        let expr = Expression::new(ExpressionKind::FunctionCall {
            name: "NOW".to_string(),
            args: vec![],
        });
        let result = eval_expr(&expr, &ctx).unwrap();
        assert!(matches!(result, Value::DateTime(_)));
    }

    #[test]
    fn eval_function_elapsed() {
        let mut ctx = ctx_with_data(vec![]);
        ctx.state_entered_at = ctx.now - chrono::Duration::hours(2);
        let expr = Expression::new(ExpressionKind::FunctionCall {
            name: "elapsed".to_string(),
            args: vec![],
        });
        let result = eval_expr(&expr, &ctx).unwrap();
        if let Value::Duration(d) = result {
            assert!(d.seconds >= 7200); // at least 2 hours
        } else {
            panic!("Expected Duration");
        }
    }

    #[test]
    fn eval_duration_comparison() {
        let mut ctx = ctx_with_data(vec![]);
        ctx.state_entered_at = ctx.now - chrono::Duration::hours(25);

        let elapsed = Expression::new(ExpressionKind::FunctionCall {
            name: "elapsed".to_string(),
            args: vec![],
        });
        let threshold = Expression::new(ExpressionKind::DurationLiteral(
            SmqlDuration::from_hours(24),
        ));
        let expr = binop(elapsed, BinaryOperator::Gt, threshold);
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));
    }

    #[test]
    fn eval_actor_ref() {
        let mut ctx = ctx_with_data(vec![]);
        ctx.actor = Some(ActorInfo {
            id: "user_123".to_string(),
            role: Some("admin".to_string()),
            fields: HashMap::new(),
        });

        let expr = Expression::new(ExpressionKind::QualifiedAccess {
            root: Box::new(Expression::new(ExpressionKind::ActorRef)),
            path: vec!["role".to_string()],
        });
        assert_eq!(
            eval_expr(&expr, &ctx).unwrap(),
            Value::Text("admin".to_string())
        );
    }

    #[test]
    fn eval_guard_passes() {
        let ctx = ctx_with_data(vec![("priority", Value::Int(5))]);
        let guard = binop(field("priority"), BinaryOperator::Gt, lit(Value::Int(3)));
        assert!(eval_guard(&guard, &ctx).unwrap());
    }

    #[test]
    fn eval_guard_fails() {
        let ctx = ctx_with_data(vec![("priority", Value::Int(1))]);
        let guard = binop(field("priority"), BinaryOperator::Gt, lit(Value::Int(3)));
        assert!(!eval_guard(&guard, &ctx).unwrap());
    }

    #[test]
    fn eval_count_list() {
        let ctx = ctx_with_data(vec![(
            "items",
            Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)]),
        )]);
        let expr = Expression::new(ExpressionKind::Count(Some(Box::new(field("items")))));
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Int(3));
    }

    #[test]
    fn eval_neg() {
        let ctx = ctx_with_data(vec![]);
        let expr = Expression::new(ExpressionKind::UnaryOp {
            op: UnaryOperator::Neg,
            operand: Box::new(lit(Value::Int(5))),
        });
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Int(-5));
    }

    #[test]
    fn eval_string_concat() {
        let ctx = ctx_with_data(vec![]);
        let expr = binop(
            lit(Value::Text("hello".to_string())),
            BinaryOperator::Add,
            lit(Value::Text(" world".to_string())),
        );
        assert_eq!(
            eval_expr(&expr, &ctx).unwrap(),
            Value::Text("hello world".to_string())
        );
    }
}

#[cfg(test)]
mod engine_tests {
    use crate::engine::Engine;
    use smql_ast::command::{SpawnCommand, TransitionCommand};
    use smql_ast::expression::{BinaryOperator, Expression, ExpressionKind};
    use smql_ast::machine::*;
    use smql_ast::types::*;
    use smql_ast::value::Value;
    use smql_catalog::MachineCatalog;
    use smql_storage::MemoryStorage;
    use std::sync::Arc;

    fn setup_engine() -> Engine {
        let catalog = Arc::new(MachineCatalog::new());
        let storage = Arc::new(MemoryStorage::new());
        Engine::new(catalog, storage)
    }

    fn register_ticket_machine(engine: &Engine) {
        let mut m = MachineDefinition::new("Ticket".into(), "open".into());
        m.states = vec![
            StateDefinition::new("open".into()),
            StateDefinition::new("in_progress".into()),
            StateDefinition::new("resolved".into()),
            StateDefinition::new("closed".into()),
        ];
        m.terminal_states = vec!["closed".into()];
        m.data = vec![
            DataFieldDefinition {
                name: "title".into(),
                field_type: TypeDefinition::Text,
                constraints: vec![Constraint::Required],
            },
            DataFieldDefinition {
                name: "priority".into(),
                field_type: TypeDefinition::Int,
                constraints: vec![Constraint::Default(DefaultValue::Int(3))],
            },
            DataFieldDefinition {
                name: "assignee".into(),
                field_type: TypeDefinition::Text,
                constraints: vec![Constraint::Optional],
            },
        ];
        m.transitions = vec![
            TransitionDefinition::new(
                TransitionSource::State("open".into()),
                "in_progress".into(),
            ),
            TransitionDefinition::new(
                TransitionSource::State("in_progress".into()),
                "resolved".into(),
            ),
            TransitionDefinition::new(
                TransitionSource::State("resolved".into()),
                "closed".into(),
            ),
            TransitionDefinition::new(
                TransitionSource::State("in_progress".into()),
                "open".into(),
            ),
        ];

        engine.catalog.register(m).unwrap();
    }

    fn register_guarded_machine(engine: &Engine) {
        let mut m = MachineDefinition::new("GuardedMachine".into(), "draft".into());
        m.states = vec![
            StateDefinition::new("draft".into()),
            StateDefinition::new("published".into()),
            StateDefinition::new("archived".into()),
        ];
        m.terminal_states = vec!["archived".into()];
        m.data = vec![
            DataFieldDefinition {
                name: "title".into(),
                field_type: TypeDefinition::Text,
                constraints: vec![Constraint::Required],
            },
            DataFieldDefinition {
                name: "approved".into(),
                field_type: TypeDefinition::Bool,
                constraints: vec![Constraint::Default(DefaultValue::Bool(false))],
            },
        ];

        // draft -> published with guard: approved == true
        let mut t = TransitionDefinition::new(
            TransitionSource::State("draft".into()),
            "published".into(),
        );
        t.guards = vec![Expression::new(ExpressionKind::BinaryOp {
            left: Box::new(Expression::new(ExpressionKind::FieldAccess(vec![
                "approved".to_string(),
            ]))),
            op: BinaryOperator::Eq,
            right: Box::new(Expression::new(ExpressionKind::Literal(Value::Bool(true)))),
        })];

        let t2 = TransitionDefinition::new(
            TransitionSource::State("published".into()),
            "archived".into(),
        );

        m.transitions = vec![t, t2];
        engine.catalog.register(m).unwrap();
    }

    fn register_wildcard_machine(engine: &Engine) {
        let mut m = MachineDefinition::new("WildcardMachine".into(), "a".into());
        m.states = vec![
            StateDefinition::new("a".into()),
            StateDefinition::new("b".into()),
            StateDefinition::new("cancelled".into()),
        ];
        m.terminal_states = vec!["cancelled".into()];
        m.transitions = vec![
            TransitionDefinition::new(TransitionSource::State("a".into()), "b".into()),
            TransitionDefinition::new(
                TransitionSource::Any {
                    except: vec!["cancelled".into()],
                },
                "cancelled".into(),
            ),
        ];
        engine.catalog.register(m).unwrap();
    }

    fn spawn_cmd(machine: &str, data: Vec<(&str, Value)>) -> SpawnCommand {
        SpawnCommand {
            machine: machine.to_string(),
            data: data
                .into_iter()
                .map(|(k, v)| {
                    (
                        k.to_string(),
                        Expression::new(ExpressionKind::Literal(v)),
                    )
                })
                .collect(),
            then_transition: None,
            batch: false,
            batch_data: Vec::new(),
        }
    }

    fn transition_cmd(instance_id: &str, to_state: &str) -> TransitionCommand {
        TransitionCommand::new(instance_id.to_string(), to_state.to_string())
    }

    // --- Spawn tests ---

    #[tokio::test]
    async fn spawn_basic() {
        let engine = setup_engine();
        register_ticket_machine(&engine);

        let cmd = spawn_cmd("Ticket", vec![("title", Value::Text("Bug fix".into()))]);
        let result = engine.spawn(&cmd).await.unwrap();

        assert_eq!(result.instance.machine, "Ticket");
        assert_eq!(result.instance.state, "open");
        assert_eq!(
            result.instance.data.get("title"),
            Some(&Value::Text("Bug fix".into()))
        );
        // Default value should be applied
        assert_eq!(result.instance.data.get("priority"), Some(&Value::Int(3)));
    }

    #[tokio::test]
    async fn spawn_missing_required_field() {
        let engine = setup_engine();
        register_ticket_machine(&engine);

        let cmd = spawn_cmd("Ticket", vec![]); // Missing required "title"
        let result = engine.spawn(&cmd).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("title"));
    }

    #[tokio::test]
    async fn spawn_with_all_fields() {
        let engine = setup_engine();
        register_ticket_machine(&engine);

        let cmd = spawn_cmd(
            "Ticket",
            vec![
                ("title", Value::Text("Feature request".into())),
                ("priority", Value::Int(1)),
                ("assignee", Value::Text("alice".into())),
            ],
        );
        let result = engine.spawn(&cmd).await.unwrap();
        assert_eq!(result.instance.data.get("priority"), Some(&Value::Int(1)));
        assert_eq!(
            result.instance.data.get("assignee"),
            Some(&Value::Text("alice".into()))
        );
    }

    #[tokio::test]
    async fn spawn_unknown_machine() {
        let engine = setup_engine();
        let cmd = spawn_cmd("NonExistent", vec![]);
        let result = engine.spawn(&cmd).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn spawn_creates_trail() {
        let engine = setup_engine();
        register_ticket_machine(&engine);

        let cmd = spawn_cmd("Ticket", vec![("title", Value::Text("test".into()))]);
        let result = engine.spawn(&cmd).await.unwrap();

        let trail = engine
            .storage
            .get_trail(&result.instance.id)
            .await
            .unwrap();
        assert_eq!(trail.len(), 1);
        assert_eq!(trail[0].to_state, "open");
        assert_eq!(trail[0].transition_name, Some("SPAWN".to_string()));
    }

    // --- Transition tests ---

    #[tokio::test]
    async fn transition_basic() {
        let engine = setup_engine();
        register_ticket_machine(&engine);

        let spawn = spawn_cmd("Ticket", vec![("title", Value::Text("test".into()))]);
        let spawned = engine.spawn(&spawn).await.unwrap();
        let id = spawned.instance.id.as_str();

        let cmd = transition_cmd(&id, "in_progress");
        let result = engine.transition(&cmd).await.unwrap();
        assert_eq!(result.from_state, "open");
        assert_eq!(result.to_state, "in_progress");
        assert_eq!(result.instance.state, "in_progress");
        assert_eq!(result.instance.version, 2);
    }

    #[tokio::test]
    async fn transition_invalid_path() {
        let engine = setup_engine();
        register_ticket_machine(&engine);

        let spawn = spawn_cmd("Ticket", vec![("title", Value::Text("test".into()))]);
        let spawned = engine.spawn(&spawn).await.unwrap();
        let id = spawned.instance.id.as_str();

        // open -> closed is not a valid transition
        let cmd = transition_cmd(&id, "closed");
        let result = engine.transition(&cmd).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn transition_with_data() {
        let engine = setup_engine();
        register_ticket_machine(&engine);

        let spawn = spawn_cmd("Ticket", vec![("title", Value::Text("test".into()))]);
        let spawned = engine.spawn(&spawn).await.unwrap();
        let id = spawned.instance.id.as_str();

        let mut cmd = transition_cmd(&id, "in_progress");
        cmd.with_data = vec![(
            "assignee".to_string(),
            Expression::new(ExpressionKind::Literal(Value::Text("bob".into()))),
        )];
        let result = engine.transition(&cmd).await.unwrap();
        assert_eq!(
            result.instance.data.get("assignee"),
            Some(&Value::Text("bob".into()))
        );
    }

    #[tokio::test]
    async fn transition_with_memo_and_actor() {
        let engine = setup_engine();
        register_ticket_machine(&engine);

        let spawn = spawn_cmd("Ticket", vec![("title", Value::Text("test".into()))]);
        let spawned = engine.spawn(&spawn).await.unwrap();
        let id = spawned.instance.id.as_str();

        let mut cmd = transition_cmd(&id, "in_progress");
        cmd.memo = Some("Starting work".to_string());
        cmd.as_actor = Some("alice".to_string());
        engine.transition(&cmd).await.unwrap();

        let trail = engine
            .storage
            .get_trail(&spawned.instance.id)
            .await
            .unwrap();
        // Trail should have spawn entry + transition entry
        assert_eq!(trail.len(), 2);
        assert_eq!(trail[1].memo, Some("Starting work".to_string()));
        assert_eq!(trail[1].actor, Some("alice".to_string()));
    }

    // --- Guard tests ---

    #[tokio::test]
    async fn transition_guard_passes() {
        let engine = setup_engine();
        register_guarded_machine(&engine);

        let cmd = spawn_cmd(
            "GuardedMachine",
            vec![
                ("title", Value::Text("test".into())),
                ("approved", Value::Bool(true)),
            ],
        );
        let spawned = engine.spawn(&cmd).await.unwrap();
        let id = spawned.instance.id.as_str();

        let result = engine.transition(&transition_cmd(&id, "published")).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().instance.state, "published");
    }

    #[tokio::test]
    async fn transition_guard_fails() {
        let engine = setup_engine();
        register_guarded_machine(&engine);

        let cmd = spawn_cmd(
            "GuardedMachine",
            vec![
                ("title", Value::Text("test".into())),
                ("approved", Value::Bool(false)),
            ],
        );
        let spawned = engine.spawn(&cmd).await.unwrap();
        let id = spawned.instance.id.as_str();

        let result = engine.transition(&transition_cmd(&id, "published")).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, smql_ast::SmqlError::TransitionDenied(_)));
    }

    // --- TRY TRANSITION ---

    #[tokio::test]
    async fn try_transition_success() {
        let engine = setup_engine();
        register_ticket_machine(&engine);

        let spawn = spawn_cmd("Ticket", vec![("title", Value::Text("test".into()))]);
        let spawned = engine.spawn(&spawn).await.unwrap();
        let id = spawned.instance.id.as_str();

        let cmd = transition_cmd(&id, "in_progress");
        let result = engine.try_transition(&cmd).await.unwrap();
        assert!(result.is_some());
    }

    #[tokio::test]
    async fn try_transition_guard_fail_returns_none() {
        let engine = setup_engine();
        register_guarded_machine(&engine);

        let cmd = spawn_cmd(
            "GuardedMachine",
            vec![("title", Value::Text("test".into()))],
        );
        let spawned = engine.spawn(&cmd).await.unwrap();
        let id = spawned.instance.id.as_str();

        let result = engine
            .try_transition(&transition_cmd(&id, "published"))
            .await
            .unwrap();
        assert!(result.is_none());
    }

    // --- Wildcard transitions ---

    #[tokio::test]
    async fn wildcard_transition() {
        let engine = setup_engine();
        register_wildcard_machine(&engine);

        let spawn = spawn_cmd("WildcardMachine", vec![]);
        let spawned = engine.spawn(&spawn).await.unwrap();
        let id = spawned.instance.id.as_str();

        // a -> cancelled via wildcard
        let result = engine
            .transition(&transition_cmd(&id, "cancelled"))
            .await
            .unwrap();
        assert_eq!(result.instance.state, "cancelled");
    }

    #[tokio::test]
    async fn wildcard_except_blocked() {
        let engine = setup_engine();
        register_wildcard_machine(&engine);

        let spawn = spawn_cmd("WildcardMachine", vec![]);
        let spawned = engine.spawn(&spawn).await.unwrap();
        let id = spawned.instance.id.as_str();

        // First transition to cancelled
        engine
            .transition(&transition_cmd(&id, "cancelled"))
            .await
            .unwrap();

        // cancelled -> cancelled should be blocked by EXCEPT FROM
        let result = engine
            .transition(&transition_cmd(&id, "cancelled"))
            .await;
        assert!(result.is_err());
    }

    // --- THROUGH multi-hop ---

    #[tokio::test]
    async fn transition_through() {
        let engine = setup_engine();
        register_ticket_machine(&engine);

        let spawn = spawn_cmd("Ticket", vec![("title", Value::Text("test".into()))]);
        let spawned = engine.spawn(&spawn).await.unwrap();
        let id = spawned.instance.id.as_str();

        let mut cmd = transition_cmd(&id, "closed");
        cmd.through = vec!["in_progress".to_string(), "resolved".to_string()];
        let result = engine.transition(&cmd).await.unwrap();
        assert_eq!(result.instance.state, "closed");
    }

    // --- OR STAY ---

    #[tokio::test]
    async fn transition_or_stay() {
        let engine = setup_engine();
        register_guarded_machine(&engine);

        let cmd = spawn_cmd(
            "GuardedMachine",
            vec![("title", Value::Text("test".into()))],
        );
        let spawned = engine.spawn(&cmd).await.unwrap();
        let id = spawned.instance.id.as_str();

        let mut tcmd = transition_cmd(&id, "published");
        tcmd.or_stay = true;
        tcmd.with_data = vec![(
            "title".to_string(),
            Expression::new(ExpressionKind::Literal(Value::Text("updated".into()))),
        )];

        // Should not error, should stay in draft, but apply data mutations
        let result = engine.transition(&tcmd).await.unwrap();
        assert_eq!(result.instance.state, "draft");
        assert_eq!(
            result.instance.data.get("title"),
            Some(&Value::Text("updated".into()))
        );
    }

    // --- Version conflict ---

    #[tokio::test]
    async fn concurrent_transition_version_conflict() {
        let engine = setup_engine();
        register_ticket_machine(&engine);

        let spawn = spawn_cmd("Ticket", vec![("title", Value::Text("test".into()))]);
        let spawned = engine.spawn(&spawn).await.unwrap();
        let id_str = spawned.instance.id.as_str();

        // Transition once
        engine
            .transition(&transition_cmd(&id_str, "in_progress"))
            .await
            .unwrap();

        // Instance is now version 2. A second transition should work.
        let result = engine
            .transition(&transition_cmd(&id_str, "resolved"))
            .await;
        assert!(result.is_ok());
    }

    // --- Spawn then transition ---

    #[tokio::test]
    async fn spawn_then_transition() {
        let engine = setup_engine();
        register_ticket_machine(&engine);

        let mut cmd = spawn_cmd("Ticket", vec![("title", Value::Text("urgent".into()))]);
        cmd.then_transition = Some("in_progress".to_string());

        let result = engine.spawn(&cmd).await.unwrap();
        assert_eq!(result.instance.state, "in_progress");
    }

    // --- Multiple transitions create trail ---

    #[tokio::test]
    async fn trail_records_transitions() {
        let engine = setup_engine();
        register_ticket_machine(&engine);

        let spawn = spawn_cmd("Ticket", vec![("title", Value::Text("test".into()))]);
        let spawned = engine.spawn(&spawn).await.unwrap();
        let id = spawned.instance.id.as_str();

        engine
            .transition(&transition_cmd(&id, "in_progress"))
            .await
            .unwrap();
        engine
            .transition(&transition_cmd(&id, "resolved"))
            .await
            .unwrap();
        engine
            .transition(&transition_cmd(&id, "closed"))
            .await
            .unwrap();

        let trail = engine
            .storage
            .get_trail(&spawned.instance.id)
            .await
            .unwrap();
        // Spawn + 3 transitions = 4 trail entries
        assert_eq!(trail.len(), 4);
    }
}

#[cfg(test)]
mod query_tests {
    use crate::engine::Engine;
    use crate::query::QueryResult;
    use smql_ast::command::{SpawnCommand, TransitionCommand};
    use smql_ast::expression::{BinaryOperator, Expression, ExpressionKind};
    use smql_ast::machine::*;
    use smql_ast::query::*;
    use smql_ast::types::*;
    use smql_ast::value::Value;
    use smql_catalog::MachineCatalog;
    use smql_storage::MemoryStorage;
    use std::sync::Arc;

    fn setup_engine() -> Engine {
        let catalog = Arc::new(MachineCatalog::new());
        let storage = Arc::new(MemoryStorage::new());
        Engine::new(catalog, storage)
    }

    fn register_ticket_machine(engine: &Engine) {
        let mut m = MachineDefinition::new("Ticket".into(), "open".into());
        m.states = vec![
            StateDefinition::new("open".into()),
            StateDefinition::new("in_progress".into()),
            StateDefinition::new("resolved".into()),
            StateDefinition::new("closed".into()),
        ];
        m.terminal_states = vec!["closed".into()];
        m.data = vec![
            DataFieldDefinition {
                name: "title".into(),
                field_type: TypeDefinition::Text,
                constraints: vec![Constraint::Required],
            },
            DataFieldDefinition {
                name: "priority".into(),
                field_type: TypeDefinition::Int,
                constraints: vec![Constraint::Default(DefaultValue::Int(3))],
            },
        ];
        m.transitions = vec![
            TransitionDefinition::new(
                TransitionSource::State("open".into()),
                "in_progress".into(),
            ),
            TransitionDefinition::new(
                TransitionSource::State("in_progress".into()),
                "resolved".into(),
            ),
            TransitionDefinition::new(
                TransitionSource::State("resolved".into()),
                "closed".into(),
            ),
        ];
        engine.catalog.register(m).unwrap();
    }

    fn spawn_ticket(_engine: &Engine, title: &str, priority: i64) -> SpawnCommand {
        SpawnCommand {
            machine: "Ticket".to_string(),
            data: vec![
                (
                    "title".to_string(),
                    Expression::new(ExpressionKind::Literal(Value::Text(title.into()))),
                ),
                (
                    "priority".to_string(),
                    Expression::new(ExpressionKind::Literal(Value::Int(priority))),
                ),
            ],
            then_transition: None,
            batch: false,
            batch_data: Vec::new(),
        }
    }

    #[tokio::test]
    async fn query_get() {
        let engine = setup_engine();
        register_ticket_machine(&engine);

        let cmd = spawn_ticket(&engine, "Bug", 1);
        let spawned = engine.spawn(&cmd).await.unwrap();
        let id = spawned.instance.id.as_str();

        let query = Query::Get(GetQuery {
            machine: "Ticket".into(),
            instance_id: id.clone(),
        });
        let result = engine.execute_query(&query).await.unwrap();
        if let QueryResult::Instance(inst) = result {
            assert_eq!(inst.state, "open");
            assert_eq!(inst.data.get("title"), Some(&Value::Text("Bug".into())));
        } else {
            panic!("Expected Instance result");
        }
    }

    #[tokio::test]
    async fn query_get_not_found() {
        let engine = setup_engine();
        register_ticket_machine(&engine);

        let query = Query::Get(GetQuery {
            machine: "Ticket".into(),
            instance_id: "01NONEXISTENT000000000000".into(),
        });
        let result = engine.execute_query(&query).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn query_find_all() {
        let engine = setup_engine();
        register_ticket_machine(&engine);

        engine.spawn(&spawn_ticket(&engine, "A", 1)).await.unwrap();
        engine.spawn(&spawn_ticket(&engine, "B", 2)).await.unwrap();
        engine.spawn(&spawn_ticket(&engine, "C", 3)).await.unwrap();

        let query = Query::Find(FindQuery {
            machine: "Ticket".into(),
            filter: None,
            sort: Vec::new(),
            limit: None,
            offset: None,
        });
        let result = engine.execute_query(&query).await.unwrap();
        if let QueryResult::Instances(insts) = result {
            assert_eq!(insts.len(), 3);
        } else {
            panic!("Expected Instances result");
        }
    }

    #[tokio::test]
    async fn query_find_with_filter() {
        let engine = setup_engine();
        register_ticket_machine(&engine);

        engine.spawn(&spawn_ticket(&engine, "Low", 5)).await.unwrap();
        engine.spawn(&spawn_ticket(&engine, "High", 1)).await.unwrap();
        engine.spawn(&spawn_ticket(&engine, "Medium", 3)).await.unwrap();

        // FIND Ticket WHERE priority < 3
        let filter = Expression::new(ExpressionKind::BinaryOp {
            left: Box::new(Expression::new(ExpressionKind::FieldAccess(vec![
                "priority".to_string(),
            ]))),
            op: BinaryOperator::Lt,
            right: Box::new(Expression::new(ExpressionKind::Literal(Value::Int(3)))),
        });

        let query = Query::Find(FindQuery {
            machine: "Ticket".into(),
            filter: Some(filter),
            sort: Vec::new(),
            limit: None,
            offset: None,
        });
        let result = engine.execute_query(&query).await.unwrap();
        if let QueryResult::Instances(insts) = result {
            assert_eq!(insts.len(), 1);
            assert_eq!(
                insts[0].data.get("title"),
                Some(&Value::Text("High".into()))
            );
        } else {
            panic!("Expected Instances result");
        }
    }

    #[tokio::test]
    async fn query_find_with_sort() {
        let engine = setup_engine();
        register_ticket_machine(&engine);

        engine.spawn(&spawn_ticket(&engine, "C", 3)).await.unwrap();
        engine.spawn(&spawn_ticket(&engine, "A", 1)).await.unwrap();
        engine.spawn(&spawn_ticket(&engine, "B", 2)).await.unwrap();

        let query = Query::Find(FindQuery {
            machine: "Ticket".into(),
            filter: None,
            sort: vec![SortClause {
                field: "priority".into(),
                direction: SortDirection::Asc,
            }],
            limit: None,
            offset: None,
        });
        let result = engine.execute_query(&query).await.unwrap();
        if let QueryResult::Instances(insts) = result {
            assert_eq!(insts.len(), 3);
            assert_eq!(insts[0].data.get("priority"), Some(&Value::Int(1)));
            assert_eq!(insts[1].data.get("priority"), Some(&Value::Int(2)));
            assert_eq!(insts[2].data.get("priority"), Some(&Value::Int(3)));
        } else {
            panic!("Expected Instances result");
        }
    }

    #[tokio::test]
    async fn query_find_with_limit() {
        let engine = setup_engine();
        register_ticket_machine(&engine);

        for i in 0..5 {
            engine
                .spawn(&spawn_ticket(&engine, &format!("T{}", i), i))
                .await
                .unwrap();
        }

        let query = Query::Find(FindQuery {
            machine: "Ticket".into(),
            filter: None,
            sort: Vec::new(),
            limit: Some(2),
            offset: None,
        });
        let result = engine.execute_query(&query).await.unwrap();
        if let QueryResult::Instances(insts) = result {
            assert_eq!(insts.len(), 2);
        } else {
            panic!("Expected Instances result");
        }
    }

    #[tokio::test]
    async fn query_trail() {
        let engine = setup_engine();
        register_ticket_machine(&engine);

        let cmd = spawn_ticket(&engine, "test", 1);
        let spawned = engine.spawn(&cmd).await.unwrap();
        let id = spawned.instance.id.as_str();

        engine
            .transition(&TransitionCommand::new(id.to_string(), "in_progress".into()))
            .await
            .unwrap();

        let query = Query::Trail(TrailQuery {
            machine: Some("Ticket".into()),
            instance_id: id.to_string(),
            filter: None,
        });
        let result = engine.execute_query(&query).await.unwrap();
        if let QueryResult::Trail(entries) = result {
            assert_eq!(entries.len(), 2); // spawn + transition
        } else {
            panic!("Expected Trail result");
        }
    }

    #[tokio::test]
    async fn query_aggregate_count() {
        let engine = setup_engine();
        register_ticket_machine(&engine);

        engine.spawn(&spawn_ticket(&engine, "A", 1)).await.unwrap();
        engine.spawn(&spawn_ticket(&engine, "B", 2)).await.unwrap();
        engine.spawn(&spawn_ticket(&engine, "C", 3)).await.unwrap();

        let query = Query::Aggregate(AggregateQuery {
            machine: "Ticket".into(),
            measures: vec![MeasureClause {
                function: AggregateFunction::Count,
                field: None,
                alias: Some("total".into()),
            }],
            filter: None,
            group_by: Vec::new(),
        });
        let result = engine.execute_query(&query).await.unwrap();
        if let QueryResult::Aggregate(rows) = result {
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].measures.get("total"), Some(&Value::Int(3)));
        } else {
            panic!("Expected Aggregate result");
        }
    }

    #[tokio::test]
    async fn query_aggregate_sum_avg() {
        let engine = setup_engine();
        register_ticket_machine(&engine);

        engine.spawn(&spawn_ticket(&engine, "A", 10)).await.unwrap();
        engine.spawn(&spawn_ticket(&engine, "B", 20)).await.unwrap();
        engine.spawn(&spawn_ticket(&engine, "C", 30)).await.unwrap();

        let query = Query::Aggregate(AggregateQuery {
            machine: "Ticket".into(),
            measures: vec![
                MeasureClause {
                    function: AggregateFunction::Sum,
                    field: Some("priority".into()),
                    alias: Some("total_priority".into()),
                },
                MeasureClause {
                    function: AggregateFunction::Avg,
                    field: Some("priority".into()),
                    alias: Some("avg_priority".into()),
                },
            ],
            filter: None,
            group_by: Vec::new(),
        });
        let result = engine.execute_query(&query).await.unwrap();
        if let QueryResult::Aggregate(rows) = result {
            assert_eq!(rows.len(), 1);
            assert_eq!(
                rows[0].measures.get("total_priority"),
                Some(&Value::Int(60))
            );
            assert_eq!(
                rows[0].measures.get("avg_priority"),
                Some(&Value::Float(20.0))
            );
        } else {
            panic!("Expected Aggregate result");
        }
    }

    #[tokio::test]
    async fn query_aggregate_group_by_state() {
        let engine = setup_engine();
        register_ticket_machine(&engine);

        let s1 = engine.spawn(&spawn_ticket(&engine, "A", 1)).await.unwrap();
        engine.spawn(&spawn_ticket(&engine, "B", 2)).await.unwrap();
        let s3 = engine.spawn(&spawn_ticket(&engine, "C", 3)).await.unwrap();

        // Transition some to different states
        engine
            .transition(&TransitionCommand::new(
                s1.instance.id.as_str(),
                "in_progress".into(),
            ))
            .await
            .unwrap();
        engine
            .transition(&TransitionCommand::new(
                s3.instance.id.as_str(),
                "in_progress".into(),
            ))
            .await
            .unwrap();

        let query = Query::Aggregate(AggregateQuery {
            machine: "Ticket".into(),
            measures: vec![MeasureClause {
                function: AggregateFunction::Count,
                field: None,
                alias: Some("count".into()),
            }],
            filter: None,
            group_by: vec![GroupByClause::State],
        });
        let result = engine.execute_query(&query).await.unwrap();
        if let QueryResult::Aggregate(rows) = result {
            assert_eq!(rows.len(), 2); // "open" and "in_progress" groups

            let open_count = rows
                .iter()
                .find(|r| r.group_key.get("state") == Some(&Value::Text("open".into())))
                .and_then(|r| r.measures.get("count"));
            let ip_count = rows
                .iter()
                .find(|r| {
                    r.group_key.get("state") == Some(&Value::Text("in_progress".into()))
                })
                .and_then(|r| r.measures.get("count"));

            assert_eq!(open_count, Some(&Value::Int(1)));
            assert_eq!(ip_count, Some(&Value::Int(2)));
        } else {
            panic!("Expected Aggregate result");
        }
    }

    #[tokio::test]
    async fn query_paths() {
        let engine = setup_engine();
        register_ticket_machine(&engine);

        // Create two tickets with the same path
        let s1 = engine.spawn(&spawn_ticket(&engine, "A", 1)).await.unwrap();
        let s2 = engine.spawn(&spawn_ticket(&engine, "B", 2)).await.unwrap();

        engine
            .transition(&TransitionCommand::new(
                s1.instance.id.as_str(),
                "in_progress".into(),
            ))
            .await
            .unwrap();
        engine
            .transition(&TransitionCommand::new(
                s2.instance.id.as_str(),
                "in_progress".into(),
            ))
            .await
            .unwrap();

        let query = Query::Paths(PathsQuery {
            machine: "Ticket".into(),
            filter: None,
            limit: None,
        });
        let result = engine.execute_query(&query).await.unwrap();
        if let QueryResult::Paths(paths) = result {
            assert!(!paths.is_empty());
            // Both took the same path: -> open -> in_progress
            assert!(paths.iter().any(|p| p.count == 2));
        } else {
            panic!("Expected Paths result");
        }
    }

    #[tokio::test]
    async fn query_funnel() {
        let engine = setup_engine();
        register_ticket_machine(&engine);

        // Spawn 3 tickets
        let s1 = engine.spawn(&spawn_ticket(&engine, "A", 1)).await.unwrap();
        let s2 = engine.spawn(&spawn_ticket(&engine, "B", 2)).await.unwrap();
        let _s3 = engine.spawn(&spawn_ticket(&engine, "C", 3)).await.unwrap();

        // Only 2 transition to in_progress
        engine
            .transition(&TransitionCommand::new(
                s1.instance.id.as_str(),
                "in_progress".into(),
            ))
            .await
            .unwrap();
        engine
            .transition(&TransitionCommand::new(
                s2.instance.id.as_str(),
                "in_progress".into(),
            ))
            .await
            .unwrap();

        // Only 1 transitions to resolved
        engine
            .transition(&TransitionCommand::new(
                s1.instance.id.as_str(),
                "resolved".into(),
            ))
            .await
            .unwrap();

        let query = Query::Funnel(FunnelQuery {
            machine: "Ticket".into(),
            states: vec![
                "open".to_string(),
                "in_progress".to_string(),
                "resolved".to_string(),
            ],
            filter: None,
        });
        let result = engine.execute_query(&query).await.unwrap();
        if let QueryResult::Funnel(funnel) = result {
            assert_eq!(funnel.stages.len(), 3);
            assert_eq!(funnel.stages[0].state, "open");
            assert_eq!(funnel.stages[0].count, 3); // All visited open
            assert_eq!(funnel.stages[1].state, "in_progress");
            assert_eq!(funnel.stages[1].count, 2);
            assert_eq!(funnel.stages[2].state, "resolved");
            assert_eq!(funnel.stages[2].count, 1);
        } else {
            panic!("Expected Funnel result");
        }
    }
}
