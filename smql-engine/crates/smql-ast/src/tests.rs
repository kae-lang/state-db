#[cfg(test)]
mod types_tests {
    use crate::types::*;

    #[test]
    fn type_definition_display() {
        assert_eq!(TypeDefinition::Text.to_string(), "TEXT");
        assert_eq!(TypeDefinition::Int.to_string(), "INT");
        assert_eq!(TypeDefinition::Float.to_string(), "FLOAT");
        assert_eq!(TypeDefinition::Bool.to_string(), "BOOL");
        assert_eq!(TypeDefinition::Uuid.to_string(), "UUID");
        assert_eq!(TypeDefinition::Date.to_string(), "DATE");
        assert_eq!(TypeDefinition::DateTime.to_string(), "DATETIME");
        assert_eq!(TypeDefinition::Duration.to_string(), "DURATION");
        assert_eq!(TypeDefinition::Blob.to_string(), "BLOB");
        assert_eq!(TypeDefinition::Json.to_string(), "JSON");
    }

    #[test]
    fn type_definition_enum_display() {
        let e = TypeDefinition::Enum(vec!["low".into(), "medium".into(), "high".into()]);
        assert_eq!(e.to_string(), "ENUM(low, medium, high)");
    }

    #[test]
    fn type_definition_ref_display() {
        let r = TypeDefinition::Ref("Agent".into());
        assert_eq!(r.to_string(), "REF(Agent)");
    }

    #[test]
    fn type_definition_money_display() {
        let m = TypeDefinition::Money("USD".into());
        assert_eq!(m.to_string(), "MONEY(USD)");
    }

    #[test]
    fn type_definition_nested_display() {
        let l = TypeDefinition::List(Box::new(TypeDefinition::Text));
        assert_eq!(l.to_string(), "LIST(TEXT)");

        let s = TypeDefinition::Set(Box::new(TypeDefinition::Int));
        assert_eq!(s.to_string(), "SET(INT)");

        let m = TypeDefinition::Map(
            Box::new(TypeDefinition::Text),
            Box::new(TypeDefinition::Int),
        );
        assert_eq!(m.to_string(), "MAP(TEXT, INT)");
    }

    #[test]
    fn type_definition_clone_eq() {
        let t1 = TypeDefinition::Enum(vec!["a".into(), "b".into()]);
        let t2 = t1.clone();
        assert_eq!(t1, t2);

        let t3 = TypeDefinition::Text;
        assert_ne!(t1, t3);
    }

    #[test]
    fn type_definition_serde() {
        let t = TypeDefinition::List(Box::new(TypeDefinition::Ref("Machine".into())));
        let json = serde_json::to_string(&t).unwrap();
        let t2: TypeDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(t, t2);
    }

    #[test]
    fn constraint_display() {
        assert_eq!(Constraint::Required.to_string(), "REQUIRED");
        assert_eq!(Constraint::Optional.to_string(), "OPTIONAL");
        assert_eq!(Constraint::Max(200).to_string(), "MAX(200)");
        assert_eq!(Constraint::Min(1).to_string(), "MIN(1)");
        assert_eq!(Constraint::Range(1, 5).to_string(), "RANGE(1, 5)");
        assert_eq!(Constraint::Unique.to_string(), "UNIQUE");
        assert_eq!(
            Constraint::Pattern("^[A-Z]+$".into()).to_string(),
            "PATTERN(^[A-Z]+$)"
        );
    }

    #[test]
    fn default_value_display_all_variants() {
        assert_eq!(DefaultValue::String("hello".into()).to_string(), "\"hello\"");
        assert_eq!(DefaultValue::Int(42).to_string(), "42");
        assert_eq!(DefaultValue::Float(3.14).to_string(), "3.14");
        assert_eq!(DefaultValue::Bool(true).to_string(), "true");
        assert_eq!(DefaultValue::EmptySet.to_string(), "{}");
        assert_eq!(DefaultValue::EmptyList.to_string(), "[]");
        assert_eq!(DefaultValue::EmptyMap.to_string(), "{}");
        assert_eq!(DefaultValue::Null.to_string(), "NULL");
    }

    #[test]
    fn data_field_definition_no_constraints() {
        let field = DataFieldDefinition {
            name: "name".into(),
            field_type: TypeDefinition::Text,
            constraints: vec![],
        };
        assert_eq!(field.to_string(), "name : TEXT");
    }

    #[test]
    fn data_field_definition_multiple_constraints() {
        let field = DataFieldDefinition {
            name: "score".into(),
            field_type: TypeDefinition::Int,
            constraints: vec![
                Constraint::Required,
                Constraint::Min(0),
                Constraint::Max(100),
            ],
        };
        assert_eq!(
            field.to_string(),
            "score : INT -> REQUIRED, MIN(0), MAX(100)"
        );
    }

    #[test]
    fn data_field_definition_display() {
        let field = DataFieldDefinition {
            name: "priority".into(),
            field_type: TypeDefinition::Enum(vec!["low".into(), "high".into()]),
            constraints: vec![Constraint::Default(DefaultValue::String("low".into()))],
        };
        assert_eq!(
            field.to_string(),
            "priority : ENUM(low, high) -> DEFAULT(\"low\")"
        );
    }

    #[test]
    fn data_field_serde() {
        let field = DataFieldDefinition {
            name: "count".into(),
            field_type: TypeDefinition::Int,
            constraints: vec![Constraint::Required, Constraint::Min(0)],
        };
        let json = serde_json::to_string(&field).unwrap();
        let field2: DataFieldDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(field, field2);
    }

    #[test]
    fn sort_direction_display() {
        assert_eq!(SortDirection::Asc.to_string(), "ASC");
        assert_eq!(SortDirection::Desc.to_string(), "DESC");
    }

    #[test]
    fn aggregate_function_display() {
        assert_eq!(AggregateFunction::Count.to_string(), "COUNT");
        assert_eq!(AggregateFunction::Sum.to_string(), "SUM");
        assert_eq!(AggregateFunction::Avg.to_string(), "AVG");
        assert_eq!(AggregateFunction::Min.to_string(), "MIN");
        assert_eq!(AggregateFunction::Max.to_string(), "MAX");
        assert_eq!(
            AggregateFunction::Percentile(95.0).to_string(),
            "PERCENTILE(95)"
        );
    }
}

