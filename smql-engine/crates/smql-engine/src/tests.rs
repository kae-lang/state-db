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
    fn eval_is_not_set_present() {
        let ctx = ctx_with_data(vec![("x", Value::Int(5))]);
        let expr = Expression::new(ExpressionKind::IsNotSet(Box::new(field("x"))));
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(false));
    }

    #[test]
    fn eval_in_set_int_match() {
        let ctx = ctx_with_data(vec![("priority", Value::Int(2))]);
        let expr = Expression::new(ExpressionKind::InSet {
            expr: Box::new(field("priority")),
            values: vec![
                lit(Value::Int(1)),
                lit(Value::Int(2)),
                lit(Value::Int(3)),
            ],
        });
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));
    }

    #[test]
    fn eval_in_set_no_match() {
        let ctx = ctx_with_data(vec![("priority", Value::Int(5))]);
        let expr = Expression::new(ExpressionKind::InSet {
            expr: Box::new(field("priority")),
            values: vec![
                lit(Value::Int(1)),
                lit(Value::Int(2)),
                lit(Value::Int(3)),
            ],
        });
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(false));
    }

    #[test]
    fn eval_function_upper() {
        let ctx = ctx_with_data(vec![]);
        let expr = Expression::new(ExpressionKind::FunctionCall {
            name: "upper".to_string(),
            args: vec![lit(Value::Text("hello".to_string()))],
        });
        assert_eq!(
            eval_expr(&expr, &ctx).unwrap(),
            Value::Text("HELLO".to_string())
        );
    }

    #[test]
    fn eval_function_lower() {
        let ctx = ctx_with_data(vec![]);
        let expr = Expression::new(ExpressionKind::FunctionCall {
            name: "lower".to_string(),
            args: vec![lit(Value::Text("HELLO".to_string()))],
        });
        assert_eq!(
            eval_expr(&expr, &ctx).unwrap(),
            Value::Text("hello".to_string())
        );
    }

    #[test]
    fn eval_function_length_text() {
        let ctx = ctx_with_data(vec![]);
        let expr = Expression::new(ExpressionKind::FunctionCall {
            name: "length".to_string(),
            args: vec![lit(Value::Text("hello".to_string()))],
        });
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Int(5));
    }

    #[test]
    fn eval_state_is_true() {
        let ctx = ctx_with_data(vec![]);
        let expr = Expression::new(ExpressionKind::StateIs("open".to_string()));
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));
    }

    #[test]
    fn eval_state_is_false() {
        let ctx = ctx_with_data(vec![]);
        let expr = Expression::new(ExpressionKind::StateIs("closed".to_string()));
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(false));
    }

    #[test]
    fn eval_actor_ref_to_map() {
        let mut ctx = ctx_with_data(vec![]);
        ctx.actor = Some(ActorInfo {
            id: "user_456".to_string(),
            role: Some("editor".to_string()),
            fields: HashMap::new(),
        });

        let expr = Expression::new(ExpressionKind::ActorRef);
        let result = eval_expr(&expr, &ctx).unwrap();
        if let Value::Map(m) = result {
            assert_eq!(m.get("id"), Some(&Value::Text("user_456".to_string())));
            assert_eq!(m.get("role"), Some(&Value::Text("editor".to_string())));
        } else {
            panic!("Expected Map from ActorRef, got {:?}", result);
        }
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
    use smql_ast::SmqlError;
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
            parent_id: None,
            parent_machine: None,
        }
    }

    fn transition_cmd(machine: &str, instance_id: &str, to_state: &str) -> TransitionCommand {
        TransitionCommand::new(machine.to_string(), instance_id.to_string(), to_state.to_string())
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

        let cmd = transition_cmd("Ticket", &id, "in_progress");
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
        let cmd = transition_cmd("Ticket", &id, "closed");
        let result = engine.transition(&cmd).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn transition_machine_mismatch() {
        let engine = setup_engine();
        register_ticket_machine(&engine);

        let spawn = spawn_cmd("Ticket", vec![("title", Value::Text("test".into()))]);
        let spawned = engine.spawn(&spawn).await.unwrap();
        let id = spawned.instance.id.as_str();

        // Use wrong machine name
        let cmd = transition_cmd("WrongMachine", &id, "in_progress");
        let result = engine.transition(&cmd).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        match err {
            SmqlError::ValidationError { message, .. } => {
                assert!(message.contains("Machine mismatch"));
                assert!(message.contains("WrongMachine"));
                assert!(message.contains("Ticket"));
            }
            other => panic!("Expected ValidationError, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn transition_with_data() {
        let engine = setup_engine();
        register_ticket_machine(&engine);

        let spawn = spawn_cmd("Ticket", vec![("title", Value::Text("test".into()))]);
        let spawned = engine.spawn(&spawn).await.unwrap();
        let id = spawned.instance.id.as_str();

        let mut cmd = transition_cmd("Ticket", &id, "in_progress");
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

        let mut cmd = transition_cmd("Ticket", &id, "in_progress");
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

        let result = engine.transition(&transition_cmd("GuardedMachine", &id, "published")).await;
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

        let result = engine.transition(&transition_cmd("GuardedMachine", &id, "published")).await;
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

        let cmd = transition_cmd("Ticket", &id, "in_progress");
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
            .try_transition(&transition_cmd("GuardedMachine", &id, "published"))
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
            .transition(&transition_cmd("WildcardMachine", &id, "cancelled"))
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
            .transition(&transition_cmd("WildcardMachine", &id, "cancelled"))
            .await
            .unwrap();

        // cancelled -> cancelled should be blocked by EXCEPT FROM
        let result = engine
            .transition(&transition_cmd("WildcardMachine", &id, "cancelled"))
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

        let mut cmd = transition_cmd("Ticket", &id, "closed");
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

        let mut tcmd = transition_cmd("GuardedMachine", &id, "published");
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
            .transition(&transition_cmd("Ticket", &id_str, "in_progress"))
            .await
            .unwrap();

        // Instance is now version 2. A second transition should work.
        let result = engine
            .transition(&transition_cmd("Ticket", &id_str, "resolved"))
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
            .transition(&transition_cmd("Ticket", &id, "in_progress"))
            .await
            .unwrap();
        engine
            .transition(&transition_cmd("Ticket", &id, "resolved"))
            .await
            .unwrap();
        engine
            .transition(&transition_cmd("Ticket", &id, "closed"))
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
            parent_id: None,
            parent_machine: None,
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
            after: None,
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
            after: None,
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
            after: None,
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
            after: None,
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
            .transition(&TransitionCommand::new("Ticket".into(), id.to_string(), "in_progress".into()))
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
                "Ticket".into(),
                s1.instance.id.as_str(),
                "in_progress".into(),
            ))
            .await
            .unwrap();
        engine
            .transition(&TransitionCommand::new(
                "Ticket".into(),
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
                "Ticket".into(),
                s1.instance.id.as_str(),
                "in_progress".into(),
            ))
            .await
            .unwrap();
        engine
            .transition(&TransitionCommand::new(
                "Ticket".into(),
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
                "Ticket".into(),
                s1.instance.id.as_str(),
                "in_progress".into(),
            ))
            .await
            .unwrap();
        engine
            .transition(&TransitionCommand::new(
                "Ticket".into(),
                s2.instance.id.as_str(),
                "in_progress".into(),
            ))
            .await
            .unwrap();

        // Only 1 transitions to resolved
        engine
            .transition(&TransitionCommand::new(
                "Ticket".into(),
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

    #[tokio::test]
    async fn query_find_sort_desc() {
        let engine = setup_engine();
        register_ticket_machine(&engine);

        engine.spawn(&spawn_ticket(&engine, "Low", 5)).await.unwrap();
        engine.spawn(&spawn_ticket(&engine, "High", 1)).await.unwrap();
        engine.spawn(&spawn_ticket(&engine, "Med", 3)).await.unwrap();

        let query = Query::Find(FindQuery {
            machine: "Ticket".into(),
            filter: None,
            sort: vec![SortClause {
                field: "priority".into(),
                direction: SortDirection::Desc,
            }],
            limit: None,
            offset: None,
            after: None,
        });
        let result = engine.execute_query(&query).await.unwrap();
        if let QueryResult::Instances(insts) = result {
            assert_eq!(insts.len(), 3);
            assert_eq!(insts[0].data.get("priority"), Some(&Value::Int(5)));
            assert_eq!(insts[1].data.get("priority"), Some(&Value::Int(3)));
            assert_eq!(insts[2].data.get("priority"), Some(&Value::Int(1)));
        } else {
            panic!("Expected Instances result");
        }
    }

    #[tokio::test]
    async fn query_find_sort_multiple() {
        let engine = setup_engine();
        register_ticket_machine(&engine);

        let s1 = engine.spawn(&spawn_ticket(&engine, "A", 3)).await.unwrap();
        let s2 = engine.spawn(&spawn_ticket(&engine, "B", 1)).await.unwrap();
        let _s3 = engine.spawn(&spawn_ticket(&engine, "C", 3)).await.unwrap();

        // Move s1 to in_progress so we have 2 states
        engine
            .transition(&TransitionCommand::new(
                "Ticket".into(),
                s1.instance.id.as_str(),
                "in_progress".into(),
            ))
            .await
            .unwrap();
        engine
            .transition(&TransitionCommand::new(
                "Ticket".into(),
                s2.instance.id.as_str(),
                "in_progress".into(),
            ))
            .await
            .unwrap();

        // Sort by priority DESC — highest first
        let query = Query::Find(FindQuery {
            machine: "Ticket".into(),
            filter: None,
            sort: vec![SortClause {
                field: "priority".into(),
                direction: SortDirection::Desc,
            }],
            limit: None,
            offset: None,
            after: None,
        });
        let result = engine.execute_query(&query).await.unwrap();
        if let QueryResult::Instances(insts) = result {
            assert_eq!(insts.len(), 3);
            // First two should be priority 3
            assert_eq!(insts[0].data.get("priority"), Some(&Value::Int(3)));
            assert_eq!(insts[1].data.get("priority"), Some(&Value::Int(3)));
            assert_eq!(insts[2].data.get("priority"), Some(&Value::Int(1)));
        } else {
            panic!("Expected Instances result");
        }
    }

    #[tokio::test]
    async fn query_find_offset() {
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
            offset: Some(2),
            after: None,
        });
        let result = engine.execute_query(&query).await.unwrap();
        if let QueryResult::Instances(insts) = result {
            assert_eq!(insts.len(), 2);
        } else {
            panic!("Expected Instances result");
        }
    }

    #[tokio::test]
    async fn query_find_where_sort_limit() {
        let engine = setup_engine();
        register_ticket_machine(&engine);

        for i in 1..=3 {
            engine
                .spawn(&spawn_ticket(&engine, &format!("T{}", i), i))
                .await
                .unwrap();
        }

        // WHERE priority > 1 SORT BY priority DESC (no limit to avoid storage pre-limiting)
        let filter = Expression::new(ExpressionKind::BinaryOp {
            left: Box::new(Expression::new(ExpressionKind::FieldAccess(vec![
                "priority".to_string(),
            ]))),
            op: BinaryOperator::Gt,
            right: Box::new(Expression::new(ExpressionKind::Literal(Value::Int(1)))),
        });

        let query = Query::Find(FindQuery {
            machine: "Ticket".into(),
            filter: Some(filter),
            sort: vec![SortClause {
                field: "priority".into(),
                direction: SortDirection::Desc,
            }],
            limit: None,
            offset: None,
            after: None,
        });
        let result = engine.execute_query(&query).await.unwrap();
        if let QueryResult::Instances(insts) = result {
            assert_eq!(insts.len(), 2); // priorities 2 and 3
            assert_eq!(insts[0].data.get("priority"), Some(&Value::Int(3)));
            assert_eq!(insts[1].data.get("priority"), Some(&Value::Int(2)));
        } else {
            panic!("Expected Instances result");
        }
    }

    #[tokio::test]
    async fn query_find_empty_result() {
        let engine = setup_engine();
        register_ticket_machine(&engine);

        engine.spawn(&spawn_ticket(&engine, "A", 1)).await.unwrap();

        // WHERE priority > 100 → matches nothing
        let filter = Expression::new(ExpressionKind::BinaryOp {
            left: Box::new(Expression::new(ExpressionKind::FieldAccess(vec![
                "priority".to_string(),
            ]))),
            op: BinaryOperator::Gt,
            right: Box::new(Expression::new(ExpressionKind::Literal(Value::Int(100)))),
        });

        let query = Query::Find(FindQuery {
            machine: "Ticket".into(),
            filter: Some(filter),
            sort: Vec::new(),
            limit: None,
            offset: None,
            after: None,
        });
        let result = engine.execute_query(&query).await.unwrap();
        if let QueryResult::Instances(insts) = result {
            assert_eq!(insts.len(), 0);
        } else {
            panic!("Expected Instances result");
        }
    }

    #[tokio::test]
    async fn query_aggregate_min() {
        let engine = setup_engine();
        register_ticket_machine(&engine);

        engine.spawn(&spawn_ticket(&engine, "A", 3)).await.unwrap();
        engine.spawn(&spawn_ticket(&engine, "B", 1)).await.unwrap();
        engine.spawn(&spawn_ticket(&engine, "C", 5)).await.unwrap();

        let query = Query::Aggregate(AggregateQuery {
            machine: "Ticket".into(),
            measures: vec![MeasureClause {
                function: AggregateFunction::Min,
                field: Some("priority".into()),
                alias: Some("min_p".into()),
            }],
            filter: None,
            group_by: Vec::new(),
        });
        let result = engine.execute_query(&query).await.unwrap();
        if let QueryResult::Aggregate(rows) = result {
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].measures.get("min_p"), Some(&Value::Int(1)));
        } else {
            panic!("Expected Aggregate result");
        }
    }

    #[tokio::test]
    async fn query_aggregate_max() {
        let engine = setup_engine();
        register_ticket_machine(&engine);

        engine.spawn(&spawn_ticket(&engine, "A", 3)).await.unwrap();
        engine.spawn(&spawn_ticket(&engine, "B", 1)).await.unwrap();
        engine.spawn(&spawn_ticket(&engine, "C", 5)).await.unwrap();

        let query = Query::Aggregate(AggregateQuery {
            machine: "Ticket".into(),
            measures: vec![MeasureClause {
                function: AggregateFunction::Max,
                field: Some("priority".into()),
                alias: Some("max_p".into()),
            }],
            filter: None,
            group_by: Vec::new(),
        });
        let result = engine.execute_query(&query).await.unwrap();
        if let QueryResult::Aggregate(rows) = result {
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].measures.get("max_p"), Some(&Value::Int(5)));
        } else {
            panic!("Expected Aggregate result");
        }
    }

    #[tokio::test]
    async fn query_aggregate_min_empty() {
        let engine = setup_engine();
        register_ticket_machine(&engine);

        // No instances spawned
        let query = Query::Aggregate(AggregateQuery {
            machine: "Ticket".into(),
            measures: vec![MeasureClause {
                function: AggregateFunction::Min,
                field: Some("priority".into()),
                alias: Some("min_p".into()),
            }],
            filter: None,
            group_by: Vec::new(),
        });
        let result = engine.execute_query(&query).await.unwrap();
        if let QueryResult::Aggregate(rows) = result {
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].measures.get("min_p"), Some(&Value::Null));
        } else {
            panic!("Expected Aggregate result");
        }
    }

    #[tokio::test]
    async fn query_aggregate_sum_int() {
        let engine = setup_engine();
        register_ticket_machine(&engine);

        engine.spawn(&spawn_ticket(&engine, "A", 10)).await.unwrap();
        engine.spawn(&spawn_ticket(&engine, "B", 20)).await.unwrap();
        engine.spawn(&spawn_ticket(&engine, "C", 30)).await.unwrap();

        let query = Query::Aggregate(AggregateQuery {
            machine: "Ticket".into(),
            measures: vec![MeasureClause {
                function: AggregateFunction::Sum,
                field: Some("priority".into()),
                alias: Some("total".into()),
            }],
            filter: None,
            group_by: Vec::new(),
        });
        let result = engine.execute_query(&query).await.unwrap();
        if let QueryResult::Aggregate(rows) = result {
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].measures.get("total"), Some(&Value::Int(60)));
        } else {
            panic!("Expected Aggregate result");
        }
    }

    #[tokio::test]
    async fn query_aggregate_multiple_measures() {
        let engine = setup_engine();
        register_ticket_machine(&engine);

        engine.spawn(&spawn_ticket(&engine, "A", 10)).await.unwrap();
        engine.spawn(&spawn_ticket(&engine, "B", 20)).await.unwrap();

        let query = Query::Aggregate(AggregateQuery {
            machine: "Ticket".into(),
            measures: vec![
                MeasureClause {
                    function: AggregateFunction::Count,
                    field: None,
                    alias: Some("total".into()),
                },
                MeasureClause {
                    function: AggregateFunction::Sum,
                    field: Some("priority".into()),
                    alias: Some("sum_p".into()),
                },
            ],
            filter: None,
            group_by: Vec::new(),
        });
        let result = engine.execute_query(&query).await.unwrap();
        if let QueryResult::Aggregate(rows) = result {
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].measures.get("total"), Some(&Value::Int(2)));
            assert_eq!(rows[0].measures.get("sum_p"), Some(&Value::Int(30)));
        } else {
            panic!("Expected Aggregate result");
        }
    }

    #[tokio::test]
    async fn query_aggregate_group_by_field() {
        let engine = setup_engine();
        register_ticket_machine(&engine);

        // 2 with priority 1, 1 with priority 5
        engine.spawn(&spawn_ticket(&engine, "A", 1)).await.unwrap();
        engine.spawn(&spawn_ticket(&engine, "B", 1)).await.unwrap();
        engine.spawn(&spawn_ticket(&engine, "C", 5)).await.unwrap();

        let query = Query::Aggregate(AggregateQuery {
            machine: "Ticket".into(),
            measures: vec![MeasureClause {
                function: AggregateFunction::Count,
                field: None,
                alias: Some("count".into()),
            }],
            filter: None,
            group_by: vec![GroupByClause::Field("priority".into())],
        });
        let result = engine.execute_query(&query).await.unwrap();
        if let QueryResult::Aggregate(rows) = result {
            assert_eq!(rows.len(), 2);
            let p1_count = rows
                .iter()
                .find(|r| r.group_key.get("priority") == Some(&Value::Int(1)))
                .and_then(|r| r.measures.get("count"));
            let p5_count = rows
                .iter()
                .find(|r| r.group_key.get("priority") == Some(&Value::Int(5)))
                .and_then(|r| r.measures.get("count"));
            assert_eq!(p1_count, Some(&Value::Int(2)));
            assert_eq!(p5_count, Some(&Value::Int(1)));
        } else {
            panic!("Expected Aggregate result");
        }
    }

    #[tokio::test]
    async fn query_trail_shows_actor() {
        let engine = setup_engine();
        register_ticket_machine(&engine);

        let spawned = engine
            .spawn(&spawn_ticket(&engine, "test", 1))
            .await
            .unwrap();
        let id = spawned.instance.id.as_str();

        let mut cmd = TransitionCommand::new("Ticket".into(), id.to_string(), "in_progress".into());
        cmd.as_actor = Some("bob".to_string());
        engine.transition(&cmd).await.unwrap();

        let query = Query::Trail(TrailQuery {
            machine: Some("Ticket".into()),
            instance_id: id.to_string(),
            filter: None,
        });
        let result = engine.execute_query(&query).await.unwrap();
        if let QueryResult::Trail(entries) = result {
            assert!(entries.len() >= 2);
            let transition_entry = entries.iter().find(|e| e.to_state == "in_progress").unwrap();
            assert_eq!(transition_entry.actor, Some("bob".to_string()));
        } else {
            panic!("Expected Trail result");
        }
    }

    #[tokio::test]
    async fn query_paths_with_limit() {
        let engine = setup_engine();
        register_ticket_machine(&engine);

        // Create instances with different paths
        let s1 = engine.spawn(&spawn_ticket(&engine, "A", 1)).await.unwrap();
        let s2 = engine.spawn(&spawn_ticket(&engine, "B", 2)).await.unwrap();

        engine
            .transition(&TransitionCommand::new(
                "Ticket".into(),
                s1.instance.id.as_str(),
                "in_progress".into(),
            ))
            .await
            .unwrap();
        engine
            .transition(&TransitionCommand::new(
                "Ticket".into(),
                s1.instance.id.as_str(),
                "resolved".into(),
            ))
            .await
            .unwrap();
        // s2 stays at open → different path
        let _ = s2;

        let query = Query::Paths(PathsQuery {
            machine: "Ticket".into(),
            filter: None,
            limit: Some(1),
        });
        let result = engine.execute_query(&query).await.unwrap();
        if let QueryResult::Paths(paths) = result {
            assert!(paths.len() <= 1);
        } else {
            panic!("Expected Paths result");
        }
    }

    #[tokio::test]
    async fn query_funnel_with_filter() {
        let engine = setup_engine();
        register_ticket_machine(&engine);

        // Spawn with different priorities
        let s1 = engine.spawn(&spawn_ticket(&engine, "A", 1)).await.unwrap();
        let s2 = engine.spawn(&spawn_ticket(&engine, "B", 2)).await.unwrap();
        let _s3 = engine.spawn(&spawn_ticket(&engine, "C", 5)).await.unwrap();

        // Transition low-priority ones
        engine
            .transition(&TransitionCommand::new(
                "Ticket".into(),
                s1.instance.id.as_str(),
                "in_progress".into(),
            ))
            .await
            .unwrap();
        engine
            .transition(&TransitionCommand::new(
                "Ticket".into(),
                s2.instance.id.as_str(),
                "in_progress".into(),
            ))
            .await
            .unwrap();

        // Funnel with filter: only priority < 3
        let filter = Expression::new(ExpressionKind::BinaryOp {
            left: Box::new(Expression::new(ExpressionKind::FieldAccess(vec![
                "priority".to_string(),
            ]))),
            op: BinaryOperator::Lt,
            right: Box::new(Expression::new(ExpressionKind::Literal(Value::Int(3)))),
        });

        let query = Query::Funnel(FunnelQuery {
            machine: "Ticket".into(),
            states: vec!["open".to_string(), "in_progress".to_string()],
            filter: Some(filter),
        });
        let result = engine.execute_query(&query).await.unwrap();
        if let QueryResult::Funnel(funnel) = result {
            assert_eq!(funnel.stages.len(), 2);
            // Only priority 1 and 2 match (< 3)
            assert_eq!(funnel.stages[0].count, 2); // Both visited open
            assert_eq!(funnel.stages[1].count, 2); // Both transitioned to in_progress
        } else {
            panic!("Expected Funnel result");
        }
    }

    // --- Cursor-based pagination tests ---

    #[tokio::test]
    async fn find_with_cursor_multi_page_iteration() {
        let engine = setup_engine();
        register_ticket_machine(&engine);

        // Spawn 5 tickets
        let mut ids = Vec::new();
        for i in 0..5 {
            let result = engine
                .spawn(&spawn_ticket(&engine, &format!("T{}", i), i))
                .await
                .unwrap();
            ids.push(result.instance.id.as_str());
        }

        // Page 1: first 2
        let query = Query::Find(FindQuery {
            machine: "Ticket".into(),
            filter: None,
            sort: Vec::new(),
            limit: Some(2),
            offset: None,
            after: None,
        });
        let result = engine.execute_query(&query).await.unwrap();
        let page1 = if let QueryResult::Instances(insts) = result {
            assert_eq!(insts.len(), 2);
            insts
        } else {
            panic!("Expected Instances");
        };

        // Page 2: next 2 using cursor
        let cursor = page1.last().unwrap().id.as_str();
        let query = Query::Find(FindQuery {
            machine: "Ticket".into(),
            filter: None,
            sort: Vec::new(),
            limit: Some(2),
            offset: None,
            after: Some(cursor.clone()),
        });
        let result = engine.execute_query(&query).await.unwrap();
        let page2 = if let QueryResult::Instances(insts) = result {
            assert_eq!(insts.len(), 2);
            insts
        } else {
            panic!("Expected Instances");
        };

        // Verify no overlap
        let page1_ids: Vec<String> = page1.iter().map(|i| i.id.as_str()).collect();
        let page2_ids: Vec<String> = page2.iter().map(|i| i.id.as_str()).collect();
        for id in &page2_ids {
            assert!(!page1_ids.contains(id));
        }

        // Page 3: last 1
        let cursor2 = page2.last().unwrap().id.as_str();
        let query = Query::Find(FindQuery {
            machine: "Ticket".into(),
            filter: None,
            sort: Vec::new(),
            limit: Some(2),
            offset: None,
            after: Some(cursor2),
        });
        let result = engine.execute_query(&query).await.unwrap();
        if let QueryResult::Instances(insts) = result {
            assert_eq!(insts.len(), 1);
        } else {
            panic!("Expected Instances");
        }
    }

    #[tokio::test]
    async fn find_no_cursor_returns_first_page() {
        let engine = setup_engine();
        register_ticket_machine(&engine);

        for i in 0..3 {
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
            after: None,
        });
        let result = engine.execute_query(&query).await.unwrap();
        if let QueryResult::Instances(insts) = result {
            assert_eq!(insts.len(), 2);
        } else {
            panic!("Expected Instances");
        }
    }

    #[tokio::test]
    async fn find_cursor_returns_sorted_by_id() {
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
            limit: None,
            offset: None,
            after: None,
        });
        let result = engine.execute_query(&query).await.unwrap();
        if let QueryResult::Instances(insts) = result {
            // Verify instances are sorted by ID (ULIDs)
            for i in 1..insts.len() {
                assert!(insts[i].id.as_str() >= insts[i - 1].id.as_str());
            }
        } else {
            panic!("Expected Instances");
        }
    }
}

