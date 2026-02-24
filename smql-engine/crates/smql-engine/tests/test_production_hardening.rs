/// Production hardening tests for SMQL engine.
///
/// These tests target edge cases, overflow conditions, and boundary behaviors
/// that could cause silent data corruption or unexpected failures in production.

// ============================================================================
// Area 1: Guard Expression Evaluator — eval.rs
// ============================================================================

mod eval_hardening {
    use smql_ast::expression::{BinaryOperator, Expression, ExpressionKind, UnaryOperator};
    use smql_ast::value::{SmqlDuration, Value};
    use smql_engine_core::eval::{eval_expr, eval_guard, EvalContext};
    use std::collections::HashMap;

    fn lit(v: Value) -> Expression {
        Expression::new(ExpressionKind::Literal(v))
    }

    fn binop(left: Expression, op: BinaryOperator, right: Expression) -> Expression {
        Expression::new(ExpressionKind::BinaryOp {
            left: Box::new(left),
            op,
            right: Box::new(right),
        })
    }

    fn ctx() -> EvalContext {
        EvalContext::new(HashMap::new(), "open".to_string())
    }

    // -- NaN handling --

    #[test]
    fn nan_is_falsy() {
        let c = ctx();
        let nan_val = Value::Float(f64::NAN);
        assert!(!eval_guard(&lit(nan_val), &c).unwrap());
    }

    #[test]
    fn nan_equality_is_false() {
        let c = ctx();
        let expr = binop(
            lit(Value::Float(f64::NAN)),
            BinaryOperator::Eq,
            lit(Value::Float(f64::NAN)),
        );
        assert_eq!(eval_expr(&expr, &c).unwrap(), Value::Bool(false));
    }

    #[test]
    fn nan_comparison_is_false() {
        let c = ctx();
        let expr = binop(
            lit(Value::Float(f64::NAN)),
            BinaryOperator::Lt,
            lit(Value::Float(5.0)),
        );
        assert_eq!(eval_expr(&expr, &c).unwrap(), Value::Bool(false));

        let expr2 = binop(
            lit(Value::Float(f64::NAN)),
            BinaryOperator::Gt,
            lit(Value::Float(5.0)),
        );
        assert_eq!(eval_expr(&expr2, &c).unwrap(), Value::Bool(false));
    }

    // -- Float overflow to Infinity --

    #[test]
    fn float_add_overflow_to_infinity_errors() {
        let c = ctx();
        let expr = binop(
            lit(Value::Float(f64::MAX)),
            BinaryOperator::Add,
            lit(Value::Float(f64::MAX)),
        );
        let result = eval_expr(&expr, &c);
        assert!(result.is_err(), "Float overflow to Infinity should error");
    }

    #[test]
    fn float_mul_overflow_to_infinity_errors() {
        let c = ctx();
        let expr = binop(
            lit(Value::Float(1e308)),
            BinaryOperator::Mul,
            lit(Value::Float(1e308)),
        );
        let result = eval_expr(&expr, &c);
        assert!(result.is_err(), "Float multiply overflow should error");
    }

    #[test]
    fn float_sub_nan_errors() {
        let c = ctx();
        // Infinity - Infinity = NaN — should error
        let expr = binop(
            lit(Value::Float(f64::INFINITY)),
            BinaryOperator::Sub,
            lit(Value::Float(f64::INFINITY)),
        );
        let result = eval_expr(&expr, &c);
        assert!(result.is_err(), "Infinity - Infinity (NaN) should error");
    }

    #[test]
    fn float_normal_arithmetic_still_works() {
        let c = ctx();
        let expr = binop(
            lit(Value::Float(1.5)),
            BinaryOperator::Add,
            lit(Value::Float(2.5)),
        );
        assert_eq!(eval_expr(&expr, &c).unwrap(), Value::Float(4.0));

        let expr2 = binop(
            lit(Value::Float(10.0)),
            BinaryOperator::Mul,
            lit(Value::Float(3.0)),
        );
        assert_eq!(eval_expr(&expr2, &c).unwrap(), Value::Float(30.0));
    }

    // -- Integer overflow --

    #[test]
    fn int_add_overflow_errors() {
        let c = ctx();
        let expr = binop(
            lit(Value::Int(i64::MAX)),
            BinaryOperator::Add,
            lit(Value::Int(1)),
        );
        assert!(eval_expr(&expr, &c).is_err());
    }

    #[test]
    fn int_sub_overflow_errors() {
        let c = ctx();
        let expr = binop(
            lit(Value::Int(i64::MIN)),
            BinaryOperator::Sub,
            lit(Value::Int(1)),
        );
        assert!(eval_expr(&expr, &c).is_err());
    }

    #[test]
    fn int_mul_overflow_errors() {
        let c = ctx();
        let expr = binop(
            lit(Value::Int(i64::MAX)),
            BinaryOperator::Mul,
            lit(Value::Int(2)),
        );
        assert!(eval_expr(&expr, &c).is_err());
    }

    #[test]
    fn int_div_min_by_neg1_overflow_errors() {
        // i64::MIN / -1 overflows because |i64::MIN| > i64::MAX
        let c = ctx();
        let expr = binop(
            lit(Value::Int(i64::MIN)),
            BinaryOperator::Div,
            lit(Value::Int(-1)),
        );
        assert!(eval_expr(&expr, &c).is_err());
    }

    #[test]
    fn int_neg_min_overflow_errors() {
        let c = ctx();
        let expr = Expression::new(ExpressionKind::UnaryOp {
            op: UnaryOperator::Neg,
            operand: Box::new(lit(Value::Int(i64::MIN))),
        });
        assert!(eval_expr(&expr, &c).is_err());
    }

    // -- Null handling in arithmetic --

    #[test]
    fn null_add_int_errors() {
        let c = ctx();
        let expr = binop(lit(Value::Null), BinaryOperator::Add, lit(Value::Int(5)));
        assert!(eval_expr(&expr, &c).is_err());
    }

    #[test]
    fn null_compared_to_int_is_false() {
        let c = ctx();
        let expr = binop(lit(Value::Null), BinaryOperator::Gt, lit(Value::Int(5)));
        assert_eq!(eval_expr(&expr, &c).unwrap(), Value::Bool(false));
    }