#[cfg(test)]
mod value_tests {
    use crate::value::*;
    use chrono::Utc;

    #[test]
    fn value_display_primitives() {
        assert_eq!(Value::Text("hello".into()).to_string(), "\"hello\"");
        assert_eq!(Value::Int(42).to_string(), "42");
        assert_eq!(Value::Float(3.125).to_string(), "3.125");
        assert_eq!(Value::Bool(true).to_string(), "true");
        assert_eq!(Value::Null.to_string(), "NULL");
    }

    #[test]
    fn value_display_money() {
        assert_eq!(Value::Money(1999, "USD".into()).to_string(), "19.99 USD");
        assert_eq!(Value::Money(100, "EUR".into()).to_string(), "1.00 EUR");
        assert_eq!(Value::Money(0, "GBP".into()).to_string(), "0.00 GBP");
    }

    #[test]
    fn value_display_collections() {
        let list = Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        assert_eq!(list.to_string(), "[1, 2, 3]");

        let set = Value::Set(vec![Value::Text("a".into()), Value::Text("b".into())]);
        assert_eq!(set.to_string(), "{\"a\", \"b\"}");
    }

    #[test]
    fn value_display_ref() {
        assert_eq!(
            Value::Ref("Agent".into(), "ag_123".into()).to_string(),
            "Agent#ag_123"
        );
    }

    #[test]
    fn value_display_blob() {
        assert_eq!(Value::Blob(vec![0, 1, 2]).to_string(), "<blob 3 bytes>");
    }

    #[test]
    fn value_serde_roundtrip() {
        let values = vec![
            Value::Text("test".into()),
            Value::Int(42),
            Value::Float(3.125),
            Value::Bool(false),
            Value::Null,
            Value::List(vec![Value::Int(1)]),
            Value::Money(999, "USD".into()),
        ];
        for v in values {
            let json = serde_json::to_string(&v).unwrap();
            let v2: Value = serde_json::from_str(&json).unwrap();
            assert_eq!(v, v2);
        }
    }

    #[test]
    fn value_clone_eq() {
        let v1 = Value::Text("hello".into());
        let v2 = v1.clone();
        assert_eq!(v1, v2);
        assert_ne!(v1, Value::Int(0));
    }

    #[test]
    fn duration_display() {
        assert_eq!(SmqlDuration::from_seconds(0).to_string(), "0s");
        assert_eq!(SmqlDuration::from_seconds(30).to_string(), "30s");
        assert_eq!(SmqlDuration::from_minutes(5).to_string(), "5m");
        assert_eq!(SmqlDuration::from_hours(24).to_string(), "1d");
        assert_eq!(SmqlDuration::from_days(7).to_string(), "7d");
        assert_eq!(SmqlDuration::from_seconds(90061).to_string(), "1d1h1m1s");
    }

    #[test]
    fn duration_ordering() {
        let a = SmqlDuration::from_hours(1);
        let b = SmqlDuration::from_hours(2);
        assert!(a < b);
        assert!(b > a);
        assert_eq!(a, SmqlDuration::from_minutes(60));
    }

    #[test]
    fn duration_serde() {
        let d = SmqlDuration::from_hours(72);
        let json = serde_json::to_string(&d).unwrap();
        let d2: SmqlDuration = serde_json::from_str(&json).unwrap();
        assert_eq!(d, d2);
    }

    #[test]
    fn duration_to_std() {
        let d = SmqlDuration::from_seconds(100);
        assert_eq!(d.to_std(), std::time::Duration::from_secs(100));
    }

    #[test]
    fn value_datetime() {
        let now = Utc::now();
        let v = Value::DateTime(now);
        let s = v.to_string();
        assert!(s.contains("T")); // RFC3339 format
    }

    #[test]
    fn value_display_uuid() {
        let id = uuid::Uuid::nil();
        let v = Value::Uuid(id);
        assert_eq!(v.to_string(), "00000000-0000-0000-0000-000000000000");
    }

    #[test]
    fn value_display_date() {
        let d = chrono::NaiveDate::from_ymd_opt(2025, 6, 15).unwrap();
        let v = Value::Date(d);
        assert_eq!(v.to_string(), "2025-06-15");
    }

    #[test]
    fn value_display_json() {
        let j = serde_json::json!({"key": "value", "num": 42});
        let v = Value::Json(j);
        let s = v.to_string();
        assert!(s.contains("key"));
        assert!(s.contains("value"));
    }

    #[test]
    fn value_display_map() {
        let mut m = std::collections::BTreeMap::new();
        m.insert("a".into(), Value::Int(1));
        m.insert("b".into(), Value::Text("two".into()));
        let v = Value::Map(m);
        assert_eq!(v.to_string(), "{a: 1, b: \"two\"}");
    }

    #[test]
    fn value_display_empty_collections() {
        assert_eq!(Value::List(vec![]).to_string(), "[]");
        assert_eq!(Value::Set(vec![]).to_string(), "{}");
        assert_eq!(Value::Map(std::collections::BTreeMap::new()).to_string(), "{}");
    }

    #[test]
    fn value_display_duration() {
        let d = SmqlDuration::from_hours(2);
        let v = Value::Duration(d);
        assert_eq!(v.to_string(), "2h");
    }

    #[test]
    fn value_display_blob_empty() {
        assert_eq!(Value::Blob(vec![]).to_string(), "<blob 0 bytes>");
    }

    #[test]
    fn value_display_money_negative() {
        assert_eq!(Value::Money(-1050, "USD".into()).to_string(), "-10.50 USD");
    }

    #[test]
    fn duration_display_days_minutes_no_hours() {
        let d = SmqlDuration::from_seconds(86400 + 300); // 1d5m
        assert_eq!(d.to_string(), "1d5m");
    }

    #[test]
    fn duration_display_hours_seconds_no_minutes() {
        let d = SmqlDuration::from_seconds(3600 + 5); // 1h5s
        assert_eq!(d.to_string(), "1h5s");
    }

