//! Comprehensive coverage tests for the expression evaluator (eval.rs).
//!
//! These tests target code paths not already covered by the internal eval_tests module,
//! focusing on edge cases, function variants, and less common expression kinds.

use smql_ast::expression::{BinaryOperator, Expression, ExpressionKind, UnaryOperator};
use smql_ast::value::{SmqlDuration, Value};
use smql_engine_core::eval::{eval_expr, eval_guard, ActorInfo, ChildInfo, EvalContext};
use std::collections::{BTreeMap, HashMap};

// ── Helpers ──────────────────────────────────────────────────────────────

fn ctx_with_data(data: Vec<(&str, Value)>) -> EvalContext {
    let map: HashMap<String, Value> = data.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
    EvalContext::new(map, "open".to_string())
}

fn lit(v: Value) -> Expression {
    Expression::new(ExpressionKind::Literal(v))
}

fn field(name: &str) -> Expression {
    Expression::new(ExpressionKind::FieldAccess(vec![name.to_string()]))
}

fn field_path(segments: &[&str]) -> Expression {
    Expression::new(ExpressionKind::FieldAccess(
        segments.iter().map(|s| s.to_string()).collect(),
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

// ═══════════════════════════════════════════════════════════════════════════
// 1. QualifiedAccess — non-Map root returns Null
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn qualified_access_non_map_root_returns_null() {
    let ctx = ctx_with_data(vec![("x", Value::Int(42))]);
    // x.foo where x is an Int — access_field on non-Map returns Null
    let expr = Expression::new(ExpressionKind::QualifiedAccess {
        root: Box::new(field("x")),
        path: vec!["foo".to_string()],
    });
    let result = eval_expr(&expr, &ctx).unwrap();
    assert_eq!(result, Value::Null);
}

#[test]
fn qualified_access_null_root_returns_null() {
    let ctx = ctx_with_data(vec![]);
    // missing_field.bar — root evaluates to Null, path access returns Null
    let expr = Expression::new(ExpressionKind::QualifiedAccess {
        root: Box::new(field("missing_field")),
        path: vec!["bar".to_string()],
    });
    let result = eval_expr(&expr, &ctx).unwrap();
    assert_eq!(result, Value::Null);
}

#[test]
fn qualified_access_self_ref_field() {
    let ctx = ctx_with_data(vec![
        ("name", Value::Text("Alice".to_string())),
        ("age", Value::Int(30)),
    ]);
    // SELF.name — QualifiedAccess with SelfRef root
    let expr = Expression::new(ExpressionKind::QualifiedAccess {
        root: Box::new(Expression::new(ExpressionKind::SelfRef)),
        path: vec!["name".to_string()],
    });
    let result = eval_expr(&expr, &ctx).unwrap();
    assert_eq!(result, Value::Text("Alice".to_string()));
}

// ═══════════════════════════════════════════════════════════════════════════
// 2. Functions: elapsed, elapsed_since, today, timeout_remaining
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn func_elapsed_returns_duration() {
    let ctx = ctx_with_data(vec![]);
    let expr = func("elapsed", vec![]);
    let result = eval_expr(&expr, &ctx).unwrap();
    // Since state_entered_at == now in EvalContext::new, elapsed should be ~0
    match result {
        Value::Duration(d) => assert!(d.seconds <= 1, "elapsed should be ~0s, got {}s", d.seconds),
        other => panic!("Expected Duration, got {:?}", other),
    }
}

#[test]
fn func_elapsed_in_state_returns_duration() {
    let ctx = ctx_with_data(vec![]);
    let expr = func("elapsed_in_state", vec![]);
    let result = eval_expr(&expr, &ctx).unwrap();
    match result {
        Value::Duration(d) => assert!(d.seconds <= 1),
        other => panic!("Expected Duration, got {:?}", other),
    }
}

#[test]
fn func_elapsed_since_returns_duration() {
    let ctx = ctx_with_data(vec![]);
    let expr = func(
        "elapsed_since",
        vec![lit(Value::Text("some_state".to_string()))],
    );
    let result = eval_expr(&expr, &ctx).unwrap();
    match result {
        Value::Duration(d) => assert!(d.seconds <= 1),
        other => panic!("Expected Duration, got {:?}", other),
    }
}

#[test]
fn func_now_returns_datetime() {
    let ctx = ctx_with_data(vec![]);
    let expr = func("now", vec![]);
    let result = eval_expr(&expr, &ctx).unwrap();
    match result {
        Value::DateTime(_) => {} // success
        other => panic!("Expected DateTime, got {:?}", other),
    }
}

#[test]
fn func_today_returns_date() {
    let ctx = ctx_with_data(vec![]);
    let expr = func("today", vec![]);
    let result = eval_expr(&expr, &ctx).unwrap();
    match result {
        Value::Date(_) => {} // success
        other => panic!("Expected Date, got {:?}", other),
    }
}

#[test]
fn func_timeout_remaining_with_some() {
    let mut ctx = ctx_with_data(vec![]);
    ctx.timeout_remaining = Some(chrono::TimeDelta::seconds(300));
    let expr = func("timeout_remaining", vec![]);
    let result = eval_expr(&expr, &ctx).unwrap();
    assert_eq!(result, Value::Duration(SmqlDuration::from_seconds(300)));
}

#[test]
fn func_timeout_remaining_with_none() {
    let ctx = ctx_with_data(vec![]);
    // timeout_remaining is None by default
    let expr = func("timeout_remaining", vec![]);
    let result = eval_expr(&expr, &ctx).unwrap();
    assert_eq!(result, Value::Null);
}

// ═══════════════════════════════════════════════════════════════════════════
// 3. Function aliases: length, lowercase, uppercase
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn func_length_alias_text() {
    let ctx = ctx_with_data(vec![("s", Value::Text("hello".to_string()))]);
    let expr = func("length", vec![field("s")]);
    assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Int(5));
}

#[test]
fn func_length_alias_list() {
    let ctx = ctx_with_data(vec![(
        "items",
        Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)]),
    )]);
    let expr = func("length", vec![field("items")]);
    assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Int(3));
}

