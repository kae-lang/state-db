use chrono::{DateTime, Utc};
use smql_ast::expression::{BinaryOperator, Expression, ExpressionKind, UnaryOperator};
use smql_ast::value::{SmqlDuration, Value};
use smql_ast::{SmqlError, SmqlResult};
use std::collections::HashMap;

/// Information about a child instance (for composition guards).
#[derive(Debug, Clone)]
pub struct ChildInfo {
    pub id: String,
    pub machine: String,
    pub state: String,
    pub data: HashMap<String, Value>,
}

/// Context for evaluating expressions during transitions and guards.
pub struct EvalContext {
    /// Instance data fields
    pub data: HashMap<String, Value>,
    /// Current state of the instance
    pub state: String,
    /// Actor performing the transition (optional)
    pub actor: Option<ActorInfo>,
    /// When the instance entered the current state
    pub state_entered_at: DateTime<Utc>,
    /// When the instance was created
    pub created_at: DateTime<Utc>,
    /// Current timestamp
    pub now: DateTime<Utc>,
    /// Time remaining until timeout fires (None if no active timeout)
    pub timeout_remaining: Option<chrono::TimeDelta>,
    /// Child instances grouped by child ref name (for composition)
    pub children: HashMap<String, Vec<ChildInfo>>,
    /// Parent's data fields (if this instance has a parent)
    pub parent_data: Option<HashMap<String, Value>>,
    /// Parent's state (if this instance has a parent)
    pub parent_state: Option<String>,
}

/// Information about the actor performing a transition.
#[derive(Debug, Clone)]
pub struct ActorInfo {
    pub id: String,
    pub role: Option<String>,
    pub fields: HashMap<String, Value>,
}

impl EvalContext {
    pub fn new(data: HashMap<String, Value>, state: String) -> Self {
        let now = Utc::now();
        Self {
            data,
            state,
            actor: None,
            state_entered_at: now,
            created_at: now,
            now,
            timeout_remaining: None,
            children: HashMap::new(),
            parent_data: None,
            parent_state: None,
        }
    }
}