    #[test]
    fn value_serde_roundtrip_extended() {
        use std::collections::BTreeMap;
        let values = vec![
            Value::Uuid(uuid::Uuid::nil()),
            Value::Date(chrono::NaiveDate::from_ymd_opt(2025, 1, 1).unwrap()),
            Value::DateTime(Utc::now()),
            Value::Duration(SmqlDuration::from_hours(1)),
            Value::Set(vec![Value::Int(1), Value::Int(2)]),
            Value::Map({
                let mut m = BTreeMap::new();
                m.insert("k".into(), Value::Bool(true));
                m
            }),
            Value::Ref("Machine".into(), "id_1".into()),
            Value::Blob(vec![0, 1, 2, 3]),
            Value::Json(serde_json::json!({"a": 1})),
        ];
        for v in values {
            let json = serde_json::to_string(&v).unwrap();
            let v2: Value = serde_json::from_str(&json).unwrap();
            assert_eq!(v, v2);
        }
    }
}

#[cfg(test)]
mod expression_tests {
    use crate::expression::*;
    use crate::value::Value;

    #[test]
    fn literal_display() {
        let e = Expression::new(ExpressionKind::Literal(Value::Int(42)));
        assert_eq!(e.to_string(), "42");
    }

    #[test]
    fn field_access_display() {
        let e = Expression::new(ExpressionKind::FieldAccess(vec!["priority".into()]));
        assert_eq!(e.to_string(), "priority");

        let e2 = Expression::new(ExpressionKind::FieldAccess(vec![
            "a".into(),
            "b".into(),
            "c".into(),
        ]));
        assert_eq!(e2.to_string(), "a.b.c");
    }

    #[test]
    fn binary_op_display() {
        let e = Expression::new(ExpressionKind::BinaryOp {
            left: Box::new(Expression::new(ExpressionKind::FieldAccess(vec![
                "x".into()
            ]))),
            op: BinaryOperator::Gt,
            right: Box::new(Expression::new(ExpressionKind::Literal(Value::Int(10)))),
        });
        assert_eq!(e.to_string(), "(x > 10)");
    }

    #[test]
    fn binary_operator_display() {
        assert_eq!(BinaryOperator::Eq.to_string(), "==");
        assert_eq!(BinaryOperator::NotEq.to_string(), "!=");
        assert_eq!(BinaryOperator::Lt.to_string(), "<");
        assert_eq!(BinaryOperator::Gt.to_string(), ">");
        assert_eq!(BinaryOperator::LtEq.to_string(), "<=");
        assert_eq!(BinaryOperator::GtEq.to_string(), ">=");
        assert_eq!(BinaryOperator::Add.to_string(), "+");
        assert_eq!(BinaryOperator::Sub.to_string(), "-");
        assert_eq!(BinaryOperator::Mul.to_string(), "*");
        assert_eq!(BinaryOperator::Div.to_string(), "/");
        assert_eq!(BinaryOperator::And.to_string(), "AND");
        assert_eq!(BinaryOperator::Or.to_string(), "OR");
    }

    #[test]
    fn unary_operator_display() {
        assert_eq!(UnaryOperator::Not.to_string(), "NOT");
        assert_eq!(UnaryOperator::Neg.to_string(), "-");
    }

    #[test]
    fn state_predicates_display() {
        let e = Expression::new(ExpressionKind::StateIs("open".into()));
        assert_eq!(e.to_string(), "STATE IS open");

        let e2 = Expression::new(ExpressionKind::StateIn(vec![
            "open".into(),
            "triaged".into(),
        ]));
        assert_eq!(e2.to_string(), "STATE IN {open, triaged}");
    }

    #[test]
    fn is_set_display() {
        let field = Expression::new(ExpressionKind::FieldAccess(vec!["assignee".into()]));
        let e = Expression::new(ExpressionKind::IsSet(Box::new(field)));
        assert_eq!(e.to_string(), "assignee IS SET");
    }

    #[test]
    fn function_call_display() {
        let e = Expression::new(ExpressionKind::FunctionCall {
            name: "elapsed".into(),
            args: vec![],
        });
        assert_eq!(e.to_string(), "elapsed()");
    }

    #[test]
    fn expression_serde() {
        let e = Expression::new(ExpressionKind::BinaryOp {
            left: Box::new(Expression::new(ExpressionKind::FieldAccess(vec![
                "x".into()
            ]))),
            op: BinaryOperator::Eq,
            right: Box::new(Expression::new(ExpressionKind::Literal(Value::Text(
                "test".into(),
            )))),
        });
        let json = serde_json::to_string(&e).unwrap();
        let e2: Expression = serde_json::from_str(&json).unwrap();
        assert_eq!(e, e2);
    }

    #[test]
    fn expression_clone_eq() {
        let e1 = Expression::new(ExpressionKind::SelfRef);
        let e2 = e1.clone();
        assert_eq!(e1, e2);
    }

    #[test]
    fn expression_with_span() {
        use crate::span::Span;
        let e = Expression::with_span(ExpressionKind::ActorRef, Span::new(0, 5));
        assert_eq!(e.span, Some(Span::new(0, 5)));
    }

    #[test]
    fn qualified_access_display() {
        let e = Expression::new(ExpressionKind::QualifiedAccess {
            root: Box::new(Expression::new(ExpressionKind::SelfRef)),
            path: vec!["data".into(), "name".into()],
        });
        assert_eq!(e.to_string(), "SELF.data.name");
    }

    #[test]
    fn qualified_access_actor_display() {
        let e = Expression::new(ExpressionKind::QualifiedAccess {
            root: Box::new(Expression::new(ExpressionKind::ActorRef)),
            path: vec!["role".into()],
        });
        assert_eq!(e.to_string(), "ACTOR.role");
    }

    #[test]
    fn signal_from_display() {
        let e = Expression::new(ExpressionKind::SignalFrom {
            machine: "ChildMachine".into(),
            condition: Box::new(Expression::new(ExpressionKind::StateIs("done".into()))),
        });
        assert_eq!(e.to_string(), "SIGNAL FROM ChildMachine WHERE STATE IS done");
    }

    #[test]
    fn pattern_display() {
        let e = Expression::new(ExpressionKind::Pattern("^[A-Z]+$".into()));
        assert_eq!(e.to_string(), "PATTERN(^[A-Z]+$)");
    }