    #[test]
    fn null_not_equal_to_int() {
        let c = ctx();
        let expr = binop(lit(Value::Null), BinaryOperator::Eq, lit(Value::Int(0)));
        assert_eq!(eval_expr(&expr, &c).unwrap(), Value::Bool(false));
    }

    // -- Duration edge cases --

    #[test]
    fn duration_add_saturation() {
        let c = ctx();
        let d1 = Value::Duration(SmqlDuration::from_seconds(u64::MAX));
        let d2 = Value::Duration(SmqlDuration::from_seconds(1));
        let expr = binop(lit(d1), BinaryOperator::Add, lit(d2));
        let result = eval_expr(&expr, &c).unwrap();
        assert_eq!(result, Value::Duration(SmqlDuration::from_seconds(u64::MAX)));
    }

    #[test]
    fn duration_sub_saturation_to_zero() {
        let c = ctx();
        let d1 = Value::Duration(SmqlDuration::from_seconds(5));
        let d2 = Value::Duration(SmqlDuration::from_seconds(100));
        let expr = binop(lit(d1), BinaryOperator::Sub, lit(d2));
        let result = eval_expr(&expr, &c).unwrap();
        assert_eq!(result, Value::Duration(SmqlDuration::from_seconds(0)));
    }

    // -- Empty field access --

    #[test]
    fn missing_field_is_null() {
        let c = ctx();
        let expr = Expression::new(ExpressionKind::FieldAccess(vec![
            "nonexistent".to_string(),
        ]));
        assert_eq!(eval_expr(&expr, &c).unwrap(), Value::Null);
    }

    #[test]
    fn nested_access_on_null_returns_null() {
        let c = ctx();
        let expr = Expression::new(ExpressionKind::FieldAccess(vec![
            "missing".to_string(),
            "deep".to_string(),
            "nested".to_string(),
        ]));
        assert_eq!(eval_expr(&expr, &c).unwrap(), Value::Null);
    }

    // -- Boolean short-circuit behavior --

    #[test]
    fn and_with_null_is_falsy() {
        let c = ctx();
        let expr = binop(lit(Value::Null), BinaryOperator::And, lit(Value::Bool(true)));
        assert_eq!(eval_expr(&expr, &c).unwrap(), Value::Bool(false));
    }

    #[test]
    fn or_with_null_and_true() {
        let c = ctx();
        let expr = binop(lit(Value::Null), BinaryOperator::Or, lit(Value::Bool(true)));
        assert_eq!(eval_expr(&expr, &c).unwrap(), Value::Bool(true));
    }

    // -- InSet/InList with empty values --

    #[test]
    fn in_set_empty_values_is_false() {
        let c = ctx();
        let expr = Expression::new(ExpressionKind::InSet {
            expr: Box::new(lit(Value::Int(1))),
            values: vec![],
        });
        assert_eq!(eval_expr(&expr, &c).unwrap(), Value::Bool(false));
    }

    #[test]
    fn in_list_empty_values_is_false() {
        let c = ctx();
        let expr = Expression::new(ExpressionKind::InList {
            expr: Box::new(lit(Value::Text("x".to_string()))),
            values: vec![],
        });
        assert_eq!(eval_expr(&expr, &c).unwrap(), Value::Bool(false));
    }

    // -- Cross-type comparison edge cases --

    #[test]
    fn int_float_cross_type_ordering() {
        let c = ctx();
        let expr = binop(
            lit(Value::Int(3)),
            BinaryOperator::Lt,
            lit(Value::Float(3.5)),
        );
        assert_eq!(eval_expr(&expr, &c).unwrap(), Value::Bool(true));
    }

    #[test]
    fn money_not_comparable_to_int() {
        let c = ctx();
        let expr = binop(
            lit(Value::Money(9999, "USD".to_string())),
            BinaryOperator::Gt,
            lit(Value::Int(0)),
        );
        assert_eq!(eval_expr(&expr, &c).unwrap(), Value::Bool(false));
    }
}

// ============================================================================
// Area 5: Query Engine — production edge cases
// ============================================================================

mod query_hardening {
    use smql_ast::command::SpawnCommand;
    use smql_ast::expression::{BinaryOperator, Expression, ExpressionKind};
    use smql_ast::machine::{MachineDefinition, StateDefinition};
    use smql_ast::query::{AggregateQuery, FindQuery, MeasureClause};
    use smql_ast::types::{AggregateFunction, DataFieldDefinition, SortClause, SortDirection, TypeDefinition};
    use smql_ast::value::Value;
    use smql_catalog::MachineCatalog;
    use smql_engine_core::query::QueryResult;
    use smql_engine_core::Engine;
    use smql_storage::MemoryStorage;
    use std::sync::Arc;

    fn simple_machine() -> MachineDefinition {
        let mut m = MachineDefinition::new("Item".to_string(), "active".to_string());
        m.states.push(StateDefinition::new("active".to_string()));
        m.states.push(StateDefinition::new("done".to_string()));
        m.terminal_states.push("done".to_string());
        m
    }

    fn lit(v: Value) -> Expression {
        Expression::new(ExpressionKind::Literal(v))
    }

