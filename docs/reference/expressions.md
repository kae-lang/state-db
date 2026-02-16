# Expressions Reference

Expressions are used throughout SMQL in guards, WHERE clauses, MUTATE statements, and action arguments. This page documents all expression types and their evaluation rules.

## Operator Precedence

Operators are evaluated in this order (highest precedence first):

| Precedence | Operator | Description |
|------------|----------|-------------|
| 1 | `.` | Field access / dot notation |
| 2 | `NOT` | Logical negation |
| 3 | `*`, `/` | Multiplication, division |
| 4 | `+`, `-` | Addition, subtraction |
| 5 | `==`, `!=`, `<`, `>`, `<=`, `>=` | Comparison |
| 6 | `IS SET`, `IS NOT SET`, `IS NULL` | Null checks |
| 7 | `IN` | Set membership |
| 8 | `AND` | Logical AND |
| 9 | `OR` | Logical OR |

Parentheses `()` can override precedence:

```sql
GUARD : (priority == "high" OR priority == "critical") AND assignee IS SET
```

## Literals

| Type | Syntax | Examples |
|------|--------|---------|
| Text | `"string"` | `"hello"`, `"agent_1"` |
| Integer | digits | `42`, `0`, `-1` |
| Float | digits with decimal | `3.14`, `0.5` |
| Boolean | `true`, `false` | `true` |
| Duration | number + unit | `30s`, `5m`, `2h`, `7d`, `1h 30m` |
| Map | `{key: value}` | `{id: "agent_1", role: "admin"}` |
| Set | `{val, val}` in IN clauses | `("high", "critical")` |

## Field Access

### Simple Field Access

Reference a data field by name:

```sql
GUARD : priority == "high"
GUARD : assignee IS SET
GUARD : total > 0
```

### SELF

References the current instance's data as a map:

```sql
ACTION : EMIT("created", { order: SELF })
MUTATE : shipment = SPAWN Shipment { order: SELF }
```

### ACTOR

References the person or system performing the transition. Evaluates to a map with `id` and `role` fields:

```sql
-- Simple identity check
GUARD : ACTOR.id == assignee

-- Role-based check
GUARD : ACTOR.role == "admin"

-- Full actor comparison (ACTOR is a map)
GUARD : ACTOR == assignee
```

When you use `AS "user_id"`, ACTOR evaluates to `{id: "user_id"}`. To match against an actor in a guard like `ACTOR == assignee`, the `assignee` field must also be a map (e.g., `{id: "user_id"}`).

### Dot Notation

Access nested fields with dot notation:

```sql
GUARD : ACTOR.role == "admin"
GUARD : ACTOR.id == customer_id
GUARD : shipment.STATE IS dispatched
GUARD : PARENT.customer == "acme"
```

### PARENT

Access the parent instance's data in child machines:

```sql
ACTION : NOTIFY(PARENT.customer, "item.backordered")
GUARD  : PARENT.total > 0
```

## Comparison Operators

| Operator | Description | Example |
|----------|-------------|---------|
| `==` | Equal | `priority == "high"` |
| `!=` | Not equal | `state != "closed"` |
| `<` | Less than | `total < 1000` |
| `>` | Greater than | `effort_days > 5` |
| `<=` | Less than or equal | `satisfaction <= 3` |
| `>=` | Greater than or equal | `total >= 500` |

### Type Compatibility

Comparisons require compatible types:

| Left Type | Right Type | Result |
|-----------|-----------|--------|
| Int | Int | OK |
| Float | Float | OK |
| Int | Float | OK (promoted) |
| Text | Text | OK (lexicographic) |
| Bool | Bool | OK |
| Duration | Duration | OK |
| DateTime | DateTime | OK |
| Map | Map | OK (structural equality) |

::: warning
`Money` and `Ref` types do not compare with `Int`. A guard like `total > 0` will fail if `total` is `MONEY(USD)`. Use the appropriate type in comparisons.
:::

## Arithmetic Operators