    #[test]
    fn is_not_set_display() {
        let field = Expression::new(ExpressionKind::FieldAccess(vec!["email".into()]));
        let e = Expression::new(ExpressionKind::IsNotSet(Box::new(field)));
        assert_eq!(e.to_string(), "email IS NOT SET");
    }

    #[test]
    fn in_set_display() {
        let e = Expression::new(ExpressionKind::InSet {
            expr: Box::new(Expression::new(ExpressionKind::FieldAccess(vec![
                "status".into(),
            ]))),
            values: vec![
                Expression::new(ExpressionKind::Literal(Value::Text("a".into()))),
                Expression::new(ExpressionKind::Literal(Value::Text("b".into()))),
            ],
        });
        assert_eq!(e.to_string(), "status IN {\"a\", \"b\"}");
    }

    #[test]
    fn in_list_display() {
        let e = Expression::new(ExpressionKind::InList {
            expr: Box::new(Expression::new(ExpressionKind::FieldAccess(vec![
                "tag".into(),
            ]))),
            values: vec![
                Expression::new(ExpressionKind::Literal(Value::Text("x".into()))),
                Expression::new(ExpressionKind::Literal(Value::Text("y".into()))),
            ],
        });
        assert_eq!(e.to_string(), "tag IN (\"x\", \"y\")");
    }

    #[test]
    fn all_any_count_display() {
        let collection = Expression::new(ExpressionKind::FieldAccess(vec!["items".into()]));
        let predicate = Expression::new(ExpressionKind::BinaryOp {
            left: Box::new(Expression::new(ExpressionKind::FieldAccess(vec![
                "x".into(),
            ]))),
            op: BinaryOperator::Gt,
            right: Box::new(Expression::new(ExpressionKind::Literal(Value::Int(0)))),
        });

        let all = Expression::new(ExpressionKind::All {
            collection: Box::new(collection.clone()),
            predicate: Box::new(predicate.clone()),
        });
        assert_eq!(all.to_string(), "ALL(items, (x > 0))");

        let any = Expression::new(ExpressionKind::Any {
            collection: Box::new(collection),
            predicate: Box::new(predicate),
        });
        assert_eq!(any.to_string(), "ANY(items, (x > 0))");

        let count_with = Expression::new(ExpressionKind::Count(Some(Box::new(
            Expression::new(ExpressionKind::FieldAccess(vec!["children".into()])),
        ))));
        assert_eq!(count_with.to_string(), "COUNT(children)");

        let count_empty = Expression::new(ExpressionKind::Count(None));
        assert_eq!(count_empty.to_string(), "COUNT()");
    }

    #[test]
    fn duration_literal_display() {
        use crate::value::SmqlDuration;
        let e = Expression::new(ExpressionKind::DurationLiteral(SmqlDuration::from_hours(24)));
        assert_eq!(e.to_string(), "1d");
    }

    #[test]
    fn unary_op_display() {
        let e = Expression::new(ExpressionKind::UnaryOp {
            op: UnaryOperator::Not,
            operand: Box::new(Expression::new(ExpressionKind::FieldAccess(vec![
                "active".into(),
            ]))),
        });
        assert_eq!(e.to_string(), "(NOT active)");

        let neg = Expression::new(ExpressionKind::UnaryOp {
            op: UnaryOperator::Neg,
            operand: Box::new(Expression::new(ExpressionKind::Literal(Value::Int(5)))),
        });
        assert_eq!(neg.to_string(), "(- 5)");
    }

    #[test]
    fn function_call_with_args_display() {
        let e = Expression::new(ExpressionKind::FunctionCall {
            name: "MAX".into(),
            args: vec![
                Expression::new(ExpressionKind::FieldAccess(vec!["a".into()])),
                Expression::new(ExpressionKind::Literal(Value::Int(10))),
            ],
        });
        assert_eq!(e.to_string(), "MAX(a, 10)");
    }

    #[test]
    fn expression_serde_extended() {
        let exprs = vec![
            Expression::new(ExpressionKind::QualifiedAccess {
                root: Box::new(Expression::new(ExpressionKind::SelfRef)),
                path: vec!["field".into()],
            }),
            Expression::new(ExpressionKind::SignalFrom {
                machine: "M".into(),
                condition: Box::new(Expression::new(ExpressionKind::Literal(Value::Bool(true)))),
            }),
            Expression::new(ExpressionKind::Pattern(".*".into())),
            Expression::new(ExpressionKind::InSet {
                expr: Box::new(Expression::new(ExpressionKind::FieldAccess(vec![
                    "x".into(),
                ]))),
                values: vec![Expression::new(ExpressionKind::Literal(Value::Int(1)))],
            }),
            Expression::new(ExpressionKind::Count(None)),
            Expression::new(ExpressionKind::DurationLiteral(
                crate::value::SmqlDuration::from_minutes(5),
            )),
        ];
        for e in exprs {
            let json = serde_json::to_string(&e).unwrap();
            let e2: Expression = serde_json::from_str(&json).unwrap();
            assert_eq!(e, e2);
        }
    }
}

#[cfg(test)]
mod machine_tests {
    use crate::machine::*;

    #[test]
    fn machine_definition_new() {
        let m = MachineDefinition::new("SupportTicket".into(), "open".into());
        assert_eq!(m.name, "SupportTicket");
        assert_eq!(m.initial_state, "open");
        assert_eq!(m.version, 1);
        assert!(m.states.is_empty());
        assert!(m.transitions.is_empty());
        assert!(m.terminal_states.is_empty());
    }

    #[test]
    fn machine_definition_display() {
        let m = MachineDefinition::new("Order".into(), "draft".into());
        assert_eq!(m.to_string(), "MACHINE Order (v1)");
    }

    #[test]
    fn state_definition_new() {
        let s = StateDefinition::new("open".into());
        assert_eq!(s.name, "open");
        assert!(s.metadata.is_empty());
    }

    #[test]
    fn state_definition_display() {
        let s = StateDefinition::new("in_progress".into());
        assert_eq!(s.to_string(), "in_progress");
    }