/// Evaluate an expression against a context, returning a Value.
pub fn eval_expr(expr: &Expression, ctx: &EvalContext) -> SmqlResult<Value> {
    match &expr.kind {
        ExpressionKind::Literal(v) => Ok(v.clone()),

        ExpressionKind::DurationLiteral(d) => Ok(Value::Duration(d.clone())),

        ExpressionKind::FieldAccess(path) => {
            if path.is_empty() {
                return Ok(Value::Null);
            }
            // Check if first path segment is a child ref name
            if path.len() >= 2 {
                if let Some(children) = ctx.children.get(&path[0]) {
                    let field = &path[1];
                    if field.to_uppercase() == "STATE" {
                        // child_ref.STATE — returns the state of the first child (or Null)
                        return Ok(children
                            .first()
                            .map(|c| Value::Text(c.state.clone()))
                            .unwrap_or(Value::Null));
                    }
                    if field == "count" {
                        return Ok(Value::Int(children.len() as i64));
                    }
                    // Access a data field on the first child
                    if let Some(child) = children.first() {
                        let mut current = child.data.get(field).cloned().unwrap_or(Value::Null);
                        for segment in &path[2..] {
                            current = access_field(&current, segment);
                        }
                        return Ok(current);
                    }
                    return Ok(Value::Null);
                }
                // Check PARENT access
                if path[0].to_uppercase() == "PARENT" {
                    let field = &path[1];
                    if field.to_uppercase() == "STATE" {
                        return Ok(ctx
                            .parent_state
                            .as_ref()
                            .map(|s| Value::Text(s.clone()))
                            .unwrap_or(Value::Null));
                    }
                    if let Some(parent_data) = &ctx.parent_data {
                        let mut current = parent_data.get(field).cloned().unwrap_or(Value::Null);
                        for segment in &path[2..] {
                            current = access_field(&current, segment);
                        }
                        return Ok(current);
                    }
                    return Ok(Value::Null);
                }
            }
            let mut current = ctx.data.get(&path[0]).cloned().unwrap_or(Value::Null);
            for segment in &path[1..] {
                current = access_field(&current, segment);
            }
            Ok(current)
        }

        ExpressionKind::SelfRef => {
            // SELF returns the full data map
            let map = ctx
                .data
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            Ok(Value::Map(map))
        }

        ExpressionKind::ActorRef => {
            if let Some(actor) = &ctx.actor {
                let mut map = std::collections::BTreeMap::new();
                map.insert("id".to_string(), Value::Text(actor.id.clone()));
                if let Some(role) = &actor.role {
                    map.insert("role".to_string(), Value::Text(role.clone()));
                }
                for (k, v) in &actor.fields {
                    map.insert(k.clone(), v.clone());
                }
                Ok(Value::Map(map))
            } else {
                Ok(Value::Null)
            }
        }

        ExpressionKind::QualifiedAccess { root, path } => {
            let root_val = eval_expr(root, ctx)?;
            let mut current = root_val;
            for segment in path {
                current = access_field(&current, segment);
            }
            Ok(current)
        }

        ExpressionKind::BinaryOp { left, op, right } => {
            let left_val = eval_expr(left, ctx)?;
            let right_val = eval_expr(right, ctx)?;
            eval_binary_op(&left_val, *op, &right_val)
        }

        ExpressionKind::UnaryOp { op, operand } => {
            let val = eval_expr(operand, ctx)?;
            eval_unary_op(*op, &val)
        }

        ExpressionKind::FunctionCall { name, args } => eval_function(name, args, ctx),

        ExpressionKind::StateIs(state_name) => Ok(Value::Bool(ctx.state == *state_name)),

        ExpressionKind::StateIn(states) => Ok(Value::Bool(states.contains(&ctx.state))),

        ExpressionKind::IsSet(inner) => {
            let val = eval_expr(inner, ctx)?;
            Ok(Value::Bool(!matches!(val, Value::Null)))
        }

        ExpressionKind::IsNotSet(inner) => {
            let val = eval_expr(inner, ctx)?;
            Ok(Value::Bool(matches!(val, Value::Null)))
        }

        ExpressionKind::InSet { expr, values } => {
            let val = eval_expr(expr, ctx)?;
            let mut found = false;
            for v in values {
                let member = eval_expr(v, ctx)?;
                if val == member {
                    found = true;
                    break;
                }
            }
            Ok(Value::Bool(found))
        }

        ExpressionKind::InList { expr, values } => {
            let val = eval_expr(expr, ctx)?;
            let mut found = false;
            for v in values {
                let member = eval_expr(v, ctx)?;
                if val == member {
                    found = true;
                    break;
                }
            }
            Ok(Value::Bool(found))
        }

        ExpressionKind::All {
            collection,
            predicate,
        } => {
            let children = resolve_children_from_expr(collection, ctx);
            if children.is_empty() {
                // ALL over empty collection is vacuously true
                return Ok(Value::Bool(true));
            }
            for child in &children {
                let child_ctx = make_child_eval_context(child, ctx);
                let result = eval_expr(predicate, &child_ctx)?;
                if !is_truthy(&result) {
                    return Ok(Value::Bool(false));
                }
            }
            Ok(Value::Bool(true))
        }

        ExpressionKind::Any {
            collection,
            predicate,
        } => {
            let children = resolve_children_from_expr(collection, ctx);
            for child in &children {
                let child_ctx = make_child_eval_context(child, ctx);
                let result = eval_expr(predicate, &child_ctx)?;
                if is_truthy(&result) {
                    return Ok(Value::Bool(true));
                }
            }
            Ok(Value::Bool(false))
        }

        ExpressionKind::Count(inner) => {
            match inner {
                Some(expr) => {
                    // Check if inner expression refers to a children collection
                    if let ExpressionKind::FieldAccess(path) = &expr.kind {
                        if path.len() == 1 {
                            if let Some(children) = ctx.children.get(&path[0]) {
                                return Ok(Value::Int(children.len() as i64));
                            }
                        }
                    }
                    let val = eval_expr(expr, ctx)?;
                    match val {
                        Value::List(items) => Ok(Value::Int(items.len() as i64)),
                        Value::Set(items) => Ok(Value::Int(items.len() as i64)),
                        _ => Ok(Value::Int(0)),
                    }
                }
                None => Ok(Value::Int(0)),
            }
        }

        ExpressionKind::SignalFrom { machine, condition } => {
            // Find children matching the given machine name
            for children in ctx.children.values() {
                for child in children {
                    if child.machine == *machine {
                        let child_ctx = make_child_eval_context(child, ctx);
                        let result = eval_expr(condition, &child_ctx)?;
                        if is_truthy(&result) {
                            return Ok(Value::Bool(true));
                        }
                    }
                }
            }
            Ok(Value::Bool(false))
        }

        ExpressionKind::Pattern(regex_str) => {
            // Return the pattern as a text value; comparison happens in binary ops
            Ok(Value::Text(regex_str.clone()))
        }
    }
}