| Operator | Description | Example |
|----------|-------------|---------|
| `+` | Addition | `total + tax` |
| `-` | Subtraction | `budget - spent` |
| `*` | Multiplication | `quantity * price` |
| `/` | Division | `total / count` |

Arithmetic works on `Int` and `Float` values. Int/Float mixed operations promote to Float.

## Logical Operators

| Operator | Description | Example |
|----------|-------------|---------|
| `AND` | Both must be true | `assignee IS SET AND priority == "high"` |
| `OR` | Either can be true | `ACTOR == assignee OR ACTOR.role == "admin"` |
| `NOT` | Negation | `NOT assignee IS SET` |

## Null Checks

| Predicate | Description | Example |
|-----------|-------------|---------|
| `IS SET` | Field has a non-null value | `assignee IS SET` |
| `IS NOT SET` | Field is null or absent | `resolution_note IS NOT SET` |
| `IS NULL` | Alias for IS NOT SET | `assignee IS NULL` |

## Set Membership

The `IN` operator checks if a value belongs to a set:

```sql
GUARD : priority IN ("high", "critical")
GUARD : ACTOR.role IN ("admin", "supervisor")
GUARD : state IN ("open", "triaged")
```

## State Predicates

| Predicate | Description | Context |
|-----------|-------------|---------|
| `STATE IS state` | Instance is in the named state | WHERE, GUARD |
| `STATE IN {s1, s2}` | Instance is in one of the named states | WHERE, GUARD |

In composition guards, check child state:

```sql
GUARD : ALL(items, STATE IS confirmed)
GUARD : ANY(stages, STATE IS failed)
GUARD : shipment.STATE IS dispatched
```

## Collection Predicates

| Predicate | Description | Empty Collection |
|-----------|-------------|------------------|
| `ALL(collection, predicate)` | Every item matches | **true** (vacuous truth) |
| `ANY(collection, predicate)` | At least one matches | **false** |
| `COUNT(collection)` | Number of items | 0 |

```sql
-- All line items must be confirmed
GUARD : ALL(items, STATE IS confirmed)

-- At least one stage failed
GUARD : ANY(stages, STATE IS failed)

-- Must have at least one item
GUARD : items.count > 0
```

## Signal Predicate

Check if a signal has been received from another machine:

```sql
GUARD : SIGNAL FROM PaymentProcess WHERE state == "succeeded"
```

## Temporal Expressions

### Built-in Temporal Functions

| Function | Returns | Description |
|----------|---------|-------------|
| `elapsed()` | Duration | Time since instance was spawned |
| `elapsed_in_state()` | Duration | Time in the current state |
| `elapsed_since(state)` | Duration | Time since the instance was last in the named state |
| `timeout_remaining()` | Duration or Null | Time remaining on the active timeout |
| `NOW()` | DateTime | Current date and time (UTC) |
| `TODAY()` | Date | Current date (UTC) |

### Duration Comparisons

```sql
GUARD : elapsed_since(resolved) < 30d
GUARD : elapsed_since(resolved) >= 7d
GUARD : elapsed_in_state() > 1h
GUARD : timeout_remaining() < 1h
```

### Duration Literals

| Unit | Suffix | Example |
|------|--------|---------|
| Seconds | `s` | `30s` |
| Minutes | `m` | `5m` |
| Hours | `h` | `2h` |
| Days | `d` | `7d` |

Combined: `1h 30m`, `2d 12h`

::: info
24 hours is displayed as `1d`, not `24h`. This is a normalization behavior of the duration type.
:::

## Guard Evaluation Rules

When multiple guards appear on the same transition, **all must pass**:

```sql
in_progress -> resolved {
  GUARD : resolution_note IS SET      -- must have a note
  GUARD : ACTOR == assignee OR ACTOR.role == "admin"  -- must be authorized
}
```

If any guard fails, the transition is denied and all failures are reported in the error message.

Guards are evaluated **after** WITH data is applied. This means you can provide a required field and satisfy its guard in the same command:

```sql
TRANSITION "id" TO resolved WITH { resolution_note: "Fixed" } AS "agent_1"
-- The "resolution_note IS SET" guard passes because WITH runs first
```