    #[test]
    fn transition_source_display() {
        assert_eq!(TransitionSource::State("open".into()).to_string(), "open");
        assert_eq!(TransitionSource::Any { except: vec![] }.to_string(), "ANY");
        assert_eq!(
            TransitionSource::Any {
                except: vec!["closed".into(), "delivered".into()]
            }
            .to_string(),
            "ANY EXCEPT FROM {closed, delivered}"
        );
        assert_eq!(
            TransitionSource::Group("active".into()).to_string(),
            "GROUP(active)"
        );
    }

    #[test]
    fn transition_definition_new() {
        let t = TransitionDefinition::new(TransitionSource::State("open".into()), "triaged".into());
        assert_eq!(t.from, TransitionSource::State("open".into()));
        assert_eq!(t.to, "triaged");
        assert!(t.guards.is_empty());
        assert!(t.actions.is_empty());
        assert!(t.timeout.is_none());
    }

    #[test]
    fn transition_definition_display() {
        let t = TransitionDefinition::new(TransitionSource::State("open".into()), "triaged".into());
        assert_eq!(t.to_string(), "open -> triaged");
    }

    #[test]
    fn timeout_clause_display() {
        use crate::value::SmqlDuration;
        let tc = TimeoutClause {
            duration: SmqlDuration::from_hours(72),
            target_state: "resolved".into(),
        };
        assert_eq!(tc.to_string(), "TIMEOUT: 3d -> resolved");
    }

    #[test]
    fn action_display() {
        assert_eq!(
            Action::Log("test message".into()).to_string(),
            "LOG(\"test message\")"
        );
        assert_eq!(
            Action::SignalParent {
                target_state: "delivered".into()
            }
            .to_string(),
            "SIGNAL PARENT TO delivered"
        );
    }

    #[test]
    fn child_cardinality_display() {
        assert_eq!(
            ChildCardinality::List {
                min: Some(1),
                max: None
            }
            .to_string(),
            "LIST(1+)"
        );
        assert_eq!(ChildCardinality::Optional.to_string(), "OPTIONAL");
        assert_eq!(ChildCardinality::Required.to_string(), "REQUIRED");
    }

    #[test]
    fn hook_trigger_display() {
        assert_eq!(HookTrigger::OnSpawn.to_string(), "ON SPAWN");
        assert_eq!(
            HookTrigger::BeforeEachTransition.to_string(),
            "BEFORE EACH TRANSITION"
        );
        assert_eq!(
            HookTrigger::AfterEachTransition.to_string(),
            "AFTER EACH TRANSITION"
        );
        assert_eq!(
            HookTrigger::OnEnter("open".into()).to_string(),
            "ON ENTER open"
        );
        assert_eq!(
            HookTrigger::OnExit("closed".into()).to_string(),
            "ON EXIT closed"
        );
        assert_eq!(
            HookTrigger::OnDwell {
                state: "pending".into(),
                duration: crate::value::SmqlDuration::from_hours(2),
            }
            .to_string(),
            "ON DWELL(pending, > 2h)"
        );
    }

    #[test]
    fn child_cardinality_display_all_variants() {
        assert_eq!(
            ChildCardinality::List {
                min: Some(1),
                max: Some(10)
            }
            .to_string(),
            "LIST(1..10)"
        );
        assert_eq!(
            ChildCardinality::List {
                min: None,
                max: Some(5)
            }
            .to_string(),
            "LIST(..5)"
        );
        assert_eq!(
            ChildCardinality::List {
                min: None,
                max: None
            }
            .to_string(),
            "LIST"
        );
    }

    #[test]
    fn action_display_all_variants() {
        use crate::expression::{Expression, ExpressionKind};
        use crate::value::Value;

        assert_eq!(
            Action::Notify {
                target: Expression::new(ExpressionKind::FieldAccess(vec!["assignee".into()])),
                event: "assigned".into(),
            }
            .to_string(),
            "NOTIFY(assignee, \"assigned\")"
        );

        assert_eq!(
            Action::Emit {
                event: "ticket_created".into(),
                payload: None,
            }
            .to_string(),
            "EMIT(\"ticket_created\")"
        );

        assert_eq!(
            Action::Webhook {
                url: "https://example.com/hook".into(),
                payload: Some(Expression::new(ExpressionKind::Literal(Value::Int(1)))),
            }
            .to_string(),
            "WEBHOOK(\"https://example.com/hook\")"
        );

        assert_eq!(
            Action::SpawnChild {
                machine: "SubTask".into(),
                data: vec![],
            }
            .to_string(),
            "SPAWN SubTask"
        );
    }

    #[test]
    fn role_permission_variants() {
        let r = RoleDefinition {
            name: "admin".into(),
            permissions: vec![
                RolePermission::CanSpawn,
                RolePermission::CanTransition(vec!["open".into(), "close".into()]),
                RolePermission::CanQuery,
                RolePermission::CanAlter,
            ],
        };
        assert_eq!(r.name, "admin");
        assert_eq!(r.permissions.len(), 4);

        let json = serde_json::to_string(&r).unwrap();
        let r2: RoleDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(r, r2);
    }

    #[test]
    fn hook_definition_serde() {
        let h = HookDefinition {
            trigger: HookTrigger::OnSpawn,
            actions: vec![Action::Log("spawned".into())],
        };
        let json = serde_json::to_string(&h).unwrap();
        let h2: HookDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(h, h2);
    }

    #[test]
    fn child_definition_serde() {
        let c = ChildDefinition {
            name: "items".into(),
            machine: "OrderItem".into(),
            cardinality: ChildCardinality::List {
                min: Some(1),
                max: None,
            },
        };
        let json = serde_json::to_string(&c).unwrap();
        let c2: ChildDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(c, c2);
    }

    #[test]
    fn mutate_clause_display() {
        use crate::expression::{Expression, ExpressionKind};
        use crate::value::Value;
        let m = MutateClause {
            field: "count".into(),
            value: Expression::new(ExpressionKind::Literal(Value::Int(1))),
        };
        assert_eq!(m.to_string(), "count = 1");
    }

    #[test]
    fn machine_definition_serde() {
        let mut m = MachineDefinition::new("Test".into(), "init".into());
        m.states.push(StateDefinition::new("init".into()));
        m.terminal_states.push("done".into());
        let json = serde_json::to_string(&m).unwrap();
        let m2: MachineDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(m, m2);
    }
}