/// Access a field on a Value (for dot-access chains).
fn access_field(value: &Value, field: &str) -> Value {
    match value {
        Value::Map(map) => map.get(field).cloned().unwrap_or(Value::Null),
        _ => Value::Null,
    }
}

/// Evaluate a binary operation.
fn eval_binary_op(left: &Value, op: BinaryOperator, right: &Value) -> SmqlResult<Value> {
    match op {
        BinaryOperator::And => Ok(Value::Bool(is_truthy(left) && is_truthy(right))),
        BinaryOperator::Or => Ok(Value::Bool(is_truthy(left) || is_truthy(right))),
        BinaryOperator::Eq => Ok(Value::Bool(values_equal(left, right))),
        BinaryOperator::NotEq => Ok(Value::Bool(!values_equal(left, right))),
        BinaryOperator::Lt => Ok(Value::Bool(
            compare_values(left, right) == Some(std::cmp::Ordering::Less),
        )),
        BinaryOperator::Gt => Ok(Value::Bool(
            compare_values(left, right) == Some(std::cmp::Ordering::Greater),
        )),
        BinaryOperator::LtEq => Ok(Value::Bool(matches!(
            compare_values(left, right),
            Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)
        ))),
        BinaryOperator::GtEq => Ok(Value::Bool(matches!(
            compare_values(left, right),
            Some(std::cmp::Ordering::Greater | std::cmp::Ordering::Equal)
        ))),
        BinaryOperator::Add => eval_arithmetic_add(left, right),
        BinaryOperator::Sub => eval_arithmetic_sub(left, right),
        BinaryOperator::Mul => eval_arithmetic_mul(left, right),
        BinaryOperator::Div => eval_arithmetic_div(left, right),
    }
}

/// Evaluate a unary operation.
fn eval_unary_op(op: UnaryOperator, val: &Value) -> SmqlResult<Value> {
    match op {
        UnaryOperator::Not => Ok(Value::Bool(!is_truthy(val))),
        UnaryOperator::Neg => match val {
            Value::Int(v) => Ok(Value::Int(-v)),
            Value::Float(v) => Ok(Value::Float(-v)),
            _ => Err(SmqlError::GuardFailed {
                message: format!("Cannot negate value: {}", val),
                guard_expr: format!("-{}", val),
                actual_value: Some(val.to_string()),
                hint: None,
            }),
        },
    }
}

/// Check if a value is truthy.
fn is_truthy(val: &Value) -> bool {
    match val {
        Value::Bool(b) => *b,
        Value::Null => false,
        Value::Int(0) => false,
        Value::Text(s) if s.is_empty() => false,
        Value::List(items) if items.is_empty() => false,
        Value::Set(items) if items.is_empty() => false,
        _ => true,
    }
}

/// Compare two values for equality with type coercion.
fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        // Same-type comparison
        (Value::Int(a), Value::Int(b)) => a == b,
        (Value::Float(a), Value::Float(b)) => a == b,
        (Value::Text(a), Value::Text(b)) => a == b,
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::Null, Value::Null) => true,
        // Cross-type coercion
        (Value::Int(a), Value::Float(b)) => (*a as f64) == *b,
        (Value::Float(a), Value::Int(b)) => *a == (*b as f64),
        // Default: structural equality
        _ => a == b,
    }
}

/// Compare two values for ordering.
fn compare_values(a: &Value, b: &Value) -> Option<std::cmp::Ordering> {
    match (a, b) {
        (Value::Int(a), Value::Int(b)) => Some(a.cmp(b)),
        (Value::Float(a), Value::Float(b)) => a.partial_cmp(b),
        (Value::Int(a), Value::Float(b)) => (*a as f64).partial_cmp(b),
        (Value::Float(a), Value::Int(b)) => a.partial_cmp(&(*b as f64)),
        (Value::Text(a), Value::Text(b)) => Some(a.cmp(b)),
        (Value::DateTime(a), Value::DateTime(b)) => Some(a.cmp(b)),
        (Value::Date(a), Value::Date(b)) => Some(a.cmp(b)),
        (Value::Duration(a), Value::Duration(b)) => Some(a.cmp(b)),
        _ => None,
    }
}