#[test]
fn func_lowercase_alias() {
    let ctx = ctx_with_data(vec![("s", Value::Text("HELLO".to_string()))]);
    let expr = func("lowercase", vec![field("s")]);
    assert_eq!(
        eval_expr(&expr, &ctx).unwrap(),
        Value::Text("hello".to_string())
    );
}

#[test]
fn func_lowercase_no_args() {
    let ctx = ctx_with_data(vec![]);
    let expr = func("lowercase", vec![]);
    assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Null);
}

#[test]
fn func_uppercase_alias() {
    let ctx = ctx_with_data(vec![("s", Value::Text("hello".to_string()))]);
    let expr = func("uppercase", vec![field("s")]);
    assert_eq!(
        eval_expr(&expr, &ctx).unwrap(),
        Value::Text("HELLO".to_string())
    );
}

#[test]
fn func_uppercase_no_args() {
    let ctx = ctx_with_data(vec![]);
    let expr = func("uppercase", vec![]);
    assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Null);
}

// ═══════════════════════════════════════════════════════════════════════════
// 4. __map with non-text key (key is silently skipped)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn func_map_non_text_key_skipped() {
    let ctx = ctx_with_data(vec![]);
    // First pair: key=Int(1), value=Text("one") — key is not Text, so it's skipped
    // Second pair: key=Text("two"), value=Int(2) — this one is included
    let expr = func(
        "__map",
        vec![
            lit(Value::Int(1)),
            lit(Value::Text("one".to_string())),
            lit(Value::Text("two".to_string())),
            lit(Value::Int(2)),
        ],
    );
    let result = eval_expr(&expr, &ctx).unwrap();
    let expected = Value::Map(make_map(vec![("two", Value::Int(2))]));
    assert_eq!(result, expected);
}

// ═══════════════════════════════════════════════════════════════════════════
// 5. Date comparison (NaiveDate, not DateTime)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn compare_date_values_lt() {
    let ctx = ctx_with_data(vec![]);
    let d1 = chrono::NaiveDate::from_ymd_opt(2025, 1, 15).unwrap();
    let d2 = chrono::NaiveDate::from_ymd_opt(2025, 6, 15).unwrap();

    let expr = binop(
        lit(Value::Date(d1)),
        BinaryOperator::Lt,
        lit(Value::Date(d2)),
    );
    assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));
}

