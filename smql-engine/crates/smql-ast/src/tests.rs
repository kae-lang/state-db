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

        let m = TypeDefinition::Map(Box::new(TypeDefinition::Text), Box::new(TypeDefinition::Int));
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
    }

    #[test]
    fn data_field_definition_display() {
        let field = DataFieldDefinition {
            name: "priority".into(),
            field_type: TypeDefinition::Enum(vec!["low".into(), "high".into()]),
            constraints: vec![Constraint::Default(DefaultValue::String("low".into()))],
        };
        assert_eq!(field.to_string(), "priority : ENUM(low, high) -> DEFAULT(\"low\")");
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
        assert_eq!(AggregateFunction::Percentile(95.0).to_string(), "PERCENTILE(95)");
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
        assert_eq!(Value::Float(3.14).to_string(), "3.14");
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
        assert_eq!(Value::Ref("Agent".into(), "ag_123".into()).to_string(), "Agent#ag_123");
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
            Value::Float(3.14),
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

        let e2 = Expression::new(ExpressionKind::FieldAccess(vec!["a".into(), "b".into(), "c".into()]));
        assert_eq!(e2.to_string(), "a.b.c");
    }

    #[test]
    fn binary_op_display() {
        let e = Expression::new(ExpressionKind::BinaryOp {
            left: Box::new(Expression::new(ExpressionKind::FieldAccess(vec!["x".into()]))),
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

        let e2 = Expression::new(ExpressionKind::StateIn(vec!["open".into(), "triaged".into()]));
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
            left: Box::new(Expression::new(ExpressionKind::FieldAccess(vec!["x".into()]))),
            op: BinaryOperator::Eq,
            right: Box::new(Expression::new(ExpressionKind::Literal(Value::Text("test".into())))),
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
        assert_eq!(TransitionSource::Group("active".into()).to_string(), "GROUP(active)");
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
        assert_eq!(Action::Log("test message".into()).to_string(), "LOG(\"test message\")");
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
        assert_eq!(HookTrigger::BeforeEachTransition.to_string(), "BEFORE EACH TRANSITION");
        assert_eq!(HookTrigger::OnEnter("open".into()).to_string(), "ON ENTER open");
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
        });
        let json = serde_json::to_string(&q).unwrap();
        let q2: Query = serde_json::from_str(&json).unwrap();
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
            vec![("subject".into(), Expression::new(ExpressionKind::Literal(Value::Text("Test".into()))))],
        ));
        assert_eq!(c.to_string(), "SPAWN SupportTicket");
    }

    #[test]
    fn transition_command_display() {
        let c = Command::Transition(TransitionCommand::new("SupportTicket".into(), "tk_123".into(), "triaged".into()));
        assert_eq!(c.to_string(), "TRANSITION SupportTicket tk_123 TO triaged");
    }

    #[test]
    fn try_transition_command_display() {
        let c = Command::TryTransition(TransitionCommand::new("SupportTicket".into(), "tk_123".into(), "resolved".into()));
        assert_eq!(c.to_string(), "TRY TRANSITION SupportTicket tk_123 TO resolved");
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
        let c = Command::Transition(TransitionCommand::new("Machine".into(), "tk_1".into(), "open".into()));
        let json = serde_json::to_string(&c).unwrap();
        let c2: Command = serde_json::from_str(&json).unwrap();
        assert_eq!(c, c2);
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
            SmqlError::ParseError { message, span, hint } => {
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
            (SmqlError::ParseError { message: m1, .. }, SmqlError::ParseError { message: m2, .. }) => {
                assert_eq!(m1, m2);
            }
            _ => panic!("Serde roundtrip failed"),
        }
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
