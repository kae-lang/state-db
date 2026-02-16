# AST Walking & EvalContext

The expression evaluator lives in `smql-engine-core`, not in `smql-query`. This avoids a circular dependency: queries need evaluation, and evaluation needs engine context. Keeping `eval_expr()` in the engine crate breaks the cycle.

## EvalContext

Every expression is evaluated within an `EvalContext` that provides access to instance data and runtime information:

```rust
pub struct EvalContext {
    /// Instance data fields (BTreeMap<String, Value>)
    pub data: BTreeMap<String, Value>,

    /// The actor performing the current operation (Option<Value>)
    pub actor: Option<Value>,

    /// SELF reference — the instance ID
    pub self_id: Option<InstanceId>,

    /// Time remaining until timeout (for queries)
    pub timeout_remaining: Option<Duration>,

    /// Children data for ALL/ANY predicates (for composition queries)
    pub children: Vec<BTreeMap<String, Value>>,
}
```

### Field Resolution

When the evaluator encounters a field reference like `priority`, it looks up the key in `data`. The `ACTOR` keyword resolves to the `actor` field. `SELF` resolves to the instance ID.

**Important**: `ACTOR` evaluates to a `Value::Map` containing `{id: "...", role: "..."}`, not a simple string. So a guard like `ACTOR == assignee` requires `assignee` to be a `BTreeMap`, not `Text`. This is a common source of test failures.

## eval_expr

The core function walks the `Expression` AST recursively:

```rust
pub fn eval_expr(expr: &Expression, ctx: &EvalContext) -> Result<Value> {
    match expr {
        Expression::Literal(value) => Ok(value.clone()),
        Expression::FieldAccess(name) => resolve_field(name, ctx),
        Expression::BinaryOp { left, op, right } => eval_binary(left, op, right, ctx),
        Expression::UnaryOp { op, operand } => eval_unary(op, operand, ctx),
        Expression::FunctionCall { name, args } => eval_function(name, args, ctx),
        // ...
    }
}
```

### Literal

Returns the `Value` directly. Literals include strings, integers, floats, booleans, and null.

### FieldAccess

Looks up a field name in the context:

```rust
fn resolve_field(name: &str, ctx: &EvalContext) -> Result<Value> {
    match name {
        "ACTOR" => ctx.actor.clone().ok_or(SmqlError::NoActor),
        "SELF" => ctx.self_id.clone()
            .map(Value::Text)
            .ok_or(SmqlError::NoSelf),
        _ => ctx.data.get(name)
            .cloned()
            .ok_or(SmqlError::FieldNotFound(name.to_string())),
    }
}
```

Nested field access (e.g., `actor.role`) is handled by first resolving `actor` to a `Value::Map`, then looking up `role` within it.

### BinaryOp

Evaluates both sides, then applies the operator:

```rust
fn eval_binary(
    left: &Expression,
    op: &BinaryOp,
    right: &Expression,
    ctx: &EvalContext,
) -> Result<Value> {
    let lhs = eval_expr(left, ctx)?;
    let rhs = eval_expr(right, ctx)?;

    match op {
        BinaryOp::Eq => Ok(Value::Bool(lhs == rhs)),
        BinaryOp::Neq => Ok(Value::Bool(lhs != rhs)),
        BinaryOp::Gt => compare(&lhs, &rhs, |a, b| a > b),
        BinaryOp::Lt => compare(&lhs, &rhs, |a, b| a < b),
        BinaryOp::And => logical_and(&lhs, &rhs),
        BinaryOp::Or => logical_or(&lhs, &rhs),
        BinaryOp::Add => arithmetic(&lhs, &rhs, |a, b| a + b),
        // ...
    }
}
```

### Value Comparison Rules

Comparison is only defined for values of the same type. Cross-type comparisons fail:

| Left | Right | Result |
|---|---|---|
| `Int(5)` | `Int(3)` | `Bool(true)` for `>` |
| `Text("a")` | `Text("b")` | `Bool(true)` for `<` |
| `Money(9999, "USD")` | `Int(0)` | **Error** |
| `Bool(true)` | `Bool(false)` | `Bool(false)` for `==` |