#[test]
fn compare_date_values_gt() {
    let ctx = ctx_with_data(vec![]);
    let d1 = chrono::NaiveDate::from_ymd_opt(2025, 6, 15).unwrap();
    let d2 = chrono::NaiveDate::from_ymd_opt(2025, 1, 15).unwrap();

    let expr = binop(
        lit(Value::Date(d1)),
        BinaryOperator::Gt,
        lit(Value::Date(d2)),
    );
    assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));
}

#[test]
fn compare_date_values_eq() {
    let ctx = ctx_with_data(vec![]);
    let d = chrono::NaiveDate::from_ymd_opt(2025, 3, 1).unwrap();

    let expr = binop(
        lit(Value::Date(d)),
        BinaryOperator::GtEq,
        lit(Value::Date(d)),
    );
    assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));
}

// ═══════════════════════════════════════════════════════════════════════════
// 6. values_equal — structural equality for Map and List
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn values_equal_map_structural() {
    let ctx = ctx_with_data(vec![]);
    let map1 = Value::Map(make_map(vec![
        ("a", Value::Int(1)),
        ("b", Value::Text("x".to_string())),
    ]));
    let map2 = Value::Map(make_map(vec![
        ("a", Value::Int(1)),
        ("b", Value::Text("x".to_string())),
    ]));
    let expr = binop(lit(map1), BinaryOperator::Eq, lit(map2));
    assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));
}

#[test]
fn values_equal_map_not_equal() {
    let ctx = ctx_with_data(vec![]);
    let map1 = Value::Map(make_map(vec![("a", Value::Int(1))]));
    let map2 = Value::Map(make_map(vec![("a", Value::Int(2))]));
    let expr = binop(lit(map1), BinaryOperator::Eq, lit(map2));
    assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(false));
}

#[test]
fn values_equal_list_structural() {
    let ctx = ctx_with_data(vec![]);
    let list1 = Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
    let list2 = Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
    let expr = binop(lit(list1), BinaryOperator::Eq, lit(list2));
    assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));
}

#[test]
fn values_equal_list_not_equal() {
    let ctx = ctx_with_data(vec![]);
    let list1 = Value::List(vec![Value::Int(1), Value::Int(2)]);
    let list2 = Value::List(vec![Value::Int(1), Value::Int(3)]);
    let expr = binop(lit(list1), BinaryOperator::Eq, lit(list2));
    assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(false));
}

// ═══════════════════════════════════════════════════════════════════════════
// 7. is_truthy edge cases — Float(0.0) falls through to the _ => true arm
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn truthy_float_zero_is_truthy() {
    // Float(0.0) is NOT matched by Int(0), and falls through to _ => true
    // This is by design: is_truthy does not check Float(0.0)
    let ctx = ctx_with_data(vec![]);
    let guard_expr = lit(Value::Float(0.0));
    assert_eq!(eval_guard(&guard_expr, &ctx).unwrap(), true);
}

#[test]
fn truthy_empty_map_is_truthy() {
    // Empty map falls through to _ => true
    let ctx = ctx_with_data(vec![]);
    let guard_expr = lit(Value::Map(BTreeMap::new()));
    assert_eq!(eval_guard(&guard_expr, &ctx).unwrap(), true);
}

#[test]
fn truthy_negative_int_is_truthy() {
    let ctx = ctx_with_data(vec![]);
    let guard_expr = lit(Value::Int(-1));
    assert_eq!(eval_guard(&guard_expr, &ctx).unwrap(), true);
}

// ═══════════════════════════════════════════════════════════════════════════
// 8. ALL/ANY over empty children Vec (not unknown field, but actual empty Vec)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn all_empty_children_vec_vacuous_true() {
    let mut ctx = ctx_with_data(vec![]);
    ctx.children.insert("items".to_string(), vec![]);

    let expr = Expression::new(ExpressionKind::All {
        collection: Box::new(field("items")),
        predicate: Box::new(lit(Value::Bool(false))), // predicate doesn't matter
    });
    assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));
}

#[test]
fn any_empty_children_vec_false() {
    let mut ctx = ctx_with_data(vec![]);
    ctx.children.insert("items".to_string(), vec![]);

    let expr = Expression::new(ExpressionKind::Any {
        collection: Box::new(field("items")),
        predicate: Box::new(lit(Value::Bool(true))), // predicate doesn't matter
    });
    assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(false));
}