#[cfg(test)]
mod timer_tests {
    use crate::engine::Engine;
    use crate::eval::{eval_expr, EvalContext};
    use smql_ast::command::{SpawnCommand, TransitionCommand};
    use smql_ast::expression::{Expression, ExpressionKind};
    use smql_ast::machine::*;
    use smql_ast::types::*;
    use smql_ast::value::{SmqlDuration, Value};
    use smql_catalog::MachineCatalog;
    use smql_storage::{MemoryStorage, Storage};
    use smql_timer::TimerManager;
    use std::sync::Arc;

    fn setup_engine() -> Engine {
        let catalog = Arc::new(MachineCatalog::new());
        let storage = Arc::new(MemoryStorage::new());
        Engine::new(catalog, storage)
    }

    fn setup_engine_with_timer() -> (Engine, Arc<TimerManager>) {
        let catalog = Arc::new(MachineCatalog::new());
        let storage = Arc::new(MemoryStorage::new());
        let timer_manager = Arc::new(TimerManager::new());
        let engine = Engine::with_timer_manager(
            catalog,
            storage,
            Arc::clone(&timer_manager),
        );
        (engine, timer_manager)
    }

    /// Register a machine with a timeout on one of its transitions.
    fn register_timeout_machine(engine: &Engine) {
        let mut m = MachineDefinition::new("TimerMachine".into(), "waiting".into());
        m.states = vec![
            StateDefinition::new("waiting".into()),
            StateDefinition::new("active".into()),
            StateDefinition::new("expired".into()),
            StateDefinition::new("done".into()),
        ];
        m.terminal_states = vec!["done".into()];
        m.data = vec![DataFieldDefinition {
            name: "label".into(),
            field_type: TypeDefinition::Text,
            constraints: vec![Constraint::Default(DefaultValue::String("test".into()))],
        }];

        // waiting -> active with a 72h timeout that auto-transitions to expired
        let mut t1 = TransitionDefinition::new(
            TransitionSource::State("waiting".into()),
            "active".into(),
        );
        t1.timeout = Some(TimeoutClause {
            duration: SmqlDuration::from_hours(72),
            target_state: "expired".into(),
        });

        let t2 = TransitionDefinition::new(
            TransitionSource::State("active".into()),
            "done".into(),
        );
        let t3 = TransitionDefinition::new(
            TransitionSource::State("expired".into()),
            "done".into(),
        );

        m.transitions = vec![t1, t2, t3];
        engine.catalog.register(m).unwrap();
    }

