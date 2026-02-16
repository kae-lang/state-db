#[cfg(test)]
mod tests {
    use crate::eval::*;
    use chrono::{TimeZone, Utc};
    use smql_ast::expression::{BinaryOperator, Expression, ExpressionKind, UnaryOperator};
    use smql_ast::value::{SmqlDuration, Value};
    use std::collections::{BTreeMap, HashMap};

    // ── Helpers ──────────────────────────────────────────────────────────

    fn lit(v: Value) -> Expression {
        Expression::new(ExpressionKind::Literal(v))
    }

    fn field(path: Vec<&str>) -> Expression {
        Expression::new(ExpressionKind::FieldAccess(
            path.into_iter().map(|s| s.to_string()).collect(),
        ))
    }

    fn binop(left: Expression, op: BinaryOperator, right: Expression) -> Expression {
        Expression::new(ExpressionKind::BinaryOp {
            left: Box::new(left),
            op,
            right: Box::new(right),
        })
    }

    fn unaryop(op: UnaryOperator, operand: Expression) -> Expression {
        Expression::new(ExpressionKind::UnaryOp {
            op,
            operand: Box::new(operand),
        })
    }

    fn func(name: &str, args: Vec<Expression>) -> Expression {
        Expression::new(ExpressionKind::FunctionCall {
            name: name.to_string(),
            args,
        })
    }

    fn ctx_with_data(data: Vec<(&str, Value)>) -> EvalContext {
        let map: HashMap<String, Value> =
            data.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
        EvalContext::new(map, "open".to_string())
    }

    fn make_child(id: &str, machine: &str, state: &str, data: Vec<(&str, Value)>) -> ChildInfo {
        ChildInfo {
            id: id.to_string(),
            machine: machine.to_string(),
            state: state.to_string(),
            data: data.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
        }
    }

    fn make_map(entries: Vec<(&str, Value)>) -> BTreeMap<String, Value> {
        entries
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect()
    }

    // ── 1. FieldAccess: child data field access ─────────────────────────

    #[test]
    fn child_data_field_access_simple() {
        let mut ctx = ctx_with_data(vec![]);
        let child = make_child("c1", "Order", "pending", vec![("total", Value::Int(100))]);
        ctx.children.insert("items".to_string(), vec![child]);

        // Access items.total
        let expr = field(vec!["items", "total"]);
        let result = eval_expr(&expr, &ctx).unwrap();
        assert_eq!(result, Value::Int(100));
    }

    #[test]
    fn child_data_field_access_nested() {
        let mut ctx = ctx_with_data(vec![]);
        let nested_map = Value::Map(make_map(vec![("city", Value::Text("NYC".to_string()))]));
        let child = make_child("c1", "Order", "pending", vec![("address", nested_map)]);
        ctx.children.insert("items".to_string(), vec![child]);

        // Access items.address.city
        let expr = field(vec!["items", "address", "city"]);
        let result = eval_expr(&expr, &ctx).unwrap();
        assert_eq!(result, Value::Text("NYC".to_string()));
    }

    #[test]
    fn child_data_field_access_missing_field() {
        let mut ctx = ctx_with_data(vec![]);
        let child = make_child("c1", "Order", "pending", vec![]);
        ctx.children.insert("items".to_string(), vec![child]);

        let expr = field(vec!["items", "nonexistent"]);
        let result = eval_expr(&expr, &ctx).unwrap();
        assert_eq!(result, Value::Null);
    }

    #[test]
    fn child_data_field_access_empty_children() {
        let mut ctx = ctx_with_data(vec![]);
        ctx.children.insert("items".to_string(), vec![]);

        // Empty children list returns Null for data field access
        let expr = field(vec!["items", "total"]);
        let result = eval_expr(&expr, &ctx).unwrap();
        assert_eq!(result, Value::Null);
    }

    // ── 2. FieldAccess: PARENT access with nested fields ────────────────

    #[test]
    fn parent_field_access_simple() {
        let mut ctx = ctx_with_data(vec![]);
        let mut parent_data = HashMap::new();
        parent_data.insert("priority".to_string(), Value::Int(3));
        ctx.parent_data = Some(parent_data);

        let expr = field(vec!["PARENT", "priority"]);
        let result = eval_expr(&expr, &ctx).unwrap();
        assert_eq!(result, Value::Int(3));
    }

    #[test]
    fn parent_field_access_nested() {
        let mut ctx = ctx_with_data(vec![]);
        let nested = Value::Map(make_map(vec![("name", Value::Text("Alice".to_string()))]));
        let mut parent_data = HashMap::new();
        parent_data.insert("customer".to_string(), nested);
        ctx.parent_data = Some(parent_data);

        // PARENT.customer.name
        let expr = field(vec!["PARENT", "customer", "name"]);
        let result = eval_expr(&expr, &ctx).unwrap();
        assert_eq!(result, Value::Text("Alice".to_string()));
    }

    #[test]
    fn parent_field_access_missing_field() {
        let mut ctx = ctx_with_data(vec![]);
        let parent_data = HashMap::new();
        ctx.parent_data = Some(parent_data);

        let expr = field(vec!["PARENT", "missing"]);
        let result = eval_expr(&expr, &ctx).unwrap();
        assert_eq!(result, Value::Null);
    }

    #[test]
    fn parent_field_access_no_parent_data() {
        let ctx = ctx_with_data(vec![]);
        // parent_data is None by default

        let expr = field(vec!["PARENT", "anything"]);
        let result = eval_expr(&expr, &ctx).unwrap();
        assert_eq!(result, Value::Null);
    }

    #[test]
    fn parent_state_access() {
        let mut ctx = ctx_with_data(vec![]);
        ctx.parent_state = Some("active".to_string());

        let expr = field(vec!["PARENT", "STATE"]);
        let result = eval_expr(&expr, &ctx).unwrap();
        assert_eq!(result, Value::Text("active".to_string()));
    }

    #[test]
    fn parent_state_access_no_parent() {
        let ctx = ctx_with_data(vec![]);

        let expr = field(vec!["PARENT", "STATE"]);
        let result = eval_expr(&expr, &ctx).unwrap();
        assert_eq!(result, Value::Null);
    }

    // ── 3. Regular field access: dot chain ──────────────────────────────

    #[test]
    fn dot_chain_field_access() {
        let deep = Value::Map(make_map(vec![("value", Value::Int(42))]));
        let nested = Value::Map(make_map(vec![("deep", deep)]));
        let ctx = ctx_with_data(vec![("data", nested)]);

        // data.deep.value
        let expr = field(vec!["data", "deep", "value"]);
        let result = eval_expr(&expr, &ctx).unwrap();
        assert_eq!(result, Value::Int(42));
    }

    #[test]
    fn dot_chain_field_access_missing_intermediate() {
        let ctx = ctx_with_data(vec![("data", Value::Int(5))]);

        // data.nested.deep — data is Int, not Map, so nested returns Null
        let expr = field(vec!["data", "nested", "deep"]);
        let result = eval_expr(&expr, &ctx).unwrap();
        assert_eq!(result, Value::Null);
    }

    // ── 4. SelfRef ──────────────────────────────────────────────────────

