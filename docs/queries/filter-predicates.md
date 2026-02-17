# Filter Predicates

Predicates are used in `WHERE` clauses for `FIND`, `AGGREGATE`, `TRANSITION ALL`, `PATHS`, `FUNNEL`, and `COMPARE PATHS` queries.

## State Predicates

| Predicate | Description | Example |
|-----------|-------------|---------|
| `STATE IS state` | Instance is in exact state | `STATE IS open` |
| `STATE IN {s1, s2}` | Instance is in one of listed states | `STATE IN {open, triaged}` |

## Data Predicates

### Comparison Operators

| Operator | Description | Example |
|----------|-------------|---------|
| `==` | Equality | `priority == "critical"` |
| `!=` | Inequality | `status != "draft"` |
| `>` | Greater than | `item_count > 10` |
| `<` | Less than | `age < 30` |
| `>=` | Greater or equal | `version >= 2` |
| `<=` | Less or equal | `score <= 100` |

### Null Checks

| Predicate | Description | Example |
|-----------|-------------|---------|
| `field IS SET` | Field is not null | `assignee IS SET` |
| `field IS NOT SET` | Field is null | `assignee IS NOT SET` |
| `field IS NULL` | Alias for IS NOT SET | `assignee IS NULL` |

### Set Membership

| Predicate | Description | Example |
|-----------|-------------|---------|
| `field IN (a, b, c)` | Value in list | `priority IN ("high", "critical")` |

## Logical Operators

Use `AND`, `OR`, and `NOT` to combine predicates. Parentheses control grouping:

```smql
-- AND / OR
FIND SupportTicket WHERE STATE IS open AND priority == "critical"
FIND SupportTicket WHERE STATE IS open OR STATE IS triaged

-- NOT
FIND SupportTicket WHERE NOT (STATE IS closed)

-- Parentheses for grouping
FIND SupportTicket WHERE (STATE IS open OR STATE IS triaged) AND assignee IS SET
```

Operator precedence (highest to lowest): `NOT` > `AND` > `OR`.

## Arithmetic Expressions

Arithmetic operators can be used inside predicates:

| Operator | Description | Example |
|----------|-------------|---------|
| `+` | Addition | `score + bonus > 100` |
| `-` | Subtraction | `total - discount > 0` |
| `*` | Multiplication | `quantity * price > 500` |
| `/` | Division | `total / item_count < 10` |

```smql
FIND Order WHERE quantity * unit_price > 1000
```

## Special References

| Reference | Description | Example |
|-----------|-------------|---------|
| `SELF` | The current instance | `SELF.state` |
| `ACTOR` | The actor performing a transition | `ACTOR.role == "admin"` |
| `ACTOR.field` | Field on the actor | `ACTOR.id == assignee.id` |

Dot notation allows accessing nested fields:

```smql
FIND SupportTicket WHERE assignee.role == "support"
```

## Functions

Functions can be used in expressions:

| Function | Description | Returns |
|----------|-------------|---------|
| `elapsed()` | Time spent in current state | Duration |
| `elapsed_in_state()` | Alias for `elapsed()` | Duration |
| `NOW()` | Current timestamp | DateTime |
| `TODAY()` | Current date | Date |
| `timeout_remaining()` | Time until timeout fires | Duration or null |
| `count(collection)` | Number of items in a list/set | Int |
| `len(field)` | Length of a string, list, or set | Int |
| `lower(field)` | Lowercase a string | Text |
| `upper(field)` | Uppercase a string | Text |

```smql
-- Find instances that have been in their current state for over an hour
FIND SupportTicket WHERE elapsed() > 1h

-- Find tickets where the timeout is about to expire
FIND SupportTicket WHERE timeout_remaining() < 5m
```

## Pattern Matching

`PATTERN` is recognized in the grammar as a data constraint (see [Grammar](../reference/grammar.md)), but runtime regex matching in `WHERE` clauses is not currently implemented. The syntax below is reserved for future use:

```smql
-- Not yet supported at runtime
FIND SupportTicket WHERE PATTERN("^bug-.*") == title
```

## Collection Predicates

For machines with children, use `ALL` and `ANY` to query child instance states:

| Predicate | Description |
|-----------|-------------|
| `ALL(children, predicate)` | True if every child matches (true for empty collections) |
| `ANY(children, predicate)` | True if at least one child matches (false for empty collections) |

```smql
-- Guard: all child items must be in "completed" state
GUARD : ALL(items, STATE IS completed)

-- Guard: at least one item is still pending
GUARD : ANY(items, STATE IS pending)
```

## Signal Predicates

Used in transition guards to check the state of instances in another machine:

```smql
GUARD : SIGNAL FROM PaymentService WHERE STATE IS approved
```

## Reserved Keywords

The following keywords are recognized by the parser but not yet available as query filters:

- `STUCK_IN` — reserved for future dwell-time queries
- `HAS_VISITED` — reserved for trail-based history queries
- `ALIVE` — reserved for non-terminal state filter
- `TERMINATED` — reserved for terminal state filter