fn eval_arithmetic_add(left: &Value, right: &Value) -> SmqlResult<Value> {
    match (left, right) {
        (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a + b)),
        (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a + b)),
        (Value::Int(a), Value::Float(b)) => Ok(Value::Float(*a as f64 + b)),
        (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a + *b as f64)),
        (Value::Text(a), Value::Text(b)) => Ok(Value::Text(format!("{}{}", a, b))),
        (Value::Duration(a), Value::Duration(b)) => Ok(Value::Duration(
            SmqlDuration::from_seconds(a.seconds + b.seconds),
        )),
        _ => Err(SmqlError::GuardFailed {
            message: format!("Cannot add {} + {}", left, right),
            guard_expr: format!("{} + {}", left, right),
            actual_value: None,
            hint: None,
        }),
    }
}

fn eval_arithmetic_sub(left: &Value, right: &Value) -> SmqlResult<Value> {
    match (left, right) {
        (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a - b)),
        (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a - b)),
        (Value::Int(a), Value::Float(b)) => Ok(Value::Float(*a as f64 - b)),
        (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a - *b as f64)),
        (Value::Duration(a), Value::Duration(b)) => Ok(Value::Duration(
            SmqlDuration::from_seconds(a.seconds.saturating_sub(b.seconds)),
        )),
        _ => Err(SmqlError::GuardFailed {
            message: format!("Cannot subtract {} - {}", left, right),
            guard_expr: format!("{} - {}", left, right),
            actual_value: None,
            hint: None,
        }),
    }
}

fn eval_arithmetic_mul(left: &Value, right: &Value) -> SmqlResult<Value> {
    match (left, right) {
        (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a * b)),
        (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a * b)),
        (Value::Int(a), Value::Float(b)) => Ok(Value::Float(*a as f64 * b)),
        (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a * *b as f64)),
        _ => Err(SmqlError::GuardFailed {
            message: format!("Cannot multiply {} * {}", left, right),
            guard_expr: format!("{} * {}", left, right),
            actual_value: None,
            hint: None,
        }),
    }
}

fn eval_arithmetic_div(left: &Value, right: &Value) -> SmqlResult<Value> {
    match (left, right) {
        (Value::Int(_), Value::Int(0)) | (Value::Float(_), Value::Int(0)) => {
            Err(SmqlError::GuardFailed {
                message: "Division by zero".to_string(),
                guard_expr: format!("{} / {}", left, right),
                actual_value: None,
                hint: None,
            })
        }
        (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a / b)),
        (Value::Float(a), Value::Float(b)) => {
            if *b == 0.0 {
                Err(SmqlError::GuardFailed {
                    message: "Division by zero".to_string(),
                    guard_expr: format!("{} / {}", left, right),
                    actual_value: None,
                    hint: None,
                })
            } else {
                Ok(Value::Float(a / b))
            }
        }
        (Value::Int(a), Value::Float(b)) => {
            if *b == 0.0 {
                Err(SmqlError::GuardFailed {
                    message: "Division by zero".to_string(),
                    guard_expr: format!("{} / {}", left, right),
                    actual_value: None,
                    hint: None,
                })
            } else {
                Ok(Value::Float(*a as f64 / b))
            }
        }
        (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a / *b as f64)),
        _ => Err(SmqlError::GuardFailed {
            message: format!("Cannot divide {} / {}", left, right),
            guard_expr: format!("{} / {}", left, right),
            actual_value: None,
            hint: None,
        }),
    }
}