    #[test]
    fn self_ref_returns_data_as_map() {
        let ctx = ctx_with_data(vec![
            ("name", Value::Text("Alice".to_string())),
            ("age", Value::Int(30)),
        ]);

        let expr = Expression::new(ExpressionKind::SelfRef);
        let result = eval_expr(&expr, &ctx).unwrap();

        let expected = Value::Map(make_map(vec![
            ("age", Value::Int(30)),
            ("name", Value::Text("Alice".to_string())),
        ]));
        assert_eq!(result, expected);
    }

    #[test]
    fn self_ref_empty_data() {
        let ctx = ctx_with_data(vec![]);
        let expr = Expression::new(ExpressionKind::SelfRef);
        let result = eval_expr(&expr, &ctx).unwrap();
        assert_eq!(result, Value::Map(BTreeMap::new()));
    }

    // ── 5. ActorRef with custom fields + ActorRef when None ─────────────

    #[test]
    fn actor_ref_with_custom_fields() {
        let mut ctx = ctx_with_data(vec![]);
        let mut fields = HashMap::new();
        fields.insert(
            "department".to_string(),
            Value::Text("engineering".to_string()),
        );
        fields.insert("level".to_string(), Value::Int(5));
        ctx.actor = Some(ActorInfo {
            id: "user-123".to_string(),
            role: Some("admin".to_string()),
            fields,
        });

        let expr = Expression::new(ExpressionKind::ActorRef);
        let result = eval_expr(&expr, &ctx).unwrap();

        if let Value::Map(map) = result {
            assert_eq!(map.get("id"), Some(&Value::Text("user-123".to_string())));
            assert_eq!(map.get("role"), Some(&Value::Text("admin".to_string())));
            assert_eq!(
                map.get("department"),
                Some(&Value::Text("engineering".to_string()))
            );
            assert_eq!(map.get("level"), Some(&Value::Int(5)));
        } else {
            panic!("Expected Map, got {:?}", result);
        }
    }

    #[test]
    fn actor_ref_without_role() {
        let mut ctx = ctx_with_data(vec![]);
        ctx.actor = Some(ActorInfo {
            id: "user-456".to_string(),
            role: None,
            fields: HashMap::new(),
        });

        let expr = Expression::new(ExpressionKind::ActorRef);
        let result = eval_expr(&expr, &ctx).unwrap();

        if let Value::Map(map) = result {
            assert_eq!(map.get("id"), Some(&Value::Text("user-456".to_string())));
            assert_eq!(map.get("role"), None);
        } else {
            panic!("Expected Map, got {:?}", result);
        }
    }

    #[test]
    fn actor_ref_none() {
        let ctx = ctx_with_data(vec![]);
        // actor is None by default
        let expr = Expression::new(ExpressionKind::ActorRef);
        let result = eval_expr(&expr, &ctx).unwrap();
        assert_eq!(result, Value::Null);
    }

    // ── 6. InList ───────────────────────────────────────────────────────

    #[test]
    fn in_list_found() {
        let ctx = ctx_with_data(vec![("status", Value::Text("active".to_string()))]);
        let expr = Expression::new(ExpressionKind::InList {
            expr: Box::new(field(vec!["status"])),
            values: vec![
                lit(Value::Text("active".to_string())),
                lit(Value::Text("pending".to_string())),
                lit(Value::Text("closed".to_string())),
            ],
        });
        let result = eval_expr(&expr, &ctx).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn in_list_not_found() {
        let ctx = ctx_with_data(vec![("status", Value::Text("archived".to_string()))]);
        let expr = Expression::new(ExpressionKind::InList {
            expr: Box::new(field(vec!["status"])),
            values: vec![
                lit(Value::Text("active".to_string())),
                lit(Value::Text("pending".to_string())),
            ],
        });
        let result = eval_expr(&expr, &ctx).unwrap();
        assert_eq!(result, Value::Bool(false));
    }

    #[test]
    fn in_list_with_integers() {
        let ctx = ctx_with_data(vec![("code", Value::Int(42))]);
        let expr = Expression::new(ExpressionKind::InList {
            expr: Box::new(field(vec!["code"])),
            values: vec![
                lit(Value::Int(10)),
                lit(Value::Int(42)),
                lit(Value::Int(99)),
            ],
        });
        let result = eval_expr(&expr, &ctx).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    // ── 7. Count on Set / Count with None ───────────────────────────────

    #[test]
    fn count_expr_with_set() {
        let ctx = ctx_with_data(vec![(
            "tags",
            Value::Set(vec![
                Value::Text("a".to_string()),
                Value::Text("b".to_string()),
                Value::Text("c".to_string()),
            ]),
        )]);

        let expr = Expression::new(ExpressionKind::Count(Some(Box::new(field(vec!["tags"])))));
        let result = eval_expr(&expr, &ctx).unwrap();
        assert_eq!(result, Value::Int(3));
    }

    #[test]
    fn count_expr_with_none() {
        let ctx = ctx_with_data(vec![]);
        let expr = Expression::new(ExpressionKind::Count(None));
        let result = eval_expr(&expr, &ctx).unwrap();
        assert_eq!(result, Value::Int(0));
    }

    #[test]
    fn count_expr_with_list() {
        let ctx = ctx_with_data(vec![(
            "items",
            Value::List(vec![Value::Int(1), Value::Int(2)]),
        )]);

        let expr = Expression::new(ExpressionKind::Count(Some(Box::new(field(vec!["items"])))));
        let result = eval_expr(&expr, &ctx).unwrap();
        assert_eq!(result, Value::Int(2));
    }

    #[test]
    fn count_expr_with_non_collection() {
        let ctx = ctx_with_data(vec![("x", Value::Int(42))]);
        let expr = Expression::new(ExpressionKind::Count(Some(Box::new(field(vec!["x"])))));
        let result = eval_expr(&expr, &ctx).unwrap();
        assert_eq!(result, Value::Int(0));
    }

    // ── 8. Pattern ──────────────────────────────────────────────────────

    #[test]
    fn pattern_returns_regex_string() {
        let ctx = ctx_with_data(vec![]);
        let expr = Expression::new(ExpressionKind::Pattern("^[a-z]+$".to_string()));
        let result = eval_expr(&expr, &ctx).unwrap();
        assert_eq!(result, Value::Text("^[a-z]+$".to_string()));
    }

    // ── 9. access_field on non-Map ──────────────────────────────────────

    #[test]
    fn access_field_on_int_returns_null() {
        let ctx = ctx_with_data(vec![("x", Value::Int(42))]);
        // x.anything — x is Int, so .anything returns Null
        let expr = field(vec!["x", "anything"]);
        let result = eval_expr(&expr, &ctx).unwrap();
        assert_eq!(result, Value::Null);
    }

    #[test]
    fn access_field_on_text_returns_null() {
        let ctx = ctx_with_data(vec![("name", Value::Text("hello".to_string()))]);
        let expr = field(vec!["name", "length"]);
        let result = eval_expr(&expr, &ctx).unwrap();
        assert_eq!(result, Value::Null);
    }

    #[test]
    fn access_field_on_bool_returns_null() {
        let ctx = ctx_with_data(vec![("flag", Value::Bool(true))]);
        let expr = field(vec!["flag", "sub"]);
        let result = eval_expr(&expr, &ctx).unwrap();
        assert_eq!(result, Value::Null);
    }

    // ── 10. NotEq operator ──────────────────────────────────────────────

    #[test]
    fn not_eq_different_values() {
        let ctx = ctx_with_data(vec![]);
        let expr = binop(
            lit(Value::Int(1)),
            BinaryOperator::NotEq,
            lit(Value::Int(2)),
        );
        let result = eval_expr(&expr, &ctx).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn not_eq_same_values() {
        let ctx = ctx_with_data(vec![]);
        let expr = binop(
            lit(Value::Int(5)),
            BinaryOperator::NotEq,
            lit(Value::Int(5)),
        );
        let result = eval_expr(&expr, &ctx).unwrap();
        assert_eq!(result, Value::Bool(false));
    }

    #[test]
    fn not_eq_text_values() {
        let ctx = ctx_with_data(vec![]);
        let expr = binop(
            lit(Value::Text("hello".to_string())),
            BinaryOperator::NotEq,
            lit(Value::Text("world".to_string())),
        );
        let result = eval_expr(&expr, &ctx).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    // ── 11. LtEq operator ──────────────────────────────────────────────

    #[test]
    fn lt_eq_less() {
        let ctx = ctx_with_data(vec![]);
        let expr = binop(lit(Value::Int(3)), BinaryOperator::LtEq, lit(Value::Int(5)));
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));
    }

    #[test]
    fn lt_eq_equal() {
        let ctx = ctx_with_data(vec![]);
        let expr = binop(lit(Value::Int(5)), BinaryOperator::LtEq, lit(Value::Int(5)));
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));
    }