// ═══════════════════════════════════════════════════════════════════════════
// 9. Count expression with multi-segment field path (not matching children)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn count_expr_multi_segment_path_not_children() {
    // path.len() != 1, so it won't match children lookup; falls through to eval_expr
    let inner = Value::Map(make_map(vec![(
        "items",
        Value::List(vec![Value::Int(1), Value::Int(2)]),
    )]));
    let ctx = ctx_with_data(vec![("data", inner)]);

    let expr = Expression::new(ExpressionKind::Count(Some(Box::new(field_path(&[
        "data", "items",
    ])))));
    let result = eval_expr(&expr, &ctx).unwrap();
    // data.items resolves to a List of 2 elements
    assert_eq!(result, Value::Int(2));
}

#[test]
fn count_expr_field_not_in_children_non_collection() {
    // Single-segment path, but the name isn't a children key — falls through to eval_expr
    let ctx = ctx_with_data(vec![("x", Value::Int(42))]);
    let expr = Expression::new(ExpressionKind::Count(Some(Box::new(field("x")))));
    // x is Int(42), not a List/Set, so count returns 0
    assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Int(0));
}

// ═══════════════════════════════════════════════════════════════════════════
// 10. SignalFrom — no children at all
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn signal_from_no_children() {
    let ctx = ctx_with_data(vec![]);
    let expr = Expression::new(ExpressionKind::SignalFrom {
        machine: "Payment".to_string(),
        condition: Box::new(Expression::new(ExpressionKind::StateIs(
            "completed".to_string(),
        ))),
    });
    assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(false));
}

#[test]
fn signal_from_multiple_children_groups() {
    let mut ctx = ctx_with_data(vec![]);
    // Two child groups, signal should find across groups
    let c1 = make_child("c1", "Invoice", "pending", vec![]);
    ctx.children.insert("invoices".to_string(), vec![c1]);
    let c2 = make_child("c2", "Payment", "completed", vec![]);
    ctx.children.insert("payments".to_string(), vec![c2]);

    let expr = Expression::new(ExpressionKind::SignalFrom {
        machine: "Payment".to_string(),
        condition: Box::new(Expression::new(ExpressionKind::StateIs(
            "completed".to_string(),
        ))),
    });
    assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));
}

// ═══════════════════════════════════════════════════════════════════════════
// 11. Unary Neg on Null/Bool (error cases beyond Text)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn neg_null_error() {
    let ctx = ctx_with_data(vec![]);
    let expr = unaryop(UnaryOperator::Neg, lit(Value::Null));
    assert!(eval_expr(&expr, &ctx).is_err());
}

#[test]
fn neg_list_error() {
    let ctx = ctx_with_data(vec![]);
    let expr = unaryop(UnaryOperator::Neg, lit(Value::List(vec![Value::Int(1)])));
    assert!(eval_expr(&expr, &ctx).is_err());
}

// ═══════════════════════════════════════════════════════════════════════════
// 12. ALL/ANY predicates that use child data fields
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn all_children_data_predicate() {
    let mut ctx = ctx_with_data(vec![]);
    let c1 = make_child("c1", "Item", "active", vec![("qty", Value::Int(10))]);
    let c2 = make_child("c2", "Item", "active", vec![("qty", Value::Int(5))]);
    ctx.children.insert("items".to_string(), vec![c1, c2]);

    // ALL items WHERE qty > 3
    let predicate = binop(field("qty"), BinaryOperator::Gt, lit(Value::Int(3)));
    let expr = Expression::new(ExpressionKind::All {
        collection: Box::new(field("items")),
        predicate: Box::new(predicate),
    });
    assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));
}

#[test]
fn all_children_data_predicate_one_fails() {
    let mut ctx = ctx_with_data(vec![]);
    let c1 = make_child("c1", "Item", "active", vec![("qty", Value::Int(10))]);
    let c2 = make_child("c2", "Item", "active", vec![("qty", Value::Int(2))]);
    ctx.children.insert("items".to_string(), vec![c1, c2]);

    // ALL items WHERE qty > 3 — second child has qty=2, fails
    let predicate = binop(field("qty"), BinaryOperator::Gt, lit(Value::Int(3)));
    let expr = Expression::new(ExpressionKind::All {
        collection: Box::new(field("items")),
        predicate: Box::new(predicate),
    });
    assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(false));
}