    #[tokio::test]
    async fn aggregate_avg_on_zero_instances_returns_null() {
        let catalog = Arc::new(MachineCatalog::new());
        catalog.register(simple_machine()).unwrap();
        let storage = Arc::new(MemoryStorage::new());
        let engine = Engine::new(catalog, storage);

        let query = AggregateQuery {
            machine: "Item".to_string(),
            measures: vec![MeasureClause {
                function: AggregateFunction::Avg,
                field: Some("price".to_string()),
                alias: Some("avg_price".to_string()),
            }],
            group_by: vec![],
            filter: None,
        };

        let result = engine.execute_query(&smql_ast::query::Query::Aggregate(query)).await.unwrap();
        if let QueryResult::Aggregate(rows) = result {
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].measures.get("avg_price"), Some(&Value::Null));
        } else {
            panic!("Expected Aggregate result");
        }
    }

    #[tokio::test]
    async fn aggregate_percentile_on_zero_instances_returns_null() {
        let catalog = Arc::new(MachineCatalog::new());
        catalog.register(simple_machine()).unwrap();
        let storage = Arc::new(MemoryStorage::new());
        let engine = Engine::new(catalog, storage);

        let query = AggregateQuery {
            machine: "Item".to_string(),
            measures: vec![MeasureClause {
                function: AggregateFunction::Percentile(50.0),
                field: Some("price".to_string()),
                alias: Some("p50".to_string()),
            }],
            group_by: vec![],
            filter: None,
        };

        let result = engine.execute_query(&smql_ast::query::Query::Aggregate(query)).await.unwrap();
        if let QueryResult::Aggregate(rows) = result {
            assert_eq!(rows[0].measures.get("p50"), Some(&Value::Null));
        } else {
            panic!("Expected Aggregate result");
        }
    }

    #[tokio::test]
    async fn aggregate_min_max_on_zero_instances_returns_null() {
        let catalog = Arc::new(MachineCatalog::new());
        catalog.register(simple_machine()).unwrap();
        let storage = Arc::new(MemoryStorage::new());
        let engine = Engine::new(catalog, storage);

        let query = AggregateQuery {
            machine: "Item".to_string(),
            measures: vec![
                MeasureClause {
                    function: AggregateFunction::Min,
                    field: Some("price".to_string()),
                    alias: Some("min_price".to_string()),
                },
                MeasureClause {
                    function: AggregateFunction::Max,
                    field: Some("price".to_string()),
                    alias: Some("max_price".to_string()),
                },
            ],
            group_by: vec![],
            filter: None,
        };

        let result = engine.execute_query(&smql_ast::query::Query::Aggregate(query)).await.unwrap();
        if let QueryResult::Aggregate(rows) = result {
            assert_eq!(rows[0].measures.get("min_price"), Some(&Value::Null));
            assert_eq!(rows[0].measures.get("max_price"), Some(&Value::Null));
        } else {
            panic!("Expected Aggregate result");
        }
    }

    #[tokio::test]
    async fn aggregate_count_on_zero_instances_is_zero() {
        let catalog = Arc::new(MachineCatalog::new());
        catalog.register(simple_machine()).unwrap();
        let storage = Arc::new(MemoryStorage::new());
        let engine = Engine::new(catalog, storage);

        let query = AggregateQuery {
            machine: "Item".to_string(),
            measures: vec![MeasureClause {
                function: AggregateFunction::Count,
                field: None,
                alias: Some("total".to_string()),
            }],
            group_by: vec![],
            filter: None,
        };

        let result = engine.execute_query(&smql_ast::query::Query::Aggregate(query)).await.unwrap();
        if let QueryResult::Aggregate(rows) = result {
            assert_eq!(rows[0].measures.get("total"), Some(&Value::Int(0)));
        } else {
            panic!("Expected Aggregate result");
        }
    }

    #[tokio::test]
    async fn find_with_filter_and_limit_applies_correctly() {
        let mut machine = simple_machine();
        machine.data.push(DataFieldDefinition {
            name: "priority".to_string(),
            field_type: TypeDefinition::Int,
            constraints: vec![],
        });
        let catalog = Arc::new(MachineCatalog::new());
        catalog.register(machine).unwrap();
        let storage = Arc::new(MemoryStorage::new());
        let engine = Engine::new(catalog, storage);

        // Spawn 10 instances with priority 1-10
        for i in 1..=10 {
            let cmd = SpawnCommand {
                machine: "Item".to_string(),
                data: vec![(
                    "priority".to_string(),
                    lit(Value::Int(i)),
                )],
                then_transition: None,
                batch: false,
                batch_data: vec![],
                parent_id: None,
                parent_machine: None,
                as_actor: None,
            };
            engine.spawn(&cmd).await.unwrap();
        }

        // FIND Item WHERE priority > 5 LIMIT 2
        let query = FindQuery {
            machine: "Item".to_string(),
            filter: Some(Expression::new(ExpressionKind::BinaryOp {
                left: Box::new(Expression::new(ExpressionKind::FieldAccess(vec!["priority".to_string()]))),
                op: BinaryOperator::Gt,
                right: Box::new(lit(Value::Int(5))),
            })),
            sort: vec![],
            limit: Some(2),
            offset: None,
            after: None,
            as_actor: None,
        };

        let result = engine.execute_query(&smql_ast::query::Query::Find(query)).await.unwrap();
        if let QueryResult::Instances(instances) = result {
            assert_eq!(instances.len(), 2, "Should get exactly 2 results with LIMIT 2 after filtering");
            for inst in &instances {
                let p = inst.data.get("priority").unwrap();
                if let Value::Int(v) = p {
                    assert!(v > &5, "Filtered instance should have priority > 5, got {}", v);
                }
            }
        } else {
            panic!("Expected Instances result");
        }
    }

    #[tokio::test]
    async fn find_with_filter_and_offset_applies_correctly() {
        let mut machine = simple_machine();
        machine.data.push(DataFieldDefinition {
            name: "priority".to_string(),
            field_type: TypeDefinition::Int,
            constraints: vec![],
        });
        let catalog = Arc::new(MachineCatalog::new());
        catalog.register(machine).unwrap();
        let storage = Arc::new(MemoryStorage::new());
        let engine = Engine::new(catalog, storage);

        for i in 1..=10 {
            let cmd = SpawnCommand {
                machine: "Item".to_string(),
                data: vec![(
                    "priority".to_string(),
                    lit(Value::Int(i)),
                )],
                then_transition: None,
                batch: false,
                batch_data: vec![],
                parent_id: None,
                parent_machine: None,
                as_actor: None,
            };
            engine.spawn(&cmd).await.unwrap();
        }

        // FIND Item WHERE priority > 3 SORT BY priority ASC OFFSET 2 LIMIT 3
        let query = FindQuery {
            machine: "Item".to_string(),
            filter: Some(Expression::new(ExpressionKind::BinaryOp {
                left: Box::new(Expression::new(ExpressionKind::FieldAccess(vec!["priority".to_string()]))),
                op: BinaryOperator::Gt,
                right: Box::new(lit(Value::Int(3))),
            })),
            sort: vec![SortClause {
                field: "priority".to_string(),
                direction: SortDirection::Asc,
            }],
            limit: Some(3),
            offset: Some(2),
            after: None,
            as_actor: None,
        };

        let result = engine.execute_query(&smql_ast::query::Query::Find(query)).await.unwrap();
        if let QueryResult::Instances(instances) = result {
            assert_eq!(instances.len(), 3, "Should get 3 results after filter+offset+limit");
        } else {
            panic!("Expected Instances result");
        }
    }

    #[tokio::test]
    async fn find_sort_by_missing_field_uses_null_ordering() {
        let catalog = Arc::new(MachineCatalog::new());
        catalog.register(simple_machine()).unwrap();
        let storage = Arc::new(MemoryStorage::new());
        let engine = Engine::new(catalog, storage);

        for _ in 0..3 {
            let cmd = SpawnCommand {
                machine: "Item".to_string(),
                data: vec![],
                then_transition: None,
                batch: false,
                batch_data: vec![],
                parent_id: None,
                parent_machine: None,
                as_actor: None,
            };
            engine.spawn(&cmd).await.unwrap();
        }

        let query = FindQuery {
            machine: "Item".to_string(),
            filter: None,
            sort: vec![SortClause {
                field: "nonexistent_field".to_string(),
                direction: SortDirection::Asc,
            }],
            limit: None,
            offset: None,
            after: None,
            as_actor: None,
        };

        let result = engine.execute_query(&smql_ast::query::Query::Find(query)).await.unwrap();
        if let QueryResult::Instances(instances) = result {
            assert_eq!(instances.len(), 3, "Should return all instances even when sorting by missing field");
        } else {
            panic!("Expected Instances result");
        }
    }
}