    #[test]
    fn lt_eq_greater() {
        let ctx = ctx_with_data(vec![]);
        let expr = binop(lit(Value::Int(7)), BinaryOperator::LtEq, lit(Value::Int(5)));
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(false));
    }

    // ── 12. GtEq operator ──────────────────────────────────────────────

    #[test]
    fn gt_eq_less() {
        let ctx = ctx_with_data(vec![]);
        let expr = binop(lit(Value::Int(3)), BinaryOperator::GtEq, lit(Value::Int(5)));
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(false));
    }

    #[test]
    fn gt_eq_equal() {
        let ctx = ctx_with_data(vec![]);
        let expr = binop(lit(Value::Int(5)), BinaryOperator::GtEq, lit(Value::Int(5)));
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));
    }

    #[test]
    fn gt_eq_greater() {
        let ctx = ctx_with_data(vec![]);
        let expr = binop(lit(Value::Int(7)), BinaryOperator::GtEq, lit(Value::Int(5)));
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));
    }

    // ── 13. Mul operator ────────────────────────────────────────────────

    #[test]
    fn mul_int_int() {
        let ctx = ctx_with_data(vec![]);
        let expr = binop(lit(Value::Int(3)), BinaryOperator::Mul, lit(Value::Int(4)));
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Int(12));
    }

    #[test]
    fn mul_float_float() {
        let ctx = ctx_with_data(vec![]);
        let expr = binop(
            lit(Value::Float(2.5)),
            BinaryOperator::Mul,
            lit(Value::Float(4.0)),
        );
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Float(10.0));
    }

    #[test]
    fn mul_int_float() {
        let ctx = ctx_with_data(vec![]);
        let expr = binop(
            lit(Value::Int(3)),
            BinaryOperator::Mul,
            lit(Value::Float(2.5)),
        );
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Float(7.5));
    }

    #[test]
    fn mul_float_int() {
        let ctx = ctx_with_data(vec![]);
        let expr = binop(
            lit(Value::Float(2.5)),
            BinaryOperator::Mul,
            lit(Value::Int(4)),
        );
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Float(10.0));
    }

    // ── 14. Neg on non-numeric ──────────────────────────────────────────

    #[test]
    fn neg_int() {
        let ctx = ctx_with_data(vec![]);
        let expr = unaryop(UnaryOperator::Neg, lit(Value::Int(42)));
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Int(-42));
    }

    #[test]
    fn neg_float() {
        let ctx = ctx_with_data(vec![]);
        let expr = unaryop(UnaryOperator::Neg, lit(Value::Float(3.125)));
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Float(-3.125));
    }

    #[test]
    fn neg_text_error() {
        let ctx = ctx_with_data(vec![]);
        let expr = unaryop(UnaryOperator::Neg, lit(Value::Text("hello".to_string())));
        let result = eval_expr(&expr, &ctx);
        assert!(result.is_err());
        let err = result.unwrap_err();
        match err {
            smql_ast::SmqlError::GuardFailed { message, .. } => {
                assert!(message.contains("Cannot negate"));
            }
            other => panic!("Expected GuardFailed, got {:?}", other),
        }
    }

    #[test]
    fn neg_bool_error() {
        let ctx = ctx_with_data(vec![]);
        let expr = unaryop(UnaryOperator::Neg, lit(Value::Bool(true)));
        assert!(eval_expr(&expr, &ctx).is_err());
    }

    // ── 15. is_truthy edge cases ────────────────────────────────────────

    #[test]
    fn truthy_bool_true() {
        let ctx = ctx_with_data(vec![]);
        let expr = Expression::new(ExpressionKind::IsSet(Box::new(lit(Value::Bool(true)))));
        // IS SET on Bool(true) => Bool(true) is not Null => true
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));

        // Use eval_guard which internally calls is_truthy on the result
        let guard_expr = lit(Value::Bool(true));
        assert!(eval_guard(&guard_expr, &ctx).unwrap());
    }

    #[test]
    fn truthy_bool_false() {
        let ctx = ctx_with_data(vec![]);
        let guard_expr = lit(Value::Bool(false));
        assert!(!eval_guard(&guard_expr, &ctx).unwrap());
    }

    #[test]
    fn truthy_null() {
        let ctx = ctx_with_data(vec![]);
        let guard_expr = lit(Value::Null);
        assert!(!eval_guard(&guard_expr, &ctx).unwrap());
    }

    #[test]
    fn truthy_int_zero() {
        let ctx = ctx_with_data(vec![]);
        let guard_expr = lit(Value::Int(0));
        assert!(!eval_guard(&guard_expr, &ctx).unwrap());
    }

    #[test]
    fn truthy_int_nonzero() {
        let ctx = ctx_with_data(vec![]);
        let guard_expr = lit(Value::Int(1));
        assert!(eval_guard(&guard_expr, &ctx).unwrap());
    }

    #[test]
    fn truthy_empty_text() {
        let ctx = ctx_with_data(vec![]);
        let guard_expr = lit(Value::Text("".to_string()));
        assert!(!eval_guard(&guard_expr, &ctx).unwrap());
    }

    #[test]
    fn truthy_nonempty_text() {
        let ctx = ctx_with_data(vec![]);
        let guard_expr = lit(Value::Text("hello".to_string()));
        assert!(eval_guard(&guard_expr, &ctx).unwrap());
    }

    #[test]
    fn truthy_empty_list() {
        let ctx = ctx_with_data(vec![]);
        let guard_expr = lit(Value::List(vec![]));
        assert!(!eval_guard(&guard_expr, &ctx).unwrap());
    }

    #[test]
    fn truthy_nonempty_list() {
        let ctx = ctx_with_data(vec![]);
        let guard_expr = lit(Value::List(vec![Value::Int(1)]));
        assert!(eval_guard(&guard_expr, &ctx).unwrap());
    }

    #[test]
    fn truthy_empty_set() {
        let ctx = ctx_with_data(vec![]);
        let guard_expr = lit(Value::Set(vec![]));
        assert!(!eval_guard(&guard_expr, &ctx).unwrap());
    }

    #[test]
    fn truthy_nonempty_set() {
        let ctx = ctx_with_data(vec![]);
        let guard_expr = lit(Value::Set(vec![Value::Int(1)]));
        assert!(eval_guard(&guard_expr, &ctx).unwrap());
    }

    // ── 16. values_equal edge cases ─────────────────────────────────────

    #[test]
    fn values_equal_float_float() {
        let ctx = ctx_with_data(vec![]);
        let expr = binop(
            lit(Value::Float(3.125)),
            BinaryOperator::Eq,
            lit(Value::Float(3.125)),
        );
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));
    }

    #[test]
    fn values_equal_float_float_not_equal() {
        let ctx = ctx_with_data(vec![]);
        let expr = binop(
            lit(Value::Float(3.125)),
            BinaryOperator::Eq,
            lit(Value::Float(2.75)),
        );
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(false));
    }

    #[test]
    fn values_equal_null_null() {
        let ctx = ctx_with_data(vec![]);
        let expr = binop(lit(Value::Null), BinaryOperator::Eq, lit(Value::Null));
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));
    }

    #[test]
    fn values_equal_int_float_cross_coercion() {
        let ctx = ctx_with_data(vec![]);
        // Int(5) == Float(5.0) should be true
        let expr = binop(
            lit(Value::Int(5)),
            BinaryOperator::Eq,
            lit(Value::Float(5.0)),
        );
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));
    }

    #[test]
    fn values_equal_float_int_cross_coercion() {
        let ctx = ctx_with_data(vec![]);
        // Float(5.0) == Int(5) should be true
        let expr = binop(
            lit(Value::Float(5.0)),
            BinaryOperator::Eq,
            lit(Value::Int(5)),
        );
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));
    }

    #[test]
    fn values_equal_int_float_not_equal() {
        let ctx = ctx_with_data(vec![]);
        let expr = binop(
            lit(Value::Int(5)),
            BinaryOperator::Eq,
            lit(Value::Float(5.1)),
        );
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(false));
    }

    #[test]
    fn values_equal_bool_bool() {
        let ctx = ctx_with_data(vec![]);
        let expr = binop(
            lit(Value::Bool(true)),
            BinaryOperator::Eq,
            lit(Value::Bool(true)),
        );
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));

        let expr2 = binop(
            lit(Value::Bool(true)),
            BinaryOperator::Eq,
            lit(Value::Bool(false)),
        );
        assert_eq!(eval_expr(&expr2, &ctx).unwrap(), Value::Bool(false));
    }

    // ── 17. compare_values for Text, Duration, incompatible types ───────

    #[test]
    fn compare_text_values() {
        let ctx = ctx_with_data(vec![]);
        // "apple" < "banana"
        let expr = binop(
            lit(Value::Text("apple".to_string())),
            BinaryOperator::Lt,
            lit(Value::Text("banana".to_string())),
        );
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));

        // "banana" > "apple"
        let expr2 = binop(
            lit(Value::Text("banana".to_string())),
            BinaryOperator::Gt,
            lit(Value::Text("apple".to_string())),
        );
        assert_eq!(eval_expr(&expr2, &ctx).unwrap(), Value::Bool(true));

        // "apple" == "apple" in ordering
        let expr3 = binop(
            lit(Value::Text("apple".to_string())),
            BinaryOperator::LtEq,
            lit(Value::Text("apple".to_string())),
        );
        assert_eq!(eval_expr(&expr3, &ctx).unwrap(), Value::Bool(true));
    }

    #[test]
    fn compare_duration_values() {
        let ctx = ctx_with_data(vec![]);
        let d1 = Value::Duration(SmqlDuration::from_seconds(60));
        let d2 = Value::Duration(SmqlDuration::from_seconds(120));

        // 60s < 120s
        let expr = binop(lit(d1.clone()), BinaryOperator::Lt, lit(d2.clone()));
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));

        // 120s > 60s
        let expr2 = binop(lit(d2.clone()), BinaryOperator::Gt, lit(d1.clone()));
        assert_eq!(eval_expr(&expr2, &ctx).unwrap(), Value::Bool(true));

        // 60s >= 60s
        let expr3 = binop(lit(d1.clone()), BinaryOperator::GtEq, lit(d1.clone()));
        assert_eq!(eval_expr(&expr3, &ctx).unwrap(), Value::Bool(true));
    }

    #[test]
    fn compare_incompatible_types_returns_false() {
        let ctx = ctx_with_data(vec![]);
        // Int < Text => compare_values returns None => false
        let expr = binop(
            lit(Value::Int(5)),
            BinaryOperator::Lt,
            lit(Value::Text("hello".to_string())),
        );
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(false));

        // Also for GtEq
        let expr2 = binop(
            lit(Value::Int(5)),
            BinaryOperator::GtEq,
            lit(Value::Text("hello".to_string())),
        );
        assert_eq!(eval_expr(&expr2, &ctx).unwrap(), Value::Bool(false));
    }

    #[test]
    fn compare_float_ordering() {
        let ctx = ctx_with_data(vec![]);
        let expr = binop(
            lit(Value::Float(1.5)),
            BinaryOperator::Lt,
            lit(Value::Float(2.5)),
        );
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));
    }

    #[test]
    fn compare_int_float_cross_ordering() {
        let ctx = ctx_with_data(vec![]);
        // Int(3) < Float(3.5)
        let expr = binop(
            lit(Value::Int(3)),
            BinaryOperator::Lt,
            lit(Value::Float(3.5)),
        );
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));

        // Float(2.5) < Int(3)
        let expr2 = binop(
            lit(Value::Float(2.5)),
            BinaryOperator::Lt,
            lit(Value::Int(3)),
        );
        assert_eq!(eval_expr(&expr2, &ctx).unwrap(), Value::Bool(true));
    }

    // ── 18. Arithmetic add: all combinations ────────────────────────────

    #[test]
    fn add_int_float() {
        let ctx = ctx_with_data(vec![]);
        let expr = binop(
            lit(Value::Int(3)),
            BinaryOperator::Add,
            lit(Value::Float(1.5)),
        );
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Float(4.5));
    }

    #[test]
    fn add_float_int() {
        let ctx = ctx_with_data(vec![]);
        let expr = binop(
            lit(Value::Float(1.5)),
            BinaryOperator::Add,
            lit(Value::Int(3)),
        );
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Float(4.5));
    }

    #[test]
    fn add_float_float() {
        let ctx = ctx_with_data(vec![]);
        let expr = binop(
            lit(Value::Float(1.5)),
            BinaryOperator::Add,
            lit(Value::Float(2.5)),
        );
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Float(4.0));
    }

    #[test]
    fn add_duration_duration() {
        let ctx = ctx_with_data(vec![]);
        let d1 = Value::Duration(SmqlDuration::from_seconds(60));
        let d2 = Value::Duration(SmqlDuration::from_seconds(30));
        let expr = binop(lit(d1), BinaryOperator::Add, lit(d2));
        assert_eq!(
            eval_expr(&expr, &ctx).unwrap(),
            Value::Duration(SmqlDuration::from_seconds(90))
        );
    }

    #[test]
    fn add_text_text() {
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

    #[test]
    fn add_incompatible_error() {
        let ctx = ctx_with_data(vec![]);
        let expr = binop(
            lit(Value::Int(5)),
            BinaryOperator::Add,
            lit(Value::Bool(true)),
        );
        assert!(eval_expr(&expr, &ctx).is_err());
    }

    // ── 19. Arithmetic sub: all combinations ────────────────────────────

    #[test]
    fn sub_int_int() {
        let ctx = ctx_with_data(vec![]);
        let expr = binop(lit(Value::Int(10)), BinaryOperator::Sub, lit(Value::Int(3)));
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Int(7));
    }

    #[test]
    fn sub_float_float() {
        let ctx = ctx_with_data(vec![]);
        let expr = binop(
            lit(Value::Float(5.5)),
            BinaryOperator::Sub,
            lit(Value::Float(2.5)),
        );
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Float(3.0));
    }

    #[test]
    fn sub_int_float() {
        let ctx = ctx_with_data(vec![]);
        let expr = binop(
            lit(Value::Int(5)),
            BinaryOperator::Sub,
            lit(Value::Float(1.5)),
        );
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Float(3.5));
    }

    #[test]
    fn sub_float_int() {
        let ctx = ctx_with_data(vec![]);
        let expr = binop(
            lit(Value::Float(5.5)),
            BinaryOperator::Sub,
            lit(Value::Int(2)),
        );
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Float(3.5));
    }

    #[test]
    fn sub_duration_duration() {
        let ctx = ctx_with_data(vec![]);
        let d1 = Value::Duration(SmqlDuration::from_seconds(120));
        let d2 = Value::Duration(SmqlDuration::from_seconds(30));
        let expr = binop(lit(d1), BinaryOperator::Sub, lit(d2));
        assert_eq!(
            eval_expr(&expr, &ctx).unwrap(),
            Value::Duration(SmqlDuration::from_seconds(90))
        );
    }

    #[test]
    fn sub_duration_saturating() {
        let ctx = ctx_with_data(vec![]);
        // Subtracting larger from smaller should saturate to 0
        let d1 = Value::Duration(SmqlDuration::from_seconds(10));
        let d2 = Value::Duration(SmqlDuration::from_seconds(100));
        let expr = binop(lit(d1), BinaryOperator::Sub, lit(d2));
        assert_eq!(
            eval_expr(&expr, &ctx).unwrap(),
            Value::Duration(SmqlDuration::from_seconds(0))
        );
    }

    #[test]
    fn sub_incompatible_error() {
        let ctx = ctx_with_data(vec![]);
        let expr = binop(
            lit(Value::Text("hello".to_string())),
            BinaryOperator::Sub,
            lit(Value::Int(1)),
        );
        assert!(eval_expr(&expr, &ctx).is_err());
    }

    // ── 20. Arithmetic mul: all combinations + error ────────────────────

    #[test]
    fn mul_arithmetic_int_int() {
        let ctx = ctx_with_data(vec![]);
        let expr = binop(lit(Value::Int(6)), BinaryOperator::Mul, lit(Value::Int(7)));
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Int(42));
    }

    #[test]
    fn mul_arithmetic_float_float() {
        let ctx = ctx_with_data(vec![]);
        let expr = binop(
            lit(Value::Float(3.0)),
            BinaryOperator::Mul,
            lit(Value::Float(2.0)),
        );
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Float(6.0));
    }

    #[test]
    fn mul_arithmetic_int_float() {
        let ctx = ctx_with_data(vec![]);
        let expr = binop(
            lit(Value::Int(2)),
            BinaryOperator::Mul,
            lit(Value::Float(3.5)),
        );
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Float(7.0));
    }

    #[test]
    fn mul_arithmetic_float_int() {
        let ctx = ctx_with_data(vec![]);
        let expr = binop(
            lit(Value::Float(3.5)),
            BinaryOperator::Mul,
            lit(Value::Int(2)),
        );
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Float(7.0));
    }

    #[test]
    fn mul_incompatible_error() {
        let ctx = ctx_with_data(vec![]);
        let expr = binop(
            lit(Value::Text("hello".to_string())),
            BinaryOperator::Mul,
            lit(Value::Int(3)),
        );
        assert!(eval_expr(&expr, &ctx).is_err());
    }

    // ── 21. Arithmetic div: all combinations + div-by-zero ──────────────

    #[test]
    fn div_int_int() {
        let ctx = ctx_with_data(vec![]);
        let expr = binop(lit(Value::Int(10)), BinaryOperator::Div, lit(Value::Int(3)));
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Int(3)); // integer division
    }

    #[test]
    fn div_float_float() {
        let ctx = ctx_with_data(vec![]);
        let expr = binop(
            lit(Value::Float(10.0)),
            BinaryOperator::Div,
            lit(Value::Float(4.0)),
        );
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Float(2.5));
    }

    #[test]
    fn div_int_float() {
        let ctx = ctx_with_data(vec![]);
        let expr = binop(
            lit(Value::Int(7)),
            BinaryOperator::Div,
            lit(Value::Float(2.0)),
        );
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Float(3.5));
    }

    #[test]
    fn div_float_int() {
        let ctx = ctx_with_data(vec![]);
        let expr = binop(
            lit(Value::Float(7.0)),
            BinaryOperator::Div,
            lit(Value::Int(2)),
        );
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Float(3.5));
    }

    #[test]
    fn div_int_by_zero() {
        let ctx = ctx_with_data(vec![]);
        let expr = binop(lit(Value::Int(10)), BinaryOperator::Div, lit(Value::Int(0)));
        let result = eval_expr(&expr, &ctx);
        assert!(result.is_err());
        match result.unwrap_err() {
            smql_ast::SmqlError::GuardFailed { message, .. } => {
                assert!(message.contains("Division by zero"));
            }
            other => panic!("Expected GuardFailed, got {:?}", other),
        }
    }

    #[test]
    fn div_float_by_zero_float() {
        let ctx = ctx_with_data(vec![]);
        let expr = binop(
            lit(Value::Float(10.0)),
            BinaryOperator::Div,
            lit(Value::Float(0.0)),
        );
        let result = eval_expr(&expr, &ctx);
        assert!(result.is_err());
        match result.unwrap_err() {
            smql_ast::SmqlError::GuardFailed { message, .. } => {
                assert!(message.contains("Division by zero"));
            }
            other => panic!("Expected GuardFailed, got {:?}", other),
        }
    }

    #[test]
    fn div_int_by_zero_float() {
        let ctx = ctx_with_data(vec![]);
        let expr = binop(
            lit(Value::Int(10)),
            BinaryOperator::Div,
            lit(Value::Float(0.0)),
        );
        let result = eval_expr(&expr, &ctx);
        assert!(result.is_err());
        match result.unwrap_err() {
            smql_ast::SmqlError::GuardFailed { message, .. } => {
                assert!(message.contains("Division by zero"));
            }
            other => panic!("Expected GuardFailed, got {:?}", other),
        }
    }

    #[test]
    fn div_float_by_int_zero() {
        let ctx = ctx_with_data(vec![]);
        let expr = binop(
            lit(Value::Float(10.0)),
            BinaryOperator::Div,
            lit(Value::Int(0)),
        );
        let result = eval_expr(&expr, &ctx);
        assert!(result.is_err());
        match result.unwrap_err() {
            smql_ast::SmqlError::GuardFailed { message, .. } => {
                assert!(message.contains("Division by zero"));
            }
            other => panic!("Expected GuardFailed, got {:?}", other),
        }
    }

    #[test]
    fn div_incompatible_error() {
        let ctx = ctx_with_data(vec![]);
        let expr = binop(
            lit(Value::Text("hello".to_string())),
            BinaryOperator::Div,
            lit(Value::Int(2)),
        );
        assert!(eval_expr(&expr, &ctx).is_err());
    }

    // ── 22. Functions: count, len, lower, upper, __map, unknown ─────────

    #[test]
    fn func_count_no_args() {
        let ctx = ctx_with_data(vec![]);
        let expr = func("count", vec![]);
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Int(0));
    }

    #[test]
    fn func_count_with_set() {
        let ctx = ctx_with_data(vec![(
            "tags",
            Value::Set(vec![
                Value::Text("a".to_string()),
                Value::Text("b".to_string()),
            ]),
        )]);
        let expr = func("count", vec![field(vec!["tags"])]);
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Int(2));
    }

    #[test]
    fn func_count_with_list() {
        let ctx = ctx_with_data(vec![(
            "items",
            Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)]),
        )]);
        let expr = func("count", vec![field(vec!["items"])]);
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Int(3));
    }

    #[test]
    fn func_count_with_non_collection() {
        let ctx = ctx_with_data(vec![("x", Value::Int(42))]);
        let expr = func("count", vec![field(vec!["x"])]);
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Int(0));
    }

    #[test]
    fn func_len_no_args() {
        let ctx = ctx_with_data(vec![]);
        let expr = func("len", vec![]);
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Int(0));
    }

    #[test]
    fn func_len_text() {
        let ctx = ctx_with_data(vec![("name", Value::Text("hello".to_string()))]);
        let expr = func("len", vec![field(vec!["name"])]);
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Int(5));
    }

    #[test]
    fn func_len_list() {
        let ctx = ctx_with_data(vec![(
            "items",
            Value::List(vec![Value::Int(1), Value::Int(2)]),
        )]);
        let expr = func("len", vec![field(vec!["items"])]);
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Int(2));
    }

    #[test]
    fn func_len_set() {
        let ctx = ctx_with_data(vec![(
            "tags",
            Value::Set(vec![
                Value::Text("a".to_string()),
                Value::Text("b".to_string()),
                Value::Text("c".to_string()),
            ]),
        )]);
        let expr = func("len", vec![field(vec!["tags"])]);
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Int(3));
    }

    #[test]
    fn func_len_non_collection() {
        let ctx = ctx_with_data(vec![("x", Value::Int(42))]);
        let expr = func("len", vec![field(vec!["x"])]);
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Int(0));
    }

    #[test]
    fn func_lower_no_args() {
        let ctx = ctx_with_data(vec![]);
        let expr = func("lower", vec![]);
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Null);
    }

    #[test]
    fn func_lower_text() {
        let ctx = ctx_with_data(vec![("name", Value::Text("Hello World".to_string()))]);
        let expr = func("lower", vec![field(vec!["name"])]);
        assert_eq!(
            eval_expr(&expr, &ctx).unwrap(),
            Value::Text("hello world".to_string())
        );
    }

    #[test]
    fn func_lower_non_text() {
        let ctx = ctx_with_data(vec![("x", Value::Int(42))]);
        let expr = func("lower", vec![field(vec!["x"])]);
        // Non-text returns the value as-is
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Int(42));
    }

    #[test]
    fn func_upper_no_args() {
        let ctx = ctx_with_data(vec![]);
        let expr = func("upper", vec![]);
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Null);
    }

    #[test]
    fn func_upper_text() {
        let ctx = ctx_with_data(vec![("name", Value::Text("Hello World".to_string()))]);
        let expr = func("upper", vec![field(vec!["name"])]);
        assert_eq!(
            eval_expr(&expr, &ctx).unwrap(),
            Value::Text("HELLO WORLD".to_string())
        );
    }

    #[test]
    fn func_upper_non_text() {
        let ctx = ctx_with_data(vec![("x", Value::Bool(true))]);
        let expr = func("upper", vec![field(vec!["x"])]);
        // Non-text returns the value as-is
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));
    }

    #[test]
    fn func_map_construction() {
        let ctx = ctx_with_data(vec![]);
        let expr = func(
            "__map",
            vec![
                lit(Value::Text("name".to_string())),
                lit(Value::Text("Alice".to_string())),
                lit(Value::Text("age".to_string())),
                lit(Value::Int(30)),
            ],
        );
        let result = eval_expr(&expr, &ctx).unwrap();
        let expected = Value::Map(make_map(vec![
            ("age", Value::Int(30)),
            ("name", Value::Text("Alice".to_string())),
        ]));
        assert_eq!(result, expected);
    }

    #[test]
    fn func_map_empty() {
        let ctx = ctx_with_data(vec![]);
        let expr = func("__map", vec![]);
        let result = eval_expr(&expr, &ctx).unwrap();
        assert_eq!(result, Value::Map(BTreeMap::new()));
    }

    #[test]
    fn func_map_odd_args_ignores_last() {
        let ctx = ctx_with_data(vec![]);
        // Odd number of args: last arg (the key without a value) is ignored
        let expr = func(
            "__map",
            vec![
                lit(Value::Text("key".to_string())),
                lit(Value::Int(1)),
                lit(Value::Text("orphan".to_string())),
            ],
        );
        let result = eval_expr(&expr, &ctx).unwrap();
        let expected = Value::Map(make_map(vec![("key", Value::Int(1))]));
        assert_eq!(result, expected);
    }

    #[test]
    fn func_unknown_returns_null() {
        let ctx = ctx_with_data(vec![]);
        let expr = func("some_unknown_function", vec![lit(Value::Int(1))]);
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Null);
    }

    // ── 23. resolve_children_from_expr non-field ────────────────────────

    #[test]
    fn all_with_non_field_collection_returns_vacuous_true() {
        let ctx = ctx_with_data(vec![]);
        // ALL with a literal as collection (not FieldAccess) resolves to empty children
        // ALL over empty => vacuously true
        let expr = Expression::new(ExpressionKind::All {
            collection: Box::new(lit(Value::Int(42))),
            predicate: Box::new(lit(Value::Bool(false))),
        });
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));
    }

    #[test]
    fn any_with_non_field_collection_returns_false() {
        let ctx = ctx_with_data(vec![]);
        // ANY with a literal as collection resolves to empty children
        // ANY over empty => false
        let expr = Expression::new(ExpressionKind::Any {
            collection: Box::new(lit(Value::Int(42))),
            predicate: Box::new(lit(Value::Bool(true))),
        });
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(false));
    }

    #[test]
    fn all_with_unknown_field_returns_vacuous_true() {
        let ctx = ctx_with_data(vec![]);
        // FieldAccess with unknown name resolves to empty children
        let expr = Expression::new(ExpressionKind::All {
            collection: Box::new(field(vec!["nonexistent"])),
            predicate: Box::new(lit(Value::Bool(false))),
        });
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));
    }

    // ── Additional coverage: eval_guard ─────────────────────────────────

    #[test]
    fn eval_guard_true_condition() {
        let ctx = ctx_with_data(vec![("x", Value::Int(10))]);
        let expr = binop(field(vec!["x"]), BinaryOperator::Gt, lit(Value::Int(5)));
        assert!(eval_guard(&expr, &ctx).unwrap());
    }

    #[test]
    fn eval_guard_false_condition() {
        let ctx = ctx_with_data(vec![("x", Value::Int(3))]);
        let expr = binop(field(vec!["x"]), BinaryOperator::Gt, lit(Value::Int(5)));
        assert!(!eval_guard(&expr, &ctx).unwrap());
    }

    // ── Additional: QualifiedAccess ─────────────────────────────────────

    #[test]
    fn qualified_access_actor_field() {
        let mut ctx = ctx_with_data(vec![]);
        ctx.actor = Some(ActorInfo {
            id: "u1".to_string(),
            role: Some("admin".to_string()),
            fields: HashMap::new(),
        });

        let expr = Expression::new(ExpressionKind::QualifiedAccess {
            root: Box::new(Expression::new(ExpressionKind::ActorRef)),
            path: vec!["role".to_string()],
        });
        let result = eval_expr(&expr, &ctx).unwrap();
        assert_eq!(result, Value::Text("admin".to_string()));
    }

    #[test]
    fn qualified_access_deep_path() {
        let inner_map = Value::Map(make_map(vec![("z", Value::Int(99))]));
        let outer_map = Value::Map(make_map(vec![("y", inner_map)]));
        let ctx = ctx_with_data(vec![("x", outer_map)]);

        let expr = Expression::new(ExpressionKind::QualifiedAccess {
            root: Box::new(field(vec!["x"])),
            path: vec!["y".to_string(), "z".to_string()],
        });
        let result = eval_expr(&expr, &ctx).unwrap();
        assert_eq!(result, Value::Int(99));
    }

    // ── Additional: DurationLiteral ─────────────────────────────────────

    #[test]
    fn duration_literal() {
        let ctx = ctx_with_data(vec![]);
        let dur = SmqlDuration::from_hours(2);
        let expr = Expression::new(ExpressionKind::DurationLiteral(dur.clone()));
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Duration(dur));
    }

    // ── Additional: StateIs and StateIn ─────────────────────────────────

    #[test]
    fn state_is_matching() {
        let ctx = ctx_with_data(vec![]); // state = "open"
        let expr = Expression::new(ExpressionKind::StateIs("open".to_string()));
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));
    }

    #[test]
    fn state_is_not_matching() {
        let ctx = ctx_with_data(vec![]);
        let expr = Expression::new(ExpressionKind::StateIs("closed".to_string()));
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(false));
    }

    #[test]
    fn state_in_matching() {
        let ctx = ctx_with_data(vec![]);
        let expr = Expression::new(ExpressionKind::StateIn(vec![
            "open".to_string(),
            "pending".to_string(),
        ]));
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));
    }

    #[test]
    fn state_in_not_matching() {
        let ctx = ctx_with_data(vec![]);
        let expr = Expression::new(ExpressionKind::StateIn(vec![
            "closed".to_string(),
            "archived".to_string(),
        ]));
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(false));
    }

    // ── Additional: IsSet / IsNotSet ────────────────────────────────────

    #[test]
    fn is_set_non_null() {
        let ctx = ctx_with_data(vec![("x", Value::Int(5))]);
        let expr = Expression::new(ExpressionKind::IsSet(Box::new(field(vec!["x"]))));
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));
    }

    #[test]
    fn is_set_null() {
        let ctx = ctx_with_data(vec![]);
        let expr = Expression::new(ExpressionKind::IsSet(Box::new(field(vec!["missing"]))));
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(false));
    }

    #[test]
    fn is_not_set_null() {
        let ctx = ctx_with_data(vec![]);
        let expr = Expression::new(ExpressionKind::IsNotSet(Box::new(field(vec!["missing"]))));
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));
    }

    #[test]
    fn is_not_set_non_null() {
        let ctx = ctx_with_data(vec![("x", Value::Int(5))]);
        let expr = Expression::new(ExpressionKind::IsNotSet(Box::new(field(vec!["x"]))));
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(false));
    }

    // ── Additional: InSet ───────────────────────────────────────────────

    #[test]
    fn in_set_found() {
        let ctx = ctx_with_data(vec![("x", Value::Int(2))]);
        let expr = Expression::new(ExpressionKind::InSet {
            expr: Box::new(field(vec!["x"])),
            values: vec![lit(Value::Int(1)), lit(Value::Int(2)), lit(Value::Int(3))],
        });
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));
    }

    #[test]
    fn in_set_not_found() {
        let ctx = ctx_with_data(vec![("x", Value::Int(5))]);
        let expr = Expression::new(ExpressionKind::InSet {
            expr: Box::new(field(vec!["x"])),
            values: vec![lit(Value::Int(1)), lit(Value::Int(2)), lit(Value::Int(3))],
        });
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(false));
    }

    // ── Additional: empty FieldAccess path ──────────────────────────────

    #[test]
    fn empty_field_access_returns_null() {
        let ctx = ctx_with_data(vec![]);
        let expr = Expression::new(ExpressionKind::FieldAccess(vec![]));
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Null);
    }

    // ── Additional: child STATE access ──────────────────────────────────

    #[test]
    fn child_state_access() {
        let mut ctx = ctx_with_data(vec![]);
        let child = make_child("c1", "Order", "shipped", vec![]);
        ctx.children.insert("order".to_string(), vec![child]);

        let expr = field(vec!["order", "STATE"]);
        let result = eval_expr(&expr, &ctx).unwrap();
        assert_eq!(result, Value::Text("shipped".to_string()));
    }

    #[test]
    fn child_state_access_empty_children() {
        let mut ctx = ctx_with_data(vec![]);
        ctx.children.insert("order".to_string(), vec![]);

        let expr = field(vec!["order", "STATE"]);
        let result = eval_expr(&expr, &ctx).unwrap();
        assert_eq!(result, Value::Null);
    }

    // ── Additional: child count access ──────────────────────────────────

    #[test]
    fn child_count_access() {
        let mut ctx = ctx_with_data(vec![]);
        let c1 = make_child("c1", "Item", "active", vec![]);
        let c2 = make_child("c2", "Item", "active", vec![]);
        ctx.children.insert("items".to_string(), vec![c1, c2]);

        let expr = field(vec!["items", "count"]);
        let result = eval_expr(&expr, &ctx).unwrap();
        assert_eq!(result, Value::Int(2));
    }

    // ── Additional: Count expression with children collection ───────────

    #[test]
    fn count_expr_children_collection() {
        let mut ctx = ctx_with_data(vec![]);
        let c1 = make_child("c1", "Item", "active", vec![]);
        let c2 = make_child("c2", "Item", "pending", vec![]);
        let c3 = make_child("c3", "Item", "active", vec![]);
        ctx.children.insert("items".to_string(), vec![c1, c2, c3]);

        let expr = Expression::new(ExpressionKind::Count(Some(Box::new(field(vec!["items"])))));
        let result = eval_expr(&expr, &ctx).unwrap();
        assert_eq!(result, Value::Int(3));
    }

    // ── Additional: ALL / ANY with actual children ──────────────────────

    #[test]
    fn all_children_all_match() {
        let mut ctx = ctx_with_data(vec![]);
        let c1 = make_child("c1", "Item", "done", vec![]);
        let c2 = make_child("c2", "Item", "done", vec![]);
        ctx.children.insert("items".to_string(), vec![c1, c2]);

        let expr = Expression::new(ExpressionKind::All {
            collection: Box::new(field(vec!["items"])),
            predicate: Box::new(Expression::new(ExpressionKind::StateIs("done".to_string()))),
        });
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));
    }

    #[test]
    fn all_children_not_all_match() {
        let mut ctx = ctx_with_data(vec![]);
        let c1 = make_child("c1", "Item", "done", vec![]);
        let c2 = make_child("c2", "Item", "pending", vec![]);
        ctx.children.insert("items".to_string(), vec![c1, c2]);

        let expr = Expression::new(ExpressionKind::All {
            collection: Box::new(field(vec!["items"])),
            predicate: Box::new(Expression::new(ExpressionKind::StateIs("done".to_string()))),
        });
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(false));
    }

    #[test]
    fn any_children_one_matches() {
        let mut ctx = ctx_with_data(vec![]);
        let c1 = make_child("c1", "Item", "pending", vec![]);
        let c2 = make_child("c2", "Item", "done", vec![]);
        ctx.children.insert("items".to_string(), vec![c1, c2]);

        let expr = Expression::new(ExpressionKind::Any {
            collection: Box::new(field(vec!["items"])),
            predicate: Box::new(Expression::new(ExpressionKind::StateIs("done".to_string()))),
        });
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));
    }

    #[test]
    fn any_children_none_match() {
        let mut ctx = ctx_with_data(vec![]);
        let c1 = make_child("c1", "Item", "pending", vec![]);
        let c2 = make_child("c2", "Item", "pending", vec![]);
        ctx.children.insert("items".to_string(), vec![c1, c2]);

        let expr = Expression::new(ExpressionKind::Any {
            collection: Box::new(field(vec!["items"])),
            predicate: Box::new(Expression::new(ExpressionKind::StateIs("done".to_string()))),
        });
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(false));
    }

    // ── Additional: SignalFrom ──────────────────────────────────────────

    #[test]
    fn signal_from_matching() {
        let mut ctx = ctx_with_data(vec![]);
        let c1 = make_child("c1", "Payment", "completed", vec![]);
        ctx.children.insert("payments".to_string(), vec![c1]);

        let expr = Expression::new(ExpressionKind::SignalFrom {
            machine: "Payment".to_string(),
            condition: Box::new(Expression::new(ExpressionKind::StateIs(
                "completed".to_string(),
            ))),
        });
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));
    }

    #[test]
    fn signal_from_not_matching() {
        let mut ctx = ctx_with_data(vec![]);
        let c1 = make_child("c1", "Payment", "pending", vec![]);
        ctx.children.insert("payments".to_string(), vec![c1]);

        let expr = Expression::new(ExpressionKind::SignalFrom {
            machine: "Payment".to_string(),
            condition: Box::new(Expression::new(ExpressionKind::StateIs(
                "completed".to_string(),
            ))),
        });
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(false));
    }

    #[test]
    fn signal_from_wrong_machine() {
        let mut ctx = ctx_with_data(vec![]);
        let c1 = make_child("c1", "Invoice", "completed", vec![]);
        ctx.children.insert("invoices".to_string(), vec![c1]);

        let expr = Expression::new(ExpressionKind::SignalFrom {
            machine: "Payment".to_string(),
            condition: Box::new(Expression::new(ExpressionKind::StateIs(
                "completed".to_string(),
            ))),
        });
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(false));
    }

    // ── Additional: Not operator ────────────────────────────────────────

    #[test]
    fn not_true() {
        let ctx = ctx_with_data(vec![]);
        let expr = unaryop(UnaryOperator::Not, lit(Value::Bool(true)));
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(false));
    }

    #[test]
    fn not_false() {
        let ctx = ctx_with_data(vec![]);
        let expr = unaryop(UnaryOperator::Not, lit(Value::Bool(false)));
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));
    }

    #[test]
    fn not_null_is_true() {
        let ctx = ctx_with_data(vec![]);
        // NOT Null => !false => true
        let expr = unaryop(UnaryOperator::Not, lit(Value::Null));
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));
    }

    // ── Additional: And / Or with truthy semantics ──────────────────────

    #[test]
    fn and_both_truthy() {
        let ctx = ctx_with_data(vec![]);
        let expr = binop(
            lit(Value::Int(1)),
            BinaryOperator::And,
            lit(Value::Text("yes".to_string())),
        );
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));
    }

    #[test]
    fn and_one_falsy() {
        let ctx = ctx_with_data(vec![]);
        let expr = binop(lit(Value::Int(1)), BinaryOperator::And, lit(Value::Int(0)));
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(false));
    }

    #[test]
    fn or_both_falsy() {
        let ctx = ctx_with_data(vec![]);
        let expr = binop(lit(Value::Int(0)), BinaryOperator::Or, lit(Value::Null));
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(false));
    }

    #[test]
    fn or_one_truthy() {
        let ctx = ctx_with_data(vec![]);
        let expr = binop(lit(Value::Int(0)), BinaryOperator::Or, lit(Value::Int(1)));
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));
    }

    // ── Additional: parent lowercase also works ─────────────────────────

    #[test]
    fn parent_field_access_case_insensitive() {
        let mut ctx = ctx_with_data(vec![]);
        let mut parent_data = HashMap::new();
        parent_data.insert("priority".to_string(), Value::Int(1));
        ctx.parent_data = Some(parent_data);
        ctx.parent_state = Some("active".to_string());

        // "Parent" (mixed case) should match because of to_uppercase()
        let expr = field(vec!["Parent", "priority"]);
        let result = eval_expr(&expr, &ctx).unwrap();
        assert_eq!(result, Value::Int(1));
    }

    // ── Additional: DateTime comparison ─────────────────────────────────

    #[test]
    fn compare_datetime_values() {
        let ctx = ctx_with_data(vec![]);
        let dt1 = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
        let dt2 = Utc.with_ymd_and_hms(2025, 6, 1, 0, 0, 0).unwrap();

        let expr = binop(
            lit(Value::DateTime(dt1)),
            BinaryOperator::Lt,
            lit(Value::DateTime(dt2)),
        );
        assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));
    }

    // ── Additional: truthy with Map and Float ───────────────────────────

    #[test]
    fn truthy_map_is_truthy() {
        let ctx = ctx_with_data(vec![]);
        let guard_expr = lit(Value::Map(make_map(vec![("a", Value::Int(1))])));
        assert!(eval_guard(&guard_expr, &ctx).unwrap());
    }

    #[test]
    fn truthy_float_nonzero() {
        let ctx = ctx_with_data(vec![]);
        let guard_expr = lit(Value::Float(0.1));
        assert!(eval_guard(&guard_expr, &ctx).unwrap());
    }

    #[test]
    fn truthy_duration_is_truthy() {
        let ctx = ctx_with_data(vec![]);
        let guard_expr = lit(Value::Duration(SmqlDuration::from_seconds(10)));
        assert!(eval_guard(&guard_expr, &ctx).unwrap());
    }
}