#[test]
fn any_children_data_predicate() {
    let mut ctx = ctx_with_data(vec![]);
    let c1 = make_child("c1", "Item", "active", vec![("qty", Value::Int(1))]);
    let c2 = make_child("c2", "Item", "active", vec![("qty", Value::Int(10))]);
    ctx.children.insert("items".to_string(), vec![c1, c2]);

    // ANY items WHERE qty > 5 — second child passes
    let predicate = binop(field("qty"), BinaryOperator::Gt, lit(Value::Int(5)));
    let expr = Expression::new(ExpressionKind::Any {
        collection: Box::new(field("items")),
        predicate: Box::new(predicate),
    });
    assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));
}

// ═══════════════════════════════════════════════════════════════════════════
// 13. Child context inherits parent data/state
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn all_children_can_access_parent_state() {
    let mut ctx = ctx_with_data(vec![("total", Value::Int(100))]);
    let c1 = make_child("c1", "Item", "active", vec![]);
    ctx.children.insert("items".to_string(), vec![c1]);

    // In child context, PARENT.STATE should be "open" (the parent's state)
    let predicate = Expression::new(ExpressionKind::StateIs("active".to_string()));
    let expr = Expression::new(ExpressionKind::All {
        collection: Box::new(field("items")),
        predicate: Box::new(predicate),
    });
    assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));
}

// ═══════════════════════════════════════════════════════════════════════════
// 14. compare_values — incompatible types return false for all comparisons
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn compare_incompatible_gt() {
    let ctx = ctx_with_data(vec![]);
    let expr = binop(
        lit(Value::Bool(true)),
        BinaryOperator::Gt,
        lit(Value::Int(5)),
    );
    assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(false));
}

#[test]
fn compare_incompatible_lt_eq() {
    let ctx = ctx_with_data(vec![]);
    let expr = binop(
        lit(Value::Bool(true)),
        BinaryOperator::LtEq,
        lit(Value::Int(5)),
    );
    assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(false));
}

#[test]
fn compare_incompatible_gt_eq() {
    let ctx = ctx_with_data(vec![]);
    let expr = binop(
        lit(Value::Bool(true)),
        BinaryOperator::GtEq,
        lit(Value::Int(5)),
    );
    assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(false));
}

// ═══════════════════════════════════════════════════════════════════════════
// 15. Arithmetic error messages
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn add_bool_and_list_error() {
    let ctx = ctx_with_data(vec![]);
    let expr = binop(
        lit(Value::Bool(true)),
        BinaryOperator::Add,
        lit(Value::List(vec![])),
    );
    let result = eval_expr(&expr, &ctx);
    assert!(result.is_err());
    match result.unwrap_err() {
        smql_ast::SmqlError::GuardFailed { message, .. } => {
            assert!(message.contains("Cannot add"));
        }
        other => panic!("Expected GuardFailed, got {:?}", other),
    }
}

#[test]
fn sub_bool_error() {
    let ctx = ctx_with_data(vec![]);
    let expr = binop(
        lit(Value::Bool(true)),
        BinaryOperator::Sub,
        lit(Value::Bool(false)),
    );
    let result = eval_expr(&expr, &ctx);
    assert!(result.is_err());
    match result.unwrap_err() {
        smql_ast::SmqlError::GuardFailed { message, .. } => {
            assert!(message.contains("Cannot subtract"));
        }
        other => panic!("Expected GuardFailed, got {:?}", other),
    }
}

#[test]
fn mul_bool_error() {
    let ctx = ctx_with_data(vec![]);
    let expr = binop(
        lit(Value::Bool(true)),
        BinaryOperator::Mul,
        lit(Value::Bool(false)),
    );
    let result = eval_expr(&expr, &ctx);
    assert!(result.is_err());
    match result.unwrap_err() {
        smql_ast::SmqlError::GuardFailed { message, .. } => {
            assert!(message.contains("Cannot multiply"));
        }
        other => panic!("Expected GuardFailed, got {:?}", other),
    }
}