// ============================================================================
// Area 2: Transition Logic — engine.rs edge cases
// ============================================================================

mod transition_hardening {
    use smql_ast::command::{SpawnCommand, TransitionCommand};
    use smql_ast::machine::{MachineDefinition, StateDefinition, TransitionDefinition, TransitionSource};
    use smql_catalog::MachineCatalog;
    use smql_engine_core::Engine;
    use smql_storage::MemoryStorage;
    use std::sync::Arc;

    fn make_machine_with_transitions() -> MachineDefinition {
        let mut m = MachineDefinition::new("Ticket".to_string(), "open".to_string());
        m.states.push(StateDefinition::new("open".to_string()));
        m.states.push(StateDefinition::new("in_progress".to_string()));
        m.states.push(StateDefinition::new("resolved".to_string()));
        m.states.push(StateDefinition::new("closed".to_string()));
        m.terminal_states.push("closed".to_string());
        m.transitions.push(TransitionDefinition::new(
            TransitionSource::State("open".to_string()),
            "in_progress".to_string(),
        ));
        m.transitions.push(TransitionDefinition::new(
            TransitionSource::State("in_progress".to_string()),
            "resolved".to_string(),
        ));
        m.transitions.push(TransitionDefinition::new(
            TransitionSource::State("resolved".to_string()),
            "closed".to_string(),
        ));
        // Wildcard: any (except closed) -> closed
        m.transitions.push(TransitionDefinition::new(
            TransitionSource::Any {
                except: vec!["closed".to_string()],
            },
            "closed".to_string(),
        ));
        m
    }

    #[tokio::test]
    async fn transition_through_multi_hop() {
        let catalog = Arc::new(MachineCatalog::new());
        catalog.register(make_machine_with_transitions()).unwrap();
        let storage = Arc::new(MemoryStorage::new());
        let engine = Engine::new(catalog, storage);

        let spawn = SpawnCommand {
            machine: "Ticket".to_string(),
            data: vec![],
            then_transition: None,
            batch: false,
            batch_data: vec![],
            parent_id: None,
            parent_machine: None,
            as_actor: None,
        };
        let result = engine.spawn(&spawn).await.unwrap();
        let id = result.instance.id.as_str();

        // THROUGH open -> in_progress -> resolved -> closed
        let mut cmd = TransitionCommand::new(
            "Ticket".to_string(),
            id.clone(),
            "closed".to_string(),
        );
        cmd.through = vec!["in_progress".to_string(), "resolved".to_string()];

        let result = engine.transition(&cmd).await.unwrap();
        assert_eq!(result.to_state, "closed");
        assert_eq!(result.instance.state, "closed");
    }

    #[tokio::test]
    async fn wildcard_except_prevents_transition() {
        let catalog = Arc::new(MachineCatalog::new());
        catalog.register(make_machine_with_transitions()).unwrap();
        let storage = Arc::new(MemoryStorage::new());
        let engine = Engine::new(catalog, storage);

        let spawn = SpawnCommand {
            machine: "Ticket".to_string(),
            data: vec![],
            then_transition: None,
            batch: false,
            batch_data: vec![],
            parent_id: None,
            parent_machine: None,
            as_actor: None,
        };
        let result = engine.spawn(&spawn).await.unwrap();
        let id = result.instance.id.as_str();

        // First close it
        let cmd = TransitionCommand::new("Ticket".to_string(), id.clone(), "closed".to_string());
        engine.transition(&cmd).await.unwrap();

        // Now try to transition from closed -> closed (wildcard EXCEPT closed)
        let cmd2 = TransitionCommand::new("Ticket".to_string(), id.clone(), "closed".to_string());
        let result = engine.transition(&cmd2).await;
        assert!(result.is_err(), "Transition from excepted state should fail");
    }

    #[tokio::test]
    async fn try_transition_returns_none_on_no_path() {
        let catalog = Arc::new(MachineCatalog::new());
        catalog.register(make_machine_with_transitions()).unwrap();
        let storage = Arc::new(MemoryStorage::new());
        let engine = Engine::new(catalog, storage);

        let spawn = SpawnCommand {
            machine: "Ticket".to_string(),
            data: vec![],
            then_transition: None,
            batch: false,
            batch_data: vec![],
            parent_id: None,
            parent_machine: None,
            as_actor: None,
        };
        let result = engine.spawn(&spawn).await.unwrap();
        let id = result.instance.id.as_str();

        // TRY to go from open -> resolved (no direct transition)
        let cmd = TransitionCommand::new("Ticket".to_string(), id.clone(), "resolved".to_string());
        let result = engine.try_transition(&cmd).await.unwrap();
        assert!(result.is_none(), "TRY should return None when transition not defined");
    }