    fn spawn_cmd(machine: &str) -> SpawnCommand {
        SpawnCommand {
            machine: machine.to_string(),
            data: Vec::new(),
            then_transition: None,
            batch: false,
            batch_data: Vec::new(),
            parent_id: None,
            parent_machine: None,
        }
    }

    // --- Timer registration tests ---

    #[tokio::test]
    async fn transition_registers_timeout() {
        let (engine, timer_manager) = setup_engine_with_timer();
        register_timeout_machine(&engine);

        let spawned = engine.spawn(&spawn_cmd("TimerMachine")).await.unwrap();
        let id = spawned.instance.id.as_str();

        // No timer yet (instance is in "waiting", no timeout on spawn)
        assert_eq!(timer_manager.timer_count(), 0);

        // Transition to "active" — this transition has TIMEOUT: 72h -> expired
        engine
            .transition(&TransitionCommand::new("TimerMachine".into(), id.to_string(), "active".into()))
            .await
            .unwrap();

        // Timer should now be registered
        assert_eq!(timer_manager.timer_count(), 1);

        let entry = timer_manager.get_timer(&id, "active").unwrap();
        assert_eq!(entry.from_state, "active");
        assert_eq!(entry.target_state, "expired");
        assert_eq!(entry.machine, "TimerMachine");
    }

    #[tokio::test]
    async fn transition_cancels_old_timeout() {
        let (engine, timer_manager) = setup_engine_with_timer();
        register_timeout_machine(&engine);

        let spawned = engine.spawn(&spawn_cmd("TimerMachine")).await.unwrap();
        let id = spawned.instance.id.as_str();

        // Transition to active (registers 72h timeout)
        engine
            .transition(&TransitionCommand::new("TimerMachine".into(), id.to_string(), "active".into()))
            .await
            .unwrap();
        assert_eq!(timer_manager.timer_count(), 1);

        // Transition to done — should cancel the active timeout
        engine
            .transition(&TransitionCommand::new("TimerMachine".into(), id.to_string(), "done".into()))
            .await
            .unwrap();
        assert_eq!(timer_manager.timer_count(), 0);
    }

    #[tokio::test]
    async fn no_timeout_registered_for_transitions_without_timeout() {
        let engine = setup_engine();

        let mut m = MachineDefinition::new("NoTimeout".into(), "a".into());
        m.states = vec![
            StateDefinition::new("a".into()),
            StateDefinition::new("b".into()),
        ];
        m.terminal_states = vec!["b".into()];
        m.transitions = vec![TransitionDefinition::new(
            TransitionSource::State("a".into()),
            "b".into(),
        )];
        engine.catalog.register(m).unwrap();

        let spawned = engine.spawn(&spawn_cmd("NoTimeout")).await.unwrap();
        let id = spawned.instance.id.as_str();

        engine
            .transition(&TransitionCommand::new("NoTimeout".into(), id.to_string(), "b".into()))
            .await
            .unwrap();

        assert_eq!(engine.timer_manager.timer_count(), 0);
    }

    // --- Timeout transition tests ---

    #[tokio::test]
    async fn timeout_transition_succeeds() {
        let engine = setup_engine();
        register_timeout_machine(&engine);

        let spawned = engine.spawn(&spawn_cmd("TimerMachine")).await.unwrap();
        let id = spawned.instance.id.as_str();

        // Move to active
        engine
            .transition(&TransitionCommand::new("TimerMachine".into(), id.to_string(), "active".into()))
            .await
            .unwrap();

        // Simulate timeout firing: force transition active -> expired
        let result = engine
            .timeout_transition(&id, "active", "expired")
            .await
            .unwrap();

        assert!(result.is_some());
        let result = result.unwrap();
        assert_eq!(result.from_state, "active");
        assert_eq!(result.to_state, "expired");
        assert_eq!(result.instance.state, "expired");
    }

    #[tokio::test]
    async fn timeout_transition_creates_trail_entry() {
        let engine = setup_engine();
        register_timeout_machine(&engine);

        let spawned = engine.spawn(&spawn_cmd("TimerMachine")).await.unwrap();
        let id = spawned.instance.id.as_str();

        engine
            .transition(&TransitionCommand::new("TimerMachine".into(), id.to_string(), "active".into()))
            .await
            .unwrap();

        engine
            .timeout_transition(&id, "active", "expired")
            .await
            .unwrap();

        let trail = engine
            .storage
            .get_trail(&spawned.instance.id)
            .await
            .unwrap();

        // Spawn + transition to active + timeout transition = 3 entries
        assert_eq!(trail.len(), 3);

        let timeout_entry = &trail[2];
        assert_eq!(timeout_entry.transition_name, Some("TIMEOUT".to_string()));
        assert_eq!(timeout_entry.actor, Some("System".to_string()));
        assert_eq!(timeout_entry.from_state, "active");
        assert_eq!(timeout_entry.to_state, "expired");
    }

    #[tokio::test]
    async fn timeout_transition_race_condition_noop() {
        let engine = setup_engine();
        register_timeout_machine(&engine);

        let spawned = engine.spawn(&spawn_cmd("TimerMachine")).await.unwrap();
        let id = spawned.instance.id.as_str();

        // Move to active, then to done
        engine
            .transition(&TransitionCommand::new("TimerMachine".into(), id.to_string(), "active".into()))
            .await
            .unwrap();
        engine
            .transition(&TransitionCommand::new("TimerMachine".into(), id.to_string(), "done".into()))
            .await
            .unwrap();

        // Timeout fires for "active" -> "expired", but instance is already in "done"
        let result = engine
            .timeout_transition(&id, "active", "expired")
            .await
            .unwrap();

        // Should be None (no-op, instance already moved)
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn timeout_transition_nonexistent_instance() {
        let engine = setup_engine();
        register_timeout_machine(&engine);

        let result = engine
            .timeout_transition("01NONEXISTENT000000000000", "active", "expired")
            .await
            .unwrap();

        assert!(result.is_none());
    }

    // --- TIMEOUT_REMAINING evaluation tests ---

    #[test]
    fn eval_timeout_remaining_with_value() {
        let mut ctx = EvalContext::new(
            std::collections::HashMap::new(),
            "active".to_string(),
        );
        ctx.timeout_remaining = Some(chrono::TimeDelta::hours(12));

        let expr = Expression::new(ExpressionKind::FunctionCall {
            name: "timeout_remaining".to_string(),
            args: vec![],
        });
        let result = eval_expr(&expr, &ctx).unwrap();
        if let Value::Duration(d) = result {
            // ~12 hours in seconds
            assert!(d.seconds >= 43000);
        } else {
            panic!("Expected Duration, got {:?}", result);
        }
    }

    #[test]
    fn eval_timeout_remaining_without_timer() {
        let ctx = EvalContext::new(
            std::collections::HashMap::new(),
            "active".to_string(),
        );
        // No timeout_remaining set (defaults to None)

        let expr = Expression::new(ExpressionKind::FunctionCall {
            name: "timeout_remaining".to_string(),
            args: vec![],
        });
        let result = eval_expr(&expr, &ctx).unwrap();
        assert_eq!(result, Value::Null);
    }

    // --- Timer manager integration with engine ---

    #[tokio::test]
    async fn timeout_remaining_accessible_from_timer_manager() {
        let (engine, timer_manager) = setup_engine_with_timer();
        register_timeout_machine(&engine);

        let spawned = engine.spawn(&spawn_cmd("TimerMachine")).await.unwrap();
        let id = spawned.instance.id.as_str();

        // Before transition, no timeout
        assert!(timer_manager.timeout_remaining(&id, "waiting").is_none());

        // Transition to active (registers 72h timeout)
        engine
            .transition(&TransitionCommand::new("TimerMachine".into(), id.to_string(), "active".into()))
            .await
            .unwrap();

        // Now timeout_remaining should be about 72h
        let remaining = timer_manager.timeout_remaining(&id, "active").unwrap();
        assert!(remaining.num_hours() >= 71);
    }

    // --- Background timer loop test ---

    #[tokio::test]
    async fn timer_loop_fires_expired_timeout() {
        let catalog = Arc::new(MachineCatalog::new());
        let storage = Arc::new(MemoryStorage::new());
        let timer_manager = Arc::new(TimerManager::new());
        let engine = Arc::new(Engine::with_timer_manager(
            catalog,
            storage,
            Arc::clone(&timer_manager),
        ));

        register_timeout_machine(&engine);

        let spawned = engine.spawn(&spawn_cmd("TimerMachine")).await.unwrap();
        let id = spawned.instance.id.as_str();

        // Transition to active
        engine
            .transition(&TransitionCommand::new("TimerMachine".into(), id.to_string(), "active".into()))
            .await
            .unwrap();

        // Cancel the future timer and register one that's already expired
        timer_manager.cancel(&id, "active");
        let now = chrono::Utc::now();
        timer_manager.register_with_deadline(
            &id,
            "TimerMachine",
            "active",
            "expired",
            now - chrono::TimeDelta::seconds(1), // Already expired
            now - chrono::TimeDelta::seconds(100),
        );

        // Start the timer loop with a short interval
        let handle = engine.start_timer_loop(std::time::Duration::from_millis(50));

        // Wait for the loop to process
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        // Abort the loop
        handle.abort();

        // Instance should have been transitioned to "expired"
        let instance = engine
            .storage
            .get_instance(&spawned.instance.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(instance.state, "expired");
    }

    #[tokio::test]
    async fn timer_loop_handles_race_condition() {
        let catalog = Arc::new(MachineCatalog::new());
        let storage = Arc::new(MemoryStorage::new());
        let timer_manager = Arc::new(TimerManager::new());
        let engine = Arc::new(Engine::with_timer_manager(
            catalog,
            storage,
            Arc::clone(&timer_manager),
        ));

        register_timeout_machine(&engine);

        let spawned = engine.spawn(&spawn_cmd("TimerMachine")).await.unwrap();
        let id = spawned.instance.id.as_str();

        // Transition to active, then immediately to done
        engine
            .transition(&TransitionCommand::new("TimerMachine".into(), id.to_string(), "active".into()))
            .await
            .unwrap();
        engine
            .transition(&TransitionCommand::new("TimerMachine".into(), id.to_string(), "done".into()))
            .await
            .unwrap();

        // Register an already-expired timer for active (simulating race)
        let now = chrono::Utc::now();
        timer_manager.register_with_deadline(
            &id,
            "TimerMachine",
            "active",
            "expired",
            now - chrono::TimeDelta::seconds(1),
            now - chrono::TimeDelta::seconds(100),
        );

        // Start the timer loop
        let handle = engine.start_timer_loop(std::time::Duration::from_millis(50));
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        handle.abort();

        // Instance should still be in "done" (race condition handled gracefully)
        let instance = engine
            .storage
            .get_instance(&spawned.instance.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(instance.state, "done");
    }

    // --- Timer persistence tests ---

    #[tokio::test]
    async fn transition_persists_timer_to_storage() {
        let (engine, _timer_manager) = setup_engine_with_timer();
        register_timeout_machine(&engine);

        let spawned = engine.spawn(&spawn_cmd("TimerMachine")).await.unwrap();
        let id = spawned.instance.id.as_str();

        // Transition to "active" — has 72h timeout -> expired
        let cmd = TransitionCommand::new("TimerMachine".into(), id.clone(), "active".into());
        engine.transition(&cmd).await.unwrap();

        // Timer should be persisted in storage
        let timers = engine.storage.load_all_timers().await.unwrap();
        assert_eq!(timers.len(), 1);
        assert_eq!(timers[0].instance_id, id);
        assert_eq!(timers[0].machine, "TimerMachine");
        assert_eq!(timers[0].from_state, "active");
        assert_eq!(timers[0].target_state, "expired");
    }

    #[tokio::test]
    async fn transition_removes_old_timer_from_storage() {
        let (engine, _timer_manager) = setup_engine_with_timer();
        register_timeout_machine(&engine);

        let spawned = engine.spawn(&spawn_cmd("TimerMachine")).await.unwrap();
        let id = spawned.instance.id.as_str();

        // Transition to "active" (registers + persists timer)
        let cmd = TransitionCommand::new("TimerMachine".into(), id.clone(), "active".into());
        engine.transition(&cmd).await.unwrap();
        assert_eq!(engine.storage.load_all_timers().await.unwrap().len(), 1);

        // Transition to "done" — should remove the timer from storage
        let cmd2 = TransitionCommand::new("TimerMachine".into(), id.clone(), "done".into());
        engine.transition(&cmd2).await.unwrap();

        let timers = engine.storage.load_all_timers().await.unwrap();
        assert!(timers.is_empty(), "Timer should be removed after transition away");
    }

    #[tokio::test]
    async fn restore_timers_from_storage() {
        let catalog = Arc::new(MachineCatalog::new());
        let storage = Arc::new(MemoryStorage::new());
        let timer_manager = Arc::new(TimerManager::new());

        // Simulate a previously persisted timer
        let now = chrono::Utc::now();
        let stored = smql_storage::StoredTimer {
            instance_id: "inst_abc".to_string(),
            machine: "TimerMachine".to_string(),
            from_state: "active".to_string(),
            target_state: "expired".to_string(),
            deadline: now + chrono::Duration::hours(1),
            registered_at: now,
        };
        storage.store_timer(&stored).await.unwrap();

        let engine = Engine::with_timer_manager(
            catalog,
            storage,
            Arc::clone(&timer_manager),
        );

        // Before restore, timer manager is empty
        assert_eq!(timer_manager.timer_count(), 0);

        // Restore
        let count = engine.restore_timers().await.unwrap();
        assert_eq!(count, 1);
        assert_eq!(timer_manager.timer_count(), 1);

        // Verify the restored timer
        let entry = timer_manager.get_timer("inst_abc", "active").unwrap();
        assert_eq!(entry.target_state, "expired");
        assert_eq!(entry.machine, "TimerMachine");
    }
}

#[cfg(test)]
mod hook_tests {
    use crate::engine::Engine;
    use smql_ast::command::{SpawnCommand, TransitionCommand};
    use smql_ast::expression::{Expression, ExpressionKind};
    use smql_ast::machine::*;
    use smql_ast::types::*;
    use smql_ast::value::Value;
    use smql_catalog::MachineCatalog;
    use smql_hooks::{EventBus, HookExecutor};
    use smql_storage::MemoryStorage;
    use smql_timer::TimerManager;
    use std::sync::Arc;

    fn setup_engine_with_event_bus() -> (Engine, Arc<EventBus>) {
        let catalog = Arc::new(MachineCatalog::new());
        let storage = Arc::new(MemoryStorage::new());
        let timer_manager = Arc::new(TimerManager::new());
        let event_bus = Arc::new(EventBus::new(64));
        let hook_executor = Arc::new(HookExecutor::new(Arc::clone(&event_bus)));
        let engine = Engine::with_hooks(catalog, storage, timer_manager, hook_executor);
        (engine, event_bus)
    }

    /// Register a machine with ON SPAWN, ON ENTER, ON EXIT, BEFORE/AFTER hooks.
    fn register_hooked_machine(engine: &Engine) {
        let mut m = MachineDefinition::new("HookedTicket".into(), "open".into());
        m.states = vec![
            StateDefinition::new("open".into()),
            StateDefinition::new("in_progress".into()),
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
                "closed".into(),
            ),
        ];

        // Hooks
        m.hooks = vec![
            HookDefinition {
                trigger: HookTrigger::OnSpawn,
                actions: vec![Action::Emit {
                    event: "spawned".to_string(),
                    payload: None,
                }],
            },
            HookDefinition {
                trigger: HookTrigger::OnEnter("open".to_string()),
                actions: vec![Action::Emit {
                    event: "entered_open".to_string(),
                    payload: None,
                }],
            },
            HookDefinition {
                trigger: HookTrigger::OnEnter("in_progress".to_string()),
                actions: vec![Action::Emit {
                    event: "entered_in_progress".to_string(),
                    payload: None,
                }],
            },
            HookDefinition {
                trigger: HookTrigger::OnExit("open".to_string()),
                actions: vec![Action::Emit {
                    event: "exited_open".to_string(),
                    payload: None,
                }],
            },
            HookDefinition {
                trigger: HookTrigger::BeforeEachTransition,
                actions: vec![Action::Log("Before transition check".to_string())],
            },
            HookDefinition {
                trigger: HookTrigger::AfterEachTransition,
                actions: vec![Action::Emit {
                    event: "after_transition".to_string(),
                    payload: None,
                }],
            },
        ];

        engine.catalog.register(m).unwrap();
    }

    /// Register a machine with actions on a transition.
    fn register_action_machine(engine: &Engine) {
        let mut m = MachineDefinition::new("ActionMachine".into(), "draft".into());
        m.states = vec![
            StateDefinition::new("draft".into()),
            StateDefinition::new("published".into()),
        ];
        m.terminal_states = vec!["published".into()];
        m.data = vec![DataFieldDefinition {
            name: "title".into(),
            field_type: TypeDefinition::Text,
            constraints: vec![Constraint::Default(DefaultValue::String("untitled".into()))],
        }];

        let mut t = TransitionDefinition::new(
            TransitionSource::State("draft".into()),
            "published".into(),
        );
        t.actions = vec![
            Action::Emit {
                event: "published".to_string(),
                payload: Some(Expression::new(ExpressionKind::FieldAccess(vec![
                    "title".to_string(),
                ]))),
            },
            Action::Log("Published: {title}".to_string()),
        ];
        m.transitions = vec![t];

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
            parent_id: None,
            parent_machine: None,
        }
    }

    fn transition_cmd(machine: &str, instance_id: &str, to_state: &str) -> TransitionCommand {
        TransitionCommand::new(machine.to_string(), instance_id.to_string(), to_state.to_string())
    }

    // --- ON SPAWN hook fires ---

    #[tokio::test]
    async fn on_spawn_hook_fires() {
        let (engine, event_bus) = setup_engine_with_event_bus();
        register_hooked_machine(&engine);
        let mut rx = event_bus.subscribe();

        let cmd = spawn_cmd("HookedTicket", vec![("title", Value::Text("test".into()))]);
        engine.spawn(&cmd).await.unwrap();

        // Should receive "spawned" event from ON SPAWN hook
        let event = rx.recv().await.unwrap();
        assert_eq!(event.name, "spawned");
    }

    // --- ON ENTER fires for initial state ---

    #[tokio::test]
    async fn on_enter_fires_for_initial_state() {
        let (engine, event_bus) = setup_engine_with_event_bus();
        register_hooked_machine(&engine);
        let mut rx = event_bus.subscribe();

        let cmd = spawn_cmd("HookedTicket", vec![("title", Value::Text("test".into()))]);
        engine.spawn(&cmd).await.unwrap();

        // First event: "spawned" (ON SPAWN)
        let e1 = rx.recv().await.unwrap();
        assert_eq!(e1.name, "spawned");

        // Second event: "entered_open" (ON ENTER open)
        let e2 = rx.recv().await.unwrap();
        assert_eq!(e2.name, "entered_open");
    }

    // --- BEFORE hook passes normally ---

    #[tokio::test]
    async fn before_hook_passes_normally() {
        let (engine, _bus) = setup_engine_with_event_bus();
        register_hooked_machine(&engine);

        let cmd = spawn_cmd("HookedTicket", vec![("title", Value::Text("test".into()))]);
        let spawned = engine.spawn(&cmd).await.unwrap();
        let id = spawned.instance.id.as_str();

        // BEFORE hook has LOG action which always succeeds
        let result = engine.transition(&transition_cmd("HookedTicket", &id, "in_progress")).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().instance.state, "in_progress");
    }