The `Money(9999, "USD") > Int(0)` case is a common pitfall. Guards like `total > 0` fail if `total` is a Money type. The guard must compare against a Money value or the field must be stored as Int.

`Value::Map` uses `BTreeMap` (not `HashMap`). This provides deterministic ordering and makes equality comparison well-defined.

### UnaryOp

Currently only `NOT` is supported:

```rust
fn eval_unary(op: &UnaryOp, operand: &Expression, ctx: &EvalContext) -> Result<Value> {
    let val = eval_expr(operand, ctx)?;
    match op {
        UnaryOp::Not => match val {
            Value::Bool(b) => Ok(Value::Bool(!b)),
            _ => Err(SmqlError::TypeError("NOT requires Bool")),
        },
    }
}
```

### FunctionCall

Function calls dispatch to built-in implementations:

```rust
fn eval_function(
    name: &str,
    args: &[Expression],
    ctx: &EvalContext,
) -> Result<Value> {
    match name {
        "__map" => eval_map_literal(args, ctx),
        "__spawn" => Err(SmqlError::Internal("__spawn handled at engine level")),
        "elapsed_since" => eval_elapsed_since(args, ctx),
        "len" => eval_len(args, ctx),
        "contains" => eval_contains(args, ctx),
        "now" => Ok(Value::DateTime(Utc::now())),
        _ => Err(SmqlError::UnknownFunction(name.to_string())),
    }
}
```

#### __map (Map Literals)

The parser converts `{key: value, ...}` syntax into a `__map` FunctionCall. The evaluator constructs a `BTreeMap<String, Value>` from the arguments:

```
{assignee: ACTOR, priority: "high"}
```

becomes:

```rust
Expression::FunctionCall {
    name: "__map",
    args: vec![
        Expression::Literal(Value::Text("assignee")),
        Expression::FieldAccess("ACTOR"),
        Expression::Literal(Value::Text("priority")),
        Expression::Literal(Value::Text("high")),
    ],
}
```

The args are pairs: key, value, key, value, etc. The evaluator zips them into a map.

#### __spawn (Spawn in MUTATE)

`__spawn` is never evaluated by `eval_expr()`. The engine detects it before calling the evaluator and handles it as an async operation (spawning a child instance). If `eval_expr()` encounters `__spawn`, it returns an error indicating the engine should have caught it earlier.

SPAWN requires `{}` even with no data: `SPAWN Machine {}`.

#### Built-in Functions

| Function | Signature | Returns |
|---|---|---|
| `elapsed_since(state)` | `(String) -> Duration` | Time since the instance entered the named state |
| `len(field)` | `(String \| Collection) -> Int` | Length of a string or collection |
| `contains(collection, value)` | `(Collection, Value) -> Bool` | Whether the collection contains the value |
| `now()` | `() -> DateTime` | Current UTC timestamp |

## ALL and ANY Predicates

Composition queries use `ALL` and `ANY` to query across child instances:

```
FIND SupportTicket WHERE ALL(tasks, status == "done")
```

The evaluator iterates over the `children` field in `EvalContext`:

- **ALL**: Returns `true` if the predicate holds for every child. Returns `true` for empty children (vacuous truth).
- **ANY**: Returns `true` if the predicate holds for at least one child. Returns `false` for empty children.

```rust
fn eval_all(children: &[BTreeMap<String, Value>], predicate: &Expression) -> Result<Value> {
    for child_data in children {
        let child_ctx = EvalContext { data: child_data.clone(), ..Default::default() };
        let result = eval_expr(predicate, &child_ctx)?;
        if result != Value::Bool(true) {
            return Ok(Value::Bool(false));
        }
    }
    Ok(Value::Bool(true)) // vacuous truth for empty
}
```

The vacuous truth semantics for `ALL` over an empty set is a deliberate design decision. A `SupportTicket` with no tasks satisfies `ALL(tasks, status == "done")`. If you want "has tasks and all are done", combine with `len(tasks) > 0 AND ALL(tasks, ...)`.