#[test]
fn div_bool_error() {
    let ctx = ctx_with_data(vec![]);
    let expr = binop(
        lit(Value::Bool(true)),
        BinaryOperator::Div,
        lit(Value::Bool(false)),
    );
    let result = eval_expr(&expr, &ctx);
    assert!(result.is_err());
    match result.unwrap_err() {
        smql_ast::SmqlError::GuardFailed { message, .. } => {
            assert!(message.contains("Cannot divide"));
        }
        other => panic!("Expected GuardFailed, got {:?}", other),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 16. InList edge cases
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn in_list_empty_values() {
    let ctx = ctx_with_data(vec![("x", Value::Int(1))]);
    let expr = Expression::new(ExpressionKind::InList {
        expr: Box::new(field("x")),
        values: vec![],
    });
    assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(false));
}

#[test]
fn in_set_empty_values() {
    let ctx = ctx_with_data(vec![("x", Value::Int(1))]);
    let expr = Expression::new(ExpressionKind::InSet {
        expr: Box::new(field("x")),
        values: vec![],
    });
    assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(false));
}

// ═══════════════════════════════════════════════════════════════════════════
// 17. values_equal — cross-type non-equal
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn values_equal_int_vs_text_not_equal() {
    let ctx = ctx_with_data(vec![]);
    // Int(5) == Text("5") — different types, no coercion => structural equality => false
    let expr = binop(
        lit(Value::Int(5)),
        BinaryOperator::Eq,
        lit(Value::Text("5".to_string())),
    );
    assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(false));
}

#[test]
fn values_equal_null_vs_int_not_equal() {
    let ctx = ctx_with_data(vec![]);
    let expr = binop(lit(Value::Null), BinaryOperator::Eq, lit(Value::Int(0)));
    assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(false));
}

#[test]
fn values_not_equal_null_vs_int() {
    let ctx = ctx_with_data(vec![]);
    let expr = binop(lit(Value::Null), BinaryOperator::NotEq, lit(Value::Int(0)));
    assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Bool(true));
}

// ═══════════════════════════════════════════════════════════════════════════
// 18. DurationLiteral expression
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn duration_literal_evaluates() {
    let ctx = ctx_with_data(vec![]);
    let dur = SmqlDuration::from_minutes(30);
    let expr = Expression::new(ExpressionKind::DurationLiteral(dur.clone()));
    assert_eq!(eval_expr(&expr, &ctx).unwrap(), Value::Duration(dur));
}

// ═══════════════════════════════════════════════════════════════════════════
// 19. Complex combined expressions
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn complex_and_or_not_combination() {
    let ctx = ctx_with_data(vec![("x", Value::Int(10)), ("y", Value::Int(5))]);

    // (x > 5 AND y < 10) OR NOT(x == 0)
    let x_gt_5 = binop(field("x"), BinaryOperator::Gt, lit(Value::Int(5)));
    let y_lt_10 = binop(field("y"), BinaryOperator::Lt, lit(Value::Int(10)));
    let and_expr = binop(x_gt_5, BinaryOperator::And, y_lt_10);
    let x_eq_0 = binop(field("x"), BinaryOperator::Eq, lit(Value::Int(0)));
    let not_x_eq_0 = unaryop(UnaryOperator::Not, x_eq_0);
    let full = binop(and_expr, BinaryOperator::Or, not_x_eq_0);

    assert_eq!(eval_guard(&full, &ctx).unwrap(), true);
}

#[test]
fn nested_arithmetic_expression() {
    let ctx = ctx_with_data(vec![("x", Value::Int(10))]);

    // (x + 5) * 2 == 30
    let x_plus_5 = binop(field("x"), BinaryOperator::Add, lit(Value::Int(5)));
    let times_2 = binop(x_plus_5, BinaryOperator::Mul, lit(Value::Int(2)));
    let eq_30 = binop(times_2, BinaryOperator::Eq, lit(Value::Int(30)));

    assert_eq!(eval_expr(&eq_30, &ctx).unwrap(), Value::Bool(true));
}

// ═══════════════════════════════════════════════════════════════════════════
// 20. Child deep path with nested Map data
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn child_deep_path_nested_map() {
    let mut ctx = ctx_with_data(vec![]);
    let inner = Value::Map(make_map(vec![("zip", Value::Text("10001".to_string()))]));
    let address = Value::Map(make_map(vec![("location", inner)]));
    let child = make_child("c1", "Order", "active", vec![("address", address)]);
    ctx.children.insert("orders".to_string(), vec![child]);

    // orders.address.location.zip
    let expr = field_path(&["orders", "address", "location", "zip"]);
    let result = eval_expr(&expr, &ctx).unwrap();
    assert_eq!(result, Value::Text("10001".to_string()));
}