/// Evaluate a built-in function call.
fn eval_function(name: &str, args: &[Expression], ctx: &EvalContext) -> SmqlResult<Value> {
    match name.to_lowercase().as_str() {
        "elapsed" | "elapsed_in_state" => {
            let elapsed = ctx.now.signed_duration_since(ctx.state_entered_at);
            let seconds = elapsed.num_seconds().max(0) as u64;
            Ok(Value::Duration(SmqlDuration::from_seconds(seconds)))
        }

        "elapsed_since" => {
            // elapsed_since(state_name) — for now, same as elapsed()
            // Full implementation requires trail lookup
            let elapsed = ctx.now.signed_duration_since(ctx.state_entered_at);
            let seconds = elapsed.num_seconds().max(0) as u64;
            Ok(Value::Duration(SmqlDuration::from_seconds(seconds)))
        }

        "now" => Ok(Value::DateTime(ctx.now)),

        "today" => Ok(Value::Date(ctx.now.date_naive())),

        "timeout_remaining" => match &ctx.timeout_remaining {
            Some(remaining) => {
                let seconds = remaining.num_seconds().max(0) as u64;
                Ok(Value::Duration(SmqlDuration::from_seconds(seconds)))
            }
            None => Ok(Value::Null),
        },

        "count" => {
            if args.is_empty() {
                Ok(Value::Int(0))
            } else {
                let val = eval_expr(&args[0], ctx)?;
                match val {
                    Value::List(items) => Ok(Value::Int(items.len() as i64)),
                    Value::Set(items) => Ok(Value::Int(items.len() as i64)),
                    _ => Ok(Value::Int(0)),
                }
            }
        }

        "len" | "length" => {
            if args.is_empty() {
                return Ok(Value::Int(0));
            }
            let val = eval_expr(&args[0], ctx)?;
            match val {
                Value::Text(s) => Ok(Value::Int(s.len() as i64)),
                Value::List(items) => Ok(Value::Int(items.len() as i64)),
                Value::Set(items) => Ok(Value::Int(items.len() as i64)),
                _ => Ok(Value::Int(0)),
            }
        }

        "lower" | "lowercase" => {
            if args.is_empty() {
                return Ok(Value::Null);
            }
            let val = eval_expr(&args[0], ctx)?;
            match val {
                Value::Text(s) => Ok(Value::Text(s.to_lowercase())),
                other => Ok(other),
            }
        }

        "upper" | "uppercase" => {
            if args.is_empty() {
                return Ok(Value::Null);
            }
            let val = eval_expr(&args[0], ctx)?;
            match val {
                Value::Text(s) => Ok(Value::Text(s.to_uppercase())),
                other => Ok(other),
            }
        }

        "__map" => {
            // Internal: map literal { key: value, ... }
            // Args are alternating key (text literal), value pairs
            let mut map = std::collections::BTreeMap::new();
            let mut i = 0;
            while i + 1 < args.len() {
                let key = eval_expr(&args[i], ctx)?;
                let val = eval_expr(&args[i + 1], ctx)?;
                if let Value::Text(k) = key {
                    map.insert(k, val);
                }
                i += 2;
            }
            Ok(Value::Map(map))
        }

        _ => {
            // Unknown function — return Null for now
            Ok(Value::Null)
        }
    }
}

/// Resolve a collection expression to a list of child instances.
/// The collection is typically a FieldAccess like `items` referring to a child ref name.
fn resolve_children_from_expr<'a>(
    collection: &Expression,
    ctx: &'a EvalContext,
) -> Vec<&'a ChildInfo> {
    match &collection.kind {
        ExpressionKind::FieldAccess(path) if path.len() == 1 => {
            if let Some(children) = ctx.children.get(&path[0]) {
                children.iter().collect()
            } else {
                Vec::new()
            }
        }
        _ => Vec::new(),
    }
}

/// Create a temporary EvalContext for evaluating predicates against a child instance.
fn make_child_eval_context(child: &ChildInfo, parent_ctx: &EvalContext) -> EvalContext {
    EvalContext {
        data: child.data.clone(),
        state: child.state.clone(),
        actor: parent_ctx.actor.clone(),
        state_entered_at: parent_ctx.now, // Approximate
        created_at: parent_ctx.now,
        now: parent_ctx.now,
        timeout_remaining: None,
        children: HashMap::new(),
        parent_data: Some(parent_ctx.data.clone()),
        parent_state: Some(parent_ctx.state.clone()),
    }
}

/// Evaluate a guard expression and return whether it passes.
/// Returns the boolean result and the display string of the expression for error reporting.
pub fn eval_guard(expr: &Expression, ctx: &EvalContext) -> SmqlResult<bool> {
    let result = eval_expr(expr, ctx)?;
    Ok(is_truthy(&result))
}