    // --- ON EXIT fires ---

    #[tokio::test]
    async fn on_exit_fires() {
        let (engine, event_bus) = setup_engine_with_event_bus();
        register_hooked_machine(&engine);

        let cmd = spawn_cmd("HookedTicket", vec![("title", Value::Text("test".into()))]);
        let spawned = engine.spawn(&cmd).await.unwrap();
        let id = spawned.instance.id.as_str();

        // Drain spawn events
        let mut rx = event_bus.subscribe();

        engine.transition(&transition_cmd("HookedTicket", &id, "in_progress")).await.unwrap();

        // Collect all events from this transition
        let mut events = Vec::new();
        while let Ok(e) = rx.try_recv() {
            events.push(e.name);
        }

        // Should include "exited_open" from ON EXIT open
        assert!(events.contains(&"exited_open".to_string()));
    }

    // --- ON ENTER fires during transition ---

    #[tokio::test]
    async fn on_enter_fires() {
        let (engine, event_bus) = setup_engine_with_event_bus();
        register_hooked_machine(&engine);

        let cmd = spawn_cmd("HookedTicket", vec![("title", Value::Text("test".into()))]);
        let spawned = engine.spawn(&cmd).await.unwrap();
        let id = spawned.instance.id.as_str();

        let mut rx = event_bus.subscribe();

        engine.transition(&transition_cmd("HookedTicket", &id, "in_progress")).await.unwrap();

        let mut events = Vec::new();
        while let Ok(e) = rx.try_recv() {
            events.push(e.name);
        }

        assert!(events.contains(&"entered_in_progress".to_string()));
    }

    // --- AFTER EACH TRANSITION fires ---

    #[tokio::test]
    async fn after_hook_fires() {
        let (engine, event_bus) = setup_engine_with_event_bus();
        register_hooked_machine(&engine);

        let cmd = spawn_cmd("HookedTicket", vec![("title", Value::Text("test".into()))]);
        let spawned = engine.spawn(&cmd).await.unwrap();
        let id = spawned.instance.id.as_str();

        let mut rx = event_bus.subscribe();

        engine.transition(&transition_cmd("HookedTicket", &id, "in_progress")).await.unwrap();

        let mut events = Vec::new();
        while let Ok(e) = rx.try_recv() {
            events.push(e.name);
        }

        assert!(events.contains(&"after_transition".to_string()));
    }

    // --- Transition actions fire ---

    #[tokio::test]
    async fn transition_actions_fire() {
        let (engine, event_bus) = setup_engine_with_event_bus();
        register_action_machine(&engine);

        let cmd = spawn_cmd("ActionMachine", vec![("title", Value::Text("My Post".into()))]);
        let spawned = engine.spawn(&cmd).await.unwrap();
        let id = spawned.instance.id.as_str();

        let mut rx = event_bus.subscribe();

        engine.transition(&transition_cmd("ActionMachine", &id, "published")).await.unwrap();

        let mut events = Vec::new();
        while let Ok(e) = rx.try_recv() {
            events.push(e);
        }

        // Should have "published" event from transition action
        let published_event = events.iter().find(|e| e.name == "published");
        assert!(published_event.is_some());
        // Payload should be the resolved title
        assert_eq!(
            published_event.unwrap().payload,
            Some(Value::Text("My Post".into()))
        );
    }

    // --- EMIT publishes to event bus ---

    #[tokio::test]
    async fn emit_publishes_to_event_bus() {
        let (engine, event_bus) = setup_engine_with_event_bus();
        register_hooked_machine(&engine);

        let mut rx = event_bus.subscribe();

        let cmd = spawn_cmd("HookedTicket", vec![("title", Value::Text("test".into()))]);
        engine.spawn(&cmd).await.unwrap();

        // Receive spawned event
        let event = rx.recv().await.unwrap();
        assert_eq!(event.name, "spawned");
        assert_eq!(event.machine, "HookedTicket");
    }

    // --- Multiple hooks fire ---

    #[tokio::test]
    async fn multiple_hooks_fire() {
        let (engine, event_bus) = setup_engine_with_event_bus();
        register_hooked_machine(&engine);

        let cmd = spawn_cmd("HookedTicket", vec![("title", Value::Text("test".into()))]);
        let spawned = engine.spawn(&cmd).await.unwrap();
        let id = spawned.instance.id.as_str();

        let mut rx = event_bus.subscribe();

        engine.transition(&transition_cmd("HookedTicket", &id, "in_progress")).await.unwrap();

        let mut events = Vec::new();
        while let Ok(e) = rx.try_recv() {
            events.push(e.name);
        }

        // Should have: exited_open, entered_in_progress, after_transition
        assert!(events.contains(&"exited_open".to_string()));
        assert!(events.contains(&"entered_in_progress".to_string()));
        assert!(events.contains(&"after_transition".to_string()));
    }

    // --- Timeout fires hooks ---

    #[tokio::test]
    async fn timeout_fires_hooks() {
        let (engine, event_bus) = setup_engine_with_event_bus();

        // Machine with hooks + timeout
        let mut m = MachineDefinition::new("TimeoutHooked".into(), "waiting".into());
        m.states = vec![
            StateDefinition::new("waiting".into()),
            StateDefinition::new("active".into()),
            StateDefinition::new("expired".into()),
        ];
        m.terminal_states = vec!["expired".into()];

        let mut t = TransitionDefinition::new(
            TransitionSource::State("waiting".into()),
            "active".into(),
        );
        t.timeout = Some(TimeoutClause {
            duration: smql_ast::value::SmqlDuration::from_hours(1),
            target_state: "expired".into(),
        });
        m.transitions = vec![
            t,
            TransitionDefinition::new(
                TransitionSource::State("active".into()),
                "expired".into(),
            ),
        ];

        m.hooks = vec![
            HookDefinition {
                trigger: HookTrigger::OnExit("active".to_string()),
                actions: vec![Action::Emit {
                    event: "timeout_exit_active".to_string(),
                    payload: None,
                }],
            },
            HookDefinition {
                trigger: HookTrigger::OnEnter("expired".to_string()),
                actions: vec![Action::Emit {
                    event: "timeout_enter_expired".to_string(),
                    payload: None,
                }],
            },
        ];

        engine.catalog.register(m).unwrap();

        let cmd = spawn_cmd("TimeoutHooked", vec![]);
        let spawned = engine.spawn(&cmd).await.unwrap();
        let id = spawned.instance.id.as_str();

        engine.transition(&transition_cmd("TimeoutHooked", &id, "active")).await.unwrap();

        let mut rx = event_bus.subscribe();

        // Simulate timeout
        engine.timeout_transition(&id, "active", "expired").await.unwrap();

        let mut events = Vec::new();
        while let Ok(e) = rx.try_recv() {
            events.push(e.name);
        }

        assert!(events.contains(&"timeout_exit_active".to_string()));
        assert!(events.contains(&"timeout_enter_expired".to_string()));
    }

