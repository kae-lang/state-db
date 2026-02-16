# Guards

Guards are boolean expressions that must evaluate to `true` for a transition to proceed. If any guard fails, the transition is denied.

## Syntax

```sql
source -> target {
  GUARD : <expression>
}
```

Multiple guards are logically ANDed:

```sql
open -> triaged {
  GUARD : assignee IS SET
  GUARD : priority != "low"
}
```

## Operators

### Comparison
| Operator | Description |
|----------|-------------|
| `==` | Equal |
| `!=` | Not equal |
| `>` | Greater than |
| `<` | Less than |
| `>=` | Greater than or equal |
| `<=` | Less than or equal |

### Logical
| Operator | Description |
|----------|-------------|
| `AND` | Logical AND |
| `OR` | Logical OR |
| `NOT` | Logical NOT |

### Membership
| Operator | Description |
|----------|-------------|
| `IN (a, b, c)` | Value is in the list |
| `IS SET` | Value is not null |
| `IS NOT SET` | Value is null |

## Referencing Data

Guards can reference any data field by name:

```sql
GUARD : priority == "critical"
GUARD : count > 0
GUARD : assignee IS SET
```

## The ACTOR Keyword

`ACTOR` refers to the identity performing the transition. It's a map with `id` and `role` fields:

```sql
GUARD : ACTOR == assignee
GUARD : ACTOR.role == "admin"
GUARD : ACTOR.id == customer_id
GUARD : ACTOR.role IN ("admin", "supervisor")
```

::: warning
`ACTOR` evaluates to a Map with `id` and `role` keys. When comparing `ACTOR == assignee`, the `assignee` field must also be a Map (not plain text). Use `ACTOR.id` for string comparisons.
:::

## The SELF Keyword

`SELF` refers to the current instance. Useful in action expressions:

```sql
ACTION : EMIT("order.placed", { order: SELF })
```

## Built-in Functions

| Function | Description | Example |
|----------|-------------|---------|
| `elapsed_since(state)` | Duration since entering a state | `elapsed_since(resolved) < 30d` |
| `len(field)` | Length of string or collection | `len(tags) > 0` |
| `contains(collection, value)` | Check membership | `contains(tags, "urgent")` |
| `now()` | Current timestamp | `created_at < now() - 7d` |

## Child Predicates

When a machine has children, guards can use `ALL()` and `ANY()`:

```sql
GUARD : ALL(items, STATE IS confirmed)
GUARD : ANY(stages, STATE IS failed)
```

See [Child Predicates](../composition/child-predicates) for details.

## Duration Literals

Duration values are used in time comparisons:

| Literal | Meaning |
|---------|---------|
| `30s` | 30 seconds |
| `5m` | 5 minutes |
| `24h` | 24 hours |
| `7d` | 7 days |
| `1w` | 1 week |

::: tip
`elapsed_since()` takes a state name, not a timestamp. The engine tracks when each state was entered.
:::