    #[tokio::test]
    async fn transition_nonexistent_instance_errors() {
        let catalog = Arc::new(MachineCatalog::new());
        catalog.register(make_machine_with_transitions()).unwrap();
        let storage = Arc::new(MemoryStorage::new());
        let engine = Engine::new(catalog, storage);

        let cmd = TransitionCommand::new(
            "Ticket".to_string(),
            "01NONEXISTENT00000000000000".to_string(),
            "in_progress".to_string(),
        );
        let result = engine.transition(&cmd).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn transition_wrong_machine_name_errors() {
        let catalog = Arc::new(MachineCatalog::new());
        catalog.register(make_machine_with_transitions()).unwrap();
        let storage = Arc::new(MemoryStorage::new());
        let engine = Engine::new(catalog, storage);

        let spawn = SpawnCommand {
            machine: "Ticket".to_string(),
            data: vec![],
            then_transition: None,
            batch: false,
            batch_data: vec![],
            parent_id: None,
            parent_machine: None,
            as_actor: None,
        };
        let result = engine.spawn(&spawn).await.unwrap();
        let id = result.instance.id.as_str();

        let cmd = TransitionCommand::new(
            "WrongMachine".to_string(),
            id.clone(),
            "in_progress".to_string(),
        );
        let result = engine.transition(&cmd).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn sequential_transitions_read_current_state() {
        let catalog = Arc::new(MachineCatalog::new());
        let mut machine = make_machine_with_transitions();
        // Add a direct open -> resolved transition for testing
        machine.transitions.push(TransitionDefinition::new(
            TransitionSource::State("open".to_string()),
            "resolved".to_string(),
        ));
        catalog.register(machine).unwrap();
        let storage = Arc::new(MemoryStorage::new());
        let engine = Engine::new(catalog, storage);

        let spawn = SpawnCommand {
            machine: "Ticket".to_string(),
            data: vec![],
            then_transition: None,
            batch: false,
            batch_data: vec![],
            parent_id: None,
            parent_machine: None,
            as_actor: None,
        };
        let result = engine.spawn(&spawn).await.unwrap();
        let id = result.instance.id.as_str();

        // First transition: open -> in_progress
        let cmd1 = TransitionCommand::new("Ticket".to_string(), id.clone(), "in_progress".to_string());
        engine.transition(&cmd1).await.unwrap();

        // Second transition from current state (in_progress -> resolved) should succeed
        let cmd2 = TransitionCommand::new("Ticket".to_string(), id.clone(), "resolved".to_string());
        let result = engine.transition(&cmd2).await;
        assert!(result.is_ok());
    }
}

// ============================================================================
// Area 3: Timer System — edge cases
// ============================================================================

mod timer_hardening {
    use smql_ast::value::SmqlDuration;
    use smql_timer::TimerManager;
    use chrono::{TimeDelta, Utc};

    #[test]
    fn zero_duration_timer_fires_immediately() {
        let tm = TimerManager::new();
        let dur = SmqlDuration::from_seconds(0);
        tm.register("inst_1", "Machine", "waiting", &dur, "done");
        let expired = tm.drain_expired();
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].instance_id, "inst_1");
    }

    #[test]
    fn cancel_after_drain_is_noop() {
        let tm = TimerManager::new();
        let now = Utc::now();
        tm.register_with_deadline(
            "inst_1", "Machine", "waiting", "done",
            now - TimeDelta::seconds(10), now - TimeDelta::seconds(100),
        );
        let expired = tm.drain_expired();
        assert_eq!(expired.len(), 1);

        // Cancel after drain should be safe no-op
        tm.cancel("inst_1", "waiting");
        assert_eq!(tm.timer_count(), 0);
    }

    #[test]
    fn very_large_timeout_registers_correctly() {
        let tm = TimerManager::new();
        let dur = SmqlDuration::from_seconds(365 * 24 * 3600);
        tm.register("inst_1", "Machine", "waiting", &dur, "done");
        assert_eq!(tm.timer_count(), 1);

        let entry = tm.get_timer("inst_1", "waiting").unwrap();
        assert_eq!(entry.target_state, "done");

        let remaining = tm.timeout_remaining("inst_1", "waiting").unwrap();
        assert!(remaining.num_seconds() > 364 * 24 * 3600);
    }

    #[test]
    fn register_replaces_existing_timer() {
        let tm = TimerManager::new();
        let dur1 = SmqlDuration::from_hours(1);
        let dur2 = SmqlDuration::from_hours(2);

        tm.register("inst_1", "Machine", "waiting", &dur1, "timeout_1");
        tm.register("inst_1", "Machine", "waiting", &dur2, "timeout_2");

        assert_eq!(tm.timer_count(), 1);
        let entry = tm.get_timer("inst_1", "waiting").unwrap();
        assert_eq!(entry.target_state, "timeout_2");
    }

    #[test]
    fn dwell_timer_register_and_cancel() {
        let tm = TimerManager::new();
        let dur = SmqlDuration::from_hours(1);
        tm.register_dwell("inst_1", "Machine", "waiting", &dur);
        assert_eq!(tm.dwell_timer_count(), 1);

        tm.cancel_dwell_for_state("inst_1", "waiting");
        assert_eq!(tm.dwell_timer_count(), 0);
    }

    #[test]
    fn dwell_timer_cancel_all() {
        let tm = TimerManager::new();
        let dur1 = SmqlDuration::from_hours(1);
        let dur2 = SmqlDuration::from_hours(2);
        tm.register_dwell("inst_1", "Machine", "state_a", &dur1);
        tm.register_dwell("inst_1", "Machine", "state_b", &dur2);
        assert_eq!(tm.dwell_timer_count(), 2);

        tm.cancel_all_dwell("inst_1");
        assert_eq!(tm.dwell_timer_count(), 0);
    }
}