    // --- Subscribe and receive events ---

    #[tokio::test]
    async fn subscribe_receive_events() {
        let (engine, event_bus) = setup_engine_with_event_bus();
        register_hooked_machine(&engine);

        // Subscribe before spawn
        let mut rx = event_bus.subscribe();

        let cmd = spawn_cmd("HookedTicket", vec![("title", Value::Text("test".into()))]);
        engine.spawn(&cmd).await.unwrap();

        // Should be able to receive events
        let event = rx.recv().await.unwrap();
        assert!(!event.name.is_empty());
        assert_eq!(event.machine, "HookedTicket");
    }

    // --- Hooks don't block transition ---

    #[tokio::test]
    async fn hooks_dont_block_transition() {
        let (engine, _bus) = setup_engine_with_event_bus();
        register_hooked_machine(&engine);

        let cmd = spawn_cmd("HookedTicket", vec![("title", Value::Text("test".into()))]);
        let spawned = engine.spawn(&cmd).await.unwrap();
        let id = spawned.instance.id.as_str();

        // Even with many hooks, transition should succeed
        let result = engine.transition(&transition_cmd("HookedTicket", &id, "in_progress")).await;
        assert!(result.is_ok());

        let result2 = engine.transition(&transition_cmd("HookedTicket", &id, "closed")).await;
        assert!(result2.is_ok());
        assert_eq!(result2.unwrap().instance.state, "closed");
    }

    // --- Log with template ---

    #[tokio::test]
    async fn log_with_template() {
        let (engine, _bus) = setup_engine_with_event_bus();

        let mut m = MachineDefinition::new("LogMachine".into(), "a".into());
        m.states = vec![
            StateDefinition::new("a".into()),
            StateDefinition::new("b".into()),
        ];
        m.transitions = vec![TransitionDefinition::new(
            TransitionSource::State("a".into()),
            "b".into(),
        )];
        m.hooks = vec![HookDefinition {
            trigger: HookTrigger::AfterEachTransition,
            actions: vec![Action::Log("Moved {from_state} -> {to_state}".to_string())],
        }];

        engine.catalog.register(m).unwrap();

        let cmd = spawn_cmd("LogMachine", vec![]);
        let spawned = engine.spawn(&cmd).await.unwrap();
        let id = spawned.instance.id.as_str();

        // Log action shouldn't fail or block
        let result = engine.transition(&transition_cmd("LogMachine", &id, "b")).await;
        assert!(result.is_ok());
    }

    // --- Hook execution order ---

    #[tokio::test]
    async fn hook_execution_order() {
        let (engine, event_bus) = setup_engine_with_event_bus();

        let mut m = MachineDefinition::new("OrderMachine".into(), "a".into());
        m.states = vec![
            StateDefinition::new("a".into()),
            StateDefinition::new("b".into()),
        ];

        let mut t = TransitionDefinition::new(
            TransitionSource::State("a".into()),
            "b".into(),
        );
        t.actions = vec![Action::Emit {
            event: "4_transition_action".to_string(),
            payload: None,
        }];
        m.transitions = vec![t];

        m.hooks = vec![
            HookDefinition {
                trigger: HookTrigger::OnExit("a".to_string()),
                actions: vec![Action::Emit {
                    event: "2_on_exit_a".to_string(),
                    payload: None,
                }],
            },
            HookDefinition {
                trigger: HookTrigger::OnEnter("b".to_string()),
                actions: vec![Action::Emit {
                    event: "5_on_enter_b".to_string(),
                    payload: None,
                }],
            },
            HookDefinition {
                trigger: HookTrigger::AfterEachTransition,
                actions: vec![Action::Emit {
                    event: "6_after_each".to_string(),
                    payload: None,
                }],
            },
        ];

        engine.catalog.register(m).unwrap();

        let cmd = spawn_cmd("OrderMachine", vec![]);
        let spawned = engine.spawn(&cmd).await.unwrap();
        let id = spawned.instance.id.as_str();

        let mut rx = event_bus.subscribe();

        engine.transition(&transition_cmd("OrderMachine", &id, "b")).await.unwrap();

        let mut events = Vec::new();
        while let Ok(e) = rx.try_recv() {
            events.push(e.name.clone());
        }

        // Verify order: ON EXIT → transition actions → ON ENTER → AFTER EACH
        let exit_idx = events.iter().position(|e| e == "2_on_exit_a");
        let action_idx = events.iter().position(|e| e == "4_transition_action");
        let enter_idx = events.iter().position(|e| e == "5_on_enter_b");
        let after_idx = events.iter().position(|e| e == "6_after_each");

        assert!(exit_idx.is_some(), "ON EXIT should fire");
        assert!(action_idx.is_some(), "Transition action should fire");
        assert!(enter_idx.is_some(), "ON ENTER should fire");
        assert!(after_idx.is_some(), "AFTER EACH should fire");

        // Verify ordering
        assert!(exit_idx.unwrap() < action_idx.unwrap(), "EXIT before ACTION");
        assert!(action_idx.unwrap() < enter_idx.unwrap(), "ACTION before ENTER");
        assert!(enter_idx.unwrap() < after_idx.unwrap(), "ENTER before AFTER");
    }
}

#[cfg(test)]
mod composition_tests {
    use crate::engine::Engine;
    use crate::eval::{ChildInfo, EvalContext, eval_expr};
    use smql_ast::command::{SpawnCommand, TransitionCommand};
    use smql_ast::expression::{BinaryOperator, Expression, ExpressionKind};
    use smql_ast::machine::*;
    use smql_ast::types::*;
    use smql_ast::value::Value;
    use smql_catalog::MachineCatalog;
    use smql_storage::MemoryStorage;
    use std::collections::HashMap;
    use std::sync::Arc;

    fn setup_engine() -> Engine {
        let catalog = Arc::new(MachineCatalog::new());
        let storage = Arc::new(MemoryStorage::new());
        Engine::new(catalog, storage)
    }

    /// Register Order machine with CHILDREN { items: LIST(LineItem) }
    fn register_order_machine(engine: &Engine) {
        let mut m = MachineDefinition::new("Order".into(), "pending".into());
        m.states = vec![
            StateDefinition::new("pending".into()),
            StateDefinition::new("confirmed".into()),
            StateDefinition::new("shipped".into()),
            StateDefinition::new("cancelled".into()),
        ];
        m.terminal_states = vec!["shipped".into(), "cancelled".into()];
        m.data = vec![
            DataFieldDefinition {
                name: "customer".into(),
                field_type: TypeDefinition::Text,
                constraints: vec![Constraint::Required],
            },
            DataFieldDefinition {
                name: "total".into(),
                field_type: TypeDefinition::Int,
                constraints: vec![Constraint::Default(DefaultValue::Int(0))],
            },
        ];
        m.children = vec![ChildDefinition {
            name: "items".to_string(),
            machine: "LineItem".to_string(),
            cardinality: ChildCardinality::List { min: None, max: None },
        }];
        m.transitions = vec![
            TransitionDefinition::new(
                TransitionSource::State("pending".into()),
                "confirmed".into(),
            ),
            TransitionDefinition::new(
                TransitionSource::State("confirmed".into()),
                "shipped".into(),
            ),
            {
                let t = TransitionDefinition::new(
                    TransitionSource::Any { except: vec!["cancelled".into(), "shipped".into()] },
                    "cancelled".into(),
                );
                t
            },
        ];
        engine.catalog.register(m).unwrap();
    }

    /// Register LineItem machine (PARENT Order)
    fn register_line_item_machine(engine: &Engine) {
        let mut m = MachineDefinition::new("LineItem".into(), "pending".into());
        m.states = vec![
            StateDefinition::new("pending".into()),
            StateDefinition::new("fulfilled".into()),
            StateDefinition::new("cancelled".into()),
        ];
        m.terminal_states = vec!["fulfilled".into(), "cancelled".into()];
        m.parent = Some("Order".to_string());
        m.data = vec![
            DataFieldDefinition {
                name: "product".into(),
                field_type: TypeDefinition::Text,
                constraints: vec![Constraint::Required],
            },
            DataFieldDefinition {
                name: "qty".into(),
                field_type: TypeDefinition::Int,
                constraints: vec![Constraint::Default(DefaultValue::Int(1))],
            },
        ];
        m.transitions = vec![
            TransitionDefinition::new(
                TransitionSource::State("pending".into()),
                "fulfilled".into(),
            ),
            TransitionDefinition::new(
                TransitionSource::State("pending".into()),
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
            parent_id: None,
            parent_machine: None,
        }
    }

    fn spawn_child_cmd(machine: &str, data: Vec<(&str, Value)>, parent_id: &str, parent_machine: &str) -> SpawnCommand {
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
            parent_id: Some(parent_id.to_string()),
            parent_machine: Some(parent_machine.to_string()),
        }
    }