#[cfg(test)]
mod query_tests {
    use crate::query::*;

    #[test]
    fn get_query_display() {
        let q = Query::Get(GetQuery {
            machine: "SupportTicket".into(),
            instance_id: "tk_123".into(),
            as_actor: None,
        });
        assert_eq!(q.to_string(), "GET SupportTicket tk_123");
    }

    #[test]
    fn find_query_display() {
        let q = Query::Find(FindQuery {
            machine: "Order".into(),
            filter: None,
            sort: vec![],
            limit: Some(10),
            offset: None,
            after: None,
            as_actor: None,
        });
        assert_eq!(q.to_string(), "FIND Order");
    }

    #[test]
    fn trail_query_display() {
        let q = Query::Trail(TrailQuery {
            machine: Some("SupportTicket".into()),
            instance_id: "tk_123".into(),
            filter: None,
        });
        assert_eq!(q.to_string(), "TRAIL OF tk_123");
    }

    #[test]
    fn funnel_query_display() {
        let q = Query::Funnel(FunnelQuery {
            machine: "Order".into(),
            states: vec!["draft".into(), "placed".into(), "paid".into()],
            filter: None,
        });
        assert_eq!(q.to_string(), "FUNNEL Order");
    }

    #[test]
    fn query_serde() {
        let q = Query::Get(GetQuery {
            machine: "Test".into(),
            instance_id: "id_1".into(),
            as_actor: None,
        });
        let json = serde_json::to_string(&q).unwrap();
        let q2: Query = serde_json::from_str(&json).unwrap();
        assert_eq!(q, q2);
    }

    #[test]
    fn aggregate_query_display() {
        let q = Query::Aggregate(AggregateQuery {
            machine: "Order".into(),
            measures: vec![],
            filter: None,
            group_by: vec![],
        });
        assert_eq!(q.to_string(), "AGGREGATE Order");
    }

    #[test]
    fn paths_query_display() {
        let q = Query::Paths(PathsQuery {
            machine: "Order".into(),
            filter: None,
            limit: Some(10),
        });
        assert_eq!(q.to_string(), "PATHS FROM Order");
    }

    #[test]
    fn compare_paths_query_display() {
        let q = Query::ComparePaths(ComparePathsQuery {
            machine: "Order".into(),
            segment_by: "region".into(),
            filter: None,
        });
        assert_eq!(q.to_string(), "COMPARE PATHS Order");
    }

    #[test]
    fn group_by_clause_serde() {
        let clauses = vec![
            GroupByClause::Field("priority".into()),
            GroupByClause::State,
            GroupByClause::TimeBucket {
                field: "created_at".into(),
                interval: "1h".into(),
            },
        ];
        for c in clauses {
            let json = serde_json::to_string(&c).unwrap();
            let c2: GroupByClause = serde_json::from_str(&json).unwrap();
            assert_eq!(c, c2);
        }
    }

    #[test]
    fn trail_filter_serde() {
        let f = TrailFilter {
            actor: Some("admin".into()),
            from_state: Some("open".into()),
            to_state: Some("closed".into()),
            since: None,
            until: None,
        };
        let json = serde_json::to_string(&f).unwrap();
        let f2: TrailFilter = serde_json::from_str(&json).unwrap();
        assert_eq!(f, f2);
    }

    #[test]
    fn measure_clause_serde() {
        use crate::types::AggregateFunction;
        let m = MeasureClause {
            function: AggregateFunction::Percentile(95.0),
            field: Some("duration".into()),
            alias: Some("p95".into()),
        };
        let json = serde_json::to_string(&m).unwrap();
        let m2: MeasureClause = serde_json::from_str(&json).unwrap();
        assert_eq!(m, m2);
    }

    #[test]
    fn sort_clause_serde() {
        use crate::types::{SortClause, SortDirection};
        let s = SortClause {
            field: "created_at".into(),
            direction: SortDirection::Desc,
        };
        let json = serde_json::to_string(&s).unwrap();
        let s2: SortClause = serde_json::from_str(&json).unwrap();
        assert_eq!(s, s2);
    }

    #[test]
    fn find_query_with_all_options() {
        let q = FindQuery {
            machine: "Order".into(),
            filter: None,
            sort: vec![crate::types::SortClause {
                field: "created_at".into(),
                direction: crate::types::SortDirection::Desc,
            }],
            limit: Some(20),
            offset: Some(5),
            after: Some("01HXYZ".into()),
            as_actor: None,
        };
        let json = serde_json::to_string(&q).unwrap();
        let q2: FindQuery = serde_json::from_str(&json).unwrap();
        assert_eq!(q, q2);
    }
}

#[cfg(test)]
mod command_tests {
    use crate::command::*;
    use crate::expression::{Expression, ExpressionKind};
    use crate::machine::MachineDefinition;
    use crate::value::Value;

    #[test]
    fn spawn_command_display() {
        let c = Command::Spawn(SpawnCommand::new(
            "SupportTicket".into(),
            vec![(
                "subject".into(),
                Expression::new(ExpressionKind::Literal(Value::Text("Test".into()))),
            )],
        ));
        assert_eq!(c.to_string(), "SPAWN SupportTicket");
    }

    #[test]
    fn transition_command_display() {
        let c = Command::Transition(TransitionCommand::new(
            "SupportTicket".into(),
            "tk_123".into(),
            "triaged".into(),
        ));
        assert_eq!(c.to_string(), "TRANSITION SupportTicket tk_123 TO triaged");
    }

    #[test]
    fn try_transition_command_display() {
        let c = Command::TryTransition(TransitionCommand::new(
            "SupportTicket".into(),
            "tk_123".into(),
            "resolved".into(),
        ));
        assert_eq!(
            c.to_string(),
            "TRY TRANSITION SupportTicket tk_123 TO resolved"
        );
    }

    #[test]
    fn define_machine_command_display() {
        let m = MachineDefinition::new("NewMachine".into(), "init".into());
        let c = Command::DefineMachine(m);
        assert_eq!(c.to_string(), "DEFINE MACHINE NewMachine (v1)");
    }