// ============================================================================
// Area 4: Storage Layer — consistency edge cases
// ============================================================================

mod storage_hardening {
    use smql_ast::value::Value;
    use smql_storage::{Filter, Instance, InstanceId, Mutation, TrailEntry};
    use smql_storage::MemoryStorage;
    use smql_storage::Storage;
    use chrono::Utc;
    use std::collections::HashMap;

    #[tokio::test]
    async fn version_conflict_on_concurrent_update() {
        let storage = MemoryStorage::new();
        let instance = Instance::new("Machine".to_string(), "active".to_string(), HashMap::new());
        let id = instance.id.clone();
        storage.store_instance(&instance).await.unwrap();

        // First update succeeds (version 1 -> 2)
        storage
            .update_instance(
                &id,
                1,
                &[Mutation::SetField("x".to_string(), Value::Int(1))],
            )
            .await
            .unwrap();

        // Second update with stale version 1 fails (current version is now 2)
        let result = storage
            .update_instance(
                &id,
                1,
                &[Mutation::SetField("x".to_string(), Value::Int(2))],
            )
            .await;
        assert!(result.is_err(), "Stale version should cause conflict error");
    }

    #[tokio::test]
    async fn delete_removes_from_all_indices() {
        let storage = MemoryStorage::new();
        let instance = Instance::new("Machine".to_string(), "active".to_string(), HashMap::new());
        let id = instance.id.clone();
        storage.store_instance(&instance).await.unwrap();

        assert!(storage.get_instance(&id).await.unwrap().is_some());

        storage.delete_instance(&id).await.unwrap();

        assert!(storage.get_instance(&id).await.unwrap().is_none());

        let filter = Filter::default();
        let results = storage.find_instances("Machine", &filter).await.unwrap();
        assert!(results.is_empty());

        let trail = storage.get_trail(&id).await;
        assert!(trail.is_err());
    }

    #[tokio::test]
    async fn state_index_consistent_after_transition() {
        let storage = MemoryStorage::new();
        let instance = Instance::new("Machine".to_string(), "state_a".to_string(), HashMap::new());
        let id = instance.id.clone();
        storage.store_instance(&instance).await.unwrap();

        let trail = TrailEntry {
            instance_id: id.clone(),
            machine: "Machine".to_string(),
            sequence: 1,
            from_state: "state_a".to_string(),
            to_state: "state_b".to_string(),
            transition_name: None,
            actor: None,
            memo: None,
            timestamp: Utc::now(),
            data_snapshot: None,
        };

        storage
            .transition_instance(&id, 1, "state_b", &[], trail)
            .await
            .unwrap();

        let filter_a = Filter {
            state: Some("state_a".to_string()),
            ..Default::default()
        };
        let in_a = storage.find_instances("Machine", &filter_a).await.unwrap();
        assert_eq!(in_a.len(), 0, "No instances should remain in state_a");

        let filter_b = Filter {
            state: Some("state_b".to_string()),
            ..Default::default()
        };
        let in_b = storage.find_instances("Machine", &filter_b).await.unwrap();
        assert_eq!(in_b.len(), 1, "Instance should be in state_b");
    }

    #[tokio::test]
    async fn cursor_pagination_empty_result() {
        let storage = MemoryStorage::new();
        let filter = Filter {
            after_id: Some(InstanceId::new().as_str()),
            ..Default::default()
        };
        let results = storage.find_instances("Machine", &filter).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn duplicate_store_errors() {
        let storage = MemoryStorage::new();
        let instance = Instance::new("Machine".to_string(), "active".to_string(), HashMap::new());
        storage.store_instance(&instance).await.unwrap();

        let result = storage.store_instance(&instance).await;
        assert!(result.is_err(), "Duplicate store should fail");
    }
}

// ============================================================================
// Area 6: Composition — parent-child edge cases
// ============================================================================

mod composition_hardening {
    use smql_ast::command::SpawnCommand;
    use smql_ast::expression::{Expression, ExpressionKind};
    use smql_ast::machine::{ChildCardinality, ChildDefinition, MachineDefinition, StateDefinition, TransitionDefinition, TransitionSource};
    use smql_ast::value::Value;
    use smql_catalog::MachineCatalog;
    use smql_engine_core::Engine;
    use smql_storage::MemoryStorage;
    use smql_storage::Storage;
    use std::sync::Arc;

    fn lit(v: Value) -> Expression {
        Expression::new(ExpressionKind::Literal(v))
    }

    fn parent_machine() -> MachineDefinition {
        let mut m = MachineDefinition::new("Order".to_string(), "open".to_string());
        m.states.push(StateDefinition::new("open".to_string()));
        m.states.push(StateDefinition::new("fulfilled".to_string()));
        m.states.push(StateDefinition::new("cancelled".to_string()));
        m.terminal_states.push("fulfilled".to_string());
        m.terminal_states.push("cancelled".to_string());
        m.transitions.push(TransitionDefinition::new(
            TransitionSource::State("open".to_string()),
            "fulfilled".to_string(),
        ));
        m.transitions.push(TransitionDefinition::new(
            TransitionSource::State("open".to_string()),
            "cancelled".to_string(),
        ));
        m.children.push(ChildDefinition {
            name: "items".to_string(),
            machine: "LineItem".to_string(),
            cardinality: ChildCardinality::List { min: None, max: None },
        });
        m
    }

    fn child_machine() -> MachineDefinition {
        let mut m = MachineDefinition::new("LineItem".to_string(), "pending".to_string());
        m.states.push(StateDefinition::new("pending".to_string()));
        m.states.push(StateDefinition::new("shipped".to_string()));
        m.states.push(StateDefinition::new("cancelled".to_string()));
        m.terminal_states.push("shipped".to_string());
        m.terminal_states.push("cancelled".to_string());
        m.transitions.push(TransitionDefinition::new(
            TransitionSource::State("pending".to_string()),
            "shipped".to_string(),
        ));
        m.transitions.push(TransitionDefinition::new(
            TransitionSource::State("pending".to_string()),
            "cancelled".to_string(),
        ));
        m.parent = Some("Order".to_string());
        m
    }