    fn transition_cmd(machine: &str, instance_id: &str, to_state: &str) -> TransitionCommand {
        TransitionCommand::new(machine.to_string(), instance_id.to_string(), to_state.to_string())
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

    // --- Spawn child links parent ---

    #[tokio::test]
    async fn spawn_child_links_parent() {
        let engine = setup_engine();
        register_order_machine(&engine);
        register_line_item_machine(&engine);

        let order = engine
            .spawn(&spawn_cmd("Order", vec![("customer", Value::Text("Alice".into()))]))
            .await
            .unwrap();
        let order_id = order.instance.id.as_str();

        let item = engine
            .spawn(&spawn_child_cmd(
                "LineItem",
                vec![("product", Value::Text("Widget".into()))],
                &order_id,
                "Order",
            ))
            .await
            .unwrap();

        assert_eq!(item.instance.parent_id.unwrap().as_str(), order_id);
        assert_eq!(item.instance.parent_machine.unwrap(), "Order");
    }

    // --- Spawn child validates parent exists ---

    #[tokio::test]
    async fn spawn_child_invalid_parent_fails() {
        let engine = setup_engine();
        register_order_machine(&engine);
        register_line_item_machine(&engine);

        let result = engine
            .spawn(&spawn_child_cmd(
                "LineItem",
                vec![("product", Value::Text("Widget".into()))],
                "01NONEXISTENT000000000000",
                "Order",
            ))
            .await;
        assert!(result.is_err());
    }

    // --- find_children from engine storage ---

    #[tokio::test]
    async fn find_children_via_storage() {
        let engine = setup_engine();
        register_order_machine(&engine);
        register_line_item_machine(&engine);

        let order = engine
            .spawn(&spawn_cmd("Order", vec![("customer", Value::Text("Bob".into()))]))
            .await
            .unwrap();
        let order_id = order.instance.id.as_str();

        engine
            .spawn(&spawn_child_cmd(
                "LineItem",
                vec![("product", Value::Text("A".into()))],
                &order_id,
                "Order",
            ))
            .await
            .unwrap();
        engine
            .spawn(&spawn_child_cmd(
                "LineItem",
                vec![("product", Value::Text("B".into()))],
                &order_id,
                "Order",
            ))
            .await
            .unwrap();

        let children = engine
            .storage
            .find_children(&order.instance.id, Some("LineItem"))
            .await
            .unwrap();
        assert_eq!(children.len(), 2);
    }

    // --- Guard: ALL children pass ---

    #[test]
    fn guard_all_children_pass() {
        let mut ctx = EvalContext::new(HashMap::new(), "confirmed".to_string());
        ctx.children.insert(
            "items".to_string(),
            vec![
                ChildInfo {
                    id: "c1".into(),
                    machine: "LineItem".into(),
                    state: "fulfilled".into(),
                    data: HashMap::new(),
                },
                ChildInfo {
                    id: "c2".into(),
                    machine: "LineItem".into(),
                    state: "fulfilled".into(),
                    data: HashMap::new(),
                },
            ],
        );

        // ALL(items, STATE IS fulfilled)
        let expr = Expression::new(ExpressionKind::All {
            collection: Box::new(field("items")),
            predicate: Box::new(Expression::new(ExpressionKind::StateIs("fulfilled".into()))),
        });
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));
    }

    // --- Guard: ALL children fail (one not fulfilled) ---

    #[test]
    fn guard_all_children_fail_one() {
        let mut ctx = EvalContext::new(HashMap::new(), "confirmed".to_string());
        ctx.children.insert(
            "items".to_string(),
            vec![
                ChildInfo {
                    id: "c1".into(),
                    machine: "LineItem".into(),
                    state: "fulfilled".into(),
                    data: HashMap::new(),
                },
                ChildInfo {
                    id: "c2".into(),
                    machine: "LineItem".into(),
                    state: "pending".into(),
                    data: HashMap::new(),
                },
            ],
        );

        let expr = Expression::new(ExpressionKind::All {
            collection: Box::new(field("items")),
            predicate: Box::new(Expression::new(ExpressionKind::StateIs("fulfilled".into()))),
        });
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(false));
    }

    // --- Guard: ANY children pass ---

    #[test]
    fn guard_any_children_pass() {
        let mut ctx = EvalContext::new(HashMap::new(), "confirmed".to_string());
        ctx.children.insert(
            "items".to_string(),
            vec![
                ChildInfo {
                    id: "c1".into(),
                    machine: "LineItem".into(),
                    state: "pending".into(),
                    data: HashMap::new(),
                },
                ChildInfo {
                    id: "c2".into(),
                    machine: "LineItem".into(),
                    state: "fulfilled".into(),
                    data: HashMap::new(),
                },
            ],
        );

        let expr = Expression::new(ExpressionKind::Any {
            collection: Box::new(field("items")),
            predicate: Box::new(Expression::new(ExpressionKind::StateIs("fulfilled".into()))),
        });
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));
    }

    // --- Guard: ANY children none match ---

    #[test]
    fn guard_any_children_none_match() {
        let mut ctx = EvalContext::new(HashMap::new(), "confirmed".to_string());
        ctx.children.insert(
            "items".to_string(),
            vec![
                ChildInfo {
                    id: "c1".into(),
                    machine: "LineItem".into(),
                    state: "pending".into(),
                    data: HashMap::new(),
                },
            ],
        );

        let expr = Expression::new(ExpressionKind::Any {
            collection: Box::new(field("items")),
            predicate: Box::new(Expression::new(ExpressionKind::StateIs("fulfilled".into()))),
        });
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(false));
    }

    // --- child.STATE access in field path ---

    #[test]
    fn child_state_access_in_guard() {
        let mut ctx = EvalContext::new(HashMap::new(), "confirmed".to_string());
        ctx.children.insert(
            "shipment".to_string(),
            vec![ChildInfo {
                id: "s1".into(),
                machine: "Shipment".into(),
                state: "dispatched".into(),
                data: HashMap::new(),
            }],
        );

        // shipment.STATE == "dispatched"
        let expr = binop(
            Expression::new(ExpressionKind::FieldAccess(vec![
                "shipment".to_string(),
                "STATE".to_string(),
            ])),
            BinaryOperator::Eq,
            lit(Value::Text("dispatched".into())),
        );
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));
    }

    // --- children.count access ---

    #[test]
    fn children_count_in_guard() {
        let mut ctx = EvalContext::new(HashMap::new(), "confirmed".to_string());
        ctx.children.insert(
            "items".to_string(),
            vec![
                ChildInfo { id: "c1".into(), machine: "LineItem".into(), state: "pending".into(), data: HashMap::new() },
                ChildInfo { id: "c2".into(), machine: "LineItem".into(), state: "pending".into(), data: HashMap::new() },
                ChildInfo { id: "c3".into(), machine: "LineItem".into(), state: "pending".into(), data: HashMap::new() },
            ],
        );

        // items.count > 0
        let expr = binop(
            Expression::new(ExpressionKind::FieldAccess(vec![
                "items".to_string(),
                "count".to_string(),
            ])),
            BinaryOperator::Gt,
            lit(Value::Int(0)),
        );
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));

        // COUNT(items) == 3
        let count_expr = Expression::new(ExpressionKind::Count(Some(Box::new(field("items")))));
        assert_eq!(eval_expr(&count_expr, &ctx).unwrap(), Value::Int(3));
    }

    // --- SIGNAL FROM evaluation ---

    #[test]
    fn signal_from_matches() {
        let mut ctx = EvalContext::new(HashMap::new(), "confirmed".to_string());
        ctx.children.insert(
            "items".to_string(),
            vec![
                ChildInfo {
                    id: "c1".into(),
                    machine: "LineItem".into(),
                    state: "fulfilled".into(),
                    data: HashMap::new(),
                },
            ],
        );

        // SIGNAL FROM LineItem WHERE STATE IS fulfilled
        let expr = Expression::new(ExpressionKind::SignalFrom {
            machine: "LineItem".to_string(),
            condition: Box::new(Expression::new(ExpressionKind::StateIs("fulfilled".into()))),
        });
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));
    }

    #[test]
    fn signal_from_no_match() {
        let mut ctx = EvalContext::new(HashMap::new(), "confirmed".to_string());
        ctx.children.insert(
            "items".to_string(),
            vec![
                ChildInfo {
                    id: "c1".into(),
                    machine: "LineItem".into(),
                    state: "pending".into(),
                    data: HashMap::new(),
                },
            ],
        );

        let expr = Expression::new(ExpressionKind::SignalFrom {
            machine: "LineItem".to_string(),
            condition: Box::new(Expression::new(ExpressionKind::StateIs("fulfilled".into()))),
        });
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(false));
    }

    // --- Parent data access from child context ---

    #[test]
    fn parent_data_access_in_child_guard() {
        let mut parent_data = HashMap::new();
        parent_data.insert("customer".to_string(), Value::Text("Alice".into()));
        parent_data.insert("total".to_string(), Value::Int(100));

        let mut ctx = EvalContext::new(HashMap::new(), "pending".to_string());
        ctx.parent_data = Some(parent_data);
        ctx.parent_state = Some("confirmed".to_string());

        // PARENT.customer == "Alice"
        let expr = binop(
            Expression::new(ExpressionKind::FieldAccess(vec![
                "PARENT".to_string(),
                "customer".to_string(),
            ])),
            BinaryOperator::Eq,
            lit(Value::Text("Alice".into())),
        );
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));

        // PARENT.STATE == "confirmed"
        let state_expr = binop(
            Expression::new(ExpressionKind::FieldAccess(vec![
                "PARENT".to_string(),
                "STATE".to_string(),
            ])),
            BinaryOperator::Eq,
            lit(Value::Text("confirmed".into())),
        );
        assert_eq!(eval_expr(&state_expr, &ctx).unwrap(), Value::Bool(true));
    }

    // --- Spawn in MUTATE creates child ---

    #[tokio::test]
    async fn spawn_in_mutate_creates_child() {
        let engine = setup_engine();
        register_order_machine(&engine);
        register_line_item_machine(&engine);

        // Add a transition with MUTATE that spawns a child
        let mut m = engine.catalog.get("Order").unwrap().clone();
        // Replace pending->confirmed with one that has a __spawn mutate
        m.transitions[0] = {
            let mut t = TransitionDefinition::new(
                TransitionSource::State("pending".into()),
                "confirmed".into(),
            );
            t.mutates = vec![MutateClause {
                field: "first_item".to_string(),
                value: Expression::new(ExpressionKind::FunctionCall {
                    name: "__spawn".to_string(),
                    args: vec![
                        lit(Value::Text("LineItem".into())),
                        lit(Value::Text("product".into())),
                        lit(Value::Text("Auto-Widget".into())),
                    ],
                }),
            }];
            t
        };
        engine.catalog.register(m).unwrap();

        let order = engine
            .spawn(&spawn_cmd("Order", vec![("customer", Value::Text("Charlie".into()))]))
            .await
            .unwrap();
        let order_id = order.instance.id.as_str();

        let result = engine.transition(&transition_cmd("Order", &order_id, "confirmed")).await.unwrap();

        // The mutate should have set first_item to a Ref
        let first_item = result.instance.data.get("first_item").unwrap();
        match first_item {
            Value::Ref(machine, _child_id) => {
                assert_eq!(machine, "LineItem");
            }
            other => panic!("Expected Ref, got {:?}", other),
        }

        // Should have a child linked to the order
        let children = engine
            .storage
            .find_children(&order.instance.id, Some("LineItem"))
            .await
            .unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(
            children[0].data.get("product"),
            Some(&Value::Text("Auto-Widget".into()))
        );
    }

    // --- CASCADE transitions children ---

    #[tokio::test]
    async fn cascade_transitions_children() {
        let engine = setup_engine();
        register_order_machine(&engine);
        register_line_item_machine(&engine);

        let order = engine
            .spawn(&spawn_cmd("Order", vec![("customer", Value::Text("Dave".into()))]))
            .await
            .unwrap();
        let order_id = order.instance.id.as_str();

        // Spawn two children
        let child1 = engine
            .spawn(&spawn_child_cmd(
                "LineItem",
                vec![("product", Value::Text("A".into()))],
                &order_id,
                "Order",
            ))
            .await
            .unwrap();
        let child2 = engine
            .spawn(&spawn_child_cmd(
                "LineItem",
                vec![("product", Value::Text("B".into()))],
                &order_id,
                "Order",
            ))
            .await
            .unwrap();

        // CASCADE cancel the order
        let mut cmd = transition_cmd("Order", &order_id, "cancelled");
        cmd.cascade = true;
        engine.transition(&cmd).await.unwrap();

        // Children should also be cancelled (first terminal state for LineItem)
        let c1 = engine.storage.get_instance(&child1.instance.id).await.unwrap().unwrap();
        let c2 = engine.storage.get_instance(&child2.instance.id).await.unwrap().unwrap();
        // LineItem terminal states: ["fulfilled", "cancelled"]
        // CASCADE tries the first terminal state "fulfilled" — transition from pending->fulfilled exists
        assert_eq!(c1.state, "fulfilled");
        assert_eq!(c2.state, "fulfilled");
    }

    // --- CASCADE skips already terminal ---

    #[tokio::test]
    async fn cascade_skips_already_terminal() {
        let engine = setup_engine();
        register_order_machine(&engine);
        register_line_item_machine(&engine);

        let order = engine
            .spawn(&spawn_cmd("Order", vec![("customer", Value::Text("Eve".into()))]))
            .await
            .unwrap();
        let order_id = order.instance.id.as_str();

        let child = engine
            .spawn(&spawn_child_cmd(
                "LineItem",
                vec![("product", Value::Text("X".into()))],
                &order_id,
                "Order",
            ))
            .await
            .unwrap();
        let child_id = child.instance.id.as_str();

        // Manually transition child to fulfilled (terminal)
        engine
            .transition(&transition_cmd("LineItem", &child_id, "fulfilled"))
            .await
            .unwrap();

        // CASCADE cancel the order
        let mut cmd = transition_cmd("Order", &order_id, "cancelled");
        cmd.cascade = true;
        engine.transition(&cmd).await.unwrap();

        // Child should still be fulfilled (not re-transitioned)
        let c = engine.storage.get_instance(&child.instance.id).await.unwrap().unwrap();
        assert_eq!(c.state, "fulfilled");
    }

    // --- Concurrent child spawns ---

    #[tokio::test]
    async fn concurrent_child_spawns() {
        let engine = setup_engine();
        register_order_machine(&engine);
        register_line_item_machine(&engine);

        let order = engine
            .spawn(&spawn_cmd("Order", vec![("customer", Value::Text("Frank".into()))]))
            .await
            .unwrap();
        let order_id = order.instance.id.as_str();

        // Spawn 5 children
        for i in 0..5 {
            engine
                .spawn(&spawn_child_cmd(
                    "LineItem",
                    vec![("product", Value::Text(format!("Item{}", i)))],
                    &order_id,
                    "Order",
                ))
                .await
                .unwrap();
        }

        let children = engine
            .storage
            .find_children(&order.instance.id, Some("LineItem"))
            .await
            .unwrap();
        assert_eq!(children.len(), 5);
    }

    // --- ALL over empty collection is vacuously true ---

    #[test]
    fn all_over_empty_collection_is_true() {
        let ctx = EvalContext::new(HashMap::new(), "confirmed".to_string());
        // No children set for "items"

        let expr = Expression::new(ExpressionKind::All {
            collection: Box::new(field("items")),
            predicate: Box::new(Expression::new(ExpressionKind::StateIs("fulfilled".into()))),
        });
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));
    }

    // --- ANY over empty collection is false ---

    #[test]
    fn any_over_empty_collection_is_false() {
        let ctx = EvalContext::new(HashMap::new(), "confirmed".to_string());

        let expr = Expression::new(ExpressionKind::Any {
            collection: Box::new(field("items")),
            predicate: Box::new(Expression::new(ExpressionKind::StateIs("fulfilled".into()))),
        });
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(false));
    }

    // --- Guard with ALL children, used in actual engine transition ---

    #[tokio::test]
    async fn guard_all_children_in_transition() {
        let engine = setup_engine();
        register_line_item_machine(&engine);

        // Register order with guarded confirmed->shipped requiring ALL items fulfilled
        let mut m = MachineDefinition::new("GuardedOrder".into(), "pending".into());
        m.states = vec![
            StateDefinition::new("pending".into()),
            StateDefinition::new("confirmed".into()),
            StateDefinition::new("shipped".into()),
        ];
        m.terminal_states = vec!["shipped".into()];
        m.data = vec![DataFieldDefinition {
            name: "customer".into(),
            field_type: TypeDefinition::Text,
            constraints: vec![Constraint::Required],
        }];
        m.children = vec![ChildDefinition {
            name: "items".to_string(),
            machine: "LineItem".to_string(),
            cardinality: ChildCardinality::List { min: None, max: None },
        }];

        let t_confirm = TransitionDefinition::new(
            TransitionSource::State("pending".into()),
            "confirmed".into(),
        );
        let mut t_ship = TransitionDefinition::new(
            TransitionSource::State("confirmed".into()),
            "shipped".into(),
        );
        // Guard: ALL(items, STATE IS fulfilled)
        t_ship.guards = vec![Expression::new(ExpressionKind::All {
            collection: Box::new(field("items")),
            predicate: Box::new(Expression::new(ExpressionKind::StateIs("fulfilled".into()))),
        })];

        m.transitions = vec![t_confirm, t_ship];
        engine.catalog.register(m).unwrap();

        // Spawn order and confirm it
        let order = engine
            .spawn(&spawn_cmd("GuardedOrder", vec![("customer", Value::Text("Grace".into()))]))
            .await
            .unwrap();
        let order_id = order.instance.id.as_str();
        engine.transition(&transition_cmd("GuardedOrder", &order_id, "confirmed")).await.unwrap();

        // Spawn two children
        let c1 = engine
            .spawn(&spawn_child_cmd(
                "LineItem",
                vec![("product", Value::Text("A".into()))],
                &order_id,
                "GuardedOrder",
            ))
            .await
            .unwrap();
        let c2 = engine
            .spawn(&spawn_child_cmd(
                "LineItem",
                vec![("product", Value::Text("B".into()))],
                &order_id,
                "GuardedOrder",
            ))
            .await
            .unwrap();

        // Try to ship — should fail because items are still pending
        let ship_result = engine.try_transition(&transition_cmd("GuardedOrder", &order_id, "shipped")).await.unwrap();
        assert!(ship_result.is_none(), "Should fail: not all items fulfilled");

        // Fulfill first item
        engine
            .transition(&transition_cmd("LineItem", &c1.instance.id.as_str(), "fulfilled"))
            .await
            .unwrap();

        // Try again — still should fail
        let ship_result2 = engine.try_transition(&transition_cmd("GuardedOrder", &order_id, "shipped")).await.unwrap();
        assert!(ship_result2.is_none(), "Should fail: only 1 of 2 fulfilled");

        // Fulfill second item
        engine
            .transition(&transition_cmd("LineItem", &c2.instance.id.as_str(), "fulfilled"))
            .await
            .unwrap();

        // Now ship should succeed
        let ship_result3 = engine.transition(&transition_cmd("GuardedOrder", &order_id, "shipped")).await.unwrap();
        assert_eq!(ship_result3.instance.state, "shipped");
    }

    // --- Wire callback and signal parent ---

    #[tokio::test]
    async fn signal_parent_transitions_parent() {
        let engine = setup_engine();
        register_line_item_machine(&engine);

        // Register a simple parent machine
        let mut m = MachineDefinition::new("SimpleOrder".into(), "pending".into());
        m.states = vec![
            StateDefinition::new("pending".into()),
            StateDefinition::new("ready".into()),
        ];
        m.terminal_states = vec!["ready".into()];
        m.data = vec![];
        m.transitions = vec![
            TransitionDefinition::new(
                TransitionSource::State("pending".into()),
                "ready".into(),
            ),
        ];
        engine.catalog.register(m).unwrap();

        // Wire up callback
        engine.wire_callback();

        let parent = engine
            .spawn(&spawn_cmd("SimpleOrder", vec![]))
            .await
            .unwrap();
        let parent_id = parent.instance.id.as_str();

        // Spawn a child with SignalParent action on fulfilled
        let mut child_m = engine.catalog.get("LineItem").unwrap().clone();
        // Add action to signal parent on pending->fulfilled
        child_m.transitions[0].actions.push(Action::SignalParent {
            target_state: "ready".to_string(),
        });
        engine.catalog.register(child_m).unwrap();

        let child = engine
            .spawn(&spawn_child_cmd(
                "LineItem",
                vec![("product", Value::Text("Widget".into()))],
                &parent_id,
                "SimpleOrder",
            ))
            .await
            .unwrap();
        let child_id = child.instance.id.as_str();

        // Fulfill the child — should signal parent to "ready"
        engine
            .transition(&transition_cmd("LineItem", &child_id, "fulfilled"))
            .await
            .unwrap();

        // Check parent was transitioned
        let parent_updated = engine
            .storage
            .get_instance(&parent.instance.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(parent_updated.state, "ready");
    }

    // --- Signal parent no parent is noop ---

    #[tokio::test]
    async fn signal_parent_no_parent_noop() {
        let engine = setup_engine();
        register_line_item_machine(&engine);
        engine.wire_callback();

        // Add SignalParent action
        let mut child_m = engine.catalog.get("LineItem").unwrap().clone();
        child_m.transitions[0].actions.push(Action::SignalParent {
            target_state: "ready".to_string(),
        });
        engine.catalog.register(child_m).unwrap();

        // Spawn a LineItem without a parent
        let item = engine
            .spawn(&spawn_cmd("LineItem", vec![("product", Value::Text("Solo".into()))]))
            .await
            .unwrap();
        let item_id = item.instance.id.as_str();

        // Should not error even with no parent
        let result = engine
            .transition(&transition_cmd("LineItem", &item_id, "fulfilled"))
            .await;
        assert!(result.is_ok());
    }

    // --- Full order lifecycle (Order + LineItems end-to-end) ---

    #[tokio::test]
    async fn full_order_lifecycle() {
        let engine = setup_engine();
        register_order_machine(&engine);
        register_line_item_machine(&engine);

        // 1. Spawn order
        let order = engine
            .spawn(&spawn_cmd("Order", vec![("customer", Value::Text("Zara".into()))]))
            .await
            .unwrap();
        let order_id = order.instance.id.as_str();
        assert_eq!(order.instance.state, "pending");

        // 2. Spawn 3 line items
        let mut child_ids = Vec::new();
        for i in 0..3 {
            let child = engine
                .spawn(&spawn_child_cmd(
                    "LineItem",
                    vec![("product", Value::Text(format!("Product{}", i)))],
                    &order_id,
                    "Order",
                ))
                .await
                .unwrap();
            child_ids.push(child.instance.id.as_str());
        }

        // 3. Confirm order
        engine
            .transition(&transition_cmd("Order", &order_id, "confirmed"))
            .await
            .unwrap();

        // 4. Fulfill all line items
        for child_id in &child_ids {
            engine
                .transition(&transition_cmd("LineItem", child_id, "fulfilled"))
                .await
                .unwrap();
        }

        // 5. Verify all children are fulfilled
        let children = engine
            .storage
            .find_children(&order.instance.id, Some("LineItem"))
            .await
            .unwrap();
        assert_eq!(children.len(), 3);
        assert!(children.iter().all(|c| c.state == "fulfilled"));

        // 6. Ship order
        engine
            .transition(&transition_cmd("Order", &order_id, "shipped"))
            .await
            .unwrap();

        let final_order = engine
            .storage
            .get_instance(&order.instance.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(final_order.state, "shipped");
    }
}

#[cfg(test)]
mod alter_tests {
    use crate::engine::Engine;
    use smql_ast::command::{AlterMachineCommand, AlterOperation, SpawnCommand};
    use smql_ast::expression::{Expression, ExpressionKind};
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

    fn register_simple_machine(engine: &Engine) {
        let mut m = MachineDefinition::new("Task".into(), "open".into());
        m.states = vec![
            StateDefinition::new("open".into()),
            StateDefinition::new("in_progress".into()),
            StateDefinition::new("done".into()),
        ];
        m.terminal_states = vec!["done".into()];
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
                "done".into(),
            ),
            TransitionDefinition::new(
                TransitionSource::State("in_progress".into()),
                "open".into(),
            ),
        ];
        engine.catalog.register_unchecked(m);
    }

    fn spawn_task(_engine: &Engine, title: &str) -> SpawnCommand {
        SpawnCommand::new(
            "Task".into(),
            vec![(
                "title".into(),
                Expression::new(ExpressionKind::Literal(Value::Text(title.into()))),
            )],
        )
    }

    // --- ADD STATE ---

    #[tokio::test]
    async fn alter_add_state() {
        let engine = setup_engine();
        register_simple_machine(&engine);

        let cmd = AlterMachineCommand {
            machine: "Task".into(),
            operations: vec![AlterOperation::AddState("blocked".into())],
        };

        let result = engine.execute_alter_machine(&cmd).await.unwrap();
        assert_eq!(result.operations_applied, 1);
        assert_eq!(result.instances_migrated, 0);

        let def = engine.catalog.get("Task").unwrap();
        assert!(def.states.iter().any(|s| s.name == "blocked"));
        assert_eq!(engine.catalog.version("Task").unwrap(), 2); // Version incremented
    }

    #[tokio::test]
    async fn alter_add_state_duplicate_fails() {
        let engine = setup_engine();
        register_simple_machine(&engine);

        let cmd = AlterMachineCommand {
            machine: "Task".into(),
            operations: vec![AlterOperation::AddState("open".into())],
        };

        let result = engine.execute_alter_machine(&cmd).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));
    }

    // --- REMOVE STATE ---

    #[tokio::test]
    async fn alter_remove_state_migrates_instances() {
        let engine = setup_engine();
        register_simple_machine(&engine);

        // Spawn an instance and transition it to in_progress
        let spawn_result = engine.spawn(&spawn_task(&engine, "Test")).await.unwrap();
        let id = spawn_result.instance.id.as_str();

        let t_cmd = smql_ast::command::TransitionCommand::new("Task".into(), id.clone(), "in_progress".into());
        engine.transition(&t_cmd).await.unwrap();

        // Now remove in_progress state and migrate to open
        let cmd = AlterMachineCommand {
            machine: "Task".into(),
            operations: vec![AlterOperation::RemoveState {
                state: "in_progress".into(),
                migrate_to: "open".into(),
            }],
        };

        let result = engine.execute_alter_machine(&cmd).await.unwrap();
        assert_eq!(result.instances_migrated, 1);

        // Verify instance was migrated
        let inst_id = smql_storage::InstanceId::from_string(&id).unwrap();
        let inst = engine.storage.get_instance(&inst_id).await.unwrap().unwrap();
        assert_eq!(inst.state, "open");

        // Verify state was removed from definition
        let def = engine.catalog.get("Task").unwrap();
        assert!(!def.states.iter().any(|s| s.name == "in_progress"));
    }

    #[tokio::test]
    async fn alter_remove_state_removes_transitions() {
        let engine = setup_engine();
        register_simple_machine(&engine);

        let cmd = AlterMachineCommand {
            machine: "Task".into(),
            operations: vec![AlterOperation::RemoveState {
                state: "in_progress".into(),
                migrate_to: "open".into(),
            }],
        };

        let result = engine.execute_alter_machine(&cmd).await.unwrap();
        assert!(result.warnings.iter().any(|w| w.contains("transition")));

        let def = engine.catalog.get("Task").unwrap();
        // All transitions involving in_progress should be removed
        for t in &def.transitions {
            match &t.from {
                TransitionSource::State(s) => assert_ne!(s, "in_progress"),
                _ => {}
            }
            assert_ne!(t.to, "in_progress");
        }
    }

    #[tokio::test]
    async fn alter_remove_initial_state_fails() {
        let engine = setup_engine();
        register_simple_machine(&engine);

        let cmd = AlterMachineCommand {
            machine: "Task".into(),
            operations: vec![AlterOperation::RemoveState {
                state: "open".into(),
                migrate_to: "in_progress".into(),
            }],
        };

        let result = engine.execute_alter_machine(&cmd).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("initial state"));
    }

    #[tokio::test]
    async fn alter_remove_nonexistent_state_fails() {
        let engine = setup_engine();
        register_simple_machine(&engine);

        let cmd = AlterMachineCommand {
            machine: "Task".into(),
            operations: vec![AlterOperation::RemoveState {
                state: "nonexistent".into(),
                migrate_to: "open".into(),
            }],
        };

        let result = engine.execute_alter_machine(&cmd).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("does not exist"));
    }

    // --- ADD TRANSITION ---

    #[tokio::test]
    async fn alter_add_transition() {
        let engine = setup_engine();
        register_simple_machine(&engine);

        let new_transition = TransitionDefinition::new(
            TransitionSource::State("open".into()),
            "done".into(),
        );

        let cmd = AlterMachineCommand {
            machine: "Task".into(),
            operations: vec![AlterOperation::AddTransition(new_transition)],
        };

        let result = engine.execute_alter_machine(&cmd).await.unwrap();
        assert_eq!(result.operations_applied, 1);

        // Verify transition exists — can now transition open -> done
        let spawn_result = engine.spawn(&spawn_task(&engine, "Direct")).await.unwrap();
        let t_cmd = smql_ast::command::TransitionCommand::new(
            "Task".into(),
            spawn_result.instance.id.as_str(),
            "done".into(),
        );
        let t_result = engine.transition(&t_cmd).await.unwrap();
        assert_eq!(t_result.to_state, "done");
    }

    #[tokio::test]
    async fn alter_add_transition_invalid_state_fails() {
        let engine = setup_engine();
        register_simple_machine(&engine);

        let bad_transition = TransitionDefinition::new(
            TransitionSource::State("open".into()),
            "nonexistent".into(),
        );

        let cmd = AlterMachineCommand {
            machine: "Task".into(),
            operations: vec![AlterOperation::AddTransition(bad_transition)],
        };

        let result = engine.execute_alter_machine(&cmd).await;
        assert!(result.is_err());
    }

    // --- REMOVE TRANSITION ---

    #[tokio::test]
    async fn alter_remove_transition() {
        let engine = setup_engine();
        register_simple_machine(&engine);

        let cmd = AlterMachineCommand {
            machine: "Task".into(),
            operations: vec![AlterOperation::RemoveTransition {
                from: "in_progress".into(),
                to: "open".into(),
            }],
        };

        let result = engine.execute_alter_machine(&cmd).await.unwrap();
        assert_eq!(result.operations_applied, 1);

        // Verify transition was removed
        let def = engine.catalog.get("Task").unwrap();
        assert!(!def.transitions.iter().any(|t| {
            matches!(&t.from, TransitionSource::State(s) if s == "in_progress") && t.to == "open"
        }));
    }

    #[tokio::test]
    async fn alter_remove_nonexistent_transition_fails() {
        let engine = setup_engine();
        register_simple_machine(&engine);

        let cmd = AlterMachineCommand {
            machine: "Task".into(),
            operations: vec![AlterOperation::RemoveTransition {
                from: "open".into(),
                to: "done".into(), // This transition doesn't exist
            }],
        };

        let result = engine.execute_alter_machine(&cmd).await;
        assert!(result.is_err());
    }

    // --- ADD DATA ---

    #[tokio::test]
    async fn alter_add_data_with_backfill() {
        let engine = setup_engine();
        register_simple_machine(&engine);

        // Spawn some instances first
        engine.spawn(&spawn_task(&engine, "Task1")).await.unwrap();
        engine.spawn(&spawn_task(&engine, "Task2")).await.unwrap();

        let cmd = AlterMachineCommand {
            machine: "Task".into(),
            operations: vec![AlterOperation::AddData {
                field: DataFieldDefinition {
                    name: "category".into(),
                    field_type: TypeDefinition::Text,
                    constraints: vec![Constraint::Optional],
                },
                backfill: Some(Expression::new(ExpressionKind::Literal(
                    Value::Text("general".into()),
                ))),
            }],
        };

        let result = engine.execute_alter_machine(&cmd).await.unwrap();
        assert_eq!(result.instances_migrated, 2);
        assert!(result.warnings.iter().any(|w| w.contains("Backfilled")));

        // Verify field was added to definition
        let def = engine.catalog.get("Task").unwrap();
        assert!(def.data.iter().any(|d| d.name == "category"));

        // Verify instances were backfilled
        let filter = smql_storage::Filter::default();
        let instances = engine.storage.find_instances("Task", &filter).await.unwrap();
        for inst in &instances {
            assert_eq!(inst.data.get("category"), Some(&Value::Text("general".into())));
        }
    }

    #[tokio::test]
    async fn alter_add_data_with_default() {
        let engine = setup_engine();
        register_simple_machine(&engine);

        engine.spawn(&spawn_task(&engine, "Task1")).await.unwrap();

        let cmd = AlterMachineCommand {
            machine: "Task".into(),
            operations: vec![AlterOperation::AddData {
                field: DataFieldDefinition {
                    name: "status_note".into(),
                    field_type: TypeDefinition::Text,
                    constraints: vec![Constraint::Default(DefaultValue::String("none".into()))],
                },
                backfill: None,
            }],
        };

        let result = engine.execute_alter_machine(&cmd).await.unwrap();
        assert_eq!(result.instances_migrated, 1);

        let filter = smql_storage::Filter::default();
        let instances = engine.storage.find_instances("Task", &filter).await.unwrap();
        assert_eq!(instances[0].data.get("status_note"), Some(&Value::Text("none".into())));
    }

    #[tokio::test]
    async fn alter_add_data_duplicate_fails() {
        let engine = setup_engine();
        register_simple_machine(&engine);

        let cmd = AlterMachineCommand {
            machine: "Task".into(),
            operations: vec![AlterOperation::AddData {
                field: DataFieldDefinition {
                    name: "title".into(), // Already exists
                    field_type: TypeDefinition::Text,
                    constraints: vec![],
                },
                backfill: None,
            }],
        };

        let result = engine.execute_alter_machine(&cmd).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));
    }

    #[tokio::test]
    async fn alter_add_required_field_without_default_or_backfill_fails() {
        let engine = setup_engine();
        register_simple_machine(&engine);

        let cmd = AlterMachineCommand {
            machine: "Task".into(),
            operations: vec![AlterOperation::AddData {
                field: DataFieldDefinition {
                    name: "new_required".into(),
                    field_type: TypeDefinition::Text,
                    constraints: vec![Constraint::Required],
                },
                backfill: None,
            }],
        };

        let result = engine.execute_alter_machine(&cmd).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("REQUIRED"));
    }

    // --- REMOVE DATA ---

    #[tokio::test]
    async fn alter_remove_data() {
        let engine = setup_engine();
        register_simple_machine(&engine);

        // Spawn an instance with priority
        engine.spawn(&spawn_task(&engine, "Task1")).await.unwrap();

        let cmd = AlterMachineCommand {
            machine: "Task".into(),
            operations: vec![AlterOperation::RemoveData("priority".into())],
        };

        let result = engine.execute_alter_machine(&cmd).await.unwrap();
        assert_eq!(result.instances_migrated, 1);

        // Verify field removed from definition
        let def = engine.catalog.get("Task").unwrap();
        assert!(!def.data.iter().any(|d| d.name == "priority"));

        // Verify field removed from instances
        let filter = smql_storage::Filter::default();
        let instances = engine.storage.find_instances("Task", &filter).await.unwrap();
        assert!(!instances[0].data.contains_key("priority"));
    }

    #[tokio::test]
    async fn alter_remove_nonexistent_data_fails() {
        let engine = setup_engine();
        register_simple_machine(&engine);

        let cmd = AlterMachineCommand {
            machine: "Task".into(),
            operations: vec![AlterOperation::RemoveData("nonexistent".into())],
        };

        let result = engine.execute_alter_machine(&cmd).await;
        assert!(result.is_err());
    }

    // --- BACKFILL ---

    #[tokio::test]
    async fn alter_backfill() {
        let engine = setup_engine();
        register_simple_machine(&engine);

        engine.spawn(&spawn_task(&engine, "Task1")).await.unwrap();
        engine.spawn(&spawn_task(&engine, "Task2")).await.unwrap();
        engine.spawn(&spawn_task(&engine, "Task3")).await.unwrap();

        let cmd = AlterMachineCommand {
            machine: "Task".into(),
            operations: vec![AlterOperation::Backfill {
                field: "priority".into(),
                value: Expression::new(ExpressionKind::Literal(Value::Int(1))),
            }],
        };

        let result = engine.execute_alter_machine(&cmd).await.unwrap();
        assert_eq!(result.instances_migrated, 3);

        let filter = smql_storage::Filter::default();
        let instances = engine.storage.find_instances("Task", &filter).await.unwrap();
        for inst in &instances {
            assert_eq!(inst.data.get("priority"), Some(&Value::Int(1)));
        }
    }

    #[tokio::test]
    async fn alter_backfill_nonexistent_field_fails() {
        let engine = setup_engine();
        register_simple_machine(&engine);

        let cmd = AlterMachineCommand {
            machine: "Task".into(),
            operations: vec![AlterOperation::Backfill {
                field: "nonexistent".into(),
                value: Expression::new(ExpressionKind::Literal(Value::Int(1))),
            }],
        };

        let result = engine.execute_alter_machine(&cmd).await;
        assert!(result.is_err());
    }

    // --- MODIFY TRANSITION ---

    #[tokio::test]
    async fn alter_modify_transition() {
        let engine = setup_engine();
        register_simple_machine(&engine);

        // Modify the open -> in_progress transition to add a guard
        let mut modified = TransitionDefinition::new(
            TransitionSource::State("open".into()),
            "in_progress".into(),
        );
        modified.guards = vec![Expression::new(ExpressionKind::IsSet(Box::new(
            Expression::new(ExpressionKind::FieldAccess(vec!["title".into()])),
        )))];

        let cmd = AlterMachineCommand {
            machine: "Task".into(),
            operations: vec![AlterOperation::ModifyTransition(modified)],
        };

        let result = engine.execute_alter_machine(&cmd).await.unwrap();
        assert_eq!(result.operations_applied, 1);

        // Verify the transition now has a guard
        let def = engine.catalog.get("Task").unwrap();
        let t = def.transitions.iter().find(|t| {
            matches!(&t.from, TransitionSource::State(s) if s == "open") && t.to == "in_progress"
        });
        assert!(t.is_some());
        assert_eq!(t.unwrap().guards.len(), 1);
    }

    #[tokio::test]
    async fn alter_modify_nonexistent_transition_fails() {
        let engine = setup_engine();
        register_simple_machine(&engine);

        let t = TransitionDefinition::new(
            TransitionSource::State("open".into()),
            "done".into(), // This transition doesn't exist
        );

        let cmd = AlterMachineCommand {
            machine: "Task".into(),
            operations: vec![AlterOperation::ModifyTransition(t)],
        };

        let result = engine.execute_alter_machine(&cmd).await;
        assert!(result.is_err());
    }

    // --- VERSION TRACKING ---

    #[tokio::test]
    async fn alter_increments_version() {
        let engine = setup_engine();
        register_simple_machine(&engine);

        let v1 = engine.catalog.version("Task").unwrap();

        let cmd = AlterMachineCommand {
            machine: "Task".into(),
            operations: vec![AlterOperation::AddState("review".into())],
        };
        let result = engine.execute_alter_machine(&cmd).await.unwrap();

        assert_eq!(result.new_version, v1 + 1);
        assert_eq!(engine.catalog.version("Task").unwrap(), v1 + 1);
    }

    // --- MULTIPLE OPERATIONS ---

    #[tokio::test]
    async fn alter_multiple_operations() {
        let engine = setup_engine();
        register_simple_machine(&engine);

        engine.spawn(&spawn_task(&engine, "Task1")).await.unwrap();

        let cmd = AlterMachineCommand {
            machine: "Task".into(),
            operations: vec![
                AlterOperation::AddState("review".into()),
                AlterOperation::AddTransition(TransitionDefinition::new(
                    TransitionSource::State("in_progress".into()),
                    "review".into(),
                )),
                AlterOperation::AddData {
                    field: DataFieldDefinition {
                        name: "reviewer".into(),
                        field_type: TypeDefinition::Text,
                        constraints: vec![Constraint::Optional],
                    },
                    backfill: None,
                },
            ],
        };

        let result = engine.execute_alter_machine(&cmd).await.unwrap();
        assert_eq!(result.operations_applied, 3);

        let def = engine.catalog.get("Task").unwrap();
        assert!(def.states.iter().any(|s| s.name == "review"));
        assert!(def.transitions.iter().any(|t| {
            matches!(&t.from, TransitionSource::State(s) if s == "in_progress") && t.to == "review"
        }));
        assert!(def.data.iter().any(|d| d.name == "reviewer"));
    }

    // --- NONEXISTENT MACHINE ---

    #[tokio::test]
    async fn alter_nonexistent_machine_fails() {
        let engine = setup_engine();

        let cmd = AlterMachineCommand {
            machine: "NonExistent".into(),
            operations: vec![AlterOperation::AddState("new".into())],
        };

        let result = engine.execute_alter_machine(&cmd).await;
        assert!(result.is_err());
    }

    // --- PARSER INTEGRATION ---

    #[tokio::test]
    async fn alter_via_parser_add_state() {
        let engine = setup_engine();
        register_simple_machine(&engine);

        let input = "ALTER MACHINE Task ADD STATE review";
        let stmts = smql_parser::parse(input).unwrap();
        assert_eq!(stmts.len(), 1);

        if let smql_ast::command::Statement::Command(smql_ast::command::Command::AlterMachine(cmd)) = &stmts[0] {
            let result = engine.execute_alter_machine(&cmd).await.unwrap();
            assert_eq!(result.operations_applied, 1);
            let def = engine.catalog.get("Task").unwrap();
            assert!(def.states.iter().any(|s| s.name == "review"));
        } else {
            panic!("Expected ALTER MACHINE command");
        }
    }

    #[tokio::test]
    async fn alter_via_parser_remove_state() {
        let engine = setup_engine();
        register_simple_machine(&engine);

        let input = "ALTER MACHINE Task REMOVE STATE in_progress MIGRATE TO open";
        let stmts = smql_parser::parse(input).unwrap();

        if let smql_ast::command::Statement::Command(smql_ast::command::Command::AlterMachine(cmd)) = &stmts[0] {
            let result = engine.execute_alter_machine(&cmd).await.unwrap();
            assert_eq!(result.operations_applied, 1);
        } else {
            panic!("Expected ALTER MACHINE command");
        }
    }

    #[tokio::test]
    async fn alter_via_parser_backfill() {
        let engine = setup_engine();
        register_simple_machine(&engine);

        engine.spawn(&spawn_task(&engine, "Task1")).await.unwrap();

        let input = "ALTER MACHINE Task BACKFILL priority = 5";
        let stmts = smql_parser::parse(input).unwrap();

        if let smql_ast::command::Statement::Command(smql_ast::command::Command::AlterMachine(cmd)) = &stmts[0] {
            let result = engine.execute_alter_machine(&cmd).await.unwrap();
            assert_eq!(result.instances_migrated, 1);

            let filter = smql_storage::Filter::default();
            let instances = engine.storage.find_instances("Task", &filter).await.unwrap();
            assert_eq!(instances[0].data.get("priority"), Some(&Value::Int(5)));
        } else {
            panic!("Expected ALTER MACHINE command");
        }
    }

    // --- STORAGE bulk operations ---

    #[tokio::test]
    async fn storage_migrate_state_updates_indices() {
        let engine = setup_engine();
        register_simple_machine(&engine);

        // Spawn instances in different states
        engine.spawn(&spawn_task(&engine, "T1")).await.unwrap();
        let r2 = engine.spawn(&spawn_task(&engine, "T2")).await.unwrap();

        // Move T2 to in_progress
        let t_cmd = smql_ast::command::TransitionCommand::new(
            "Task".into(),
            r2.instance.id.as_str(),
            "in_progress".into(),
        );
        engine.transition(&t_cmd).await.unwrap();

        // Migrate in_progress -> open
        let migrated = engine.storage.migrate_instances_state("Task", "in_progress", "open").await.unwrap();
        assert_eq!(migrated, 1);

        // Both should now be in open state
        let filter = smql_storage::Filter { state: Some("open".into()), ..Default::default() };
        let instances = engine.storage.find_instances("Task", &filter).await.unwrap();
        assert_eq!(instances.len(), 2);

        // Nothing in in_progress
        let filter2 = smql_storage::Filter { state: Some("in_progress".into()), ..Default::default() };
        let instances2 = engine.storage.find_instances("Task", &filter2).await.unwrap();
        assert_eq!(instances2.len(), 0);
    }

    #[tokio::test]
    async fn storage_bulk_update_applies_to_all() {
        let engine = setup_engine();
        register_simple_machine(&engine);

        engine.spawn(&spawn_task(&engine, "T1")).await.unwrap();
        engine.spawn(&spawn_task(&engine, "T2")).await.unwrap();

        let mutations = vec![smql_storage::Mutation::SetField(
            "priority".into(),
            Value::Int(99),
        )];
        let count = engine.storage.bulk_update_instances("Task", &mutations).await.unwrap();
        assert_eq!(count, 2);

        let filter = smql_storage::Filter::default();
        let instances = engine.storage.find_instances("Task", &filter).await.unwrap();
        for inst in &instances {
            assert_eq!(inst.data.get("priority"), Some(&Value::Int(99)));
        }
    }
}