#[test]
fn child_deep_path_missing_intermediate() {
    let mut ctx = ctx_with_data(vec![]);
    let child = make_child(
        "c1",
        "Order",
        "active",
        vec![("name", Value::Text("test".to_string()))],
    );
    ctx.children.insert("orders".to_string(), vec![child]);

    // orders.name.nonexistent — name is Text, not Map, so returns Null
    let expr = field_path(&["orders", "name", "nonexistent"]);
    let result = eval_expr(&expr, &ctx).unwrap();
    assert_eq!(result, Value::Null);
}

// ═══════════════════════════════════════════════════════════════════════════
// 21. PARENT deep path with nested Map data
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn parent_deep_path_missing_intermediate() {
    let mut ctx = ctx_with_data(vec![]);
    let mut parent_data = HashMap::new();
    parent_data.insert("name".to_string(), Value::Text("parent".to_string()));
    ctx.parent_data = Some(parent_data);

    // PARENT.name.nonexistent — name is Text, so returns Null
    let expr = field_path(&["PARENT", "name", "nonexistent"]);
    let result = eval_expr(&expr, &ctx).unwrap();
    assert_eq!(result, Value::Null);
}

// ═══════════════════════════════════════════════════════════════════════════
// 22. QualifiedAccess with empty path
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn qualified_access_empty_path_returns_root() {
    let ctx = ctx_with_data(vec![("x", Value::Int(42))]);
    let expr = Expression::new(ExpressionKind::QualifiedAccess {
        root: Box::new(field("x")),
        path: vec![],
    });
    let result = eval_expr(&expr, &ctx).unwrap();
    assert_eq!(result, Value::Int(42));
}

// ═══════════════════════════════════════════════════════════════════════════
// 23. Actor with fields in QualifiedAccess
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn qualified_access_actor_missing_field() {
    let mut ctx = ctx_with_data(vec![]);
    ctx.actor = Some(ActorInfo {
        id: "u1".to_string(),
        role: Some("admin".to_string()),
        fields: HashMap::new(),
    });

    // ACTOR.nonexistent — Map doesn't have this key
    let expr = Expression::new(ExpressionKind::QualifiedAccess {
        root: Box::new(Expression::new(ExpressionKind::ActorRef)),
        path: vec!["nonexistent".to_string()],
    });
    let result = eval_expr(&expr, &ctx).unwrap();
    assert_eq!(result, Value::Null);
}

#[test]
fn qualified_access_actor_none_returns_null_chain() {
    let ctx = ctx_with_data(vec![]);
    // No actor set — ActorRef returns Null, then .field on Null returns Null
    let expr = Expression::new(ExpressionKind::QualifiedAccess {
        root: Box::new(Expression::new(ExpressionKind::ActorRef)),
        path: vec!["id".to_string()],
    });
    let result = eval_expr(&expr, &ctx).unwrap();
    assert_eq!(result, Value::Null);
}

// ═══════════════════════════════════════════════════════════════════════════
// 24. Duration arithmetic with zero
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn add_duration_zero() {
    let ctx = ctx_with_data(vec![]);
    let d1 = Value::Duration(SmqlDuration::from_seconds(60));
    let d2 = Value::Duration(SmqlDuration::from_seconds(0));
    let expr = binop(lit(d1), BinaryOperator::Add, lit(d2));
    assert_eq!(
        eval_expr(&expr, &ctx).unwrap(),
        Value::Duration(SmqlDuration::from_seconds(60))
    );
}

#[test]
fn sub_duration_same_is_zero() {
    let ctx = ctx_with_data(vec![]);
    let d = Value::Duration(SmqlDuration::from_seconds(100));
    let expr = binop(lit(d.clone()), BinaryOperator::Sub, lit(d));
    assert_eq!(
        eval_expr(&expr, &ctx).unwrap(),
        Value::Duration(SmqlDuration::from_seconds(0))
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// 25. eval_guard with non-boolean result
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn eval_guard_with_int_result() {
    let ctx = ctx_with_data(vec![("x", Value::Int(42))]);
    // eval_guard on a field that returns Int(42) — truthy
    assert_eq!(eval_guard(&field("x"), &ctx).unwrap(), true);
}

#[test]
fn eval_guard_with_null_field() {
    let ctx = ctx_with_data(vec![]);
    // eval_guard on missing field returns Null — falsy
    assert_eq!(eval_guard(&field("missing"), &ctx).unwrap(), false);
}