    #[tokio::test]
    async fn spawn_child_with_parent_reference() {
        let catalog = Arc::new(MachineCatalog::new());
        catalog.register(parent_machine()).unwrap();
        catalog.register(child_machine()).unwrap();
        let storage = Arc::new(MemoryStorage::new());
        let engine = Engine::new(catalog, storage.clone());

        // Spawn parent
        let parent_cmd = SpawnCommand {
            machine: "Order".to_string(),
            data: vec![],
            then_transition: None,
            batch: false,
            batch_data: vec![],
            parent_id: None,
            parent_machine: None,
            as_actor: None,
        };
        let parent = engine.spawn(&parent_cmd).await.unwrap();
        let parent_id = parent.instance.id.clone();

        // Spawn child with parent reference
        let child_cmd = SpawnCommand {
            machine: "LineItem".to_string(),
            data: vec![],
            then_transition: None,
            batch: false,
            batch_data: vec![],
            parent_id: Some(parent_id.as_str()),
            parent_machine: Some("Order".to_string()),
            as_actor: None,
        };
        let child = engine.spawn(&child_cmd).await.unwrap();

        // Verify child references parent
        assert_eq!(child.instance.parent_id.as_ref().unwrap().as_str(), parent_id.as_str());
        assert_eq!(child.instance.parent_machine.as_ref().unwrap(), "Order");

        // Verify parent can find children
        let children = storage.find_children(&parent_id, None).await.unwrap();
        assert_eq!(children.len(), 1);
    }

    #[tokio::test]
    async fn delete_parent_then_children_are_orphaned() {
        let catalog = Arc::new(MachineCatalog::new());
        catalog.register(parent_machine()).unwrap();
        catalog.register(child_machine()).unwrap();
        let storage = Arc::new(MemoryStorage::new());
        let engine = Engine::new(catalog, storage.clone());

        let parent_cmd = SpawnCommand {
            machine: "Order".to_string(),
            data: vec![],
            then_transition: None,
            batch: false,
            batch_data: vec![],
            parent_id: None,
            parent_machine: None,
            as_actor: None,
        };
        let parent = engine.spawn(&parent_cmd).await.unwrap();
        let parent_id = parent.instance.id.clone();

        let child_cmd = SpawnCommand {
            machine: "LineItem".to_string(),
            data: vec![],
            then_transition: None,
            batch: false,
            batch_data: vec![],
            parent_id: Some(parent_id.as_str()),
            parent_machine: Some("Order".to_string()),
            as_actor: None,
        };
        let child = engine.spawn(&child_cmd).await.unwrap();
        let child_id = child.instance.id.clone();

        // Delete parent
        storage.delete_instance(&parent_id).await.unwrap();

        // Child still exists but parent is gone
        let child_inst = storage.get_instance(&child_id).await.unwrap();
        assert!(child_inst.is_some(), "Child should still exist after parent deletion");
        assert_eq!(child_inst.unwrap().parent_id.as_ref().unwrap().as_str(), parent_id.as_str());
    }
}

// ============================================================================
// Area 7: Parser Robustness — edge cases
// ============================================================================

mod parser_hardening {
    use smql_parser::{tokenize, TokenKind};

    #[test]
    fn unicode_in_string_literal() {
        let tokens = tokenize(r#""hello wörld 日本語 🎉""#).unwrap();
        assert_eq!(tokens.len(), 1);
        if let TokenKind::StringLiteral(s) = &tokens[0].kind {
            assert_eq!(s, "hello wörld 日本語 🎉");
        } else {
            panic!("Expected StringLiteral");
        }
    }

    #[test]
    fn unicode_before_identifier() {
        let tokens = tokenize("-- comment with ü\nDEFINE").unwrap();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].text, "DEFINE");
    }

    #[test]
    fn unterminated_string_returns_error() {
        let result = tokenize(r#""hello world"#);
        assert!(result.is_err(), "Unterminated string should error");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Unterminated"), "Error should mention unterminated: {}", err);
    }

    #[test]
    fn empty_input_returns_empty_tokens() {
        let tokens = tokenize("").unwrap();
        assert!(tokens.is_empty());
    }

    #[test]
    fn whitespace_only_returns_empty_tokens() {
        let tokens = tokenize("   \n\t   ").unwrap();
        assert!(tokens.is_empty());
    }

    #[test]
    fn comment_only_returns_empty_tokens() {
        let tokens = tokenize("-- just a comment").unwrap();
        assert!(tokens.is_empty());
    }

    #[test]
    fn duration_overflow_returns_error() {
        let result = tokenize("999999999999999999d");
        assert!(result.is_err(), "Overflow duration should error");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("too large") || err.contains("Duration"), "Error: {}", err);
    }

    #[test]
    fn normal_duration_still_works() {
        let tokens = tokenize("30s 5m 2h 7d").unwrap();
        assert_eq!(tokens.len(), 4);
        if let TokenKind::DurationLiteral(s) = &tokens[0].kind {
            assert_eq!(*s, 30);
        }
        if let TokenKind::DurationLiteral(s) = &tokens[1].kind {
            assert_eq!(*s, 300);
        }
        if let TokenKind::DurationLiteral(s) = &tokens[2].kind {
            assert_eq!(*s, 7200);
        }
        if let TokenKind::DurationLiteral(s) = &tokens[3].kind {
            assert_eq!(*s, 604800);
        }
    }