    #[test]
    fn statement_variants() {
        let stmt = Statement::Command(Command::Spawn(SpawnCommand::new("M".into(), vec![])));
        match stmt {
            Statement::Command(Command::Spawn(s)) => assert_eq!(s.machine, "M"),
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn transition_command_new_defaults() {
        let t = TransitionCommand::new("Machine".into(), "id_1".into(), "next".into());
        assert_eq!(t.machine, "Machine");
        assert!(t.with_data.is_empty());
        assert!(t.memo.is_none());
        assert!(t.as_actor.is_none());
        assert!(t.through.is_empty());
        assert!(!t.or_stay);
    }

    #[test]
    fn command_serde() {
        let c = Command::Transition(TransitionCommand::new(
            "Machine".into(),
            "tk_1".into(),
            "open".into(),
        ));
        let json = serde_json::to_string(&c).unwrap();
        let c2: Command = serde_json::from_str(&json).unwrap();
        assert_eq!(c, c2);
    }

    #[test]
    fn alter_machine_command_display() {
        let c = Command::AlterMachine(AlterMachineCommand {
            machine: "SupportTicket".into(),
            operations: vec![AlterOperation::AddState("escalated".into())],
        });
        assert_eq!(c.to_string(), "ALTER MACHINE SupportTicket");
    }

    #[test]
    fn batch_transition_command_display() {
        let c = Command::BatchTransition(BatchTransitionCommand {
            machine: "Order".into(),
            filter: Expression::new(ExpressionKind::StateIs("pending".into())),
            to_state: "cancelled".into(),
            with_data: vec![],
            memo: None,
            as_actor: None,
        });
        assert_eq!(c.to_string(), "TRANSITION ALL Order TO cancelled");
    }

    #[test]
    fn alter_operation_serde() {
        use crate::machine::{TransitionDefinition, TransitionSource};
        let ops = vec![
            AlterOperation::AddState("new_state".into()),
            AlterOperation::RemoveState {
                state: "old".into(),
                migrate_to: "new".into(),
            },
            AlterOperation::RemoveTransition {
                from: "a".into(),
                to: "b".into(),
            },
            AlterOperation::RemoveData("field".into()),
            AlterOperation::Backfill {
                field: "f".into(),
                value: Expression::new(ExpressionKind::Literal(Value::Int(0))),
            },
            AlterOperation::AddTransition(TransitionDefinition::new(
                TransitionSource::State("a".into()),
                "b".into(),
            )),
            AlterOperation::ModifyTransition(TransitionDefinition::new(
                TransitionSource::State("x".into()),
                "y".into(),
            )),
        ];
        for op in ops {
            let json = serde_json::to_string(&op).unwrap();
            let op2: AlterOperation = serde_json::from_str(&json).unwrap();
            assert_eq!(op, op2);
        }
    }

    #[test]
    fn transition_command_cascade() {
        let mut t = TransitionCommand::new("Machine".into(), "id".into(), "done".into());
        t.cascade = true;
        t.or_stay = true;
        assert!(t.cascade);
        assert!(t.or_stay);
    }
}

#[cfg(test)]
mod error_tests {
    use crate::error::*;
    use crate::span::Span;

    #[test]
    fn smql_error_parse() {
        let e = SmqlError::parse("unexpected token");
        match &e {
            SmqlError::ParseError {
                message,
                span,
                hint,
            } => {
                assert_eq!(message, "unexpected token");
                assert!(span.is_none());
                assert!(hint.is_none());
            }
            _ => panic!("Wrong variant"),
        }
        assert!(e.to_string().contains("unexpected token"));
    }

    #[test]
    fn smql_error_parse_with_span() {
        let e = SmqlError::parse_with_span("bad token", Span::new(10, 15));
        match &e {
            SmqlError::ParseError { span, .. } => {
                assert_eq!(*span, Some(Span::new(10, 15)));
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn smql_error_validation() {
        let e = SmqlError::validation("invalid state");
        assert!(e.to_string().contains("invalid state"));
    }

    #[test]
    fn smql_error_not_found() {
        let e = SmqlError::not_found("Machine", "FooMachine");
        assert!(e.to_string().contains("FooMachine"));
    }

    #[test]
    fn smql_error_storage() {
        let e = SmqlError::storage("disk full");
        match &e {
            SmqlError::StorageError { retryable, .. } => assert!(!retryable),
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn transition_denied_error_display() {
        let e = TransitionDeniedError {
            instance_id: "tk_1".into(),
            from_state: "open".into(),
            to_state: "resolved".into(),
            guard_failures: vec![GuardFailure {
                guard_expr: "resolution_note IS SET".into(),
                actual_value: Some("NULL".into()),
                expected: Some("non-null".into()),
                hint: Some("Set resolution_note before resolving".into()),
            }],
            hint: None,
            recovery_options: vec![],
            llm_prompt: None,
        };
        let s = e.to_string();
        assert!(s.contains("open -> resolved"));
        assert!(s.contains("tk_1"));
        assert!(s.contains("resolution_note IS SET"));
    }

    #[test]
    fn smql_error_serde() {
        let e = SmqlError::parse("test");
        let json = serde_json::to_string(&e).unwrap();
        let e2: SmqlError = serde_json::from_str(&json).unwrap();
        match (&e, &e2) {
            (
                SmqlError::ParseError { message: m1, .. },
                SmqlError::ParseError { message: m2, .. },
            ) => {
                assert_eq!(m1, m2);
            }
            _ => panic!("Serde roundtrip failed"),
        }
    }

    #[test]
    fn smql_error_internal() {
        let e = SmqlError::internal("something broke");
        match &e {
            SmqlError::Internal { message } => assert_eq!(message, "something broke"),
            _ => panic!("Wrong variant"),
        }
        assert!(e.to_string().contains("something broke"));
    }

    #[test]
    fn transition_denied_error_with_hint() {
        let e = TransitionDeniedError {
            instance_id: "tk_1".into(),
            from_state: "open".into(),
            to_state: "resolved".into(),
            guard_failures: vec![],
            hint: Some("Check guards".into()),
            recovery_options: vec![],
            llm_prompt: None,
        };
        let s = e.to_string();
        assert!(s.contains("Hint: Check guards"));
    }

    #[test]
    fn transition_denied_error_no_failures() {
        let e = TransitionDeniedError {
            instance_id: "id_1".into(),
            from_state: "a".into(),
            to_state: "b".into(),
            guard_failures: vec![],
            hint: None,
            recovery_options: vec![],
            llm_prompt: None,
        };
        let s = e.to_string();
        assert!(s.contains("a -> b"));
        assert!(!s.contains(":"));
    }

    #[test]
    fn guard_failure_display_minimal() {
        let f = GuardFailure {
            guard_expr: "x > 0".into(),
            actual_value: None,
            expected: None,
            hint: None,
        };
        assert_eq!(f.to_string(), "Guard 'x > 0' failed");
    }

    #[test]
    fn guard_failure_display_with_expected() {
        let f = GuardFailure {
            guard_expr: "x > 0".into(),
            actual_value: Some("-1".into()),
            expected: Some("> 0".into()),
            hint: Some("x must be positive".into()),
        };
        let s = f.to_string();
        assert!(s.contains("got: -1"));
        assert!(s.contains("expected: > 0"));
        assert!(s.contains("x must be positive"));
    }

    #[test]
    fn smql_error_all_variants_display() {
        let errors: Vec<SmqlError> = vec![
            SmqlError::parse("p"),
            SmqlError::parse_with_span("p2", Span::new(0, 1)),
            SmqlError::validation("v"),
            SmqlError::not_found("Machine", "m1"),
            SmqlError::storage("s"),
            SmqlError::internal("i"),
            SmqlError::TransitionDenied(TransitionDeniedError {
                instance_id: "id".into(),
                from_state: "a".into(),
                to_state: "b".into(),
                guard_failures: vec![],
                hint: None,
                recovery_options: vec![],
                llm_prompt: None,
            }),
            SmqlError::GuardFailed {
                message: "failed".into(),
                guard_expr: "x > 0".into(),
                actual_value: None,
                hint: None,
            },
            SmqlError::SpawnRejected {
                message: "bad data".into(),
                field: Some("name".into()),
                hint: None,
            },
            SmqlError::QueryError {
                message: "bad query".into(),
                hint: Some("check syntax".into()),
            },
            SmqlError::TimeoutError {
                message: "timed out".into(),
                instance_id: Some("id".into()),
                state: Some("waiting".into()),
            },
            SmqlError::Conflict {
                message: "version mismatch".into(),
                hint: None,
            },
        ];
        for e in &errors {
            let s = e.to_string();
            assert!(!s.is_empty());
        }
    }

    #[test]
    fn smql_error_all_variants_serde() {
        let errors: Vec<SmqlError> = vec![
            SmqlError::parse("p"),
            SmqlError::validation("v"),
            SmqlError::not_found("Machine", "m1"),
            SmqlError::storage("s"),
            SmqlError::internal("i"),
            SmqlError::GuardFailed {
                message: "f".into(),
                guard_expr: "g".into(),
                actual_value: Some("a".into()),
                hint: Some("h".into()),
            },
            SmqlError::SpawnRejected {
                message: "r".into(),
                field: Some("f".into()),
                hint: None,
            },
            SmqlError::QueryError {
                message: "q".into(),
                hint: None,
            },
            SmqlError::TimeoutError {
                message: "t".into(),
                instance_id: None,
                state: None,
            },
            SmqlError::Conflict {
                message: "c".into(),
                hint: Some("retry".into()),
            },
        ];
        for e in errors {
            let json = serde_json::to_string(&e).unwrap();
            let _e2: SmqlError = serde_json::from_str(&json).unwrap();
        }
    }

    #[test]
    fn transition_denied_error_with_recovery_options() {
        use crate::error::{RecoveryAction, RecoveryOption};

        let e = TransitionDeniedError {
            instance_id: "tk_123".into(),
            from_state: "in_progress".into(),
            to_state: "resolved".into(),
            guard_failures: vec![GuardFailure {
                guard_expr: "resolution IS SET".into(),
                actual_value: Some("NULL".into()),
                expected: Some("non-null".into()),
                hint: Some("Provide resolution text".into()),
            }],
            hint: None,
            recovery_options: vec![
                RecoveryOption {
                    action: RecoveryAction::SetField,
                    field: Some("resolution".into()),
                    suggested_value: Some("Provide resolution text".into()),
                    reason: "Guard 'resolution IS SET' requires this field to be set.".into(),
                    example: Some("TRANSITION SupportTicket tk_123 TO resolved WITH { resolution: \"...\" }".into()),
                },
            ],
            llm_prompt: Some("Transition in_progress -> resolved for instance tk_123 was denied. Provide resolution text or escalate.".into()),
        };

        // Test serialization
        let json = serde_json::to_string(&e).unwrap();
        assert!(json.contains("recovery_options"));
        assert!(json.contains("llm_prompt"));
        assert!(json.contains("SET_FIELD"));

        // Test deserialization
        let e2: TransitionDeniedError = serde_json::from_str(&json).unwrap();
        assert_eq!(e2.recovery_options.len(), 1);
        assert_eq!(e2.recovery_options[0].action, RecoveryAction::SetField);
        assert!(e2.llm_prompt.is_some());
    }
}

#[cfg(test)]
mod span_tests {
    use crate::span::Span;

    #[test]
    fn span_new() {
        let s = Span::new(0, 10);
        assert_eq!(s.start, 0);
        assert_eq!(s.end, 10);
    }

    #[test]
    fn span_display() {
        let s = Span::new(5, 15);
        assert_eq!(s.to_string(), "5..15");
    }

    #[test]
    fn span_merge() {
        let a = Span::new(5, 10);
        let b = Span::new(8, 20);
        let merged = a.merge(b);
        assert_eq!(merged, Span::new(5, 20));
    }

    #[test]
    fn span_clone_eq() {
        let s1 = Span::new(1, 2);
        let s2 = s1;
        assert_eq!(s1, s2);
    }

    #[test]
    fn span_serde() {
        let s = Span::new(10, 20);
        let json = serde_json::to_string(&s).unwrap();
        let s2: Span = serde_json::from_str(&json).unwrap();
        assert_eq!(s, s2);
    }
}