    #[test]
    fn escape_sequences_in_strings() {
        let tokens = tokenize(r#""hello\nworld\ttab\\back\"quote""#).unwrap();
        assert_eq!(tokens.len(), 1);
        if let TokenKind::StringLiteral(s) = &tokens[0].kind {
            assert_eq!(s, "hello\nworld\ttab\\back\"quote");
        }
    }

    #[test]
    fn negative_number_token() {
        let tokens = tokenize("-42").unwrap();
        assert_eq!(tokens.len(), 1);
        if let TokenKind::IntLiteral(v) = &tokens[0].kind {
            assert_eq!(*v, -42);
        }
    }

    #[test]
    fn float_literal_parses_correctly() {
        let tokens = tokenize("3.14").unwrap();
        assert_eq!(tokens.len(), 1);
        if let TokenKind::FloatLiteral(v) = &tokens[0].kind {
            assert!((*v - 3.14).abs() < f64::EPSILON);
        }
    }

    #[test]
    fn parse_full_machine_with_unicode_data() {
        let smql = r#"
            DEFINE MACHINE Test (
                DATA { name : TEXT }
                STATES { idle }
                INITIAL STATE idle
                TERMINAL STATES { idle }
                TRANSITIONS { idle -> idle {} }
            )
        "#;
        let result = smql_parser::parse(smql);
        assert!(result.is_ok(), "Basic machine should parse: {:?}", result.err());
    }
}

// ============================================================================
// Area 8: Hook System — edge cases
// ============================================================================

mod hooks_hardening {
    use smql_hooks::EventBus;
    use std::sync::Arc;

    #[test]
    fn event_bus_emit_with_no_subscribers_is_ok() {
        let bus = EventBus::new(16);
        // Emit with no subscribers should not panic or error
        bus.emit(smql_hooks::Event {
            name: "test_event".to_string(),
            instance_id: "inst_1".to_string(),
            machine: "Machine".to_string(),
            payload: None,
        });
        // No panic = pass
    }

    #[test]
    fn event_bus_subscribe_and_emit() {
        let bus = Arc::new(EventBus::new(16));
        let mut rx = bus.subscribe();

        bus.emit(smql_hooks::Event {
            name: "test_event".to_string(),
            instance_id: "inst_1".to_string(),
            machine: "Machine".to_string(),
            payload: Some(smql_ast::value::Value::Text("hello".to_string())),
        });

        // Should receive the event
        let event = rx.try_recv().unwrap();
        assert_eq!(event.name, "test_event");
        assert_eq!(event.instance_id, "inst_1");
    }

    #[test]
    fn event_bus_multiple_subscribers() {
        let bus = Arc::new(EventBus::new(16));
        let mut rx1 = bus.subscribe();
        let mut rx2 = bus.subscribe();

        bus.emit(smql_hooks::Event {
            name: "test_event".to_string(),
            instance_id: "inst_1".to_string(),
            machine: "Machine".to_string(),
            payload: None,
        });

        // Both subscribers should get the event
        assert!(rx1.try_recv().is_ok());
        assert!(rx2.try_recv().is_ok());
    }

    #[test]
    fn hook_context_creation() {
        use smql_hooks::HookContext;
        use smql_ast::value::Value;
        use std::collections::HashMap;

        let mut data = HashMap::new();
        data.insert("title".to_string(), Value::Text("Bug report".to_string()));

        let ctx = HookContext {
            instance_id: "inst_1".to_string(),
            machine: "Ticket".to_string(),
            from_state: "open".to_string(),
            to_state: "in_progress".to_string(),
            data,
            actor: None,
            memo: None,
        };

        assert_eq!(ctx.machine, "Ticket");
        assert_eq!(ctx.from_state, "open");
        assert_eq!(ctx.to_state, "in_progress");
        assert!(ctx.data.contains_key("title"));
    }
}

// ============================================================================
// Area 9: Server — error format consistency
// ============================================================================

mod server_hardening {
    #[test]
    fn error_response_mapping() {
        use smql_ast::SmqlError;

        // Not found → 404
        let nf = SmqlError::NotFound {
            entity_type: "instance".to_string(),
            id: "123".to_string(),
        };
        assert!(matches!(nf, SmqlError::NotFound { .. }));

        // Transition denied → 409
        let td = SmqlError::TransitionDenied(smql_ast::TransitionDeniedError {
            from_state: "open".to_string(),
            to_state: "closed".to_string(),
            instance_id: "123".to_string(),
            guard_failures: vec![],
            hint: None,
            recovery_options: vec![],
            llm_prompt: None,
        });
        assert!(matches!(td, SmqlError::TransitionDenied(_)));

        // Validation error → 400
        let ve = SmqlError::ValidationError {
            message: "Required field missing".to_string(),
            field: Some("name".to_string()),
            hint: None,
        };
        assert!(matches!(ve, SmqlError::ValidationError { .. }));
    }
}

// ============================================================================
// Area 10: SDK/Codegen — edge cases
// ============================================================================

mod codegen_hardening {
    #[test]
    fn codegen_escapes_rust_keywords() {
        use smql_codegen::rust_gen::generate_machine_module;
        use smql_ast::machine::{MachineDefinition, StateDefinition};
        use smql_ast::types::{DataFieldDefinition, TypeDefinition};

        let mut m = MachineDefinition::new("Test".to_string(), "idle".to_string());
        m.states.push(StateDefinition::new("idle".to_string()));
        m.terminal_states.push("idle".to_string());
        m.data.push(DataFieldDefinition {
            name: "type".to_string(),
            field_type: TypeDefinition::Text,
            constraints: vec![],
        });
        m.data.push(DataFieldDefinition {
            name: "return".to_string(),
            field_type: TypeDefinition::Int,
            constraints: vec![],
        });

        let output = generate_machine_module(&m);
        assert!(output.contains("r#type"), "Should escape 'type' keyword: {}", output);
        assert!(output.contains("r#return"), "Should escape 'return' keyword: {}", output);
    }

    #[test]
    fn codegen_normal_fields_not_escaped() {
        use smql_codegen::rust_gen::generate_machine_module;
        use smql_ast::machine::{MachineDefinition, StateDefinition};
        use smql_ast::types::{DataFieldDefinition, TypeDefinition};

        let mut m = MachineDefinition::new("Test".to_string(), "idle".to_string());
        m.states.push(StateDefinition::new("idle".to_string()));
        m.terminal_states.push("idle".to_string());
        m.data.push(DataFieldDefinition {
            name: "name".to_string(),
            field_type: TypeDefinition::Text,
            constraints: vec![],
        });

        let output = generate_machine_module(&m);
        // Normal identifiers should NOT get r# prefix
        assert!(output.contains("pub name:"), "Normal field should not be escaped: {}", output);
        assert!(!output.contains("r#name"));
    }
}
